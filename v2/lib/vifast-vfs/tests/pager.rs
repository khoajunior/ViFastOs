// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-70 — test host của **pager + page cache tầng 1**: hit/miss, LRU, backend RAM, stats.
//!
//! Viết **trước** phần cài (đỏ-trước D2-05): log `JOURNAL/kt70-red-host.log`.

use vifast_fs::{BlockDev, FsError, BLOCK_SIZE};
use vifast_vfs::{
    byte_at, file_pages, page_sector, DevRef, PageSink, PagerCore, RamDev, Stat, StatBlock,
    CACHE_CAPACITY, FILE0_PAGES, FILE1_PAGES, PAGE_SIZE, SECTORS_PER_PAGE,
};

/// Thiết bị RAM **một phiên bản** (mỗi test một `RamDev` mới) — không `unsafe`.
fn disk() -> RamDev {
    let dev = RamDev::new();
    dev.seed();
    dev
}

/// Sink ghi trang vào `Vec<u8>` (host) — U-mode cài bằng copy vào Frame.
#[derive(Default)]
struct VecSink {
    pages: [Vec<u8>; CACHE_CAPACITY],
}

impl PageSink for VecSink {
    fn write_sector(&mut self, slot: u8, sector_in_page: u32, data: &[u8; BLOCK_SIZE]) {
        let page = &mut self.pages[slot as usize];
        let off = sector_in_page as usize * BLOCK_SIZE;
        if page.len() < PAGE_SIZE {
            page.resize(PAGE_SIZE, 0);
        }
        page[off..off + BLOCK_SIZE].copy_from_slice(data);
    }
}

fn core() -> PagerCore<CACHE_CAPACITY, DevRef<'static>> {
    // `RamDev` có interior mutability (AtomicU64) nên `DevRef` chỉ cần `&'static`.
    static DEV: RamDev = RamDev::new();
    DEV.seed();
    PagerCore::new(DevRef(&DEV))
}

#[test]
fn hinh_hoc_trang_va_file_tu_nhat_quan() {
    // Trang = 8 sector 512 B; file 0 có 3 trang, file 1 có 2 trang (tổng 40 sector = 5 Frame).
    assert_eq!(PAGE_SIZE, 4096);
    assert_eq!(SECTORS_PER_PAGE, 8);
    assert_eq!(file_pages(0), Some(FILE0_PAGES));
    assert_eq!(file_pages(1), Some(FILE1_PAGES));
    assert_eq!(file_pages(2), None);
    assert_eq!(page_sector(0, 0), Some(0));
    assert_eq!(page_sector(0, 2), Some(16));
    assert_eq!(page_sector(1, 0), Some(24));
    assert_eq!(page_sector(0, 3), None);
    assert_eq!(page_sector(1, 2), None);
    // Mẫu dữ liệu phân biệt theo (file, page, offset) — cùng hàm cho cả pager lẫn client.
    assert_eq!(byte_at(0, 0, 0), 0);
    assert_ne!(byte_at(0, 1, 0), byte_at(1, 0, 0));
    assert_ne!(byte_at(0, 0, 1), byte_at(0, 1, 0));
}

#[test]
fn backend_ram_seed_va_doc_tung_sector() {
    let dev = disk();
    let mut sector = [0u8; BLOCK_SIZE];
    // File 0 trang 2 (sector 16..24), byte đầu sector 2 = byte_at(0,2,1024).
    BlockDev::read(&mut DevRef(&dev), 18, &mut sector).expect("đọc sector");
    assert_eq!(sector[0], byte_at(0, 2, 2 * BLOCK_SIZE));
    // File 1 trang 1 → sector 32..40 (sau 24 sector của file 0 + 8).
    BlockDev::read(&mut DevRef(&dev), 32, &mut sector).expect("đọc sector");
    assert_eq!(sector[7], byte_at(1, 1, 7));
}

#[test]
fn backend_ram_ghi_doc_lai_va_ngoai_dai_loi() {
    let dev = disk();
    let mut out = [0u8; BLOCK_SIZE];
    let mut data = [0u8; BLOCK_SIZE];
    data[5] = 0x5a;
    let mut writer = DevRef(&dev);
    writer.write(3, &data).expect("ghi sector");
    writer.read(3, &mut out).expect("đọc lại");
    assert_eq!(out[5], 0x5a);
    assert_eq!(writer.read(40, &mut out), Err(FsError::Invalid));
    assert_eq!(writer.write(40, &data), Err(FsError::Invalid));
    assert_eq!(DevRef::block_count(&writer), 40);
}

#[test]
fn backend_ram_byte_moi_offset_khop_mau() {
    let dev = disk();
    let mut sector = [0u8; BLOCK_SIZE];
    let mut r = DevRef(&dev);
    for page in 0..FILE1_PAGES {
        let lba = page_sector(1, page).expect("sector");
        for s in 0..SECTORS_PER_PAGE {
            BlockDev::read(&mut r, lba + s, &mut sector).expect("đọc");
            for (i, byte) in sector.iter().enumerate() {
                assert_eq!(
                    *byte,
                    byte_at(1, page, s as usize * BLOCK_SIZE + i),
                    "file=1 page={page} sector={s} byte={i}"
                );
            }
        }
    }
}

#[test]
fn hit_khong_doc_backend() {
    let mut core = core();
    let mut sink = VecSink::default();
    let first = core.begin(0, 2);
    assert!(
        matches!(first, vifast_vfs::Begin::Ready { .. }),
        "lần đầu phải miss"
    );
    if let vifast_vfs::Begin::Ready { slot } = first {
        core.load(slot, 0, 2, &mut sink).expect("nạp");
    }
    let reads_trước = core.stats().reads;
    let sectors_trước = core.stats().sectors;
    match core.begin(0, 2) {
        vifast_vfs::Begin::Hit { slot } => assert_eq!(slot, 0, "trang duy nhất nằm slot 0"),
        other => panic!("phải hit, nhận {other:?}"),
    }
    assert_eq!(core.stats().reads, reads_trước, "hit KHÔNG đọc backend");
    assert_eq!(core.stats().sectors, sectors_trước, "hit KHÔNG đọc sector");
    assert_eq!(core.stats().hits, 1);
    assert_eq!(core.stats().misses, 1);
}

#[test]
fn miss_evict_lru_khi_vuot_nguong() {
    let mut core = core();
    let mut sink = VecSink::default();
    let keys = [(0u32, 0u32), (0, 1), (0, 2)];
    for (i, (file, page)) in keys.iter().enumerate() {
        match core.begin(*file, *page) {
            vifast_vfs::Begin::Ready { slot } => {
                assert_eq!(slot, i as u8, "slot cấp theo thứ tự");
                core.load(slot, *file, *page, &mut sink).expect("nạp");
            }
            other => panic!("lần đầu phải Ready, nhận {other:?}"),
        }
    }
    // Chạm lại trang 1 để nó thành mới nhất (LRU thật, không phải FIFO).
    assert!(matches!(core.begin(0, 1), vifast_vfs::Begin::Hit { .. }));
    // Trang thứ 4 vượt ngưỡng: phải trục xuất trang 0 (cũ nhất), không phải trang 1.
    match core.begin(1, 0) {
        vifast_vfs::Begin::Evict { slot, key, .. } => {
            assert_eq!(key, (0, 0), "LRU: trang 0 cũ nhất");
            assert_eq!(slot, 0);
            core.load(slot, 1, 0, &mut sink).expect("nạp sau trục xuất");
        }
        other => panic!("phải Evict, nhận {other:?}"),
    }
    assert_eq!(core.stats().evictions, 1);
    assert_eq!(core.stats().misses, 4);
    assert_eq!(core.stats().reads, 4);
    assert_eq!(core.stats().sectors, u64::from(4 * SECTORS_PER_PAGE));
    // Trang bị trục xuất quay lại ⇒ miss lại và nạp đúng dữ liệu (không mất dữ liệu).
    match core.begin(0, 0) {
        vifast_vfs::Begin::Evict { key, .. } => assert_eq!(key, (0, 2), "cũ nhất còn lại"),
        other => panic!("phải Evict, nhận {other:?}"),
    }
}

#[test]
fn du_lieu_nap_dung_mau_backend() {
    let mut core = core();
    let mut sink = VecSink::default();
    for (file, page) in [(0u32, 2u32), (1, 1)] {
        let slot = match core.begin(file, page) {
            vifast_vfs::Begin::Ready { slot } => slot,
            other => panic!("phải Ready, nhận {other:?}"),
        };
        core.load(slot, file, page, &mut sink).expect("nạp");
        let page_data = &sink.pages[slot as usize];
        assert_eq!(page_data.len(), PAGE_SIZE);
        for (off, byte) in page_data.iter().enumerate() {
            assert_eq!(
                *byte,
                byte_at(file, page, off),
                "file={file} page={page} off={off}"
            );
        }
    }
}

#[test]
fn ngoai_file_tra_ma_va_khong_doc_backend() {
    let mut core = core();
    let mut sink = VecSink::default();
    let reads = core.stats().reads;
    match core.begin(0, 99) {
        vifast_vfs::Begin::Reject { status } => {
            assert_eq!(status.id(), vifast_vfs::err::INVALID_ARGUMENT.id());
            assert_eq!(status.name(), "InvalidArgument");
        }
        other => panic!("phải Reject, nhận {other:?}"),
    }
    assert!(matches!(core.begin(2, 0), vifast_vfs::Begin::Reject { .. }));
    assert_eq!(
        core.stats().reads,
        reads,
        "trang ngoài file KHÔNG đọc backend"
    );
    assert_eq!(core.stats().rejects, 2, "đếm đủ hai ca từ chối");
    // Cache không bị bẩn bởi lần từ chối.
    assert_eq!(core.cache_entries(), 0);
    let _ = &mut sink;
}

#[test]
fn mot_ban_trong_cache_cho_moi_khoa() {
    let mut core = core();
    let mut sink = VecSink::default();
    let mut slots = [0u8; 3];
    for (i, (file, page)) in [(0u32, 1u32), (0, 2), (1, 0)].iter().enumerate() {
        let slot = match core.begin(*file, *page) {
            vifast_vfs::Begin::Ready { slot } => slot,
            other => panic!("phải Ready, nhận {other:?}"),
        };
        core.load(slot, *file, *page, &mut sink).expect("nạp");
        slots[i] = slot;
    }
    // Cùng khoá lần hai ⇒ hit đúng slot cũ (một bản, nhiều mapping do caller quyết định).
    match core.begin(0, 2) {
        vifast_vfs::Begin::Hit { slot } => assert_eq!(slot, slots[1], "hit đúng slot của khoá"),
        other => panic!("phải Hit, nhận {other:?}"),
    }
    assert_eq!(core.cache_entries(), 3);
    assert!(core.keys_unique(), "không có hai bản cho cùng (file,page)");
}

#[test]
fn stats_block_ghi_doc_qua_bang_tu() {
    let mut block = StatBlock::new();
    block.set(Stat::Magic, vifast_vfs::STATS_MAGIC);
    block.set(Stat::Hits, 7);
    block.set(Stat::Misses, 3);
    assert_eq!(block.get(Stat::Magic), vifast_vfs::STATS_MAGIC);
    assert_eq!(block.get(Stat::Hits), 7);
    assert_eq!(block.get(Stat::Misses), 3);
    // Mọi ô nằm trong một trang 4 KiB và không chồng nhau.
    let mut seen = [false; PAGE_SIZE / 8];
    for stat in Stat::ALL {
        let off = stat.offset();
        assert!(off + 8 <= PAGE_SIZE, "stat {stat:?} ngoài trang");
        assert!(!seen[off / 8], "stat {stat:?} trùng ô");
        seen[off / 8] = true;
    }
    assert_eq!(Stat::ALL.len(), Stat::COUNT);
}

#[test]
fn stats_phan_anh_bo_dem_do_duoc() {
    let mut core = core();
    let mut sink = VecSink::default();
    for (file, page) in [(0u32, 0u32), (0, 1), (0, 0), (1, 0)].iter() {
        match core.begin(*file, *page) {
            vifast_vfs::Begin::Ready { slot } => {
                core.load(slot, *file, *page, &mut sink).expect("nạp")
            }
            vifast_vfs::Begin::Hit { .. } => {}
            other => panic!("không mong {other:?}"),
        }
    }
    let stats = core.stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 3);
    assert_eq!(stats.reads, 3);
    assert_eq!(stats.sectors, u64::from(3 * SECTORS_PER_PAGE));
    assert_eq!(stats.evictions, 0);
    assert_eq!(stats.rejects, 0);
    // `reads == misses` là **hệ quả** của bất biến (mỗi miss một lần đọc trang) — D2-106.
    assert_eq!(stats.reads, stats.misses);
}

#[test]
fn ram_dev_khong_unsafe_va_kich_thuoc_thiet_ke() {
    // Bất biến thiết kế: cache 3 trang (ngưỡng cố định, không nới), đĩa 40 sector (5 Frame).
    assert_eq!(CACHE_CAPACITY, 3);
    assert_eq!(vifast_vfs::DISK_SECTORS, 40);
    assert_eq!(
        FILE0_PAGES * SECTORS_PER_PAGE + FILE1_PAGES * SECTORS_PER_PAGE,
        40
    );
}
