// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-75 — **đối chứng host** cho `test-persist` bảy pha (cùng ngữ nghĩa với ảnh QEMU
//! `ktest-persist`, nhưng chạy trong một tiến trình: “reboot” = **drop** `Fs` rồi mount lại từ
//! **cùng** ảnh đĩa; RAM của lần trước không còn).
//!
//! Vì sao có tệp này (bên cạnh QEMU):
//!
//! 1. cùng bảng pha được kiểm **hai** lần bằng hai cách cài đặt **độc lập** (Rust host ↔ Rust
//!    kernel ↔ Python `persist_host.py`) ⇒ lệch công thức là đỏ ngay, không phải “tin lời khai”;
//! 2. **ranh giới `flush`** kiểm được ở mức thư viện: `Volatile` là cache ghi-bay-hơi mô phỏng
//!    (QEMU ghi thẳng file ảnh nên **không** tự làm mất dữ liệu). `honor_flush=true` ⇒ `fsync`
//!    đẩy xuống “đĩa” ⇒ mount lại **thấy**; `honor_flush=false` ⇒ cache bay hơi ⇒ mount lại
//!    **không thấy**, và ảnh đĩa **không đổi byte nào** — đúng “cắt điện không flush”.
//!
//! Thiết bị là `Vec<u8>` trong bộ nhớ; không sửa `vifast-fs`, không `unsafe`.

use std::vec::Vec;

use vifast_fs::*;

/// Công thức payload — **phải** trùng `payload_byte` của
/// `kernel/vifast-kbin-riscv64/src/ktest_persist.rs` và `test/boot/persist_host.py`.
fn payload(phase: u8, size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| ((phase as usize * 37 + index * 13 + (index / BLOCK_SIZE) * 101) & 0xff) as u8)
        .collect()
}

/// Đĩa thô trong bộ nhớ.
struct RamDisk {
    data: Vec<u8>,
}

impl RamDisk {
    fn new() -> Self {
        Self {
            data: std::vec![0u8; TOTAL_BLOCKS as usize * BLOCK_SIZE],
        }
    }

    fn snapshot(&self) -> Vec<u8> {
        self.data.clone()
    }
}

impl BlockDev for RamDisk {
    fn read(&mut self, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        let start = lba as usize * BLOCK_SIZE;
        out.copy_from_slice(&self.data[start..start + BLOCK_SIZE]);
        Ok(())
    }

    fn write(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        let start = lba as usize * BLOCK_SIZE;
        self.data[start..start + BLOCK_SIZE].copy_from_slice(data);
        Ok(())
    }

    fn block_count(&self) -> u32 {
        TOTAL_BLOCKS
    }
}

/// Cache ghi-bay-hơi mô phỏng: `write` vào RAM, `flush` mới đẩy xuống `RamDisk`.
///
/// `honor_flush=true` = thiết bị tôn trọng `FLUSH`; `false` = “cắt điện không flush” (cache mất).
struct Volatile {
    inner: RamDisk,
    honor_flush: bool,
    pending: Vec<(u32, [u8; BLOCK_SIZE])>,
    flushes: u32,
    ignored: u32,
}

impl Volatile {
    fn new(inner: RamDisk, honor_flush: bool) -> Self {
        Self {
            inner,
            honor_flush,
            pending: Vec::new(),
            flushes: 0,
            ignored: 0,
        }
    }

    fn pending_blocks(&self) -> usize {
        self.pending.len()
    }

    fn push(&mut self) -> Result<(), FsError> {
        let pending = core::mem::take(&mut self.pending);
        for (lba, data) in pending {
            self.inner.write(lba, &data)?;
        }
        Ok(())
    }
}

impl BlockDev for Volatile {
    fn read(&mut self, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        for (slot_lba, data) in self.pending.iter() {
            if *slot_lba == lba {
                out.copy_from_slice(data);
                return Ok(());
            }
        }
        self.inner.read(lba, out)
    }

    fn write(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        for (slot_lba, slot_data) in self.pending.iter_mut() {
            if *slot_lba == lba {
                slot_data.copy_from_slice(data);
                return Ok(());
            }
        }
        self.pending.push((lba, *data));
        Ok(())
    }

    fn block_count(&self) -> u32 {
        self.inner.block_count()
    }

    fn flush(&mut self) -> Result<(), FsError> {
        if !self.honor_flush {
            self.ignored += 1;
            return Ok(());
        }
        self.flushes += 1;
        self.push()
    }
}

/// Một pha **ghi** (giống hệt `write_phase` của kernel).
fn apply(phase: u8, fs: &mut Fs<Volatile>) -> Result<usize, FsError> {
    match phase {
        1 => {
            let ino = fs.create(b"/p1.txt", 0, INODE_FILE)?;
            fs.write(ino, 0, &payload(1, 512), 0)
        }
        2 => {
            let ino = fs.lookup(b"/p1.txt", 0)?;
            fs.write(ino, 0, &payload(2, 2048), 0)
        }
        3 => {
            let ino = fs.lookup(b"/p1.txt", 0)?;
            let size = fs.info(ino)?.size as usize;
            fs.write(ino, size, &payload(3, 1024), 0)
        }
        4 => {
            let ino = fs.lookup(b"/p1.txt", 0)?;
            fs.truncate(ino, 0, 0)?;
            fs.write(ino, 0, &payload(4, 700), 0)
        }
        5 => {
            fs.rename(b"/p1.txt", b"/p5.bin", 0)?;
            Ok(0)
        }
        6 => {
            // Xoá rồi **tạo lại cùng tên** — như kernel.
            fs.unlink(b"/p5.bin", 0)?;
            let ino = fs.create(b"/p5.bin", 0, INODE_FILE)?;
            fs.write(ino, 0, &payload(6, 1536), 0)
        }
        _ => {
            let ino = fs.create(b"/p7.bin", 0, INODE_FILE)?;
            fs.write(ino, 0, &payload(7, 144 * BLOCK_SIZE), 0)
        }
    }
}

/// Trạng thái **kỳ vọng** sau mỗi pha: các tệp `(path, bytes)` + các tên phải **vắng**.
type Expected = (Vec<(&'static [u8], Vec<u8>)>, Vec<&'static [u8]>);

fn expected(phase: u8) -> Expected {
    let p2 = payload(2, 2048);
    let files: Vec<(&'static [u8], Vec<u8>)> = match phase {
        1 => vec![(b"/p1.txt", payload(1, 512))],
        2 => vec![(b"/p1.txt", p2)],
        3 => {
            let mut content = p2.clone();
            content.extend_from_slice(&payload(3, 1024));
            vec![(b"/p1.txt", content)]
        }
        4 => vec![(b"/p1.txt", payload(4, 700))],
        5 => vec![(b"/p5.bin", payload(4, 700))],
        6 => vec![(b"/p5.bin", payload(6, 1536))],
        _ => vec![
            (b"/p5.bin", payload(6, 1536)),
            (b"/p7.bin", payload(7, 144 * BLOCK_SIZE)),
        ],
    };
    let absent: Vec<&'static [u8]> = match phase {
        5..=7 => vec![b"/p1.txt"],
        _ => Vec::new(),
    };
    (files, absent)
}

/// Đọc + so **từng byte** một tệp trong `Fs` đã mount.
fn check(fs: &mut Fs<Volatile>, path: &[u8], want: &[u8]) {
    let ino = fs
        .lookup(path, 0)
        .unwrap_or_else(|error| panic!("lookup {:?}: {error:?}", String::from_utf8_lossy(path)));
    let info = fs.info(ino).expect("info");
    assert_eq!(info.size as usize, want.len(), "size của tệp");
    let mut got = vec![0u8; want.len()];
    let count = fs.read(ino, 0, &mut got, 0).expect("read");
    assert_eq!(count, want.len());
    assert_eq!(got, want, "nội dung tệp");
}

/// **Bảy pha sống qua “reboot”** (drop `Fs` + mount lại) — cùng bảng với QEMU.
#[test]
fn bay_pha_song_qua_reboot_host() {
    let mut disk = RamDisk::new();
    // Pha 1 cần format trước (boot đầu của kernel làm đúng vậy).
    {
        let mut disk_mut = RamDisk {
            data: disk.snapshot(),
        };
        format(&mut disk_mut).expect("format");
        disk = disk_mut;
    }
    let mut pstate = 0u32;
    for phase in 1..=7u8 {
        // ---- “boot ghi”: mount lại từ **cùng** ảnh đĩa, áp pha, fsync, drop (RAM mất)
        let mut fs = Fs::mount(Volatile::new(
            RamDisk {
                data: disk.snapshot(),
            },
            true,
        ))
        .expect("mount ghi");
        let _wrote = apply(phase, &mut fs).expect("áp pha");
        if phase == 7 {
            let ino = fs.lookup(b"/p7.bin", 0).expect("lookup p7");
            let raw = fs.inode_blocks_raw(ino).expect("bảng block");
            assert_ne!(raw[V4_SINGLE_INDIRECT_INDEX], 0, "single-indirect phải có");
            assert_ne!(raw[V4_DOUBLE_INDIRECT_INDEX], 0, "double-indirect phải có");
        }
        // Nhật ký pha: 4 B/bản ghi, append — như kernel.
        let ino = match fs.lookup(b"/pstate", 0) {
            Ok(ino) => ino,
            Err(_) => fs.create(b"/pstate", 0, INODE_FILE).expect("create pstate"),
        };
        let size = fs.info(ino).expect("info pstate").size as usize;
        fs.write(ino, size, &[phase, 1, 1, 0], 0).expect("record");
        fs.fsync().expect("fsync");
        let dev = fs.dev();
        assert_eq!(dev.pending_blocks(), 0, "pha dương phải đẩy hết cache");
        assert!(dev.flushes > 0, "pha dương phải gửi FLUSH");
        let pushed = fs.into_dev();
        disk = pushed.inner;
        pstate += 1;
        // ---- “reboot đọc lại”: mount mới từ ảnh đĩa, không còn cache cũ
        let mut fs = Fs::mount(Volatile::new(
            RamDisk {
                data: disk.snapshot(),
            },
            true,
        ))
        .expect("mount đọc");
        let (files, absent) = expected(phase);
        for (path, want) in files {
            check(&mut fs, path, &want);
        }
        for path in absent {
            assert!(fs.lookup(path, 0).is_err(), "tên phải vắng sau pha {phase}");
        }
        // Nhật ký pha **đọc lại** (action 2) — cùng định dạng với kernel.
        let ino = fs.lookup(b"/pstate", 0).expect("pstate");
        let size = fs.info(ino).expect("info").size as usize;
        fs.write(ino, size, &[phase, 2, 1, 0], 0)
            .expect("record verify");
        fs.fsync().expect("fsync record");
        pstate += 1;
        let bytes = fs.info(ino).expect("info").size as usize;
        assert_eq!(bytes, 4 * pstate as usize, "số bản ghi nhật ký pha");
        fs.fsync().expect("fsync đọc");
        // Bản ghi **đọc lại** phải bền: “boot” sau nhận đúng ảnh đĩa này.
        disk = fs.into_dev().inner;
    }
    // Tổng kết đọc từ **đĩa**: đủ bảy pha ghi + bảy pha đọc lại.
    let mut fs = Fs::mount(Volatile::new(disk, true)).expect("mount cuối");
    let ino = fs.lookup(b"/pstate", 0).expect("pstate cuối");
    let size = fs.info(ino).expect("info").size as usize;
    let mut record = vec![0u8; size];
    fs.read(ino, 0, &mut record, 0).expect("read pstate");
    let writes_ok = record.chunks(4).filter(|entry| entry[1] == 1).count();
    let verifies_ok = record.chunks(4).filter(|entry| entry[1] == 2).count();
    assert_eq!((writes_ok, verifies_ok), (7, 7), "nhật ký pha trên đĩa");
    fs.fsck().expect("fsck sạch");
}

/// **Ca âm `flush`**: cùng thao tác, cùng thiết bị mô phỏng; chỉ khác `flush` có hiệu lực.
///
/// Thiết bị đúng: sau `fsync` + “reboot”, dữ liệu **còn**. Thiết bị bỏ `flush`: cache bay hơi ⇒
/// dữ liệu **mất**, và ảnh đĩa **không đổi byte nào** (không có gì ra “đĩa”).
#[test]
fn flush_la_dieu_kien_ben() {
    let mut disk = RamDisk::new();
    {
        let mut disk_mut = RamDisk {
            data: disk.snapshot(),
        };
        format(&mut disk_mut).expect("format");
        disk = disk_mut;
    }

    // ---- (a) thiết bị TÔN TRỌNG flush
    let mut fs = Fs::mount(Volatile::new(
        RamDisk {
            data: disk.snapshot(),
        },
        true,
    ))
    .expect("mount (a)");
    let ino = fs.create(b"/pneg.txt", 0, INODE_FILE).expect("create (a)");
    fs.write(ino, 0, &payload(9, 600), 0).expect("write (a)");
    fs.fsync().expect("fsync (a)");
    assert_eq!(fs.dev().pending_blocks(), 0);
    let durable = fs.into_dev().inner.data;
    {
        let mut fs = Fs::mount(Volatile::new(
            RamDisk {
                data: durable.clone(),
            },
            true,
        ))
        .expect("mount sau (a)");
        let ino = fs.lookup(b"/pneg.txt", 0).expect("dữ liệu phải còn");
        let mut got = vec![0u8; 600];
        fs.read(ino, 0, &mut got, 0).expect("read (a)");
        assert_eq!(got, payload(9, 600), "nội dung sau reboot");
    }

    // ---- (b) thiết bị BỎ flush (cắt điện giữa chừng)
    let before = disk.snapshot();
    let mut fs = Fs::mount(Volatile::new(
        RamDisk {
            data: disk.snapshot(),
        },
        false,
    ))
    .expect("mount (b)");
    let ino = fs.create(b"/pneg.txt", 0, INODE_FILE).expect("create (b)");
    fs.write(ino, 0, &payload(9, 600), 0).expect("write (b)");
    fs.fsync().expect("fsync (b)");
    let dev = fs.dev();
    assert!(
        dev.pending_blocks() > 0,
        "cache phải còn block chưa đẩy (đây là thứ bị mất khi cắt điện)"
    );
    assert_eq!(dev.flushes, 0, "không được có FLUSH nào tới thiết bị");
    assert!(dev.ignored > 0);
    let lost = fs.into_dev().inner.data;
    assert_eq!(lost, before, "thiết bị không được nhận byte nào");
    let mut fs =
        Fs::mount(Volatile::new(RamDisk { data: lost.clone() }, true)).expect("mount sau (b)");
    assert!(
        fs.lookup(b"/pneg.txt", 0).is_err(),
        "không flush ⇒ sau reboot KHÔNG được thấy dữ liệu"
    );
    fs.fsck().expect("fsck vẫn sạch sau cắt điện");
}

/// Chứng minh `RamDisk`/`Volatile` **không** phải nguồn số: đổi một byte trên “đĩa” phải làm
/// phép đọc của pha **đỏ** (bộ kiểm không rỗng).
#[test]
fn doi_mot_byte_tren_dia_thi_doc_lai_phai_lech() {
    let mut disk = RamDisk::new();
    {
        let mut disk_mut = RamDisk {
            data: disk.snapshot(),
        };
        format(&mut disk_mut).expect("format");
        disk = disk_mut;
    }
    let mut fs = Fs::mount(Volatile::new(
        RamDisk {
            data: disk.snapshot(),
        },
        true,
    ))
    .expect("mount ghi");
    let ino = fs.create(b"/p1.txt", 0, INODE_FILE).expect("create");
    fs.write(ino, 0, &payload(1, 512), 0).expect("write");
    fs.fsync().expect("fsync");
    let mut raw = fs.into_dev().inner.data;
    // Đổi một byte **dữ liệu**: tìm block đầu tiên trong vùng dữ liệu chứa đúng byte đầu payload.
    let mut flipped = false;
    for lba in DATA_START..TOTAL_BLOCKS {
        let start = lba as usize * BLOCK_SIZE;
        if raw[start] == payload(1, 512)[0] {
            raw[start] ^= 0xFF;
            flipped = true;
            break;
        }
    }
    assert!(flipped, "phải tìm được block dữ liệu để phá");
    let mut fs = Fs::mount(Volatile::new(RamDisk { data: raw }, true)).expect("mount đọc");
    let ino = fs.lookup(b"/p1.txt", 0).expect("lookup");
    let mut got = vec![0u8; 512];
    fs.read(ino, 0, &mut got, 0).expect("read");
    assert_ne!(got, payload(1, 512), "phép đọc phải phát hiện byte bị đổi");
}
