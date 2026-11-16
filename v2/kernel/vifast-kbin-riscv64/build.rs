// SPDX-License-Identifier: MIT OR Apache-2.0
//! Đăng ký linker script và các cờ liên kết cho **cả ba** ảnh riscv64 K3 (KT-44, KT-53):
//! ảnh sản phẩm `vifast-kbin-riscv64`, ảnh test `vifast-kbin-ktest*` (feature `ktest`).
//! Cùng một `linker.ld` (`.stack` NOLOAD + `.stack_guard`, D2-72 Q5).
//!
//! KT-53: khi build ảnh **test U-mode** (`CARGO_FEATURE_USER`), build script biên dịch
//! [`init/init_user.rs`] (chỉ `global_asm!` của [`init/init_user.S`]) bằng chính `$RUSTC` thành
//! **ELF64 user** tại `0x4000_0000` theo [`init/init.ld`], ghi vào `$OUT_DIR/init.elf` để
//! `src/ktest_user.rs` `include_bytes!` — không cần toolchain ngoài Rust, không sinh file trong
//! cây nguồn, không ảnh hưởng ảnh sản phẩm (feature off ⇒ không chạy).
//!
//! KT-65: thêm **procsrv** (Rust), **shell** (Rust) và **con** (Rust) — cùng đường build, mỗi
//! chương trình một ELF64 riêng (`procsrv.elf`/`shell.elf`/`child.elf`), nạp bằng cùng loader
//! `vifast-elf` như `init`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    println!("cargo:rustc-link-arg=-T{dir}/linker.ld");
    println!("cargo:rustc-link-arg=--gc-sections");
    println!("cargo:rerun-if-changed=linker.ld");

    // Chỉ ảnh test mới cần các ELF user; ảnh sản phẩm (feature off) không sinh/không nhúng.
    println!("cargo:rerun-if-changed=init/init_user.rs");
    println!("cargo:rerun-if-changed=init/init_user.S");
    println!("cargo:rerun-if-changed=init/procsrv_user.rs");
    println!("cargo:rerun-if-changed=init/shell_user.rs");
    println!("cargo:rerun-if-changed=init/child_user.rs");
    println!("cargo:rerun-if-changed=init/inv_user.rs");
    println!("cargo:rerun-if-changed=init/inv_user.S");
    println!("cargo:rerun-if-changed=init/init.ld");
    if std::env::var_os("CARGO_FEATURE_USER").is_none() {
        return;
    }
    let target = std::env::var("TARGET").unwrap_or_default();
    if target != "riscv64gc-unknown-none-elf" {
        return;
    }
    let user_lib = build_user_lib(Path::new(&dir));
    build_user_elf(Path::new(&dir), "init_user.rs", "init.elf", None);
    build_user_elf(
        Path::new(&dir),
        "procsrv_user.rs",
        "procsrv.elf",
        Some(&user_lib),
    );
    build_user_elf(
        Path::new(&dir),
        "shell_user.rs",
        "shell.elf",
        Some(&user_lib),
    );
    build_user_elf(
        Path::new(&dir),
        "child_user.rs",
        "child.elf",
        Some(&user_lib),
    );
    // KT-67a: chương trình U-mode invocation/memsrv (D2-97).
    build_user_elf(Path::new(&dir), "inv_user.rs", "inv.elf", Some(&user_lib));
}

/// Biên dịch + liên kết một chương trình user thành ELF64 ở `$OUT_DIR/<out_name>`.
///
/// Dùng `$RUSTC` (cargo cấp) thay vì tự gọi `cargo` lồng nhau: cùng toolchain, không đổi
/// workspace, không phụ thuộc `llvm-objcopy`/assembler ngoài Rust.
/// Biên dịch shim ABI U-mode (`vifast-plat-user`) thành `rlib` riscv64 trong `$OUT_DIR`.
///
/// `unsafe` asm của U-mode nằm gọn ở crate nền tảng đã nằm trong danh sách D2-17
/// (`kernel/vifast-plat-*`); ba chương trình Rust còn lại **không** có `unsafe` (luật R5).
fn build_user_lib(dir: &Path) -> PathBuf {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rlib = out_dir.join("libvifast_user_abi.rlib");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let source = dir.join("../vifast-plat-user/lib.rs");
    let status = Command::new(&rustc)
        .current_dir(dir)
        .arg("--crate-name")
        .arg("vifast_user_abi")
        .arg("--crate-type")
        .arg("rlib")
        .arg("--target")
        .arg("riscv64gc-unknown-none-elf")
        .arg("--edition")
        .arg("2021")
        .arg("-C")
        .arg("panic=abort")
        .arg("-C")
        .arg("opt-level=1")
        .arg("-C")
        .arg("debuginfo=0")
        .arg("-D")
        .arg("warnings")
        .arg("-o")
        .arg(&rlib)
        .arg(&source)
        .status()
        .unwrap_or_else(|error| panic!("KT-65: không chạy được {:?}: {error}", rustc));
    if !status.success() {
        panic!(
            "KT-65: build shim vifast-plat-user FAIL (rustc {:?})",
            rustc
        );
    }
    println!("cargo:rerun-if-changed=../vifast-plat-user/lib.rs");
    rlib
}

fn build_user_elf(dir: &Path, source_name: &str, out_name: &str, user_lib: Option<&Path>) {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let out = out_dir.join(out_name);
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let source = dir.join("init").join(source_name);
    let script = dir.join("init/init.ld");
    let mut cmd = Command::new(&rustc);
    cmd.current_dir(dir)
        .arg("--target")
        .arg("riscv64gc-unknown-none-elf")
        .arg("--edition")
        .arg("2021")
        .arg("-C")
        .arg("panic=abort")
        .arg("-C")
        .arg("opt-level=1")
        .arg("-C")
        .arg("debuginfo=0")
        .arg("-C")
        .arg("strip=symbols")
        .arg("-C")
        .arg("relocation-model=static")
        .arg("-C")
        .arg(format!("link-arg=-T{}", script.display()))
        .arg("-C")
        .arg("link-arg=--gc-sections")
        .arg("-C")
        .arg("link-arg=--no-relax")
        .arg("-o")
        .arg(&out);
    if let Some(lib) = user_lib {
        cmd.arg("--extern")
            .arg(format!("vifast_user_abi={}", lib.display()));
    }
    let status = cmd
        .arg(&source)
        .status()
        .unwrap_or_else(|error| panic!("KT-65: không chạy được {:?}: {error}", rustc));
    if !status.success() {
        panic!(
            "KT-65: build user ELF {source_name} FAIL (rustc {:?})",
            rustc
        );
    }
    let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!("cargo:warning=KT-65 {out_name} {len} B");
}
