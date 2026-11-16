// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hình học **thiết kế** của VFS tầng 1 (KT-70): trang 4 KiB, sector 512 B, hai file mẫu.
//!
//! Mọi hằng ở đây là **hằng thiết kế** (`const:` trong bằng chứng — D2-102a), không phải số
//! đo; ngưỡng cache **không nới** (D2-84/85 tinh thần: hằng cố định, không đổi để qua cổng).

/// Kích thước trang của pager — trùng trang 4 KiB của RISC-V/object layer.
pub const PAGE_SIZE: usize = 4096;
/// Kích thước sector của backend — **dùng lại** `vifast-fs` (một nguồn, không tạo trait thứ hai).
pub const SECTOR_SIZE: usize = vifast_fs::BLOCK_SIZE;
/// Số sector mỗi trang.
pub const SECTORS_PER_PAGE: u32 = (PAGE_SIZE / SECTOR_SIZE) as u32;
/// Số trang của `file_id = 0` trong backend mẫu.
pub const FILE0_PAGES: u32 = 3;
/// Số trang của `file_id = 1` trong backend mẫu.
pub const FILE1_PAGES: u32 = 2;
/// Số file của backend mẫu (file khác ⇒ từ chối).
pub const FILE_COUNT: u32 = 2;
/// **Ngưỡng cache** (số trang) — hằng thiết kế, không nới; vượt ⇒ trục xuất LRU.
pub const CACHE_CAPACITY: usize = 3;
/// Số sector của thiết bị RAM: file 0 (3 trang) + file 1 (2 trang) × 8 sector = **40 sector**
/// (20 KiB = **5 Frame** 4 KiB) — hình học thiết kế, không nới ngưỡng (xem `k6-vfs-pager.md` §6).
pub const DISK_SECTORS: u32 = (FILE0_PAGES + FILE1_PAGES) * SECTORS_PER_PAGE;

/// Số trang của một `file_id`; `None` = file không tồn tại.
pub const fn file_pages(file: u32) -> Option<u32> {
    match file {
        0 => Some(FILE0_PAGES),
        1 => Some(FILE1_PAGES),
        _ => None,
    }
}

/// Sector đầu tiên của `(file, page)`; `None` = ngoài file/không tồn tại.
pub const fn page_sector(file: u32, page: u32) -> Option<u32> {
    let base = match file {
        0 => 0,
        1 => FILE0_PAGES * SECTORS_PER_PAGE,
        _ => return None,
    };
    match file_pages(file) {
        Some(pages) if page < pages => Some(base + page * SECTORS_PER_PAGE),
        _ => None,
    }
}

/// `(file, page, offset trong trang)` của một sector vật lý; `None` = sector không thuộc file.
pub const fn sector_page(lba: u32) -> Option<(u32, u32, u32)> {
    let file0_span = FILE0_PAGES * SECTORS_PER_PAGE;
    if lba < file0_span {
        return Some((0, lba / SECTORS_PER_PAGE, lba % SECTORS_PER_PAGE));
    }
    let file1_span = FILE1_PAGES * SECTORS_PER_PAGE;
    if lba >= file0_span && lba < file0_span + file1_span {
        let rel = lba - file0_span;
        return Some((1, rel / SECTORS_PER_PAGE, rel % SECTORS_PER_PAGE));
    }
    None
}

/// Byte thứ `offset` của trang `page` trong `file` — **cùng một hàm** cho pager (nạp đĩa) và
/// client (tính giá trị kỳ vọng), nên dữ liệu đúng/sai là phép so thật, không phải niềm tin.
pub const fn byte_at(file: u32, page: u32, offset: usize) -> u8 {
    let word = file.wrapping_mul(31) ^ page.wrapping_mul(17) ^ (offset as u32).wrapping_mul(7);
    let mixed = word ^ ((offset as u32) >> 8);
    (mixed & 0xff) as u8
}
