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
    println!("cargo:rerun-if-changed=init/memsrv_user.rs");
    println!("cargo:rerun-if-changed=init/client_user.rs");
    println!("cargo:rerun-if-changed=init/capa_server_user.rs");
    println!("cargo:rerun-if-changed=init/capa_client_user.rs");
    println!("cargo:rerun-if-changed=init/ring_server_user.rs");
    println!("cargo:rerun-if-changed=init/ring_client_user.rs");
    // KT-73: consumer ring UART → U-mode (ảnh `ktest-con`).
    println!("cargo:rerun-if-changed=init/con_client_user.rs");
    // KT-74: ba bước **A/B/C** của chuỗi `exec` — mỗi bước một ELF riêng (TAP `ktest-exec`).
    println!("cargo:rerun-if-changed=init/exec_a_user.rs");
    println!("cargo:rerun-if-changed=init/exec_b_user.rs");
    println!("cargo:rerun-if-changed=init/exec_c_user.rs");
    println!("cargo:rerun-if-changed=init/exec_d_user.rs");
    println!("cargo:rerun-if-changed=init/pager_common.rs");
    println!("cargo:rerun-if-changed=init/pager_client_common.rs");
    println!("cargo:rerun-if-changed=init/pager_server_user.rs");
    println!("cargo:rerun-if-changed=init/pager_client_a_user.rs");
    println!("cargo:rerun-if-changed=init/pager_client_b_user.rs");
    println!("cargo:rerun-if-changed=../vifast-plat-user/lib.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-abi/src/lib.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-abi/src/generated.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-ring/src/lib.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-idl/src/lib.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-idl/src/generated.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-idl/src/generated_pager.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/lib.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/cache.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/core.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/disk.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/layout.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/stats.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-vfs/src/status.rs");
    println!("cargo:rerun-if-changed=../../lib/vifast-fs/src/lib.rs");
    println!("cargo:rerun-if-changed=init/init.ld");
    if std::env::var_os("CARGO_FEATURE_USER").is_none() {
        return;
    }
    let target = std::env::var("TARGET").unwrap_or_default();
    if target != "riscv64gc-unknown-none-elf" {
        return;
    }
    let dir = Path::new(&dir);
    let user_lib = build_user_lib(dir);
    // KT-69: rlib `no_std` cho chương trình U-mode — hằng ABI sinh từ spec + stub IDL sinh từ
    // `.vidl` + ring (Acquire/Release, không unsafe). Không nằm trong workspace host vì mỗi
    // crate được biên dịch thẳng cho riscv64 ở đây (cùng toolchain, không cargo lồng).
    let abi_lib = build_rlib(dir, "vifast_abi", "../../lib/vifast-abi/src/lib.rs", &[]);
    let fs_lib = build_rlib(dir, "vifast_fs", "../../lib/vifast-fs/src/lib.rs", &[]);
    let ring_lib = build_rlib(dir, "vifast_ring", "../../lib/vifast-ring/src/lib.rs", &[]);
    let idl_lib = build_rlib(dir, "vifast_idl", "../../lib/vifast-idl/src/lib.rs", &[]);
    // KT-73: codec byte `conchan` + hằng console — **cùng** crate kernel dùng (`vifast-virtio`).
    let virtio_lib = build_rlib(
        dir,
        "vifast_virtio",
        "../../lib/vifast-virtio/src/lib.rs",
        &[],
    );
    // KT-70: pager/page-cache — **dùng lại** `vifast_fs::BlockDev` + bảng lỗi `vifast-abi`.
    let vfs_lib = build_rlib(
        dir,
        "vifast_vfs",
        "../../lib/vifast-vfs/src/lib.rs",
        &[
            ("vifast_fs", fs_lib.as_path()),
            ("vifast_abi", abi_lib.as_path()),
        ],
    );
    let shim = [("vifast_user_abi", user_lib.as_path())];
    let shim_all = [
        ("vifast_user_abi", user_lib.as_path()),
        ("vifast_abi", abi_lib.as_path()),
        ("vifast_ring", ring_lib.as_path()),
        ("vifast_idl", idl_lib.as_path()),
        ("vifast_vfs", vfs_lib.as_path()),
        ("vifast_fs", fs_lib.as_path()),
    ];
    build_user_elf(dir, "init_user.rs", "init.elf", &[]);
    build_user_elf(dir, "procsrv_user.rs", "procsrv.elf", &shim);
    // KT-73: shell đọc **byte thật** từ ring UART → cần `vifast_ring` + codec `conchan`.
    let shim_shell = [
        ("vifast_user_abi", user_lib.as_path()),
        ("vifast_ring", ring_lib.as_path()),
        ("vifast_virtio", virtio_lib.as_path()),
    ];
    build_user_elf(dir, "shell_user.rs", "shell.elf", &shim_shell);
    build_user_elf(dir, "child_user.rs", "child.elf", &shim);
    // KT-67a: chương trình U-mode invocation/memsrv (D2-97).
    build_user_elf(dir, "inv_user.rs", "inv.elf", &shim);
    // KT-67b: **hai tiến trình riêng** — memsrv (A) chuyển cap Frame, client (B) nhận + map.
    build_user_elf(dir, "memsrv_user.rs", "memsrv.elf", &shim);
    build_user_elf(dir, "client_user.rs", "client.elf", &shim);
    // KT-68: **capalloc** — server cấp/thu hồi cap + client dùng cap (hai tiến trình riêng).
    build_user_elf(dir, "capa_server_user.rs", "capa_server.elf", &shim);
    build_user_elf(dir, "capa_client_user.rs", "capa_client.elf", &shim);
    // KT-69: **hai tiến trình ring** — server producer + client consumer trên **một Frame**
    // chuyển qua cap; cả hai dùng stub IDL sinh từ `.vidl`.
    build_user_elf(dir, "ring_server_user.rs", "ring_server.elf", &shim_all);
    build_user_elf(dir, "ring_client_user.rs", "ring_client.elf", &shim_all);
    // KT-73: **consumer U-mode** của kênh UART → ring (đọc byte thật, không message cắm sẵn).
    let shim_con = [
        ("vifast_user_abi", user_lib.as_path()),
        ("vifast_ring", ring_lib.as_path()),
        ("vifast_virtio", virtio_lib.as_path()),
    ];
    build_user_elf(dir, "con_client_user.rs", "con_client.elf", &shim_con);
    // KT-74: **ba ảnh `exec`** — A/B/C chỉ in marker + `Exit` riêng, để `exec(path)` phải nạp ELF
    // **khác** thật (entry/segment khác) chứ không phải "chạy lại cùng ảnh".
    // D5 (reviewer): ảnh A phải lấy **method id từ hằng sinh tự động** (`vifast_abi::TCB_SETTLS`)
    // chứ không chép số — cùng nguồn với core, nên `const` và `method` quan sát được là **hai**
    // đường độc lập.
    build_user_elf(
        dir,
        "exec_a_user.rs",
        "exec_a.elf",
        &[
            ("vifast_user_abi", user_lib.as_path()),
            ("vifast_abi", abi_lib.as_path()),
        ],
    );
    build_user_elf(dir, "exec_b_user.rs", "exec_b.elf", &shim);
    build_user_elf(dir, "exec_c_user.rs", "exec_c.elf", &shim);
    // D = bước **không** `SetTls`: ca "TLS reset sau exec" + "SetTls rồi resume" cần ảnh không
    // tự ghi TLS (nếu không, 0 và 0xB2 không phân biệt được).
    build_user_elf(dir, "exec_d_user.rs", "exec_d.elf", &shim);
    // KT-70: **ba tiến trình** pager — server (page cache + LRU) và hai client.
    build_user_elf(dir, "pager_server_user.rs", "pager_server.elf", &shim_all);
    build_user_elf(
        dir,
        "pager_client_a_user.rs",
        "pager_client_a.elf",
        &shim_all,
    );
    build_user_elf(
        dir,
        "pager_client_b_user.rs",
        "pager_client_b.elf",
        &shim_all,
    );
}

/// Biên dịch + liên kết một chương trình user thành ELF64 ở `$OUT_DIR/<out_name>`.
///
/// Dùng `$RUSTC` (cargo cấp) thay vì tự gọi `cargo` lồng nhau: cùng toolchain, không đổi
/// workspace, không phụ thuộc `llvm-objcopy`/assembler ngoài Rust.
/// Biên dịch shim ABI U-mode (`vifast-plat-user`) thành `rlib` riscv64 trong `$OUT_DIR`.
///
/// `unsafe` asm của U-mode nằm gọn ở crate nền tảng đã nằm trong danh sách D2-17
/// (`kernel/vifast-plat-*`); các chương trình Rust còn lại **không** có `unsafe` (luật R5).
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

/// Biên dịch một crate `no_std` (`vifast-abi`/`vifast-ring`/`vifast-idl`) thành `rlib` riscv64.
///
/// `source` tương đối với thư mục crate kbin; `-D warnings` để stub/ring không cảnh báo.
fn build_rlib(dir: &Path, crate_name: &str, source: &str, externs: &[(&str, &Path)]) -> PathBuf {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rlib = out_dir.join(format!("lib{crate_name}.rlib"));
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let source = dir.join(source);
    let mut cmd = Command::new(&rustc);
    cmd.current_dir(dir)
        .arg("--crate-name")
        .arg(crate_name)
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
        .arg("-L")
        .arg(format!("dependency={}", out_dir.display()));
    for (name, lib) in externs {
        cmd.arg("--extern")
            .arg(format!("{}={}", name, lib.display()));
    }
    let status = cmd
        .arg("-o")
        .arg(&rlib)
        .arg(&source)
        .status()
        .unwrap_or_else(|error| panic!("KT-70: không chạy được {:?}: {error}", rustc));
    if !status.success() {
        panic!("KT-70: build rlib {crate_name} FAIL (rustc {:?})", rustc);
    }
    rlib
}

fn build_user_elf(dir: &Path, source_name: &str, out_name: &str, externs: &[(&str, &Path)]) {
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
    // rlib bắc cầu (`vifast_vfs` → `vifast_fs`/`vifast_abi`) nằm cùng OUT_DIR.
    cmd.arg("-L")
        .arg(format!("dependency={}", out_dir.display()));
    for (name, lib) in externs {
        cmd.arg("--extern")
            .arg(format!("{}={}", name, lib.display()));
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
