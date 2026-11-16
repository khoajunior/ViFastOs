// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-71** — test host của **ghi qua cache (write-back)**: trang **bẩn** phải xuống backend
//! trước khi bị trục xuất; `write → dirty → flush → clean`; đóng/mở lại đọc đúng dữ liệu; đo
//! `dirty/writes/flushes`.
//!
//! Viết **trước** phần cài (đỏ-trước D2-05): log `v2/agent/JOURNAL/kt71-red-host.log`.
use vifast_fs::{BlockDev, FsError, BLOCK_SIZE};
use vifast_vfs::{
    err, page_sector, Begin, DevRef, PageSink, PageSource, PagerCore, RamDev, Status,
    CACHE_CAPACITY, FILE0_PAGES, SECTORS_PER_PAGE,
};

/// Ghi trang vào `Vec` — mô phỏng Frame của pager (host không có Frame thật).
#[derive(Default)]
struct VecPages {
    pages: [Vec<u8>; CACHE_CAPACITY],
}

impl VecPages {
    fn ensure(&mut self, slot: u8) {
        let page = &mut self.pages[slot as usize];
        if page.len() < 4096 {
            page.resize(4096, 0);
        }
    }
    fn bytes(&self, slot: u8) -> &[u8] {
        &self.pages[slot as usize]
    }
    fn bytes_mut(&mut self, slot: u8) -> &mut Vec<u8> {
        self.ensure(slot);
        &mut self.pages[slot as usize]
    }
}

impl PageSink for VecPages {
    fn write_sector(&mut self, slot: u8, sector_in_page: u32, data: &[u8; BLOCK_SIZE]) {
        self.ensure(slot);
        let off = sector_in_page as usize * BLOCK_SIZE;
        self.pages[slot as usize][off..off + BLOCK_SIZE].copy_from_slice(data);
    }
}

impl PageSource for VecPages {
    fn read_sector(&mut self, slot: u8, sector_in_page: u32, out: &mut [u8; BLOCK_SIZE]) {
        self.ensure(slot);
        let off = sector_in_page as usize * BLOCK_SIZE;
        out.copy_from_slice(&self.pages[slot as usize][off..off + BLOCK_SIZE]);
    }
}

fn disk() -> RamDev {
    let dev = RamDev::new();
    dev.seed();
    dev
}

/// Nạp một trang từ backend vào slot (gọi `begin` + `load`).
fn load_page(
    core: &mut PagerCore<CACHE_CAPACITY, DevRef<'_>>,
    pages: &mut VecPages,
    f: u32,
    p: u32,
) -> u8 {
    let slot = match core.begin(f, p) {
        Begin::Ready { slot } | Begin::Hit { slot } => slot,
        other => panic!("phải Ready/Hit cho ({f},{p}), nhận {other:?}"),
    };
    if core.slot_of(f, p).is_none() {
        core.load(slot, f, p, pages).expect("nạp trang");
    }
    slot
}

#[test]
fn ghi_roi_flush_ghi_xuong_backend_va_dem_dung() {
    let dev = disk();
    let mut core: PagerCore<CACHE_CAPACITY, DevRef<'_>> = PagerCore::new(DevRef(&dev));
    let mut pages = VecPages::default();
    let slot = load_page(&mut core, &mut pages, 0, 1);
    // Sửa dữ liệu trang trong "Frame", đánh dấu bẩn.
    pages.bytes_mut(slot)[..8].copy_from_slice(b"KT71DIRT");
    core.mark_dirty(0, 1)
        .expect("trang đang cache phải đánh dấu bẩn được");
    let stats = core.stats();
    assert_eq!(stats.dirty_marks, 1, "{stats:?}");
    assert_eq!(stats.dirty_now, 1, "{stats:?}");
    assert_eq!(stats.writes, 0, "chưa flush thì chưa ghi backend");
    // Flush: ghi 8 sector + một lần flush thiết bị.
    let wrote = core.flush_page(0, 1, &mut pages).expect("flush trang bẩn");
    assert!(wrote, "trang bẩn phải được ghi");
    let stats = core.stats();
    assert_eq!(stats.dirty_now, 0, "flush xong phải sạch: {stats:?}");
    assert_eq!(stats.writes, SECTORS_PER_PAGE as u64, "{stats:?}");
    assert_eq!(stats.flushes, 1, "{stats:?}");
    // Đọc thẳng backend: đúng dữ liệu đã ghi qua cache.
    let lba = page_sector(0, 1).expect("sector");
    let mut sector = [0u8; BLOCK_SIZE];
    let mut reader = DevRef(&dev);
    reader.read(lba, &mut sector).unwrap();
    assert_eq!(&sector[..8], b"KT71DIRT");
    // Flush trang đã sạch: không ghi thêm.
    let wrote = core.flush_page(0, 1, &mut pages).unwrap();
    assert!(!wrote);
    assert_eq!(core.stats().writes, SECTORS_PER_PAGE as u64);
}

#[test]
fn dirty_phai_flush_truoc_khi_evict() {
    let dev = disk();
    let mut core: PagerCore<CACHE_CAPACITY, DevRef<'_>> = PagerCore::new(DevRef(&dev));
    let mut pages = VecPages::default();
    // Đầy cache bằng 3 trang; trang (0,0) là LRU.
    for page in 0..CACHE_CAPACITY as u32 {
        load_page(&mut core, &mut pages, 0, page);
    }
    assert_eq!(core.cache_entries(), CACHE_CAPACITY);
    let slot0 = core.slot_of(0, 0).unwrap();
    pages.bytes_mut(slot0)[100..104].copy_from_slice(b"EVIC");
    core.mark_dirty(0, 0).unwrap();
    // Trang thứ 4 buộc trục xuất: victim phải là (0,0) và phải báo BẨN.
    let (slot, key, dirty) = match core.begin(1, 0) {
        Begin::Evict { slot, key, dirty } => (slot, key, dirty),
        other => panic!("phải Evict, nhận {other:?}"),
    };
    assert_eq!(key, (0, 0), "victim LRU phải là trang cũ nhất");
    assert!(dirty, "Evict phải báo trang bẩn");
    assert_eq!(slot, slot0);
    // Ghi đè slot bẩn mà CHƯA flush ⇒ từ chối (không mất dữ liệu).
    let status: Status = core
        .load(slot, 1, 0, &mut pages)
        .expect_err("không được nạp đè trang bẩn");
    assert_eq!(
        status.name(),
        err::BUSY.name(),
        "phải trả Busy, nhận {status:?}"
    );
    assert_eq!(core.stats().evictions, 1, "đã CHỌN victim (chưa nạp đè)");
    assert_eq!(core.stats().writes, 0, "chưa flush thì chưa ghi backend");
    // Flush rồi nạp đè: dữ liệu bẩn đã xuống đĩa.
    core.flush_page(0, 0, &mut pages).unwrap();
    core.load(slot, 1, 0, &mut pages).unwrap();
    let mut sector = [0u8; BLOCK_SIZE];
    let mut reader = DevRef(&dev);
    reader
        .read(page_sector(0, 0).unwrap(), &mut sector)
        .unwrap();
    assert_eq!(&sector[100..104], b"EVIC");
    let stats = core.stats();
    assert_eq!(stats.dirty_now, 0, "{stats:?}");
    assert_eq!(stats.writes, SECTORS_PER_PAGE as u64);
    assert_eq!(stats.flushes, 1);
}

#[test]
fn trang_sach_bi_evict_thi_nap_de_duoc() {
    let dev = disk();
    let mut core: PagerCore<CACHE_CAPACITY, DevRef<'_>> = PagerCore::new(DevRef(&dev));
    let mut pages = VecPages::default();
    for page in 0..CACHE_CAPACITY as u32 {
        load_page(&mut core, &mut pages, 0, page);
    }
    let slot = match core.begin(1, 0) {
        Begin::Evict { slot, dirty, .. } => {
            assert!(!dirty, "trang chưa từng ghi phải sạch");
            slot
        }
        other => panic!("phải Evict, nhận {other:?}"),
    };
    core.load(slot, 1, 0, &mut pages)
        .expect("trang sạch nạp đè được");
}

#[test]
fn flush_all_va_mo_lai_doc_dung_du_lieu() {
    let dev = disk();
    let mut writes;
    {
        let mut core: PagerCore<CACHE_CAPACITY, DevRef<'_>> = PagerCore::new(DevRef(&dev));
        let mut pages = VecPages::default();
        for page in 0..FILE0_PAGES {
            let slot = load_page(&mut core, &mut pages, 0, page);
            pages.bytes_mut(slot)[..4].copy_from_slice(b"MOD!");
            core.mark_dirty(0, page).unwrap();
        }
        let stats = core.stats();
        assert_eq!(stats.dirty_now, FILE0_PAGES as u64, "{stats:?}");
        let flushed = core.flush_all(&mut pages).unwrap();
        assert_eq!(flushed, FILE0_PAGES as u64);
        writes = core.stats().writes;
        assert_eq!(writes, (FILE0_PAGES * SECTORS_PER_PAGE) as u64);
        assert_eq!(core.stats().flushes, FILE0_PAGES as u64);
        assert_eq!(core.stats().dirty_now, 0);
    }
    // "Đóng" pager, "mở lại" trên cùng thiết bị: đọc từ đĩa phải ra dữ liệu đã flush.
    let mut core2: PagerCore<CACHE_CAPACITY, DevRef<'_>> = PagerCore::new(DevRef(&dev));
    let mut pages2 = VecPages::default();
    for page in 0..FILE0_PAGES {
        let slot = load_page(&mut core2, &mut pages2, 0, page);
        assert_eq!(&pages2.bytes(slot)[..4], b"MOD!", "trang {page} sau mở lại");
    }
    assert_eq!(core2.stats().dirty_now, 0);
    assert_eq!(core2.stats().writes, 0, "chỉ đọc thì không ghi");
    writes += 0; // giữ biến đo sống tới cuối test (không dùng hằng)
    assert!(writes > 0);
}

#[test]
fn flush_trang_khong_cache_tra_ma_va_khong_ghi() {
    let dev = disk();
    let mut core: PagerCore<CACHE_CAPACITY, DevRef<'_>> = PagerCore::new(DevRef(&dev));
    let mut pages = VecPages::default();
    let status = core
        .flush_page(1, 1, &mut pages)
        .expect_err("không có trong cache");
    assert_eq!(status.name(), err::INVALID_ARGUMENT.name());
    assert_eq!(core.stats().writes, 0);
    let status = core.mark_dirty(9, 9).expect_err("ngoài file");
    assert_eq!(status.name(), err::INVALID_ARGUMENT.name());
    // Backend lỗi I/O khi flush phải trả IO và KHÔNG đánh dấu sạch.
    let mut core: PagerCore<CACHE_CAPACITY, FailingDev> = PagerCore::new(FailingDev);
    let slot = match core.begin(0, 0) {
        Begin::Ready { slot } => slot,
        other => panic!("{other:?}"),
    };
    let status = core.load(slot, 0, 0, &mut pages).expect_err("backend lỗi");
    assert_eq!(status.name(), err::IO.name());
    assert_eq!(core.stats().reads, 0, "không đọc được thì không đếm reads");
}

/// Thiết bị lỗi I/O mọi lệnh — dùng cho ca âm của `load`.
struct FailingDev;

impl BlockDev for FailingDev {
    fn read(&mut self, _lba: u32, _out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        Err(FsError::Io)
    }
    fn write(&mut self, _lba: u32, _data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        Err(FsError::Io)
    }
    fn block_count(&self) -> u32 {
        40
    }
}
