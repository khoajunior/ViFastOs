// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-70** — `pager_client_b` U-mode: trang 2 file 0 (**hit cache** — pager map cùng Frame như
//! client A), trang 0/1 file 1 (miss), trang 2 file 0 lần nữa (**hit lần hai** sau khi A nạp
//! lại), rồi chạm **trang ngoài file** (page 99) để pager trả **mã đúng** (kernel dừng client này).

#![no_std]
#![no_main]

extern crate vifast_abi;
extern crate vifast_user_abi;
extern crate vifast_vfs;

#[path = "pager_common.rs"]
mod common;
#[path = "pager_client_common.rs"]
mod client;

use common::{b_va, result, B_TABLE, RESULT_MAGIC_B};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    client::client_main(RESULT_MAGIC_B, &B_TABLE, b_va)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    vifast_user_abi::exit(99)
}

/// Giữ `result` trong biên dịch (bố cục dùng chung).
const _LAYOUT: u64 = result::DONE;
