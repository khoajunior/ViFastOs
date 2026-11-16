// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-74** — chương trình U-mode bước **D** của chuỗi `exec`: **không** `SetTls`.
//!
//! Dùng cho ca "TLS **reset** sau `exec`" + "`SetTls` rồi resume": nếu D tự đặt TLS thì không
//! phân biệt được giá trị do `exec` reset (0) với giá trị do chương trình tự ghi. D chỉ trap
//! (syscall `DebugPutStr`) để kernel quan sát `x4`/`tp` **tại trap**, rồi `Exit(4)`.
#![no_std]
#![no_main]

extern crate vifast_user_abi;
use vifast_user_abi::{exit, put};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    put(b"# exec: step d up\n");
    exit(4);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
