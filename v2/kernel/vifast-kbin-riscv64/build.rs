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

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    println!("cargo:rustc-link-arg=-T{dir}/linker.ld");
    println!("cargo:rustc-link-arg=--gc-sections");
    println!("cargo:rerun-if-changed=linker.ld");

    // Chỉ ảnh test mới cần `init` ELF; ảnh sản phẩm (feature off) không sinh/không nhúng.
    println!("cargo:rerun-if-changed=init/init_user.rs");
    println!("cargo:rerun-if-changed=init/init_user.S");
    println!("cargo:rerun-if-changed=init/init.ld");
    if std::env::var_os("CARGO_FEATURE_USER").is_none() {
        return;
    }
    let target = std::env::var("TARGET").unwrap_or_default();
    if target != "riscv64gc-unknown-none-elf" {
        return;
    }
    build_init_elf(Path::new(&dir));
}

/// Biên dịch + liên kết chương trình `init` user thành ELF64 ở `$OUT_DIR/init.elf`.
///
/// Dùng `$RUSTC` (cargo cấp) thay vì tự gọi `cargo` lồng nhau: cùng toolchain, không đổi
/// workspace, không phụ thuộc `llvm-objcopy`/assembler ngoài.
fn build_init_elf(dir: &Path) {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let out = out_dir.join("init.elf");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let source = dir.join("init/init_user.rs");
    let script = dir.join("init/init.ld");
    let status = Command::new(&rustc)
        .current_dir(dir)
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
        .arg("relocation-model=static")
        .arg("-C")
        .arg(format!("link-arg=-T{}", script.display()))
        .arg("-C")
        .arg("link-arg=--gc-sections")
        .arg("-C")
        .arg("link-arg=--no-relax")
        .arg("-o")
        .arg(&out)
        .arg(&source)
        .status()
        .unwrap_or_else(|error| panic!("KT-53: không chạy được {:?}: {error}", rustc));
    if !status.success() {
        panic!("KT-53: build init user ELF FAIL (rustc {:?})", rustc);
    }
    let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!("cargo:warning=KT-53 init.elf {} B", len);
}
