// SPDX-License-Identifier: MIT OR Apache-2.0
//! `PagerCore` — **lõi pager** chạy được trên host (KT-70): tra cache, nạp từ [`BlockDev`],
//! trục xuất LRU, đếm `hits/misses/evictions/reads/sectors`.
//!
//! Tách khỏi chương trình U-mode để test host phủ **toàn bộ** luồng hit/miss/evict/reject; phần
//! map Frame vào VSpace client (chỉ U-mode làm được) nằm ở `pager_server_user.rs` và được đo
//! trên QEMU.

use vifast_fs::{BlockDev, BLOCK_SIZE};

use crate::cache::{Key, PageCache, SlotChoice};
use crate::layout::{page_sector, SECTORS_PER_PAGE};
use crate::status::{err, Status};

/// Nơi nhận dữ liệu trang vừa đọc từ backend — U-mode ghi vào Frame, host ghi vào `Vec`.
pub trait PageSink {
    /// Ghi một sector (512 B) của trang `slot`.
    fn write_sector(&mut self, slot: u8, sector_in_page: u32, data: &[u8; BLOCK_SIZE]);
}

/// Quyết định của [`PagerCore::begin`] cho một fault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Begin {
    /// Cache hit — không đọc backend; slot đã có dữ liệu đúng.
    Hit {
        /// Slot Frame.
        slot: u8,
    },
    /// Miss, slot trống — caller nạp dữ liệu vào slot.
    Ready {
        /// Slot Frame.
        slot: u8,
    },
    /// Miss, phải trục xuất mục LRU — caller **gỡ mapping cũ** rồi nạp đè slot.
    Evict {
        /// Slot Frame được tái dùng.
        slot: u8,
        /// Khoá bị trục xuất.
        key: Key,
    },
    /// Từ chối (ngoài file / tham số sai) — **không** đọc backend, không đụng cache.
    Reject {
        /// Mã trả về (theo tên trong spec).
        status: Status,
    },
}

/// Bộ đếm của pager — mọi số lấy từ biến đo, không phải hằng kỳ vọng (D2-102).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PagerStats {
    /// Số lần hit cache.
    pub hits: u64,
    /// Số lần miss cache.
    pub misses: u64,
    /// Số lần trục xuất LRU.
    pub evictions: u64,
    /// Số trang đọc từ backend (= misses hợp lệ).
    pub reads: u64,
    /// Số sector đọc từ backend (= reads × SECTORS_PER_PAGE).
    pub sectors: u64,
    /// Số fault bị từ chối.
    pub rejects: u64,
    /// Số mục đang trong cache.
    pub entries: usize,
}

/// Lõi pager: cache `N` trang + backend `D`.
pub struct PagerCore<const N: usize, D: BlockDev> {
    dev: D,
    cache: PageCache<N>,
    reads: u64,
    sectors: u64,
    rejects: u64,
}

impl<const N: usize, D: BlockDev> PagerCore<N, D> {
    /// Pager mới trên backend `dev`.
    pub const fn new(dev: D) -> Self {
        Self {
            dev,
            cache: PageCache::new(),
            reads: 0,
            sectors: 0,
            rejects: 0,
        }
    }

    /// Bắt đầu xử lý fault `(file, page)`: validate → hit → chọn slot.
    pub fn begin(&mut self, file: u32, page: u32) -> Begin {
        if crate::layout::file_pages(file).map(|pages| page >= pages) != Some(false) {
            self.rejects += 1;
            return Begin::Reject {
                status: err::INVALID_ARGUMENT,
            };
        }
        if let Some(slot) = self.cache.hit((file, page)) {
            return Begin::Hit { slot };
        }
        match self.cache.begin_miss() {
            SlotChoice::Free { slot } => Begin::Ready { slot },
            SlotChoice::Evict { slot, key } => Begin::Evict { slot, key },
        }
    }

    /// Nạp trang `(file, page)` vào `slot` từ backend rồi cài cache.
    ///
    /// Caller **phải** gọi sau [`Begin::Ready`]/[`Begin::Evict`]; với `Evict`, gỡ mapping cũ
    /// trước khi gọi (nếu không client sẽ đọc dữ liệu mới qua mapping cũ).
    pub fn load(
        &mut self,
        slot: u8,
        file: u32,
        page: u32,
        sink: &mut impl PageSink,
    ) -> Result<(), Status> {
        let Some(first) = page_sector(file, page) else {
            self.rejects += 1;
            return Err(err::INVALID_ARGUMENT);
        };
        let mut sector = [0u8; BLOCK_SIZE];
        for index in 0..SECTORS_PER_PAGE {
            self.dev
                .read(first + index, &mut sector)
                .map_err(|_| err::IO)?;
            sink.write_sector(slot, index, &sector);
        }
        self.cache.install((file, page), slot);
        self.reads += 1;
        self.sectors += u64::from(SECTORS_PER_PAGE);
        Ok(())
    }

    /// Slot của một khoá nếu đang cache (không đổi LRU/đếm).
    pub fn slot_of(&self, file: u32, page: u32) -> Option<u8> {
        self.cache.slot_of((file, page))
    }

    /// Số mục đang cache.
    pub fn cache_entries(&self) -> usize {
        self.cache.len()
    }

    /// Mọi khoá trong cache không trùng.
    pub fn keys_unique(&self) -> bool {
        self.cache.keys_unique()
    }

    /// Ảnh chụp bộ đếm.
    pub fn stats(&self) -> PagerStats {
        PagerStats {
            hits: self.cache.hits(),
            misses: self.cache.misses(),
            evictions: self.cache.evictions(),
            reads: self.reads,
            sectors: self.sectors,
            rejects: self.rejects,
            entries: self.cache.len(),
        }
    }
}
