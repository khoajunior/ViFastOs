// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-71** — tầng 2 của VFS: `rename` đè, `truncate` giải phóng block, **double-indirect**,
//! **journal** (redo log) + replay sau "crash" (7 pha) và **fsck sửa** bốn loại hỏng.
//!
//! Viết **trước** phần cài (đỏ-trước D2-05): log `v2/agent/JOURNAL/kt71-red-host.log`.
use super::*;

use crate::crash::{run_op, PrefixDisk};
use crate::fs::get_u32;

fn payload(byte: u8, len: usize) -> Vec<u8> {
    vec![byte; len]
}

// ------------------------------------------------------------------ rename đè

/// POSIX: `rename` lên đích **đã tồn tại** phải thay thế đích (không trả `Exists`), giải phóng
/// inode/block của đích, và nội dung ở tên mới là của **nguồn**.
#[test]
fn rename_de_ghi_de_file_va_giai_phong_inode_dich() {
    let mut fs = fresh();
    let a = fs.create(b"/a", 0, INODE_FILE).unwrap();
    let b = fs.create(b"/b", 0, INODE_FILE).unwrap();
    fs.write(a, 0, b"AAAA", 0).unwrap();
    fs.write(b, 0, &payload(0xbb, BLOCK_SIZE * 2), 0).unwrap();
    let (used_before, _) = fs.usage();
    fs.rename(b"/a", b"/b", 0)
        .expect("rename đè phải thành công");
    assert_eq!(fs.lookup(b"/b", 0).unwrap(), a, "tên mới phải trỏ nguồn");
    assert!(fs.lookup(b"/a", 0).is_err(), "tên cũ phải biến mất");
    assert!(
        fs.info(b).is_err(),
        "inode đích phải được giải phóng sau khi bị đè"
    );
    let mut out = [0u8; 8];
    assert_eq!(fs.read(a, 0, &mut out, 0).unwrap(), 4);
    assert_eq!(&out[..4], b"AAAA");
    let (used_after, _) = fs.usage();
    assert!(
        used_after + 2 <= used_before,
        "block của đích phải được trả lại: trước={used_before} sau={used_after}"
    );
    fs.fsck().unwrap();
    let mut again = reopen(&fs);
    let mut out = [0u8; 8];
    assert_eq!(again.read(a, 0, &mut out, 0).unwrap(), 4);
    assert_eq!(&out[..4], b"AAAA");
    again.fsck().unwrap();
}

/// Đích là thư mục **không rỗng** ⇒ `NotEmpty`; file đè thư mục ⇒ `IsDir`; thư mục đè file ⇒
/// `NotDir`; sau ba ca âm, cả hai tên vẫn còn và `fsck` sạch (không nửa vời).
#[test]
fn rename_de_tu_choi_sai_loai_va_thu_muc_khong_rong() {
    let mut fs = fresh();
    let dir = fs.create(b"/d", 0, INODE_DIR).unwrap();
    let _child = fs.create(b"/d/x", 0, INODE_FILE).unwrap();
    let file = fs.create(b"/f", 0, INODE_FILE).unwrap();
    let empty = fs.create(b"/e", 0, INODE_DIR).unwrap();
    // File đè thư mục (kể cả không rỗng) ⇒ `IsDir` (POSIX EISDIR) — kiểm LOẠI trước.
    assert_eq!(fs.rename(b"/f", b"/d", 0), Err(FsError::IsDir));
    assert_eq!(fs.rename(b"/f", b"/e", 0), Err(FsError::IsDir));
    // Thư mục đè file ⇒ `NotDir`; thư mục đè thư mục KHÔNG rỗng ⇒ `NotEmpty`.
    assert_eq!(fs.rename(b"/e", b"/f", 0), Err(FsError::NotDir));
    assert_eq!(fs.rename(b"/e", b"/d", 0), Err(FsError::NotEmpty));
    assert_eq!(fs.lookup(b"/f", 0).unwrap(), file);
    assert_eq!(fs.lookup(b"/d", 0).unwrap(), dir);
    assert_eq!(fs.lookup(b"/e", 0).unwrap(), empty);
    let mut out = [0u8; 4];
    assert_eq!(fs.read(file, 0, &mut out, 0).unwrap(), 0);
    fs.fsck().unwrap();
    // Thư mục rỗng thì ĐƯỢC đè — POSIX cho phép.
    let target = fs.create(b"/t", 0, INODE_DIR).unwrap();
    fs.rename(b"/e", b"/t", 0)
        .expect("thư mục rỗng đè thư mục rỗng");
    assert_eq!(fs.lookup(b"/t", 0).unwrap(), empty);
    assert!(
        fs.info(target).is_err(),
        "inode thư mục đích phải được giải phóng"
    );
    fs.fsck().unwrap();
}

// ------------------------------------------------------------- truncate

/// `truncate(0)` phải **giải phóng** mọi block dữ liệu (bitmap giảm đúng số block) và nội dung
/// đọc lại là 0 byte.
#[test]
fn truncate_ve_0_giai_phong_moi_block_va_bitmap() {
    let mut fs = fresh();
    let ino = fs.create(b"/t", 0, INODE_FILE).unwrap();
    let data = payload(0x5a, BLOCK_SIZE * 4);
    fs.write(ino, 0, &data, 0).unwrap();
    let (used_before, total) = fs.usage();
    assert!(used_before >= 4, "phải cấp 4 block: {used_before}");
    fs.truncate(ino, 0, 0).unwrap();
    let (used_after, _) = fs.usage();
    assert!(
        used_after + 4 <= used_before,
        "truncate(0) phải trả lại ≥4 block: trước={used_before} sau={used_after}"
    );
    assert_eq!(fs.info(ino).unwrap().size, 0);
    let mut out = [0u8; 8];
    assert_eq!(fs.read(ino, 0, &mut out, 0).unwrap(), 0);
    // Block được tái dùng ngay (cùng FS): ghi lại 4 block phải thành công với ít nhất
    // `used_after + 4` block đang dùng — nếu block rò thì `NoSpace` chỉ tới khi cạn.
    fs.write(ino, 0, &data, 0).unwrap();
    let (used_again, _) = fs.usage();
    assert_eq!(
        used_again,
        used_after + 4,
        "block trả lại phải được cấp lại (không rò); total={total}"
    );
    fs.fsck().unwrap();
    let mut again = reopen(&fs);
    assert_eq!(again.info(ino).unwrap().size as usize, BLOCK_SIZE * 4);
    again.fsck().unwrap();
}

/// `truncate` trên file đã dùng **double-indirect** phải giải phóng cả bảng hai cấp
/// (root + bảng L2 + data); `usage` về đúng mức trước khi ghi.
#[test]
fn truncate_giai_phong_double_indirect_hai_cap() {
    let mut fs = fresh();
    let ino = fs.create(b"/big", 0, INODE_FILE).unwrap();
    let (used_empty, _) = fs.usage();
    let direct = fs.direct_blocks;
    let index = direct + INDIRECT_ENTRIES + 5;
    fs.write(ino, index * BLOCK_SIZE, b"KT71", 0).unwrap();
    let (used_big, _) = fs.usage();
    assert!(
        used_big >= used_empty + 3,
        "phải cấp root double + bảng L2 + data: {used_empty}→{used_big}"
    );
    fs.truncate(ino, 0, 0).unwrap();
    let (used_after, _) = fs.usage();
    assert_eq!(
        used_after, used_empty,
        "truncate phải trả MỌI block của bảng hai cấp: {used_big}→{used_after}"
    );
    assert_eq!(fs.inodes[ino as usize - 1].blocks[direct + 1], 0);
    fs.fsck().unwrap();
    // Ghi lại đúng chỗ đó: cấp lại được (bảng cũ đã trả).
    fs.write(ino, index * BLOCK_SIZE, b"KT71", 0).unwrap();
    let mut out = [0u8; 4];
    assert_eq!(fs.read(ino, index * BLOCK_SIZE, &mut out, 0).unwrap(), 4);
    assert_eq!(&out, b"KT71");
    fs.fsck().unwrap();
}

// ------------------------------------------------------- double-indirect

/// File thưa đủ lớn để **con trỏ gián tiếp hai cấp** thật sự được dùng: ghi ở block
/// `direct + 512 + 3` ⇒ inode có root double, root trỏ **bảng L2**, L2 trỏ data; đọc lại đúng;
/// lỗ giữa đọc ra 0; `fsck` sạch (kể cả sau khi mở lại).
#[test]
fn double_indirect_hai_cap_that_su_duoc_dung() {
    let mut fs = fresh();
    let ino = fs.create(b"/double", 0, INODE_FILE).unwrap();
    let direct = fs.direct_blocks;
    let index = direct + INDIRECT_ENTRIES + 3;
    let offset = index * BLOCK_SIZE;
    fs.write(ino, offset, b"2CAP", 0).unwrap();
    let inode = fs.inodes[ino as usize - 1];
    let double_root = inode.blocks[direct + 1];
    assert_ne!(double_root, 0, "phải có root double-indirect");
    assert_eq!(
        inode.blocks[direct], 0,
        "vùng double không được cấp bảng single"
    );
    let root = fs.read_staged(double_root).unwrap();
    let l2 = get_u32(&root, 0);
    assert_ne!(l2, 0, "root phải trỏ bảng L2");
    let table = fs.read_staged(l2).unwrap();
    let data = get_u32(&table, (index - direct - INDIRECT_ENTRIES) * 4);
    assert_ne!(data, 0, "L2 phải trỏ data");
    let mut out = [0u8; 4];
    assert_eq!(fs.read(ino, offset, &mut out, 0).unwrap(), 4);
    assert_eq!(&out, b"2CAP");
    // Lỗ giữa (block 0 của file) đọc ra 0 — file thưa hợp lệ.
    assert_eq!(fs.read(ino, 0, &mut out, 0).unwrap(), 4);
    assert_eq!(&out[..], &[0u8; 4][..]);
    fs.fsck().unwrap();
    let mut again = reopen(&fs);
    let mut out = [0u8; 4];
    assert_eq!(again.read(ino, offset, &mut out, 0).unwrap(), 4);
    assert_eq!(&out, b"2CAP");
    again.fsck().unwrap();
}

// ------------------------------------------------- journal: commit + replay

fn formatted_dev() -> MemDev {
    let mut dev = MemDev::new();
    format(&mut dev).unwrap();
    dev
}

fn setup_committed(fs: &mut Fs<PrefixDisk<MemDev>>) {
    let keep = fs.create(b"/keep", 0, INODE_FILE).unwrap();
    fs.write(keep, 0, b"KEEP", 0).unwrap();
    let log = fs.create(b"/log", 0, INODE_FILE).unwrap();
    fs.write(log, 0, b"OLD", 0).unwrap();
}

fn crashing_write(fs: &mut Fs<PrefixDisk<MemDev>>) {
    let log = fs.lookup(b"/log", 0).unwrap();
    fs.write(log, 0, b"NEWDATA", 0).unwrap();
}

fn log_content(fs: &mut Fs<MemDev>) -> Vec<u8> {
    let ino = fs.lookup(b"/log", 0).unwrap();
    let size = fs.info(ino).unwrap().size as usize;
    let mut out = vec![0u8; size];
    fs.read(ino, 0, &mut out, 0).unwrap();
    out
}

/// Journal phải **flush trước khi ghi commit record** (write-ahead): sau crash ở mọi tiền tố,
/// `replay` cho FS nhất quán, `/keep` (đã commit trước) không mất, và thao tác đang dở là
/// **nguyên tử** (hoặc cũ hoặc mới).
#[test]
fn crash_7_pha_nhat_quan_va_khong_mat_du_lieu_da_commit() {
    // Đo số lệnh ghi của MỘT thao tác (một chunk 512 B, commit một lần).
    let (_, writes) = run_op(formatted_dev(), None, setup_committed, crashing_write);
    assert!(
        writes >= 6,
        "một commit phải có ≥6 lệnh ghi (data journal + header + commit + targets + clear), đo {writes}"
    );
    let phase_of = |k: usize| -> usize {
        match k {
            0 => 0,
            1 => writes / 4,
            2 => writes / 2,
            3 => writes - 3,
            4 => writes - 2,
            5 => writes - 1,
            _ => writes,
        }
    };
    let mut seen_new = Vec::new();
    let mut seen_old = Vec::new();
    let mut replays = Vec::new();
    for k in 0..7 {
        let phase = phase_of(k);
        let (dev, issued) = run_op(
            formatted_dev(),
            Some(phase),
            setup_committed,
            crashing_write,
        );
        assert_eq!(issued, writes, "số lệnh ghi phải tất định");
        let mut fs = Fs::mount(dev).expect("mount sau crash");
        replays.push(fs.replayed());
        // Dữ liệu đã commit TRƯỚC thao tác bị crash: không được mất.
        let keep = fs.lookup(b"/keep", 0).expect("/keep phải còn");
        let mut out = [0u8; 8];
        assert_eq!(fs.read(keep, 0, &mut out, 0).unwrap(), 4);
        assert_eq!(&out[..4], b"KEEP", "pha {phase}: mất dữ liệu đã commit");
        let content = log_content(&mut fs);
        if content == b"NEWDATA" {
            seen_new.push(phase);
        } else if content == b"OLD" {
            seen_old.push(phase);
        } else {
            panic!("pha {phase}: nội dung nửa vời sau replay: {content:?}");
        }
        // Sau replay FS phải NHẤT QUÁN: fsck (kiểm) sạch; fsck_repair không phải sửa gì.
        fs.fsck().expect("fsck phải sạch sau replay");
        let report = fs.fsck_repair().unwrap();
        assert_eq!(
            report.found, 0,
            "pha {phase}: fsck_repair phải thấy 0 lỗi, thấy {report:?}"
        );
        // FS còn dùng được: tạo + đọc lại một file mới.
        let probe = fs.create(b"/probe", 0, INODE_FILE).unwrap();
        fs.write(probe, 0, b"P", 0).unwrap();
        let mut one = [0u8; 1];
        assert_eq!(fs.read(probe, 0, &mut one, 0).unwrap(), 1);
        fs.fsck().unwrap();
    }
    // Nguyên tử: tồn tại một ngưỡng — mọi pha dưới ngưỡng là CŨ, từ ngưỡng trở lên là MỚI.
    seen_old.sort_unstable();
    seen_new.sort_unstable();
    assert!(
        !seen_old.is_empty() && !seen_new.is_empty(),
        "7 pha phải phủ cả hai trạng thái: cũ={seen_old:?} mới={seen_new:?}"
    );
    assert!(
        seen_old.iter().max().unwrap() < seen_new.iter().min().unwrap(),
        "pha phải đơn điệu (nguyên tử): cũ={seen_old:?} mới={seen_new:?}"
    );
    // Có ít nhất một pha replay phải áp dụng lại (commit record sống sót qua crash).
    assert!(
        replays.contains(&1) || seen_new.iter().any(|p| *p >= 4),
        "phải có pha crash SAU commit record: replays={replays:?} mới={seen_new:?}"
    );
}

/// **Ca âm** của journal (D2-108B): commit record sống sót nhưng **dữ liệu journal hỏng** ⇒
/// `replay` phải từ chối (CRC) và **không** áp dụng gì: nội dung cũ, `fsck` sạch.
#[test]
fn journal_tu_choi_commit_hong_crc_khong_ap_dung() {
    let (_, _writes) = run_op(formatted_dev(), None, setup_committed, crashing_write);
    // Tiền tố ngay sau commit record: journal data + header + commit đã xuống đĩa, chưa target.
    let (mut dev, _) = run_op(formatted_dev(), Some(4), setup_committed, crashing_write);
    // Hỏng **dữ liệu** journal (không hỏng CRC): ghi đè block dữ liệu journal đầu tiên.
    let mut junk = [0u8; BLOCK_SIZE];
    junk[7] = 0xff;
    dev.write(JOURNAL_DATA_START, &junk).unwrap();
    let mut fs = Fs::mount(dev).expect("mount với journal hỏng");
    assert_eq!(fs.replayed(), 2, "replay phải từ chối journal hỏng CRC");
    assert_eq!(
        log_content(&mut fs),
        b"OLD",
        "journal hỏng không được áp dụng"
    );
    fs.fsck()
        .expect("FS phải nhất quán sau khi từ chối journal");
}

// ------------------------------------------------------------ fsck sửa

fn corrupt_four(fs: &mut Fs<MemDev>) -> (u16, u32, u32) {
    let keep = fs.lookup(b"/keep", 0).unwrap();
    // (1) block mồ côi: đánh dấu dùng một block không ai tham chiếu.
    let orphan_block = 40u32;
    fs.mark_used(orphan_block);
    // (2) block đếm sai: inode trỏ thêm một block NGOÀI size (và block đó được đánh dấu dùng).
    let extra = 45u32;
    fs.mark_used(extra);
    fs.inodes[keep as usize - 1].blocks[1] = extra;
    // (3) inode không tham chiếu: cấp một inode FILE rỗng, không nằm trong thư mục nào.
    let orphan_inode = (1..MAX_INODES)
        .find(|index| fs.inodes[*index].kind == INODE_FREE)
        .map(|index| (index + 1) as u16)
        .unwrap();
    fs.inodes[orphan_inode as usize - 1] = Inode {
        kind: INODE_FILE,
        nlink: 1,
        ..Inode::default()
    };
    // (4) link count sai: /keep đang có 1 liên kết nhưng inode ghi 5.
    fs.inodes[keep as usize - 1].nlink = 5;
    fs.stage_inode(keep).unwrap();
    fs.stage_inode(orphan_inode).unwrap();
    fs.stage_bitmap().unwrap();
    fs.commit().unwrap();
    (keep, orphan_block, extra)
}

/// `fsck_repair` phát hiện **và sửa** bốn loại hỏng có chủ đích (block mồ côi, block đếm sai,
/// inode mồ côi, link count sai), báo đúng số phát hiện/đã sửa; lần hai sạch; dữ liệu đã commit
/// còn nguyên; `usage` giảm đúng số block mồ côi.
#[test]
fn fsck_repair_bon_loai_hong_va_lan_hai_sach() {
    let mut fs = committed_dev_fs();
    let keep_count = fs.usage().0;
    let (keep, orphan_block, extra) = corrupt_four(&mut fs);
    // Hỏng thật: `fsck` (kiểm) phải đỏ trước khi sửa.
    assert!(fs.fsck().is_err(), "fsck phải phát hiện FS hỏng");
    let (used_corrupt, _) = fs.usage();
    let dev = fs.into_dev();
    // Fsck phải chạy được trên ảnh **hỏng** vừa mount lại.
    let mut fs = Fs::mount(dev).expect("mount ảnh hỏng");
    let report = fs.fsck_repair().unwrap();
    assert!(
        report.found >= 4,
        "phải phát hiện ≥4 lỗi (mồ côi block/inode, đếm block, nlink): {report:?}"
    );
    assert_eq!(
        report.fixed, report.found,
        "phải sửa hết số đã phát hiện: {report:?}"
    );
    assert_eq!(report.unfixed, 0);
    assert!(report.orphan_blocks >= 1, "{report:?}");
    assert!(report.bad_blocks >= 1, "{report:?}");
    assert!(report.orphan_inodes >= 1, "{report:?}");
    assert!(report.bad_nlink >= 1, "{report:?}");
    // Sửa xong: kiểm lại sạch, và repair lần hai không còn gì để sửa.
    fs.fsck().expect("fsck phải sạch sau khi sửa");
    let again = fs.fsck_repair().unwrap();
    assert_eq!(again.found, 0, "lần hai phải sạch: {again:?}");
    // Dữ liệu đã commit còn nguyên.
    let mut out = [0u8; 8];
    assert_eq!(fs.read(keep, 0, &mut out, 0).unwrap(), 4);
    assert_eq!(&out[..4], b"KEEP");
    // Block mồ côi đã trả: usage giảm đúng 2 (block mồ côi + block ngoài size).
    let (used_fixed, _) = fs.usage();
    assert_eq!(
        used_fixed, keep_count,
        "usage phải về mức trước khi hỏng: hỏng={used_corrupt} sửa={used_fixed} gốc={keep_count}"
    );
    assert!(!fs.block_is_used(orphan_block));
    assert!(!fs.block_is_used(extra));
    // Mở lại từ đĩa: sửa đã bền (qua journal).
    let dev = fs.into_dev();
    let mut final_fs = Fs::mount(dev).unwrap();
    final_fs.fsck().expect("sửa phải bền sau khi mở lại");
    let mut out = [0u8; 8];
    let keep2 = final_fs.lookup(b"/keep", 0).unwrap();
    assert_eq!(final_fs.read(keep2, 0, &mut out, 0).unwrap(), 4);
    assert_eq!(&out[..4], b"KEEP");
}

fn committed_dev_fs() -> Fs<MemDev> {
    let mut dev = MemDev::new();
    format(&mut dev).unwrap();
    let mut fs = Fs::mount(dev).unwrap();
    let keep = fs.create(b"/keep", 0, INODE_FILE).unwrap();
    fs.write(keep, 0, b"KEEP", 0).unwrap();
    fs
}

// ---------------------------------------------- journal + ĐẢO THỨ TỰ (ca âm D2-108B)

/// Thiết bị mô phỏng **cache ghi tái sắp xếp**: khi bật `reorder`, lệnh ghi nằm trong
/// `pending` (đọc-thấy-ghi-của-mình vẫn đúng); `flush` đẩy **toàn bộ** pending xuống đĩa theo
/// thứ tự. "Crash" = chỉ **lệnh ghi cuối** trong pending sống sót (đĩa thật có thể để commit
/// record xuống trước thân log) — đúng ca mà flush-trước-commit-record phải chống.
struct ReorderDev {
    inner: MemDev,
    pending: Vec<(u32, [u8; BLOCK_SIZE])>,
    reorder: bool,
    limit: Option<usize>,
    counting: bool,
    /// "Chết" tại điểm crash: mọi lệnh ghi/flush SAU đó không xảy ra (tiến trình dừng), khác
    /// mô hình "mất lệnh ghi" của `PrefixDisk` (dùng để mô phỏng tái sắp xếp).
    dead: bool,
    writes: usize,
}

impl ReorderDev {
    fn new(inner: MemDev) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            reorder: false,
            limit: None,
            counting: false,
            dead: false,
            writes: 0,
        }
    }
    fn arm(&mut self, reorder: bool, limit: Option<usize>) {
        self.reorder = reorder;
        self.limit = limit;
        self.counting = true;
        self.dead = false;
        self.writes = 0;
        self.pending.clear();
    }
    /// Crash đảo thứ tự: chỉ lệnh ghi **cuối** còn trong cache xuống đĩa; phần khác mất.
    fn crash_keep_last(&mut self) {
        self.reorder = false;
        if let Some((lba, data)) = self.pending.last().copied() {
            self.inner.write(lba, &data).unwrap();
        }
        self.pending.clear();
    }
}

impl BlockDev for ReorderDev {
    fn read(&mut self, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        if let Some((_, data)) = self.pending.iter().rev().find(|(at, _)| *at == lba) {
            out.copy_from_slice(data);
            return Ok(());
        }
        self.inner.read(lba, out)
    }
    fn write(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        if self.dead {
            return Ok(());
        }
        if self.counting {
            self.writes += 1;
            if let Some(limit) = self.limit {
                if self.writes > limit {
                    self.dead = true;
                    return Ok(());
                }
            }
        }
        if self.reorder {
            self.pending.push((lba, *data));
            return Ok(());
        }
        self.inner.write(lba, data)
    }
    fn block_count(&self) -> u32 {
        self.inner.block_count()
    }
    fn flush(&mut self) -> Result<(), FsError> {
        if self.dead {
            return Ok(());
        }
        if self.reorder {
            for (lba, data) in core::mem::take(&mut self.pending) {
                self.inner.write(lba, &data)?;
            }
        }
        Ok(())
    }
}

/// **Ca âm cho bất biến "flush thân log TRƯỚC commit record"** (D2-108B): nếu bỏ flush trong
/// `commit()`, crash đảo thứ tự (commit record xuống trước, thân log mất) làm replay **từ chối**
/// (CRC) và thao tác biến mất; có flush, thân log đã bền trước khi commit record được phát ra ⇒
/// replay **áp dụng** và nội dung là bản mới. Test này **đỏ** khi bỏ flush (đã đo:
/// `JOURNAL/kt71-red-reorder.log`).
#[test]
fn journal_dao_thu_tu_commit_record_phai_con_than_log_nho_flush() {
    let mut dev = MemDev::new();
    format(&mut dev).unwrap();
    let mut fs = Fs::empty(ReorderDev::new(dev));
    fs.initialize().unwrap();
    let keep = fs.create(b"/keep", 0, INODE_FILE).unwrap();
    fs.write(keep, 0, b"KEEP", 0).unwrap();
    let log = fs.create(b"/log", 0, INODE_FILE).unwrap();
    fs.write(log, 0, b"OLD", 0).unwrap();
    let log_block = fs.inode_blocks_raw(log).unwrap()[0];
    // (1) Đo số lệnh ghi trên **bản sao** của ảnh đĩa — không đụng ảnh thật.
    let snapshot = fs.into_dev();
    let mut probe = Fs::empty(ReorderDev::new(snapshot.inner.clone()));
    probe.initialize().unwrap();
    probe.dev_mut().arm(true, None);
    probe.write(log, 0, b"NEWDATA", 0).unwrap();
    let writes = probe.dev_mut().writes;
    assert!(writes >= 6, "thao tác phải có ≥6 lệnh ghi, đo {writes}");
    drop(probe);
    // (2) Crash thật trên ảnh gốc: dừng NGAY SAU commit record (`writes - 3` = 2 target +
    //     1 clear không bao giờ tới). Cache ghi giữ các lệnh chưa flush; khi "chết", chỉ lệnh
    //     ghi CUỐI trong cache xuống đĩa (mô phỏng tái sắp xếp: commit record vượt thân log).
    let mut fs = Fs::empty(snapshot);
    fs.initialize().unwrap();
    fs.dev_mut().arm(true, Some(writes - 3));
    fs.write(log, 0, b"NEWDATA", 0).unwrap();
    fs.dev_mut().crash_keep_last();
    // Trạng thái ĐĨA trước replay: target chưa được ghi (dữ liệu cũ nằm trên đĩa).
    let mut raw = [0u8; BLOCK_SIZE];
    fs.dev_mut().read(log_block, &mut raw).unwrap();
    assert_eq!(
        &raw[..3],
        b"OLD",
        "chưa crash xong đã có dữ liệu mới trên đĩa"
    );
    // (3) Mount lại = replay. Có flush trước commit record ⇒ thân log đã bền ⇒ áp dụng.
    let dev = fs.into_dev();
    let mut fs = Fs::mount(dev.inner).unwrap();
    assert_eq!(
        fs.replayed(),
        1,
        "commit record sống + thân log đã flush ⇒ replay phải áp dụng (2 = từ chối vì thân log mất)"
    );
    let mut out = [0u8; 8];
    let n = fs.read(log, 0, &mut out, 0).unwrap();
    assert_eq!(&out[..n], b"NEWDATA", "thao tác đã commit phải còn");
    fs.fsck().unwrap();
}
