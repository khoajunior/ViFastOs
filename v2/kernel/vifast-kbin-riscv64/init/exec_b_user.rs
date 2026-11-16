// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-74** — chương trình U-mode bước **B** của chuỗi `exec` (A→B→C).
//!
//! Mỗi bước là một **ELF riêng**: `exec(path)` nạp ảnh này **thay** ảnh trước, nên nếu cơ chế
//! sai (không đổi VSpace/entry, rò Frame, giữ thanh ghi cũ) thì TAP/evidence của ảnh test sẽ lộ.
//!
//! In marker qua `DebugPutStr` (syscall 12 — **trap thật**, nên kernel quan sát được `x4`/`tp`
//! của U-mode **tại trap**: bằng chứng TLS phần cứng, không phải kernel tự khai) rồi
//! `Exit(2)` (`FaultKind::Exit{2}`) để kernel biết bước đã chạy xong và chuyển sang
//! `exec` kế tiếp.
#![no_std]
#![no_main]

extern crate vifast_user_abi;
use vifast_user_abi::{exit, put};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    put(b"# exec: step b up\n");
    exit(2);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
