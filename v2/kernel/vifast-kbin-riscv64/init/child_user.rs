// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-65 — tiến trình **con** chạy U-mode (Rust, ELF riêng nạp như `init`).
//!
//! In marker qua `DebugPutStr` (chứng minh đã chạy thật trong U-mode) rồi `Exit(42)`; kernel/core
//! ghi `FaultKind::Exit{42}` vào `fault_ep` của procsrv (QD §1 Q-A), procsrv `Recv` rồi core đưa
//! TCB về `Inactive` (D2-25). Đây là mắt xích "có vòng đời" của KT-65.
#![no_std]
#![no_main]

extern crate vifast_user_abi;
use vifast_user_abi::{exit, put};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"# child: up then exit 42\n";
    put(msg);
    // Core ghi fault Exit{42} tới fault_ep (procsrv) — QD §1 Q-A.
    exit(42);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
