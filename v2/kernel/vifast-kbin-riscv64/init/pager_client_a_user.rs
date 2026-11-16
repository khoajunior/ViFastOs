// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-70** — `pager_client_a` U-mode: chạm 3 trang file 0 (làm đầy cache 3),
//! rồi trang 4 (miss + trục xuất LRU), rồi trang 0 (nạp **lại** sau trục xuất — dữ liệu phải
//! đúng), rồi trang 2 (đã map sẵn, đọc trực tiếp). Mọi dữ liệu kiểm bằng `byte_at`.

#![no_std]
#![no_main]

extern crate vifast_abi;
extern crate vifast_user_abi;
extern crate vifast_vfs;

#[path = "pager_common.rs"]
mod common;
#[path = "pager_client_common.rs"]
mod client;

use common::{a_va, result, A_TABLE, RESULT_MAGIC_A};

/// Quy đổi VA của client A: mọi trang đều thuộc file 0 nên chỉ phụ thuộc `page`.
fn va_of(file: u32, page: u32) -> u64 {
    debug_assert_eq!(file, 0);
    let _ = file;
    a_va(page)
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    client::client_main(RESULT_MAGIC_A, &A_TABLE, va_of)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    vifast_user_abi::exit(99)
}

/// Giữ `result` trong biên dịch (bố cục dùng chung).
const _LAYOUT: u64 = result::DONE;
