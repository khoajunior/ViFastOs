// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pool slot CNode trong `.bss` (`no_std`, không `alloc`): bump + free-list hole nhỏ.
//!
//! Mỗi cap chiếm **một** `u32` (id cap, `0` = rỗng) trong pool; CNode tiêu `slots` word.
//! Pool **không** nằm trong arena Untyped (byte arena vẫn được Untyped kế toán đủ
//! `cnode_base + slots*48`), nợ K3b: đưa slot vào arena vật lý khi nối vào kernel (KT-61).
//! Free-list để `Revoke` Untyped / finalise CNode trả lại word — cần cho vòng
//! `Retype → Revoke` lặp (KT-09) không rò dần pool.

use crate::MAX_HOLES;

/// Một khoảng word tự do (đã gộp, sắp theo `off`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hole {
    off: usize,
    len: usize,
}

/// Pool word cap cho CNode.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlotPool {
    words: [u32; crate::MAX_SLOT_WORDS],
    holes: [Hole; MAX_HOLES],
    n_holes: u8,
    cursor: usize,
}

impl SlotPool {
    /// Pool rỗng.
    pub(crate) const EMPTY: Self = Self {
        words: [0u32; crate::MAX_SLOT_WORDS],
        holes: [Hole { off: 0, len: 0 }; MAX_HOLES],
        n_holes: 0,
        cursor: 0,
    };

    /// Có cấp được `n` word liền không (không đổi trạng thái).
    pub(crate) fn can_alloc(&self, n: usize) -> bool {
        if n == 0 || n > crate::MAX_SLOT_WORDS {
            return false;
        }
        for i in 0..self.n_holes as usize {
            if self.holes[i].len >= n {
                return true;
            }
        }
        self.cursor
            .checked_add(n)
            .is_some_and(|end| end <= crate::MAX_SLOT_WORDS)
    }

    /// Cấp `n` word liền; trả offset trong pool. `None` khi hết chỗ.
    pub(crate) fn alloc(&mut self, n: usize) -> Option<usize> {
        if n == 0 || n > crate::MAX_SLOT_WORDS {
            return None;
        }
        for i in 0..self.n_holes as usize {
            let h = self.holes[i];
            if h.len < n {
                continue;
            }
            if h.len == n {
                Self::remove(&mut self.holes, &mut self.n_holes, i);
            } else {
                self.holes[i] = Hole {
                    off: h.off + n,
                    len: h.len - n,
                };
            }
            return Some(h.off);
        }
        let base = self.cursor;
        let end = base.checked_add(n)?;
        if end > crate::MAX_SLOT_WORDS {
            return None;
        }
        self.cursor = end;
        Some(base)
    }

    /// Trả `n` word từ `base` (gộp hole + rewind watermark nếu chạm đuôi).
    pub(crate) fn free(&mut self, base: usize, n: usize) {
        if n == 0 || base >= crate::MAX_SLOT_WORDS {
            return;
        }
        let len = n.min(crate::MAX_SLOT_WORDS - base);
        if (self.n_holes as usize) >= MAX_HOLES {
            // Hết chỗ ghi hole: bỏ (rò có biên), không panic trong kernel.
            debug_assert!(false, "SlotPool: quá MAX_HOLES hole");
            return;
        }
        // Chèn theo thứ tự tăng dần.
        let at = {
            let mut at = self.n_holes as usize;
            for i in 0..self.n_holes as usize {
                if self.holes[i].off > base {
                    at = i;
                    break;
                }
            }
            at
        };
        self.holes.copy_within(at..self.n_holes as usize, at + 1);
        self.holes[at] = Hole { off: base, len };
        self.n_holes += 1;
        Self::merge(&mut self.holes, &mut self.n_holes);
        while let Some(i) = (0..self.n_holes as usize)
            .find(|i| self.holes[*i].off + self.holes[*i].len == self.cursor)
        {
            let h = self.holes[i];
            Self::remove(&mut self.holes, &mut self.n_holes, i);
            self.cursor = h.off;
        }
    }

    fn remove(holes: &mut [Hole; MAX_HOLES], n: &mut u8, index: usize) {
        let count = *n as usize;
        holes.copy_within(index + 1..count, index);
        *n -= 1;
        holes[count - 1] = Hole { off: 0, len: 0 };
    }

    fn merge(holes: &mut [Hole; MAX_HOLES], n: &mut u8) {
        let mut i = 0usize;
        while i + 1 < *n as usize {
            let a = holes[i];
            let b = holes[i + 1];
            if a.off + a.len >= b.off {
                let end = (a.off + a.len).max(b.off + b.len);
                holes[i] = Hole {
                    off: a.off,
                    len: end - a.off,
                };
                Self::remove(holes, n, i + 1);
            } else {
                i += 1;
            }
        }
    }

    /// Đọc word thứ `i` của vùng bắt đầu `base`.
    pub(crate) fn get(&self, base: usize, i: usize) -> u32 {
        self.words.get(base + i).copied().unwrap_or(0)
    }

    /// Ghi word thứ `i` của vùng bắt đầu `base`.
    pub(crate) fn set(&mut self, base: usize, i: usize, value: u32) {
        if let Some(w) = self.words.get_mut(base + i) {
            *w = value;
        }
    }

    /// Xoá `n` word (CNode chết: slot không giữ cap nào).
    pub(crate) fn fill_zero(&mut self, base: usize, n: usize) {
        let end = base.saturating_add(n).min(crate::MAX_SLOT_WORDS);
        if base < end {
            for w in &mut self.words[base..end] {
                *w = 0;
            }
        }
    }
}
