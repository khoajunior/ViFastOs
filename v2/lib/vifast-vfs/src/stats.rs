// SPDX-License-Identifier: MIT OR Apache-2.0
//! Khối **stats** trong một Frame chia sẻ giữa pager (U-mode) và kernel (KT-70).
//!
//! Pager ghi bằng `poke_u64(VA_STATS + stat.offset(), …)`; kernel đọc qua **physmap**
//! (`read_phys_u64(pa + …)`) — cùng bố cục, không tin lời chương trình (D2-102).
//! Magic/version/capacity là hằng **thiết kế** nên in kèm `const:` trong bằng chứng.

use crate::layout::{CACHE_CAPACITY, PAGE_SIZE, SECTOR_SIZE};

/// Magic của khối stats (`"KT70PAGE"` little-endian) — bất biến: kernel từ chối đọc nếu sai.
pub const STATS_MAGIC: u64 = 0x4b54_3730_5041_4745;
/// Phiên bản bố cục stats.
pub const STATS_VERSION: u64 = 1;
/// Offset của khối stats trong Frame (byte 0..0x80 để trống cho magic/header tương lai).
pub const STATS_OFFSET: usize = 0x80;

/// Một bộ đếm trong khối stats.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stat {
    /// Magic [`STATS_MAGIC`].
    Magic = 0,
    /// Phiên bản bố cục.
    Version,
    /// Ngưỡng cache (`const:capacity`).
    Capacity,
    /// Cỡ trang.
    PageSize,
    /// Cỡ sector.
    SectorSize,
    /// Số fault đã nhận từ kernel.
    Faults,
    /// Số hit cache.
    Hits,
    /// Số miss cache.
    Misses,
    /// Số lần trục xuất LRU.
    Evictions,
    /// Số trang đọc backend.
    Reads,
    /// Số sector đọc backend.
    Sectors,
    /// Số fault đã phục vụ thành công.
    Served,
    /// Số fault bị từ chối.
    Rejected,
    /// Số lần Frame.Map thành công.
    Maps,
    /// Số lần Frame.Unmap thành công.
    Unmaps,
    /// Số khoá phân biệt trong cache (một bản mỗi khoá).
    UniqueKeys,
    /// Số mục đang cache.
    Entries,
    /// Client của fault bị từ chối gần nhất.
    LastRejectClient,
    /// Mã (id) của fault bị từ chối gần nhất.
    LastRejectStatus,
    /// Số client đã dùng pager.
    Clients,
    /// Số fault trả lời bằng một trang đã có trong cache của **client khác**.
    SharedHits,
    /// Số lần nạp lại một trang từng bị trục xuất.
    Reloads,
    /// KT-70 TAP 35: mã lỗi của `Frame.Map` với aux **không phải VSpace** (ca âm quyền).
    NegAux,
    /// KT-70 TAP 35: mã lỗi của `Frame.Map` vào VA **đã map** (ca âm không ghi đè).
    NegOver,
}

impl Stat {
    /// Số bộ đếm.
    pub const COUNT: usize = 24;
    /// Toàn bộ bộ đếm (để test bố cục + in).
    pub const ALL: [Stat; Self::COUNT] = [
        Stat::Magic,
        Stat::Version,
        Stat::Capacity,
        Stat::PageSize,
        Stat::SectorSize,
        Stat::Faults,
        Stat::Hits,
        Stat::Misses,
        Stat::Evictions,
        Stat::Reads,
        Stat::Sectors,
        Stat::Served,
        Stat::Rejected,
        Stat::Maps,
        Stat::Unmaps,
        Stat::UniqueKeys,
        Stat::Entries,
        Stat::LastRejectClient,
        Stat::LastRejectStatus,
        Stat::Clients,
        Stat::SharedHits,
        Stat::Reloads,
        Stat::NegAux,
        Stat::NegOver,
    ];

    /// Offset byte trong Frame.
    pub const fn offset(self) -> usize {
        STATS_OFFSET + self as usize * 8
    }

    /// Bộ đếm theo chỉ số (dùng khi duyệt).
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Khối stats phía host (test bố cục/đọc-ghi); trên QEMU hai bên dùng [`Stat::offset`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatBlock {
    words: [u64; Stat::COUNT],
}

impl StatBlock {
    /// Khối 0.
    pub const fn new() -> Self {
        Self {
            words: [0u64; Stat::COUNT],
        }
    }

    /// Ghi một bộ đếm.
    pub fn set(&mut self, stat: Stat, value: u64) {
        self.words[stat.index()] = value;
    }

    /// Đọc một bộ đếm.
    pub fn get(&self, stat: Stat) -> u64 {
        self.words[stat.index()]
    }

    /// Nạp magic/version/hình học thiết kế — pager gọi lúc khởi động.
    pub fn init_design(&mut self) {
        self.set(Stat::Magic, STATS_MAGIC);
        self.set(Stat::Version, STATS_VERSION);
        self.set(Stat::Capacity, CACHE_CAPACITY as u64);
        self.set(Stat::PageSize, PAGE_SIZE as u64);
        self.set(Stat::SectorSize, SECTOR_SIZE as u64);
    }
}

impl Default for StatBlock {
    fn default() -> Self {
        Self::new()
    }
}
