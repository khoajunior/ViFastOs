// SPDX-License-Identifier: MIT OR Apache-2.0
//! `vifast-vfs` — **VFS tầng 1** của K6 (KT-70): pager + page cache.
//!
//! Phạm vi: **chưa** journal (KT-71), **chưa** virtio-blk (KT-72). Ba mảnh:
//!
//! 1. [`layout`] — hình học thiết kế: trang 4 KiB, sector 512 B (`vifast_fs::BLOCK_SIZE`), hai
//!    file mẫu **3 + 2 trang**, **ngưỡng cache 3**, đĩa **5 Frame 4 KiB = 40 sector** (hằng
//!    thiết kế, không nới — xem `docs/k6-vfs-pager.md` §6).
//! 2. [`disk`] — backend RAM cài **lại** `vifast_fs::BlockDev` (đọc/ghi sector). KT-72 chỉ thêm
//!    một impl mới (virtio-blk), không sửa pager.
//! 3. [`core::PagerCore`] — tra cache `(file_id, page_no)`, hit không đọc backend, miss nạp từ
//!    backend, vượt ngưỡng ⇒ trục xuất **LRU**; đếm `hits/misses/evictions/reads/sectors`.
//!
//! `#![forbid(unsafe_code)]` — mọi truy cập bộ nhớ của pager U-mode đi qua shim
//! `vifast-plat-user` (D2-17/D2-101); crate này thuần logic nên test host phủ hết.
#![no_std]
#![forbid(unsafe_code)]

pub mod cache;
pub mod core;
pub mod disk;
pub mod layout;
pub mod stats;
pub mod status;

pub use cache::{Key, PageCache, SlotChoice, NO_SLOT};
pub use core::{Begin, PageSink, PagerCore, PagerStats};
pub use disk::{DevRef, RamDev, DISK_BYTES};
pub use layout::{
    byte_at, file_pages, page_sector, sector_page, CACHE_CAPACITY, DISK_SECTORS, FILE0_PAGES,
    FILE1_PAGES, FILE_COUNT, PAGE_SIZE, SECTORS_PER_PAGE, SECTOR_SIZE,
};
pub use stats::{Stat, StatBlock, STATS_MAGIC, STATS_OFFSET, STATS_VERSION};
pub use status::{err, Status};
pub use vifast_fs::{BlockDev, FsError};
