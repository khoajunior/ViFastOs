// SPDX-License-Identifier: MIT OR Apache-2.0
//! **Shim ABI U-mode** cho các chương trình Rust của KT-65 (`procsrv`/`shell`/`con`).
//!
//! Vì sao có crate này: `core::arch::asm!` là `unsafe`, mà luật cấu trúc R5 (D2-17) chỉ cho
//! `unsafe` trong danh sách đóng — `kernel/vifast-plat-*` nằm trong danh sách. Gom toàn bộ asm
//! `ecall` vào **một** crate nền tảng, các chương trình U-mode chỉ còn lời gọi an toàn:
//! [`syscall7`]/[`syscall2`]/[`put`]/[`exit`]. ABI đúng bảng syscall hiện có (không thêm syscall).
//!
//! Crate được `build.rs` của `vifast-kbin-riscv64` biên dịch thành `rlib` riscv64 rồi truyền
//! `--extern` cho từng ELF user; **không** là thành viên workspace (không build cho host).
#![no_std]

use core::arch::asm;

/// Syscall `NBSend` (bảng syscall KT-62).
pub const SYS_NB_SEND: u64 = 2;
/// Syscall `NBRecv`.
pub const SYS_NB_RECV: u64 = 4;
/// Syscall `Yield`.
pub const SYS_YIELD: u64 = 8;
/// Syscall `DebugPutStr` — kernel đọc vùng nhớ user và in ra UART.
pub const SYS_DEBUG_PUT_STR: u64 = 12;
/// Syscall `Exit` — core ghi `FaultKind::Exit{code}` vào `fault_ep` (QD §1 Q-A).
pub const SYS_EXIT: u64 = 14;

/// Gọi syscall 7 tham số; trả `[error, value, info, MR0..MR3]` theo façade hiện có.
#[inline(always)]
pub fn syscall7(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> [u64; 7] {
    let mut r0 = a0;
    let mut r1 = a1;
    let mut r2 = a2;
    let mut r3 = a3;
    let mut r4 = a4;
    let mut r5 = a5;
    let mut r6 = 0u64;
    // SAFETY: `ecall` vào kernel đã kiểm; thanh ghi a0..a6/a7 đúng ABI syscall của K3.
    unsafe {
        asm!(
            "ecall",
            inlateout("a0") r0,
            inlateout("a1") r1,
            inlateout("a2") r2,
            inlateout("a3") r3,
            inlateout("a4") r4,
            inlateout("a5") r5,
            inlateout("a6") r6,
            in("a7") nr,
            options(nostack)
        );
    }
    [r0, r1, r2, r3, r4, r5, r6]
}

/// Gọi syscall 2 tham số (dùng cho `DebugPutStr`/`Exit` của chương trình tối giản).
#[inline(always)]
pub fn syscall2(nr: u64, a0: u64, a1: u64) -> [u64; 2] {
    let mut r0 = a0;
    let mut r1 = a1;
    // SAFETY: như [`syscall7`].
    unsafe {
        asm!(
            "ecall",
            inlateout("a0") r0,
            inlateout("a1") r1,
            in("a7") nr,
            options(nostack)
        );
    }
    [r0, r1]
}

/// In một lát `&[u8]` của chính tiến trình (vùng đã map `U|R`) qua syscall `DebugPutStr`.
pub fn put(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let _ = syscall7(
        SYS_DEBUG_PUT_STR,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
        0,
        0,
        0,
    );
}

/// `Exit(code)` rồi chờ kernel thu hồi — không quay lại U-mode.
pub fn exit(code: u64) -> ! {
    let _ = syscall7(SYS_EXIT, code, 0, 0, 0, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}
