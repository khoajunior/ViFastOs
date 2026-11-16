// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-70 — thân chung của hai client pager: map khối kết quả, chạm từng trang theo bảng, kiểm
//! **toàn bộ** 4096 byte so với `vifast_vfs::byte_at`, ghi bộ đếm vào Frame kết quả.
//!
//! Mỗi client là một ELF riêng (`pager_client_a.elf`, `pager_client_b.elf`) — hai tiến trình
//! U-mode thật, hai VSpace/TCB/CSpace. Không `unsafe`; mọi truy cập bộ nhớ qua shim (D2-101).

use crate::common::{result, C_RESULT, C_VSPACE, FLAGS_RWU, VA_RESULT};
use vifast_user_abi::{call, exit, info, peek_u64, poke_u64, set_recv_window, yield_now};
use vifast_vfs::{byte_at, PAGE_SIZE};

/// `u64::MAX` = chưa có byte sai.
const NO_BAD: u64 = u64::MAX;

/// Ghi một trường khối kết quả.
fn put(field: u64, value: u64) {
    poke_u64(VA_RESULT + field * 8, value);
}

fn get(field: u64) -> u64 {
    peek_u64(VA_RESULT + field * 8)
}

/// Kiểm **toàn bộ** trang: trả `(ok_bytes, bad_bytes, first_bad)`.
fn validate(va: u64, file: u32, page: u32) -> (u64, u64, u64) {
    let mut ok = 0u64;
    let mut bad = 0u64;
    let mut first_bad = NO_BAD;
    let mut offset = 0usize;
    while offset < PAGE_SIZE {
        let word = peek_u64(va + offset as u64);
        let bytes = word.to_le_bytes();
        let mut index = 0usize;
        while index < 8 {
            let expected = byte_at(file, page, offset + index);
            if bytes[index] == expected {
                ok += 1;
            } else {
                if first_bad == NO_BAD {
                    first_bad = (offset + index) as u64;
                }
                bad += 1;
            }
            index += 1;
        }
        offset += 8;
    }
    (ok, bad, first_bad)
}

/// Chạy một client: bảng `(file, page)` theo đúng thứ tự; `va_of` quy đổi.
pub fn client_main(magic: u64, table: &[(u32, u32)], va_of: fn(u32, u32) -> u64) -> ! {
    set_recv_window([10, 11, 12, 13]);
    let mapped = call(
        C_RESULT,
        info(vifast_abi::FRAME_MAP, 0, false, 3),
        [C_VSPACE, VA_RESULT, FLAGS_RWU, 0],
    );
    if mapped[0] != 0 {
        // Mã lỗi ABI + 0x60 để đọc được nguyên nhân từ mã Exit (không phải evidence nghiệm thu).
        exit(0x60 + mapped[0])
    }
    put(result::MAGIC, magic);
    put(result::OK_PAGES, 0);
    put(result::BAD_PAGES, 0);
    put(result::OK_BYTES, 0);
    put(result::BAD_BYTES, 0);
    put(result::FIRST_BAD, NO_BAD);
    put(result::DONE, 0);
    let mut ok_pages = 0u64;
    let mut bad_pages = 0u64;
    let mut index = 0usize;
    while index < table.len() {
        let (file, page) = table[index];
        // Nhường CPU **trước** mỗi lần chạm: kernel có nhịp chạy pager giữa hai client.
        let _ = yield_now();
        put(result::YIELDS, get(result::YIELDS) + 1);
        let va = va_of(file, page);
        let (ok, bad, first_bad) = validate(va, file, page);
        if bad == 0 && ok == PAGE_SIZE as u64 {
            ok_pages += 1;
            put(result::OK_PAGES, ok_pages);
        } else {
            bad_pages += 1;
            put(result::BAD_PAGES, bad_pages);
            put(result::FIRST_BAD, first_bad);
        }
        put(result::OK_BYTES, get(result::OK_BYTES) + ok);
        put(result::BAD_BYTES, get(result::BAD_BYTES) + bad);
        put(result::LAST_FILE, u64::from(file));
        put(result::LAST_PAGE, u64::from(page));
        index += 1;
    }
    put(result::DONE, 1);
    let _ = yield_now();
    exit(0)
}
