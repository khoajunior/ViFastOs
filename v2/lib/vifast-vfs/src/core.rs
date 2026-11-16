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

/// Nơi **đọc** dữ liệu trang để ghi xuống backend — đối xứng với [`PageSink`] (KT-71).
pub trait PageSource {
    /// Đọc một sector (512 B) của trang `slot`.
    fn read_sector(&mut self, slot: u8, sector_in_page: u32, out: &mut [u8; BLOCK_SIZE]);
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
        /// Mục bị trục xuất đang **bẩn** ⇒ phải [`PagerCore::flush_page`] trước khi nạp đè.
        dirty: bool,
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
    /// Số lần đánh dấu bẩn (tổng, không giảm).
    pub dirty_marks: u64,
    /// Số trang đang bẩn (thời điểm hiện tại).
    pub dirty_now: u64,
    /// Số sector đã ghi xuống backend qua đường cache.
    pub writes: u64,
    /// Số lần gọi `flush` thiết bị do flush trang bẩn.
    pub flushes: u64,
    /// Số lần flush trang bẩn thành công.
    pub flushed_pages: u64,
}

/// Lõi pager: cache `N` trang + backend `D`.
pub struct PagerCore<const N: usize, D: BlockDev> {
    dev: D,
    cache: PageCache<N>,
    reads: u64,
    sectors: u64,
    rejects: u64,
    dirty_marks: u64,
    writes: u64,
    flushes: u64,
    flushed_pages: u64,
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
            dirty_marks: 0,
            writes: 0,
            flushes: 0,
            flushed_pages: 0,
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
            SlotChoice::Evict { slot, key, dirty } => Begin::Evict { slot, key, dirty },
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
        // KT-71: KHÔNG được nạp đè một trang bẩn (dữ liệu trong Frame chưa xuống đĩa).
        if self.cache.slot_dirty(slot) {
            return Err(err::BUSY);
        }
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

    /// Đánh dấu trang `(file,page)` **bẩn** — gọi sau khi chương trình đã ghi vào Frame.
    ///
    /// `Busy` khi trang không có trong cache: không có Frame thì không thể "bẩn".
    pub fn mark_dirty(&mut self, file: u32, page: u32) -> Result<(), Status> {
        if crate::layout::file_pages(file).map(|pages| page >= pages) != Some(false)
            || !self.cache.mark_dirty((file, page))
        {
            return Err(err::INVALID_ARGUMENT);
        }
        self.dirty_marks += 1;
        Ok(())
    }

    /// Trang `(file,page)` có bẩn không.
    pub fn is_dirty(&self, file: u32, page: u32) -> bool {
        self.cache.is_dirty((file, page))
    }

    /// Ghi **một trang bẩn** xuống backend rồi đánh dấu sạch.
    ///
    /// Trả `Ok(true)` nếu đã ghi; `Ok(false)` nếu trang đang sạch; `InvalidArgument` nếu khoá
    /// không có trong cache. Ghi đủ `SECTORS_PER_PAGE` sector rồi **flush thiết bị** — bằng
    /// chứng "dirty → flush → clean" đo được bằng `writes`/`flushes`.
    pub fn flush_page(
        &mut self,
        file: u32,
        page: u32,
        source: &mut impl PageSource,
    ) -> Result<bool, Status> {
        let Some(slot) = self.cache.slot_of((file, page)) else {
            return Err(err::INVALID_ARGUMENT);
        };
        if !self.cache.is_dirty((file, page)) {
            return Ok(false);
        }
        let Some(first) = page_sector(file, page) else {
            return Err(err::INVALID_ARGUMENT);
        };
        let mut sector = [0u8; BLOCK_SIZE];
        for index in 0..SECTORS_PER_PAGE {
            source.read_sector(slot, index, &mut sector);
            self.dev
                .write(first + index, &sector)
                .map_err(|_| err::IO)?;
        }
        self.dev.flush().map_err(|_| err::IO)?;
        self.cache.clear_dirty((file, page));
        self.writes += u64::from(SECTORS_PER_PAGE);
        self.flushes += 1;
        self.flushed_pages += 1;
        Ok(true)
    }

    /// Ghi **mọi** trang bẩn xuống backend; trả số trang đã ghi.
    pub fn flush_all(&mut self, source: &mut impl PageSource) -> Result<u64, Status> {
        let mut keys = [(0u32, 0u32); N];
        let count = self.cache.dirty_keys(&mut keys);
        let mut flushed = 0u64;
        for key in keys.iter().take(count) {
            if self.flush_page(key.0, key.1, source)? {
                flushed += 1;
            }
        }
        Ok(flushed)
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
            dirty_marks: self.dirty_marks,
            dirty_now: self.cache.dirty_count() as u64,
            writes: self.writes,
            flushes: self.flushes,
            flushed_pages: self.flushed_pages,
        }
    }
}
