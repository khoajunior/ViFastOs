// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-71** — tầng 2 của VFS trên ảnh `vifast-kbin-ktest` (TAP 65..69): **rename** (kể cả đè),
//! **truncate**, **double-indirect**, **ghi qua cache (write-back)**, **journal replay** sau
//! crash, **7 pha crash** và **fsck phát hiện + sửa**.
//!
//! # Vì sao chạy trong kernel (không phải U-mode)
//!
//! `vifast-fs`/`vifast-vfs` là crate `no_std`, `#![forbid(unsafe_code)]` — **cùng mã** chạy trong
//! test host (`v2/lib/vifast-fs/src/tests/part3.rs`, `v2/lib/vifast-vfs/tests/writeback.rs`).
//! Trên QEMU, pha này dùng **thiết bị RAM trong `.bss`** (`RamDisk` trên `AtomicU8`, **không
//! `unsafe`**), nên đo được đường journal/fsck thật trên nhân thật. Đường U-mode (pager ghi
//! trang) thuộc KT-72 khi có virtio-blk.
//!
//! # Ngân sách stack (đo, không đoán)
//!
//! Một `Fs<RamDisk>` là **13 KiB**; stack boot chỉ 64 KiB và các pha trước đã dùng ~26 KiB.
//! Vì vậy pha này **chỉ có MỘT** object `Fs` (tạo trong `fs_phase`) và "mount lại" bằng
//! `initialize()` **tại chỗ** — không `Fs::mount` trả giá trị 13 KiB qua nhiều khung. `# fs:
//! stack` in độ sâu đo được (`mmu::stack_range`).
//!
//! Mọi dòng `# fs:` in từ **biến đo** (D2-102); hằng thiết kế in kèm `const:`; giá trị suy ra in
//! kèm **công thức + đầu vào cùng dòng** (D2-106). Không `unsafe` trong tệp này.

use core::fmt::Write;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use vifast_fs::{
    format_with_options, BlockDev, FormatOptions, Fs, FsError, BITMAP_START, BLOCK_SIZE, INODE_DIR,
    INODE_FILE, INODE_START, JOURNAL_DATA_START, MAX_DIR_BLOCKS, MAX_INODES,
};
use vifast_plat_common::PlatformWriter;
use vifast_plat_riscv64::Riscv64Virt;
use vifast_vfs::{
    Begin, PageSink, PageSource, PagerCore, CACHE_CAPACITY, PAGE_SIZE, SECTORS_PER_PAGE,
};

static PLATFORM: Riscv64Virt = Riscv64Virt;

/// Số block của thiết bị RAM. Vùng dữ liệu = `48 - 30 = 18` block (đủ cây nhỏ + bảng
/// double-indirect của file thưa) — hằng **thiết kế**, in kèm `const:`.
const DISK_BLOCKS: u32 = 48;
/// Tổng byte thiết bị RAM = `DISK_BLOCKS * BLOCK_SIZE`.
const DISK_BYTES: usize = DISK_BLOCKS as usize * BLOCK_SIZE;

/// Thiết bị RAM của pha FS: mỗi byte một `AtomicU8` (interior mutability, **không `unsafe`**).
static DISK: [AtomicU8; DISK_BYTES] = [const { AtomicU8::new(0) }; DISK_BYTES];
/// Một "Frame" 4 KiB cho đường ghi qua cache (PageSink/PageSource).
static PAGE: [AtomicU8; PAGE_SIZE] = [const { AtomicU8::new(0) }; PAGE_SIZE];

/// Mô hình crash "đĩa bền theo tiền tố": trong thao tác được đo, chỉ `LIMIT` lệnh ghi đầu tiên
/// sống sót; phần sau **mất im lặng** (cache volatile) — FS không thấy lỗi I/O.
static LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);
/// Số lệnh ghi đã phát ra kể từ `arm`.
static WRITES: AtomicUsize = AtomicUsize::new(usize::MAX);
/// Số lệnh ghi đã bị mất (phần đuôi sau điểm crash).
static DROPPED: AtomicUsize = AtomicUsize::new(0);
/// Chế độ crash: `0` = mất **đuôi** (tiền tố bền, `PrefixDisk`); `1` = **đảo thứ tự** (cache ghi
/// giữ lệnh chưa flush; "chết" thì chỉ lệnh ghi CUỐI xuống đĩa — mô phỏng commit record vượt
/// thân log, đúng ca mà flush-trước-commit-record phải chống).
static CRASH_MODE: AtomicUsize = AtomicUsize::new(0);
/// Đã "chết" tại điểm crash: mọi lệnh ghi/flush sau đó không xảy ra.
static DEAD: AtomicUsize = AtomicUsize::new(0);
/// Cache ghi tái sắp xếp: số lệnh đang chờ + (lba, dữ liệu) của từng lệnh.
const PENDING_CAP: usize = 12;
static PENDING_COUNT: AtomicUsize = AtomicUsize::new(0);
static PENDING_LBA: [AtomicUsize; PENDING_CAP] = [const { AtomicUsize::new(0) }; PENDING_CAP];
static PENDING_DATA: [[AtomicU8; BLOCK_SIZE]; PENDING_CAP] =
    [const { [const { AtomicU8::new(0) }; BLOCK_SIZE] }; PENDING_CAP];

/// Thiết bị chặn theo `AtomicU8` — cùng `BlockDev` mà pager U-mode dùng (KT-70).
#[derive(Clone, Copy)]
struct RamDisk {
    blocks: u32,
}

impl RamDisk {
    const fn new(blocks: u32) -> Self {
        Self { blocks }
    }

    fn bytes(&self) -> usize {
        self.blocks as usize * BLOCK_SIZE
    }

    /// Ghi thẳng byte (bỏ qua mô hình crash) — dùng để format/zero đĩa.
    fn fill(&self, value: u8) {
        for byte in DISK.iter().take(self.bytes()) {
            byte.store(value, Ordering::Relaxed);
        }
    }
}

impl BlockDev for RamDisk {
    fn read(&mut self, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        let base = lba as usize * BLOCK_SIZE;
        if base + BLOCK_SIZE > self.bytes() {
            return Err(FsError::Invalid);
        }
        // Cache ghi: đọc-thấy-ghi-của-mình (lệnh chưa flush nằm trong `PENDING`).
        if CRASH_MODE.load(Ordering::Relaxed) == 1 {
            let count = PENDING_COUNT.load(Ordering::Relaxed).min(PENDING_CAP);
            let mut index = count;
            while index != 0 {
                index -= 1;
                if PENDING_LBA[index].load(Ordering::Relaxed) == lba as usize {
                    for (at, value) in out.iter_mut().enumerate() {
                        *value = PENDING_DATA[index][at].load(Ordering::Relaxed);
                    }
                    return Ok(());
                }
            }
        }
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = DISK[base + index].load(Ordering::Relaxed);
        }
        Ok(())
    }

    fn write(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        let base = lba as usize * BLOCK_SIZE;
        if base + BLOCK_SIZE > self.bytes() {
            return Err(FsError::Invalid);
        }
        if DEAD.load(Ordering::Relaxed) == 1 {
            return Ok(());
        }
        if WRITES.load(Ordering::Relaxed) != usize::MAX {
            let issued = WRITES.fetch_add(1, Ordering::Relaxed) + 1;
            if issued > LIMIT.load(Ordering::Relaxed) {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                if CRASH_MODE.load(Ordering::Relaxed) == 1 {
                    DEAD.store(1, Ordering::Relaxed);
                }
                return Ok(());
            }
        }
        if CRASH_MODE.load(Ordering::Relaxed) == 1 {
            let index = PENDING_COUNT.load(Ordering::Relaxed);
            if index < PENDING_CAP {
                PENDING_LBA[index].store(lba as usize, Ordering::Relaxed);
                for (byte, value) in data.iter().enumerate() {
                    PENDING_DATA[index][byte].store(*value, Ordering::Relaxed);
                }
                PENDING_COUNT.store(index + 1, Ordering::Relaxed);
            }
            return Ok(());
        }
        apply_block(lba, data);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), FsError> {
        if DEAD.load(Ordering::Relaxed) == 1 {
            return Ok(());
        }
        if CRASH_MODE.load(Ordering::Relaxed) == 1 {
            let count = PENDING_COUNT.swap(0, Ordering::Relaxed).min(PENDING_CAP);
            let mut index = 0usize;
            while index < count {
                let lba = PENDING_LBA[index].load(Ordering::Relaxed) as u32;
                let mut data = [0u8; BLOCK_SIZE];
                for (at, value) in data.iter_mut().enumerate() {
                    *value = PENDING_DATA[index][at].load(Ordering::Relaxed);
                }
                apply_block(lba, &data);
                index += 1;
            }
        }
        Ok(())
    }

    fn block_count(&self) -> u32 {
        self.blocks
    }
}

/// Bật mô hình crash: đếm lại từ 0, chỉ `limit` lệnh ghi đầu tiên sống sót.
fn arm(limit: usize) {
    WRITES.store(0, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    LIMIT.store(limit, Ordering::Relaxed);
}

/// Bật mô hình **đảo thứ tự**: giống `arm` nhưng khi chết thì cache giữ lệnh chưa flush; caller
/// gọi [`crash_keep_last`] để mô phỏng "chỉ commit record xuống đĩa".
fn arm_reorder(limit: usize) {
    CRASH_MODE.store(1, Ordering::Relaxed);
    DEAD.store(0, Ordering::Relaxed);
    PENDING_COUNT.store(0, Ordering::Relaxed);
    arm(limit);
}

/// Tắt mô hình crash; trả `(số lệnh ghi đã phát ra, số đã mất)`.
fn disarm() -> (usize, usize) {
    let writes = WRITES.swap(usize::MAX, Ordering::Relaxed);
    LIMIT.store(usize::MAX, Ordering::Relaxed);
    let dropped = DROPPED.load(Ordering::Relaxed);
    CRASH_MODE.store(0, Ordering::Relaxed);
    DEAD.store(0, Ordering::Relaxed);
    (writes, dropped)
}

/// "Chết" với cache ghi: chỉ **lệnh ghi cuối** trong cache xuống đĩa, phần khác mất (mô phỏng tái
/// sắp xếp để commit record vượt thân log). Trả `true` nếu có lệnh ghi nào đã xuống đĩa.
fn crash_keep_last() -> bool {
    let count = PENDING_COUNT.swap(0, Ordering::Relaxed).min(PENDING_CAP);
    CRASH_MODE.store(0, Ordering::Relaxed);
    DEAD.store(0, Ordering::Relaxed);
    if count == 0 {
        return false;
    }
    let index = count - 1;
    let lba = PENDING_LBA[index].load(Ordering::Relaxed) as u32;
    let mut data = [0u8; BLOCK_SIZE];
    for (at, value) in data.iter_mut().enumerate() {
        *value = PENDING_DATA[index][at].load(Ordering::Relaxed);
    }
    apply_block(lba, &data);
    true
}

/// Ghi thẳng một block (bỏ qua mọi mô hình crash).
fn apply_block(lba: u32, data: &[u8; BLOCK_SIZE]) {
    let base = lba as usize * BLOCK_SIZE;
    for (index, byte) in data.iter().enumerate() {
        DISK[base + index].store(*byte, Ordering::Relaxed);
    }
}

/// Đặt lại thiết bị về 0 rồi format FS (journal cũng bị xoá ⇒ không replay rác giữa các pha).
fn format_disk() {
    let dev = RamDisk::new(DISK_BLOCKS);
    dev.fill(0);
    let mut writer = dev;
    format_with_options(
        &mut writer,
        FormatOptions {
            total_blocks: DISK_BLOCKS,
            inode_limit: MAX_INODES as u16,
            dir_block_limit: MAX_DIR_BLOCKS as u8,
        },
    )
    .expect("format 48 block");
}

/// "Mount" **tại chỗ** trên `fs` đã có (một object duy nhất cho cả pha): `initialize()` đọc
/// superblock/bitmap/inode và **replay** journal — đúng đường mount, không cấp thêm 13 KiB.
#[inline(never)]
fn remount(fs: &mut Fs<RamDisk>) {
    fs.initialize().expect("initialize/replay FS");
}

/// "Frame" 4 KiB cho pager: ghi/đọc sector theo `slot`.
struct StaticPage;

impl PageSink for StaticPage {
    fn write_sector(&mut self, slot: u8, sector_in_page: u32, data: &[u8; BLOCK_SIZE]) {
        let base = slot as usize * PAGE_SIZE + sector_in_page as usize * BLOCK_SIZE;
        for (index, byte) in data.iter().enumerate() {
            PAGE[base + index].store(*byte, Ordering::Relaxed);
        }
    }
}

impl PageSource for StaticPage {
    fn read_sector(&mut self, slot: u8, sector_in_page: u32, out: &mut [u8; BLOCK_SIZE]) {
        let base = slot as usize * PAGE_SIZE + sector_in_page as usize * BLOCK_SIZE;
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = PAGE[base + index].load(Ordering::Relaxed);
        }
    }
}

fn page_byte(slot: u8, offset: usize) -> u8 {
    PAGE[slot as usize * PAGE_SIZE + offset].load(Ordering::Relaxed)
}

fn set_page_byte(slot: u8, offset: usize, value: u8) {
    PAGE[slot as usize * PAGE_SIZE + offset].store(value, Ordering::Relaxed);
}

// ---------------------------------------------------------------- setup/op cho crash

fn setup_committed(fs: &mut Fs<RamDisk>) {
    let keep = fs.create(b"/keep", 0, INODE_FILE).expect("create /keep");
    fs.write(keep, 0, b"KEEP", 0).expect("write /keep");
    let log = fs.create(b"/log", 0, INODE_FILE).expect("create /log");
    fs.write(log, 0, b"OLD", 0).expect("write /log");
}

fn crashing_write(fs: &mut Fs<RamDisk>) {
    let log = fs.lookup(b"/log", 0).expect("lookup /log");
    fs.write(log, 0, b"NEWDATA", 0)
        .expect("write /log (op bị crash)");
}

/// Nội dung `/log` sau khi mount lại: `1` = mới, `0` = cũ, `2` = nửa vời (không được phép).
fn log_state(fs: &mut Fs<RamDisk>) -> (u8, u32) {
    let log = fs.lookup(b"/log", 0).expect("lookup /log");
    let size = fs.info(log).expect("info /log").size;
    let mut out = [0u8; 8];
    let count = core::cmp::min(size as usize, out.len());
    fs.read(log, 0, &mut out[..count], 0).expect("read /log");
    let slice = &out[..count];
    if slice == b"NEWDATA" {
        (1, size)
    } else if slice == b"OLD" {
        (0, size)
    } else {
        (2, size)
    }
}

fn keep_ok(fs: &mut Fs<RamDisk>) -> bool {
    let Some(keep) = fs.lookup(b"/keep", 0).ok() else {
        return false;
    };
    let mut out = [0u8; 4];
    if fs.read(keep, 0, &mut out, 0).unwrap_or(0) != 4 {
        return false;
    }
    &out == b"KEEP"
}

/// Độ sâu đã dùng của stack boot (byte) — địa chỉ một biến cục bộ so với đỉnh stack
/// (`mmu::stack_range`, API an toàn). Bằng chứng "pha FS vừa ngân sách stack".
#[inline(never)]
fn stack_used() -> u64 {
    let probe = 0u8;
    let here = &probe as *const u8 as u64;
    let (_low, high) = vifast_plat_riscv64::mmu::stack_range();
    high.saturating_sub(here)
}

#[inline(never)]
fn put(ok: bool, number: usize, name: &str, fails: &mut usize, why: &str) {
    let mut w = PlatformWriter(&PLATFORM);
    if ok {
        let _ = writeln!(w, "ok {number} - {name}");
    } else {
        let _ = writeln!(w, "not ok {number} - {name}: {why}");
        *fails += 1;
    }
}

/// Chạy pha FS: TAP 65..69. **Một** object `Fs` cho cả pha (xem đầu tệp).
#[inline(never)]
pub fn fs_phase(fails: &mut usize) {
    let mut fs = Fs::empty(RamDisk::new(DISK_BLOCKS));
    format_disk();
    remount(&mut fs);
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(w, "# fs: stack entry used={}", stack_used());
    }
    fs_rename_and_truncate(&mut fs, fails);
    fs_writeback(fails);
    fs_journal(&mut fs, fails);
    fs_crash7(&mut fs, fails);
    fs_fsck(&mut fs, fails);
}

// ------------------------------------------------------------- TAP 65: rename/truncate

#[inline(never)]
fn fs_rename_and_truncate(fs: &mut Fs<RamDisk>, fails: &mut usize) {
    format_disk();
    remount(fs);
    let mut ok = true;
    let mut why = "rename đè thất bại";
    let a = fs.create(b"/a", 0, INODE_FILE).expect("/a");
    let b = fs.create(b"/b", 0, INODE_FILE).expect("/b");
    fs.write(a, 0, b"AAAA", 0).expect("write /a");
    fs.write(b, 0, &[0xbb; 4 * BLOCK_SIZE], 0)
        .expect("write /b");
    let used_before = fs.usage().0;
    let mut dir_over = 0u32;
    let mut src_ino = 0u16;
    let mut replaced_gone = 0u32;
    let mut used_after = 0u32;
    if fs.rename(b"/a", b"/b", 0).is_ok() {
        src_ino = fs.lookup(b"/b", 0).unwrap_or(0);
        replaced_gone = u32::from(fs.info(b).is_err());
        used_after = fs.usage().0;
        let d = fs.create(b"/d", 0, INODE_DIR).expect("/d");
        let _ = d;
        let e = fs.create(b"/e", 0, INODE_DIR).expect("/e");
        let _ = e;
        dir_over = u32::from(fs.rename(b"/d", b"/e", 0).is_ok());
        let mut out = [0u8; 4];
        ok = src_ino == a
            && replaced_gone == 1
            && used_after + 4 <= used_before
            && dir_over == 1
            && fs.read(a, 0, &mut out, 0).unwrap_or(0) == 4
            && &out == b"AAAA"
            && fs.lookup(b"/a", 0).is_err();
        why = "rename đè: tên/inode/block không đúng";
    }
    let t = fs.create(b"/t", 0, INODE_FILE).expect("/t");
    fs.write(t, 0, &[0x5a; 4 * BLOCK_SIZE], 0)
        .expect("write /t");
    let used_t = fs.usage().0;
    fs.truncate(t, 0, 0).expect("truncate /t");
    let used_t0 = fs.usage().0;
    let mut out = [0u8; 4];
    let trunc_ok = fs.read(t, 0, &mut out, 0).unwrap_or(1) == 0;
    // Block đã trả phải **dùng lại được**: ghi lại đúng 4 block ⇒ usage về mức trước truncate.
    fs.write(t, 0, &[0x33; 4 * BLOCK_SIZE], 0)
        .expect("write /t sau truncate");
    let used_reuse = fs.usage().0;
    let mut four = [0u8; 4];
    let reuse_ok =
        used_reuse == used_t && fs.read(t, 0, &mut four, 0).unwrap_or(0) == 4 && four == [0x33; 4];
    // double-indirect: file thưa ghi ở block `direct(10) + INDIRECT_ENTRIES(128 = BLOCK_SIZE/4)
    // + 5` ⇒ `slot = 5/128 = 0`, `entry = 5` của bảng L2 (đo từ đĩa thô bên dưới).
    let big = fs.create(b"/big", 0, INODE_FILE).expect("/big");
    let used_big0 = fs.usage().0;
    let index = 10 + 128 + 5;
    fs.write(big, index * BLOCK_SIZE, b"2CAP", 0)
        .expect("write /big");
    let used_big = fs.usage().0;
    let root = fs
        .inode_blocks_raw(big)
        .map(|blocks| blocks[11])
        .unwrap_or(0);
    let single = fs
        .inode_blocks_raw(big)
        .map(|blocks| blocks[10])
        .unwrap_or(1);
    let mut two = [0u8; 4];
    let read_big = fs.read(big, index * BLOCK_SIZE, &mut two, 0).unwrap_or(0);
    // Chuỗi **hai cấp** đo từ ĐĨA THÔ (TRƯỚC truncate): root → bảng L2 → data.
    let mut dev = RamDisk::new(DISK_BLOCKS);
    let mut root_block = [0u8; BLOCK_SIZE];
    if root != 0 {
        dev.read(root, &mut root_block).expect("raw root");
    }
    let l2 = u32::from_le_bytes([root_block[0], root_block[1], root_block[2], root_block[3]]);
    let mut l2_block = [0u8; BLOCK_SIZE];
    if l2 != 0 && l2 < DISK_BLOCKS {
        dev.read(l2, &mut l2_block).expect("raw l2");
    }
    let data_slot = 5usize;
    let data_ptr = u32::from_le_bytes([
        l2_block[data_slot * 4],
        l2_block[data_slot * 4 + 1],
        l2_block[data_slot * 4 + 2],
        l2_block[data_slot * 4 + 3],
    ]);
    fs.truncate(big, 0, 0).expect("truncate /big");
    let used_big1 = fs.usage().0;
    let root_after = fs
        .inode_blocks_raw(big)
        .map(|blocks| blocks[11])
        .unwrap_or(1);

    let data_str = core::str::from_utf8(&two).unwrap_or("?");
    let double_ok = used_big == used_big0 + 3
        && root != 0
        && l2 != 0
        && data_ptr != 0
        && single == 0
        && read_big == 4
        && &two == b"2CAP"
        && used_big1 == used_big0
        && root_after == 0;
    let fsck_ok = fs.fsck().is_ok();
    ok = ok && trunc_ok && reuse_ok && used_t0 + 4 <= used_t && double_ok && fsck_ok;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# fs: rename src={src_ino} exp={a} replaced_gone={replaced_gone} dir_over={dir_over} \
             used={used_before}->{used_after} impl=rename-overwrite"
        );
        let _ = writeln!(
            w,
            "# fs: truncate used={used_t}->{used_t0} freed={} read_empty={trunc_ok} \
             reuse={} (=used_again-used_t0) used_again={used_reuse} used_t0={used_t0} \
             reuse_ok={reuse_ok} const:from_blocks=4",
            used_t.saturating_sub(used_t0),
            used_reuse.saturating_sub(used_t0)
        );
        let _ = writeln!(
            w,
            "# fs: double index={index} used={used_big0}->{used_big}->{used_big1} \
             delta={} (=3: root+bang L2+data) root={root} l2={l2} data_ptr={data_ptr} single={single} \
             read={read_big} data={data_str} fsck={fsck_ok}",
            used_big.saturating_sub(used_big0)
        );
        let _ = writeln!(
            w,
            "# fs: rename_ok={} trunc_ok={trunc_ok} double_ok={double_ok} fsck={fsck_ok} \
             const:disk_blocks={DISK_BLOCKS} const:direct=10",
            u32::from(ok)
        );
    }
    put(ok, 65, "fs rename truncate double-indirect", fails, why);
}

// ------------------------------------------------------------- TAP 66: write-back

#[inline(never)]
fn fs_writeback(fails: &mut usize) {
    let mut dev = RamDisk::new(40);
    for lba in 0..40u32 {
        let mut sector = [0u8; BLOCK_SIZE];
        // Mẫu tất định: byte = (lba*7 + i) % 251 — chỉ dùng để so trước/sau flush.
        for (index, byte) in sector.iter_mut().enumerate() {
            *byte = ((lba as usize * 7 + index) % 251) as u8;
        }
        dev.write(lba, &sector).expect("seed sector");
    }
    let mut core: PagerCore<CACHE_CAPACITY, RamDisk> = PagerCore::new(RamDisk::new(40));
    let mut page = StaticPage;
    let mut sink_pages = 0u32;
    let slot = match core.begin(0, 1) {
        Begin::Ready { slot } => {
            core.load(slot, 0, 1, &mut page).expect("load trang");
            sink_pages += 1;
            slot
        }
        Begin::Hit { slot } => slot,
        other => {
            {
                let mut w = PlatformWriter(&PLATFORM);
                let _ = writeln!(w, "# fs: writeback begin={other:?}");
            }
            0
        }
    };
    let before = core.stats();
    for index in 0..8usize {
        set_page_byte(slot, index, b"KT71DIRT"[index]);
    }
    core.mark_dirty(0, 1).expect("mark_dirty");
    let dirty_before_flush = core.stats();
    let wrote = core.flush_page(0, 1, &mut page).expect("flush_page");
    let after = core.stats();
    let mut dev2 = RamDisk::new(40);
    let mut sector = [0u8; BLOCK_SIZE];
    dev2.read(8, &mut sector).expect("read backend");
    let backend_ok = &sector[..8] == b"KT71DIRT";
    // "Đóng/mở lại": pager mới trên cùng thiết bị, nạp lại trang và so trong Frame.
    let mut core2: PagerCore<CACHE_CAPACITY, RamDisk> = PagerCore::new(RamDisk::new(40));
    let mut page2 = StaticPage;
    let reopen_ok = match core2.begin(0, 1) {
        Begin::Ready { slot } => {
            core2.load(slot, 0, 1, &mut page2).is_ok()
                && page_byte(slot, 0) == b'K'
                && page_byte(slot, 7) == b'T'
        }
        Begin::Hit { slot } => page_byte(slot, 0) == b'K',
        _ => false,
    };
    let ok = wrote
        && backend_ok
        && reopen_ok
        && before.dirty_marks == 0
        && dirty_before_flush.dirty_now == 1
        && after.dirty_now == 0
        && after.writes == SECTORS_PER_PAGE as u64
        && after.flushes == 1
        && after.dirty_marks == 1
        && sink_pages == 1;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# fs: writeback dirty_marks={}->{} dirty_now={} writes={} flushes={} \
             wrote={wrote} backend_ok={backend_ok} reopen_ok={reopen_ok} const:page={PAGE_SIZE} const:sector={BLOCK_SIZE}",
            before.dirty_marks, after.dirty_marks, after.dirty_now, after.writes, after.flushes
        );
    }
    put(
        ok,
        66,
        "fs dirty flush write-back",
        fails,
        "dirty/flush/writes/backend không đúng",
    );
}

// --------------------------------------------------------- TAP 67: journal replay

/// Chạy `setup` (không giới hạn) rồi `op` với `limit` lệnh ghi sống sót; trả số lệnh ghi.
/// "Crash" = gọi `remount` sau đó (journal replay từ ảnh đĩa tiền tố).
#[inline(never)]
fn crash_run(fs: &mut Fs<RamDisk>, limit: Option<usize>, op: fn(&mut Fs<RamDisk>)) -> usize {
    format_disk();
    remount(fs);
    setup_committed(fs);
    arm(limit.unwrap_or(usize::MAX));
    op(fs);
    let (writes, _dropped) = disarm();
    writes
}

/// Đọc thẳng một block từ thiết bị (bỏ qua FS/cache) — bằng chứng độc lập với lời khai của FS.
fn raw_block(dev: &mut RamDisk, lba: u32) -> [u8; BLOCK_SIZE] {
    let mut block = [0u8; BLOCK_SIZE];
    dev.read(lba, &mut block).expect("raw read");
    block
}

/// Nội dung `/log` trên **đĩa thô**: `old` / `new` / `torn`.
fn raw_content(dev: &mut RamDisk, lba: u32) -> &'static str {
    let block = raw_block(dev, lba);
    if &block[..7] == b"NEWDATA" {
        "new"
    } else if &block[..3] == b"OLD" {
        "old"
    } else {
        "torn"
    }
}

#[inline(never)]
fn fs_journal(fs: &mut Fs<RamDisk>, fails: &mut usize) {
    let writes = crash_run(fs, None, crashing_write);
    let commit_prefix = writes.saturating_sub(3);
    let log_block = {
        let log = fs.lookup(b"/log", 0).expect("lookup /log");
        fs.inode_blocks_raw(log).expect("inode /log")[0]
    };
    let issued = crash_run(fs, Some(commit_prefix), crashing_write);
    remount(fs);
    let replay = fs.replayed();
    let (state, size) = log_state(fs);
    let keep = keep_ok(fs);
    let report = fs.fsck_repair().expect("fsck_repair sau replay");
    let fsck_ok = fs.fsck().is_ok() && report.found == 0;
    // Ca âm CRC (D2-108B): commit record sống nhưng **thân log** bị ghi rác ⇒ replay từ chối.
    // Đo thêm **đĩa thô**: block dữ liệu của /log phải còn nội dung CŨ (không áp dụng gì).
    let _ = crash_run(fs, Some(commit_prefix), crashing_write);
    let junk = [0xffu8; BLOCK_SIZE];
    let mut dev = RamDisk::new(DISK_BLOCKS);
    dev.write(JOURNAL_DATA_START, &junk)
        .expect("làm hỏng journal");
    remount(fs);
    let replay_bad = fs.replayed();
    let (state_bad, _size_bad) = log_state(fs);
    let keep_bad = keep_ok(fs);
    let fsck_bad = fs.fsck().is_ok();
    let mut dev = RamDisk::new(DISK_BLOCKS);
    let crc_disk = raw_content(&mut dev, log_block);
    // Ca âm **ĐẢO THỨ TỰ** (D2-108B): cache ghi giữ lệnh chưa flush; "chết" ngay sau commit
    // record ⇒ chỉ commit record xuống đĩa. Có flush-trước-commit-record, thân log đã bền ⇒
    // replay **áp dụng** (new) dù target chưa ghi (đĩa thô = old). Bỏ flush ⇒ replay từ chối
    // (2) và thao tác mất — mutation đã đo: TAP đỏ (`JOURNAL/kt71-red-reorder.log`).
    format_disk();
    remount(fs);
    setup_committed(fs);
    arm_reorder(commit_prefix);
    crashing_write(fs);
    let kept = crash_keep_last();
    let (reorder_writes, _dropped) = disarm();
    let mut dev = RamDisk::new(DISK_BLOCKS);
    let pre = raw_content(&mut dev, log_block);
    remount(fs);
    let replay_r = fs.replayed();
    let (state_r, size_r) = log_state(fs);
    let keep_r = keep_ok(fs);
    let report_r = fs.fsck_repair().expect("fsck_repair sau crash đảo thứ tự");
    let fsck_r = fs.fsck().is_ok() && report_r.found == 0;
    let mut dev = RamDisk::new(DISK_BLOCKS);
    let post = raw_content(&mut dev, log_block);
    let ok = replay == 1
        && state == 1
        && keep
        && fsck_ok
        && replay_bad == 2
        && state_bad == 0
        && keep_bad
        && fsck_bad
        && crc_disk == "old"
        && issued == writes
        && kept
        && reorder_writes >= commit_prefix
        && pre == "old"
        && replay_r == 1
        && state_r == 1
        && size_r == size
        && keep_r
        && fsck_r
        && post == "new";
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# fs: journal writes={writes} issued={issued} commit_prefix={commit_prefix} \
             (=writes-3: 2 target + 1 clear) replay={replay} new={state} size={size} \
             keep={keep} fsck={fsck_ok} crc_reject={replay_bad} crc_applied={state_bad} \
             crc_disk={crc_disk} reorder_writes={reorder_writes} kept={kept} pre={pre} \
             replay_r={replay_r} new_r={state_r} post={post} keep_r={keep_r}"
        );
    }
    put(
        ok,
        67,
        "fs journal replay sau crash",
        fails,
        "replay/CRC/đảo thứ tự không đúng",
    );
}

// ------------------------------------------------------------ TAP 68: 7 pha crash

#[inline(never)]
fn fs_crash7(fs: &mut Fs<RamDisk>, fails: &mut usize) {
    let writes = crash_run(fs, None, crashing_write);
    let mut phases = [0usize; 7];
    phases[0] = 0;
    phases[1] = writes / 4;
    phases[2] = writes / 2;
    phases[3] = writes.saturating_sub(3);
    phases[4] = writes.saturating_sub(2);
    phases[5] = writes.saturating_sub(1);
    phases[6] = writes;
    let mut old = 0u32;
    let mut new = 0u32;
    let mut bad = 0u32;
    let mut consistent = 0u32;
    let mut first_new = usize::MAX;
    let mut last_old = 0usize;
    let mut replays = 0u32;
    let mut index = 0usize;
    while index < 7 {
        let phase = phases[index];
        let _ = crash_run(fs, Some(phase), crashing_write);
        remount(fs);
        let replay = fs.replayed();
        if replay != 0 {
            replays += 1;
        }
        let keep = keep_ok(fs);
        let (state, size) = log_state(fs);
        let report = fs.fsck_repair().expect("fsck_repair");
        let clean = fs.fsck().is_ok() && report.found == 0 && keep;
        if state == 1 {
            new += 1;
            if phase < first_new {
                first_new = phase;
            }
        } else if state == 0 {
            old += 1;
            if phase > last_old {
                last_old = phase;
            }
        } else {
            bad += 1;
        }
        if clean {
            consistent += 1;
        }
        {
            let mut w = PlatformWriter(&PLATFORM);
            let name = if state == 1 {
                "new"
            } else if state == 0 {
                "old"
            } else {
                "torn"
            };
            let _ = writeln!(
                w,
                "# fs: crash pha={index} writes={phase} replay={replay} content={name} size={size} \
                 found={} keep={keep} clean={clean}",
                report.found
            );
        }
        index += 1;
    }
    let atomic = old + new == 7 && last_old < first_new && first_new != usize::MAX;
    let ok = bad == 0 && consistent == 7 && atomic && replays >= 1 && writes >= 6;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# fs: crash7 phases=7 writes={writes} old={old} new={new} torn={bad} consistent={consistent} \
             threshold={first_new} last_old={last_old} atomic={} replays={replays}",
            u32::from(atomic)
        );
    }
    put(
        ok,
        68,
        "fs crash 7 pha nhat quan",
        fails,
        "có pha không nhất quán/không nguyên tử",
    );
}

// ------------------------------------------------------- TAP 69: fsck phát hiện + sửa

fn write_u16(dev: &mut RamDisk, lba: u32, offset: usize, value: u16) {
    let mut block = [0u8; BLOCK_SIZE];
    dev.read(lba, &mut block).expect("read block");
    block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    dev.write(lba, &block).expect("write block");
}

fn write_u32(dev: &mut RamDisk, lba: u32, offset: usize, value: u32) {
    let mut block = [0u8; BLOCK_SIZE];
    dev.read(lba, &mut block).expect("read block");
    block[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    dev.write(lba, &block).expect("write block");
}

fn set_bitmap(dev: &mut RamDisk, block: u32) {
    let byte = block as usize / 8;
    let lba = BITMAP_START + (byte / BLOCK_SIZE) as u32;
    let offset = byte % BLOCK_SIZE;
    let bit = 1u8 << (block % 8);
    let mut data = [0u8; BLOCK_SIZE];
    dev.read(lba, &mut data).expect("read bitmap");
    data[offset] |= bit;
    dev.write(lba, &data).expect("write bitmap");
}

#[inline(never)]
fn fs_fsck(fs: &mut Fs<RamDisk>, fails: &mut usize) {
    format_disk();
    remount(fs);
    let keep = fs.create(b"/keep", 0, INODE_FILE).expect("/keep");
    fs.write(keep, 0, b"KEEP", 0).expect("write /keep");
    let used_sound = fs.usage().0;
    let mut dev = RamDisk::new(DISK_BLOCKS);
    // (1) block mồ côi: bit 40 bật trong bitmap, không inode nào tham chiếu.
    set_bitmap(&mut dev, 40);
    // (2) block đếm sai: inode /keep trỏ thêm block 45 NGOÀI size (và 45 được đánh dấu dùng).
    set_bitmap(&mut dev, 45);
    let inode_lba = INODE_START + ((keep as usize - 1) / 8) as u32;
    let inode_off = ((keep as usize - 1) % 8) * 64;
    write_u32(&mut dev, inode_lba, inode_off + 16 + 4, 45);
    // (3) inode không tham chiếu: chọn slot inode trống sau inode /keep.
    let mut orphan_ino = 0u16;
    let mut candidate = 2usize;
    while candidate < MAX_INODES {
        let lba = INODE_START + (candidate / 8) as u32;
        let off = (candidate % 8) * 64;
        let mut block = [0u8; BLOCK_SIZE];
        dev.read(lba, &mut block).expect("read inode table");
        if block[off] == 0 {
            orphan_ino = (candidate + 1) as u16;
            write_u32(&mut dev, lba, off + 8, 0); // size = 0
            write_u16(&mut dev, lba, off + 4, 1); // nlink = 1
            block[off] = INODE_FILE;
            dev.write(lba, &block).expect("write orphan inode");
            break;
        }
        candidate += 1;
    }
    // (4) link count sai: /keep ghi 5 liên kết nhưng chỉ có 1.
    write_u16(&mut dev, inode_lba, inode_off + 4, 5);
    remount(fs);
    let used_bad = fs.usage().0;
    let detect_first = fs.fsck().is_err();
    let report = fs.fsck_repair().expect("fsck_repair");
    let fsck_after = fs.fsck().is_ok();
    let second = fs.fsck_repair().expect("fsck_repair lần 2");
    let used_after = fs.usage().0;
    let mut out = [0u8; 4];
    let data_ok = fs.read(keep, 0, &mut out, 0).unwrap_or(0) == 4 && &out == b"KEEP";
    // **Ca DƯƠNG cho `unfixed`** (D2-108B): hai inode cùng trỏ MỘT block — fsck **không** sửa mù
    // (không biết ai đúng) ⇒ `unfixed>=1`, `double_blocks>=1`, và `fsck()` vẫn đỏ (báo thật).
    format_disk();
    remount(fs);
    let fa = fs.create(b"/a", 0, INODE_FILE).expect("/a");
    fs.write(fa, 0, b"AAAA", 0).expect("write /a");
    let fb = fs.create(b"/b", 0, INODE_FILE).expect("/b");
    fs.write(fb, 0, b"BBBB", 0).expect("write /b");
    let fb_block = fs.inode_blocks_raw(fb).expect("/b inode")[0];
    let fa_lba = INODE_START + ((fa as usize - 1) / 8) as u32;
    let fa_off = ((fa as usize - 1) % 8) * 64;
    let mut raw_dev = RamDisk::new(DISK_BLOCKS);
    write_u32(&mut raw_dev, fa_lba, fa_off + 16, fb_block);
    remount(fs);
    let report2 = fs.fsck_repair().expect("fsck_repair ca hai chủ");
    let unfixed_pos = report2.unfixed >= 1 && report2.double_blocks >= 1;
    let second_bad = fs.fsck().is_err();
    let _ = report2.fixed;
    let ok = detect_first
        && report.found >= 4
        && report.fixed == report.found
        && report.unfixed == 0
        && report.orphan_blocks >= 1
        && report.orphan_inodes >= 1
        && report.bad_nlink >= 1
        && report.bad_blocks >= 1
        && fsck_after
        && second.found == 0
        && used_after == used_sound
        && data_ok
        && orphan_ino != 0
        && unfixed_pos
        && second_bad;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# fs: fsck detect={} found={} fixed={} unfixed={} orphan_blocks={} orphan_inodes={} \
             bad_nlink={} bad_blocks={} used={used_sound}->{used_bad}->{used_after} keep={data_ok} \
             fsck_ok={fsck_after} second={} unfixed_pos={unfixed_pos} unfixed2={} \
             double_blocks2={} second_bad={second_bad} const:disk_blocks={DISK_BLOCKS}",
            u32::from(detect_first),
            report.found,
            report.fixed,
            report.unfixed,
            report.orphan_blocks,
            report.orphan_inodes,
            report.bad_nlink,
            report.bad_blocks,
            second.found,
            report2.unfixed,
            report2.double_blocks
        );
        let _ = writeln!(w, "# fs: stack end used={}", stack_used());
    }
    put(
        ok,
        69,
        "fs fsck phat hien va sua",
        fails,
        "fsck không phát hiện/sửa đủ bốn loại",
    );
}
