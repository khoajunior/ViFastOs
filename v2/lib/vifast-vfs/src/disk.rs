// SPDX-License-Identifier: MIT OR Apache-2.0
//! Backend **RAM** thay thế được (KT-70): cài `vifast_fs::BlockDev` (đọc/ghi sector).
//!
//! Vì sao **không** tạo trait `BlockDev` mới: `v2/lib/vifast-fs` đã có đúng trait đó (đọc/ghi
//! sector 512 B + `block_count`) và là nền của KT-71 (journal). Pager chỉ cần **cài** nó, nên
//! KT-72 (virtio-blk) sẽ thêm **một impl mới** — không sửa pager (đúng yêu cầu task).
//!
//! `RamDev` dùng `AtomicU64` (interior mutability) nên đặt được trong `static` **không cần
//! `unsafe`** (luật R5/D2-17). Ở ảnh U-mode, pager **không** dùng `RamDev` trong `.bss` (ảnh chỉ
//! có 28 KiB user): kernel cấp 5 Frame, pager map tại `VA_DISK` và cài `BlockDev` trên VA
//! (`MappedDisk` trong `init/pager_server_user.rs`) — cùng một trait, backend thay thế được.

use core::sync::atomic::{AtomicU64, Ordering};

use vifast_fs::{BlockDev, FsError, BLOCK_SIZE};

use crate::layout::{byte_at, sector_page, DISK_SECTORS, SECTOR_SIZE};

/// Số byte của thiết bị RAM.
pub const DISK_BYTES: usize = DISK_SECTORS as usize * BLOCK_SIZE;
/// Số word 64 bit của thiết bị RAM.
const DISK_WORDS: usize = DISK_BYTES / 8;

/// Thiết bị RAM 20 KiB (40 sector = 5 Frame 4 KiB), ghi/đọc theo sector 512 B. `seed` dùng
/// [`crate::byte_at`] để host test và pager U-mode có **cùng** mẫu dữ liệu.
pub struct RamDev {
    words: [AtomicU64; DISK_WORDS],
}

impl RamDev {
    /// Thiết bị rỗng (mọi byte 0) — `const` để đặt trong `static`.
    pub const fn new() -> Self {
        Self {
            words: [const { AtomicU64::new(0) }; DISK_WORDS],
        }
    }

    /// Nạp dữ liệu mẫu cho **mọi** sector thuộc file mẫu; sector ngoài file giữ 0.
    pub fn seed(&self) {
        for lba in 0..DISK_SECTORS {
            let mut bytes = [0u8; BLOCK_SIZE];
            if let Some((file, page, sector_in_page)) = sector_page(lba) {
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = byte_at(file, page, sector_in_page as usize * SECTOR_SIZE + i);
                }
            }
            for (index, word) in bytes.as_chunks::<8>().0.iter().enumerate() {
                let value = u64::from_le_bytes(*word);
                self.words[lba as usize * (BLOCK_SIZE / 8) + index].store(value, Ordering::Relaxed);
            }
        }
    }

    fn check(&self, lba: u32) -> Result<(), FsError> {
        if lba >= DISK_SECTORS {
            return Err(FsError::Invalid);
        }
        Ok(())
    }

    fn read_sector(&self, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        self.check(lba)?;
        let base = lba as usize * (BLOCK_SIZE / 8);
        for (index, chunk) in out.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            let value = self.words[base + index].load(Ordering::Relaxed);
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        Ok(())
    }

    fn write_sector(&self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        self.check(lba)?;
        let base = lba as usize * (BLOCK_SIZE / 8);
        for (index, chunk) in data.as_chunks::<8>().0.iter().enumerate() {
            let value = u64::from_le_bytes(*chunk);
            self.words[base + index].store(value, Ordering::Relaxed);
        }
        Ok(())
    }
}

impl Default for RamDev {
    fn default() -> Self {
        Self::new()
    }
}

/// Tham chiếu tới [`RamDev`] cài [`BlockDev`] (`&mut self` theo trait, dữ liệu thật do atomic).
pub struct DevRef<'a>(pub &'a RamDev);

impl BlockDev for DevRef<'_> {
    fn read(&mut self, lba: u32, out: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError> {
        self.0.read_sector(lba, out)
    }

    fn write(&mut self, lba: u32, data: &[u8; BLOCK_SIZE]) -> Result<(), FsError> {
        self.0.write_sector(lba, data)
    }

    fn block_count(&self) -> u32 {
        DISK_SECTORS
    }
}
