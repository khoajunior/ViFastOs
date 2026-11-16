// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-74** — bảng **dải VA user đã map** của từng ô tác vụ (`DebugPutStr` kiểm con trỏ theo
//! đúng VSpace đang chạy, không dùng bảng của lượt nạp trước).
//!
//! Tách khỏi [`crate::user`] để giữ luật cấu trúc **R1** (≤ 1.500 dòng/tệp); ngữ nghĩa **không**
//! đổi — `crate::user` dùng lại qua `use crate::user_ranges::UserRanges`.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Số dải VA user đọc được (`DebugPutStr`): mỗi `PT_LOAD` một dải + stack + trang IPC buffer.
const MAX_USER_RANGES: usize = vifast_elf::MAX_LOAD_SEGMENTS + 2;

/// Bảng dải VA user **đã map** (ghi lúc nạp, đọc trong syscall) — thay cho `walk` PTE sau khi
/// `satp` trỏ root user (root user **không** có physmap/high alias, xem
/// [`crate::obj::copy_kernel_low_root`]). Mỗi dải là `[base, end)` đã làm tròn trang.
pub(crate) struct UserRanges {
    count: AtomicU32,
    base: [AtomicU64; MAX_USER_RANGES],
    end: [AtomicU64; MAX_USER_RANGES],
}

impl UserRanges {
    pub(crate) const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            base: [const { AtomicU64::new(0) }; MAX_USER_RANGES],
            end: [const { AtomicU64::new(0) }; MAX_USER_RANGES],
        }
    }

    pub(crate) fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }

    /// Chụp **toàn bộ** bảng dải sang `dst` (dùng cho `exec`: lưu trước, khôi phục nếu nạp hỏng —
    /// `reset()` ở `load_init_slot` chạy **trước** khi ảnh mới chắc nạp được, nên lỗi muộn phải
    /// trả lại bảng dải của ảnh cũ).
    pub(crate) fn snapshot_into(&self, dst: &UserRanges) {
        let count = self.count.load(Ordering::Acquire) as usize;
        for index in 0..count {
            dst.base[index].store(self.base[index].load(Ordering::Relaxed), Ordering::Relaxed);
            dst.end[index].store(self.end[index].load(Ordering::Relaxed), Ordering::Relaxed);
        }
        dst.count.store(count as u32, Ordering::Release);
    }

    pub(crate) fn push(&self, base: u64, end: u64) -> bool {
        let index = self.count.load(Ordering::Relaxed) as usize;
        if index >= MAX_USER_RANGES || end <= base {
            return false;
        }
        self.base[index].store(base, Ordering::Relaxed);
        self.end[index].store(end, Ordering::Relaxed);
        self.count.store(index as u32 + 1, Ordering::Release);
        true
    }

    /// `[ptr, ptr+len)` nằm trọn trong **một** dải đã map (không vắt qua hai dải).
    pub(crate) fn contains(&self, ptr: u64, len: u64) -> bool {
        if len == 0 {
            return true;
        }
        let Some(last) = ptr.checked_add(len - 1) else {
            return false;
        };
        let count = self.count.load(Ordering::Acquire) as usize;
        for index in 0..count {
            let base = self.base[index].load(Ordering::Relaxed);
            let end = self.end[index].load(Ordering::Relaxed);
            if ptr >= base && last < end {
                return true;
            }
        }
        false
    }
}
