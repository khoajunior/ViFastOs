// SPDX-License-Identifier: MIT OR Apache-2.0
//! **Page cache** theo `(file_id, page_no)` với trục xuất **LRU** (KT-70).
//!
//! Thuần metadata: mỗi mục giữ `key` + **slot** (slot = Frame của pool); dữ liệu trang nằm trong
//! Frame do chương trình pager sở hữu. Nhờ vậy cùng `(file,page)` ⇒ **một** mục, nhiều mapping
//! (nhiều client) — bất biến "một bản trong cache" kiểm được bằng `keys_unique`.

/// Khoá cache: `(file_id, page_no)`.
pub type Key = (u32, u32);

/// Giá trị slot "không có".
pub const NO_SLOT: u8 = 0xFF;

/// Một mục cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Mục đang dùng?
    pub used: bool,
    /// Khoá `(file,page)`.
    pub key: Key,
    /// Slot Frame (0..N-1).
    pub slot: u8,
    /// Dấu LRU — lớn hơn = mới dùng hơn.
    pub stamp: u64,
}

impl Entry {
    /// Mục rỗng (mốc const).
    pub const EMPTY: Self = Self {
        used: false,
        key: (0, 0),
        slot: NO_SLOT,
        stamp: 0,
    };
}

/// Kết quả chọn slot cho một lần miss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotChoice {
    /// Slot còn trống.
    Free {
        /// Slot Frame.
        slot: u8,
    },
    /// Trục xuất mục LRU — slot của nó được tái dùng.
    Evict {
        /// Slot Frame được tái dùng.
        slot: u8,
        /// Khoá bị trục xuất (caller phải gỡ mapping trước khi ghi đè dữ liệu).
        key: Key,
    },
}

/// Page cache `N` mục (N = ngưỡng thiết kế, không nới).
pub struct PageCache<const N: usize> {
    entries: [Entry; N],
    clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl<const N: usize> PageCache<N> {
    /// Cache rỗng; slot của mục `i` là `i`.
    pub const fn new() -> Self {
        let mut entries = [Entry::EMPTY; N];
        let mut index = 0usize;
        while index < N {
            entries[index].slot = index as u8;
            index += 1;
        }
        Self {
            entries,
            clock: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    fn next_stamp(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        self.clock
    }

    /// Tra một khoá; hit ⇒ cập nhật LRU + đếm `hits`, trả slot.
    pub fn hit(&mut self, key: Key) -> Option<u8> {
        let mut found = None;
        let mut index = 0usize;
        while index < N {
            if self.entries[index].used && self.entries[index].key == key {
                let stamp = self.next_stamp();
                self.entries[index].stamp = stamp;
                found = Some(self.entries[index].slot);
            }
            index += 1;
        }
        if found.is_some() {
            self.hits += 1;
        }
        found
    }

    /// Ghi nhận **miss** và chọn slot: trống trước, hết thì trục xuất mục LRU.
    pub fn begin_miss(&mut self) -> SlotChoice {
        self.misses += 1;
        for entry in self.entries.iter() {
            if !entry.used {
                return SlotChoice::Free { slot: entry.slot };
            }
        }
        let mut victim = 0usize;
        for index in 1..N {
            if self.entries[index].stamp < self.entries[victim].stamp {
                victim = index;
            }
        }
        let entry = self.entries[victim];
        self.entries[victim].used = false;
        self.evictions += 1;
        SlotChoice::Evict {
            slot: entry.slot,
            key: entry.key,
        }
    }

    /// Cài khoá vào slot **sau khi** dữ liệu đã nạp (hoặc sau khi mapping cũ đã gỡ).
    pub fn install(&mut self, key: Key, slot: u8) {
        let stamp = self.next_stamp();
        for entry in self.entries.iter_mut() {
            if entry.slot == slot {
                *entry = Entry {
                    used: true,
                    key,
                    slot,
                    stamp,
                };
                return;
            }
        }
    }

    /// Số mục đang dùng.
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|entry| entry.used).count()
    }

    /// Rỗng?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Slot của một khoá (không đổi LRU/đếm).
    pub fn slot_of(&self, key: Key) -> Option<u8> {
        self.entries
            .iter()
            .find(|entry| entry.used && entry.key == key)
            .map(|entry| entry.slot)
    }

    /// Mọi khoá trong cache **không trùng** (một bản cho mỗi `(file,page)`).
    pub fn keys_unique(&self) -> bool {
        for (i, a) in self.entries.iter().enumerate() {
            if !a.used {
                continue;
            }
            for b in self.entries.iter().skip(i + 1) {
                if b.used && b.key == a.key {
                    return false;
                }
            }
        }
        true
    }

    /// Số lần hit.
    pub const fn hits(&self) -> u64 {
        self.hits
    }

    /// Số lần miss.
    pub const fn misses(&self) -> u64 {
        self.misses
    }

    /// Số lần trục xuất LRU.
    pub const fn evictions(&self) -> u64 {
        self.evictions
    }
}

impl<const N: usize> Default for PageCache<N> {
    fn default() -> Self {
        Self::new()
    }
}
