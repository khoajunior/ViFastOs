// SPDX-License-Identifier: MIT OR Apache-2.0
//! Phần **dùng chung** của `ring_server`/`ring_client` (KT-69) — nạp bằng `#[path] mod`.
//!
//! Chỉ chứa: hằng hình học + `impl Mem` trên Frame chia sẻ (qua shim atomic Acquire/Release) +
//! định dạng message 4 word. Không `unsafe`, không syscall trực tiếp, không in số (bằng chứng
//! `# ring:`/`# idl:` do **kernel** đo — D2-102).

use vifast_ring::Mem;
use vifast_user_abi::{
    atomic_load_acquire, atomic_load_relaxed, atomic_store_release, atomic_store_relaxed,
};

/// VA ring trong VSpace của **từng** tiến trình (kernel map cùng Frame vật lý vào cả hai).
pub const VA_RING: u64 = 0x4200_0000;
/// Số slot của ring (`const:` trong bằng chứng) — luỹ thừa 2.
pub const SLOTS: u32 = 8;
/// Byte mỗi slot — 4 word = 32 B.
pub const SLOT_BYTES: u32 = 32;
/// Tổng message trao đổi — 40 = **5 vòng** wrap với 8 slot.
pub const TOTAL: u64 = 40;
/// Số message mỗi lô server gửi sau khi client đồng bộ.
pub const BATCH: u64 = 4;
/// Số lần client thử `recv` trên ring **rỗng** trước lô đầu.
pub const EMPTY_PROBES: u64 = 3;
/// Cờ `Frame.Map` `R|W|U`.
pub const FLAGS_RWU: u64 = 0x0b;

// D2-102/N5: hình học phải hợp lệ **lúc biên dịch** (không chỉ runtime/unit test).
const _: () = assert!(SLOTS.is_power_of_two() && SLOTS > 1);
const _: () = assert!(SLOT_BYTES >= 32 && SLOT_BYTES % 8 == 0);

/// Adapter [`Mem`] trên VA user của chính tiến trình — mọi truy cập qua shim an toàn.
pub struct Shm {
    /// VA đáy Frame chia sẻ.
    pub base: u64,
}

impl Mem for Shm {
    fn load(&self, offset: usize) -> u64 {
        atomic_load_relaxed(self.base + offset as u64)
    }
    fn store(&self, offset: usize, value: u64) {
        atomic_store_relaxed(self.base + offset as u64, value);
    }
    fn acquire(&self, offset: usize) -> u64 {
        atomic_load_acquire(self.base + offset as u64)
    }
    fn store_rel(&self, offset: usize, value: u64) {
        atomic_store_release(self.base + offset as u64, value);
    }
}

/// Message chuẩn 4 word: `[seq, seq+1, !seq, 0x69]`.
pub fn message(seq: u64) -> [u64; 4] {
    [seq, seq.wrapping_add(1), !seq, 0x69]
}

/// `true` ⇔ slot đọc ra đúng message của `seq` (không torn/không lệch).
pub fn verify(seq: u64, words: &[u64; 4]) -> bool {
    *words == message(seq)
}
