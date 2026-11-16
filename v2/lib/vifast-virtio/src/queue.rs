// SPDX-License-Identifier: MIT OR Apache-2.0
//! Split virtqueue (§2.6/§2.7 Virtio 1.0) trên một lát byte — phần **thuần**.
//!
//! Bố cục (offset byte trong vùng ring, tính từ `queue_size`):
//!
//! ```text
//! 0                        desc_off        avail_off                 used_off
//! | descriptor table 16*N | avail: flags2 idx2 ring2*N used_event2 | flags2 idx2 ring8*N avail_event2 |
//! ```
//!
//! Driver **chỉ** được đọc `used` sau khi đọc `used->idx` (một chiều) và **chỉ** được thông báo
//! sau khi ghi xong `avail` (kernel lo phần `fence`/MMIO). Crate này không tự cấp phát, không
//! `unsafe`: mọi thao tác là đọc/ghi byte trên lát có sẵn.

/// Cỡ một descriptor (spec).
pub const DESC_BYTES: usize = 16;
/// Căn lề cứng của vùng virtqueue (driver cấp trang 4 KiB).
pub const VQ_ALIGN: usize = 4096;

/// Cờ descriptor (`VIRTQ_DESC_F_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DescFlags(pub u16);

impl DescFlags {
    /// Không cờ.
    pub const NONE: Self = Self(0);
    /// `NEXT` (bit 0) — `next` là descriptor kế trong chuỗi.
    pub const NEXT: Self = Self(1);
    /// `WRITE` (bit 1) — vùng là **device-writable**.
    pub const WRITE: Self = Self(2);
    /// `INDIRECT` (bit 2) — K6 không dùng.
    pub const INDIRECT: Self = Self(4);

    /// Bit thô.
    pub const fn bits(self) -> u16 {
        self.0
    }
    /// `true` ⇔ có cờ.
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    /// `true` ⇔ vùng device-writable.
    pub const fn writable(self) -> bool {
        self.has(Self::WRITE)
    }
}

/// Một descriptor (4 trường 8/8/4/2 byte theo spec, tổng 16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Desc {
    /// Địa chỉ **vật lý** của vùng đệm.
    pub addr: u64,
    /// Cỡ vùng (byte).
    pub len: u32,
    /// Cờ.
    pub flags: DescFlags,
    /// Descriptor kế (nếu `NEXT`).
    pub next: u16,
}

/// Offset từng phần của ring trong vùng virtqueue — suy từ `queue_size`, không hardcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VqLayout {
    /// Số descriptor (luỹ thừa 2).
    pub queue_size: u16,
    /// Offset bảng descriptor (0).
    pub desc_off: usize,
    /// Offset khối `avail` (sau bảng descriptor, căn 2).
    pub avail_off: usize,
    /// Offset `avail->idx`.
    pub avail_idx_off: usize,
    /// Offset `avail->ring[0]`.
    pub avail_ring_off: usize,
    /// Offset khối `used` (căn 4).
    pub used_off: usize,
    /// Offset `used->idx`.
    pub used_idx_off: usize,
    /// Offset `used->ring[0]`.
    pub used_ring_off: usize,
    /// Byte cuối của ring (không gồm đệm request).
    pub end: usize,
}

impl VqLayout {
    /// Dựng layout cho `queue_size`; `None` nếu cỡ không phải luỹ thừa 2 trong `[2, 256]`.
    pub const fn new(queue_size: u16) -> Option<Self> {
        if queue_size < 2 || queue_size > 256 || !queue_size.is_power_of_two() {
            return None;
        }
        let n = queue_size as usize;
        let desc_off = 0;
        let avail_off = desc_off + n * DESC_BYTES;
        let avail_idx_off = avail_off + 2;
        let avail_ring_off = avail_off + 4;
        let after_avail = avail_ring_off + n * 2 + 2; // + used_event
        let used_off = (after_avail + 3) & !3usize;
        let used_idx_off = used_off + 2;
        let used_ring_off = used_off + 4;
        let end = used_ring_off + n * 8 + 2; // + avail_event
        Some(Self {
            queue_size,
            desc_off,
            avail_off,
            avail_idx_off,
            avail_ring_off,
            used_off,
            used_idx_off,
            used_ring_off,
            end,
        })
    }

    /// Cỡ bảng descriptor (byte).
    pub const fn desc_bytes(&self) -> usize {
        self.queue_size as usize * DESC_BYTES
    }

    /// Cỡ bảng `used` (byte).
    pub const fn used_bytes(&self) -> usize {
        self.queue_size as usize * 8
    }

    /// Tổng byte ring chiếm (không gồm vùng request).
    pub const fn total_bytes(&self) -> usize {
        self.end
    }
}

/// Split queue thao tác trên lát byte sẵn có (driver đặt trong một trang 4 KiB căn lề).
pub struct SplitQueue<'a> {
    mem: &'a mut [u8],
    layout: VqLayout,
}

impl<'a> SplitQueue<'a> {
    /// Bọc `mem` thành queue `queue_size`. `None` nếu cỡ sai hoặc `mem` quá nhỏ (không tự cắt).
    pub fn new(mem: &'a mut [u8], queue_size: u16) -> Option<Self> {
        let layout = VqLayout::new(queue_size)?;
        if mem.len() < layout.total_bytes() {
            return None;
        }
        Some(Self { mem, layout })
    }

    /// Layout đã dùng.
    pub const fn layout(&self) -> VqLayout {
        self.layout
    }

    fn desc_at(&self, index: u16) -> usize {
        self.layout.desc_off + index as usize * DESC_BYTES
    }

    /// Ghi một descriptor (driver → ring). Trả `false` nếu `index` ngoài `queue_size` — **không**
    /// bỏ im lặng (D2-110: chain 3 descriptor cần `queue_size ≥ 3`, driver phải kiểm).
    #[must_use]
    pub fn set_desc(&mut self, index: u16, desc: Desc) -> bool {
        if index >= self.layout.queue_size {
            return false;
        }
        let at = self.desc_at(index);
        self.mem[at..at + 8].copy_from_slice(&desc.addr.to_le_bytes());
        self.mem[at + 8..at + 12].copy_from_slice(&desc.len.to_le_bytes());
        self.mem[at + 12..at + 14].copy_from_slice(&desc.flags.bits().to_le_bytes());
        self.mem[at + 14..at + 16].copy_from_slice(&desc.next.to_le_bytes());
        true
    }

    /// Đọc lại một descriptor (kiểm bằng chứng: `used->id` phải trỏ đúng chuỗi).
    pub fn desc(&self, index: u16) -> Desc {
        let at = self.desc_at(index);
        Desc {
            addr: u64::from_le_bytes(self.mem[at..at + 8].try_into().unwrap_or([0; 8])),
            len: u32::from_le_bytes(self.mem[at + 8..at + 12].try_into().unwrap_or([0; 4])),
            flags: DescFlags(u16::from_le_bytes(
                self.mem[at + 12..at + 14].try_into().unwrap_or([0; 2]),
            )),
            next: u16::from_le_bytes(self.mem[at + 14..at + 16].try_into().unwrap_or([0; 2])),
        }
    }

    /// `avail->idx` hiện tại (số head driver đã publish).
    pub fn avail_idx(&self) -> u16 {
        let at = self.layout.avail_idx_off;
        u16::from_le_bytes([self.mem[at], self.mem[at + 1]])
    }

    /// `avail->ring[slot]` — head thứ `slot` (theo `avail->idx` một chiều).
    pub fn avail_ring_entry(&self, slot: u16) -> u16 {
        let at = self.layout.avail_ring_off
            + (slot as usize & (self.layout.queue_size as usize - 1)) * 2;
        u16::from_le_bytes([self.mem[at], self.mem[at + 1]])
    }

    /// Publish head `head` vào `avail` (tăng `avail->idx` sau khi ghi ring — thứ tự spec).
    pub fn push_avail(&mut self, head: u16) {
        let idx = self.avail_idx();
        let slot = idx & (self.layout.queue_size - 1);
        let at = self.layout.avail_ring_off + slot as usize * 2;
        self.mem[at..at + 2].copy_from_slice(&head.to_le_bytes());
        let next = idx.wrapping_add(1);
        let iat = self.layout.avail_idx_off;
        self.mem[iat..iat + 2].copy_from_slice(&next.to_le_bytes());
    }

    /// `used->idx` — số phần tử thiết bị đã hoàn tất (một chiều).
    pub fn used_idx(&self) -> u16 {
        let at = self.layout.used_idx_off;
        u16::from_le_bytes([self.mem[at], self.mem[at + 1]])
    }

    /// `used->ring[slot]` = `(id, len)` — `id` là head descriptor thiết bị đã xử lý.
    pub fn used_elem(&self, slot: u16) -> (u32, u32) {
        let at =
            self.layout.used_ring_off + (slot as usize & (self.layout.queue_size as usize - 1)) * 8;
        (
            u32::from_le_bytes(self.mem[at..at + 4].try_into().unwrap_or([0; 4])),
            u32::from_le_bytes(self.mem[at + 4..at + 8].try_into().unwrap_or([0; 4])),
        )
    }

    /// **Phía thiết bị** (chỉ dùng cho test/self-check): ghi một phần tử `used` rồi tăng
    /// `used->idx` — mô phỏng đúng thứ tự thiết bị làm để kiểm driver đọc đúng.
    pub fn write_used(&mut self, slot: u16, id: u32, len: u32) {
        let at =
            self.layout.used_ring_off + (slot as usize & (self.layout.queue_size as usize - 1)) * 8;
        self.mem[at..at + 4].copy_from_slice(&id.to_le_bytes());
        self.mem[at + 4..at + 8].copy_from_slice(&len.to_le_bytes());
        let next = self.used_idx().wrapping_add(1);
        let iat = self.layout.used_idx_off;
        self.mem[iat..iat + 2].copy_from_slice(&next.to_le_bytes());
    }
}
