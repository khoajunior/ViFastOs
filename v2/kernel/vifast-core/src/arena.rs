// SPDX-License-Identifier: MIT OR Apache-2.0
//! Arena **số học** của một Untyped (`no_std`, không con trỏ, không `unsafe`).
//!
//! Thay `vifast_objmem::OwnedMem`/`OwnedArena` (chỉ có ở tầng sim, feature `heap`) bằng
//! bump allocator trên **offset logic** — đúng đại lượng mà sim công bố:
//! `untyped_used()` = `Arena::used()` của objmem, `object_bytes()` = `(offset, len)`.
//!
//! **Parity byte với sim (KT-16/KT-17):** `OwnedMem::alloc_place_span` cấp
//! `block = max(len, HEADER 16 + body Kind 48 = 64)` với `align = max(align_spec, 8)`.
//! Mọi object lõi của KT-58 có `len ≥ 64` (`Endpoint 64`, `CNode ≥ 112`, `Untyped 4096`) và
//! `align ≥ 16` (16/64/4096) nên `block == len`, `align_eff == align`; vì vậy arena ở đây
//! chỉ cần `offset = align_up(cursor, align)` rồi `cursor = offset + len` là khớp từng byte.
//! `const` assert bên dưới khoá giả thiết đó (nếu spec đổi cỡ nhỏ hơn 64 thì phải sửa).
//!
//! Không có free-list: object chết chỉ lấy lại byte qua `Revoke` Untyped (đặt watermark về
//! 0) — đúng thiết kế §4.2 và sim (`OwnedMem::reset` sau khi finalise mọi body).

/// Cỡ nhỏ nhất của một block trong sim: `HEADER_SIZE 16 + size_of::<Kind>() 48`.
pub(crate) const MIN_BLOCK_BYTES: usize = 64;
/// Căn lề nhỏ nhất trong sim: `max(align_of::<Kind>(), align_of::<ObjHeader>()) = 8`.
pub(crate) const MIN_ALIGN_BYTES: usize = 8;

const _: () = assert!(
    MIN_BLOCK_BYTES == 16 + 48,
    "sim: header 16 B + body Kind 48 B (vifast-objmem::HEADER_SIZE + size_of::<Kind>)"
);

/// Vùng nhớ của một Untyped: `[base, base + capacity)` với watermark `cursor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// Đáy vùng logic (PA của mô hình; với Untyped con = `base_cha + offset_con`).
    pub base: u64,
    /// Cỡ vùng (byte) — `capacity()` của objmem.
    pub capacity: usize,
    /// Watermark — `used()` của objmem; chỉ tiến trong một chuỗi Retype.
    pub cursor: usize,
}

impl Region {
    /// Vùng rỗng (chưa cấp).
    pub const EMPTY: Self = Self {
        base: 0,
        capacity: 0,
        cursor: 0,
    };

    /// Vùng `capacity` byte, đáy `base`, watermark 0.
    pub const fn new(base: u64, capacity: usize) -> Self {
        Self {
            base,
            capacity,
            cursor: 0,
        }
    }

    /// Cấp `len` byte căn `align` (lũy thừa 2); trả offset so với `base`.
    ///
    /// `None` khi hết chỗ — **không** đổi watermark (fail nguyên tử như objmem).
    pub fn alloc(&mut self, len: usize, align: usize) -> Option<usize> {
        debug_assert!(align.is_power_of_two(), "align phải lũy thừa 2");
        debug_assert!(len >= MIN_BLOCK_BYTES, "object lõi phải ≥ block nhỏ nhất");
        debug_assert!(align >= MIN_ALIGN_BYTES, "align hiệu dụng ≥ 8");
        let offset = align_up(self.cursor, align)?;
        let end = offset.checked_add(len)?;
        if end > self.capacity {
            return None;
        }
        self.cursor = end;
        Some(offset)
    }

    /// Untyped `Revoke`: watermark về 0 sau khi mọi object trong vùng đã chết.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Byte còn tự do (không tính byte đệm căn lề đã nằm dưới watermark).
    pub const fn free(&self) -> usize {
        self.capacity.saturating_sub(self.cursor)
    }
}

/// `align_up` tràn ⇒ `None` (fail-closed, không panic).
pub(crate) const fn align_up(value: usize, align: usize) -> Option<usize> {
    match value.checked_add(align - 1) {
        Some(v) => Some(v & !(align - 1)),
        None => None,
    }
}
