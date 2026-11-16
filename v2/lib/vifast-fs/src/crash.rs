// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-71** — mô hình "crash" để đo **nhất quán sau crash** của journal (7 pha).
//!
//! # Mô hình (ghi rõ để không ai đọc quá lời)
//!
//! Đĩa được giả định **bền theo thứ tự**: sau một điểm crash, các lệnh ghi đã phát ra được
//! giữ lại thành một **tiền tố** của dãy lệnh ghi (`PrefixDisk`), phần đuôi bị mất (cache
//! volatile). Đây là mô hình **đơn giản hoá có chủ đích** — nó không mô phỏng đĩa tái sắp xếp
//! lệnh ghi, nên journal phải tự bảo đảm bằng **flush trước commit record** (write-ahead):
//! hệ quả là "commit record sống ⇒ mọi block journal trước nó sống", và `replay` CRC vẫn từ
//! chối ảnh journal hỏng (ca âm `journal_tu_choi_commit_hong_crc_khong_ap_dung`).
//!
//! `PrefixDisk` **không** dùng `unsafe`, không cấp phát, chạy được cả host lẫn kernel.

use crate::fs::{BlockDev, Fs, FsError};

/// Thiết bị bọc: chỉ áp dụng `limit` lệnh ghi đầu tiên **sau khi `arm`**, phần sau bị mất im
/// lặng (đúng "crash mất cache đuôi", FS không thấy lỗi I/O).
pub struct PrefixDisk<D: BlockDev> {
    inner: D,
    armed: bool,
    limit: usize,
    writes: usize,
    dropped: usize,
}

impl<D: BlockDev> PrefixDisk<D> {
    /// Bọc thiết bị; chưa giới hạn (mọi lệnh ghi được áp dụng cho tới `arm`).
    pub const fn new(inner: D) -> Self {
        Self {
            inner,
            armed: false,
            limit: usize::MAX,
            writes: 0,
            dropped: 0,
        }
    }

    /// Bắt đầu "thao tác được đo": chỉ `limit` lệnh ghi đầu tiên sống sót.
    pub fn arm(&mut self, limit: usize) {
        self.armed = true;
        self.limit = limit;
        self.writes = 0;
        self.dropped = 0;
    }

    /// Số lệnh ghi đã phát ra kể từ `arm`.
    pub fn writes(&self) -> usize {
        self.writes
    }

    /// Số lệnh ghi đã bị mất (phần đuôi sau điểm crash).
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// Lấy lại thiết bị bên trong (ảnh đĩa tại điểm crash).
    pub fn into_inner(self) -> D {
        self.inner
    }
}

impl<D: BlockDev> BlockDev for PrefixDisk<D> {
    fn read(&mut self, lba: u32, out: &mut [u8; crate::consts::BLOCK_SIZE]) -> Result<(), FsError> {
        self.inner.read(lba, out)
    }

    fn write(&mut self, lba: u32, data: &[u8; crate::consts::BLOCK_SIZE]) -> Result<(), FsError> {
        if self.armed {
            self.writes += 1;
            if self.writes > self.limit {
                self.dropped += 1;
                return Ok(());
            }
        }
        self.inner.write(lba, data)
    }

    fn block_count(&self) -> u32 {
        self.inner.block_count()
    }

    fn flush(&mut self) -> Result<(), FsError> {
        self.inner.flush()
    }

    fn read_uncached(
        &mut self,
        lba: u32,
        out: &mut [u8; crate::consts::BLOCK_SIZE],
    ) -> Result<(), FsError> {
        self.inner.read_uncached(lba, out)
    }
}

/// Chạy `setup` (không giới hạn — tạo dữ liệu **đã commit**) rồi `op` trên cùng một FS, với
/// `limit` lệnh ghi sống sót của `op` (`None` = đo đầy đủ, không cắt).
///
/// Trả `(ảnh đĩa tại điểm crash, số lệnh ghi đã phát ra trong op)`.
#[inline(never)]
pub fn run_op<D, S, O>(dev: D, limit: Option<usize>, setup: S, op: O) -> (D, usize)
where
    D: BlockDev,
    S: FnOnce(&mut Fs<PrefixDisk<D>>),
    O: FnOnce(&mut Fs<PrefixDisk<D>>),
{
    // Khởi tạo **tại chỗ** (`empty` + `initialize`): `Fs::mount` trả 13 KiB theo giá trị, trên
    // kernel 64 KiB stack sẽ nhân đôi khung nếu mọi nơi đều dùng nó.
    let mut fs = Fs::empty(PrefixDisk::new(dev));
    if fs.initialize().is_err() {
        panic!("KT-71: mount thiết bị sạch phải thành công");
    }
    setup(&mut fs);
    fs.dev_mut().arm(limit.unwrap_or(usize::MAX));
    op(&mut fs);
    let disk = fs.into_dev();
    let writes = disk.writes();
    (disk.into_inner(), writes)
}
