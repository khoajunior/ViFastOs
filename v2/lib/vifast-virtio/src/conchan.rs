// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kênh **byte** trên ring 1P/1C của KT-69 (`vifast-ring`) — codec thuần, KT-73.
//!
//! # Quan hệ với KT-69/70 (không chế layout mới)
//!
//! Ring (header/magic/`head`/`tail`/slot) **giữ nguyên** `vifast_ring`; crate này chỉ định nghĩa
//! **cách nhồi byte** vào một slot 4 word (32 B): `word0..word2` là 24 byte LE, `word3` = số byte
//! thật (1..=24). Nhờ vậy ring vẫn là ring của KT-69 (kernel đọc/ghi header bằng cùng crate) và
//! không có "layout thứ hai" nào để lệch.
//!
//! Vì sao 24 B/slot: slot của `vifast_ring` là 4 word (32 B) và `MSG_BYTES` là tối thiểu; dùng
//! trọn 3 word đầu cho dữ liệu + 1 word đếm. 32 slot ⇒ 768 B đệm — đủ cho một dòng lệnh shell và
//! cho ca "ring đầy" có ý nghĩa mà không cần thêm Frame.

/// Cỡ dữ liệu mỗi slot (3 word) — trùng [`crate::console::TX_CHUNK_BYTES`].
pub const CHUNK_BYTES: usize = 24;
/// Số slot của kênh console (luỹ thừa 2, khớp `vifast_ring::FRAME_BYTES`).
pub const CHANNEL_SLOTS: u32 = 32;
/// Cỡ byte một slot của `vifast_ring` (4 word).
pub const SLOT_BYTES: u32 = 32;

// D2-102/N5: hình học hợp lệ **lúc biên dịch**, không chỉ runtime.
const _: () = assert!(CHUNK_BYTES == 24);
// Hai hằng ở hai tệp phải khớp: cỡ chunk của kênh = cỡ descriptor `tx` của driver console.
const _: () = assert!(CHUNK_BYTES == crate::console::TX_CHUNK_BYTES);
const _: () = assert!(CHANNEL_SLOTS.is_power_of_two());
const _: () = assert!(CHANNEL_SLOTS as usize * SLOT_BYTES as usize <= 4096 - 0x200);

/// Sức chứa byte của kênh = `slots × CHUNK_BYTES` (công thức, D2-106).
pub const fn channel_bytes() -> usize {
    CHANNEL_SLOTS as usize * CHUNK_BYTES
}

/// Đóng gói `bytes[..min(24)]` thành một slot 4 word. Trả `(words, count)`.
///
/// `count == 0` ⇒ slot rỗng, **không** được `send` (caller phải kiểm) — hàm vẫn mã hoá được để
/// `unpack` có ca âm tương ứng.
pub fn pack(bytes: &[u8]) -> ([u64; 4], usize) {
    let count = bytes.len().min(CHUNK_BYTES);
    let mut words = [0u64; 4];
    let mut index = 0usize;
    while index < count {
        let slot_word = index / 8;
        let slot_byte = index % 8;
        words[slot_word] |= u64::from(bytes[index]) << (8 * slot_byte);
        index += 1;
    }
    words[3] = count as u64;
    (words, count)
}

/// Bóc một slot 4 word thành byte; trả số byte đã ghi vào `out` (0 nếu slot hỏng/đếm sai).
///
/// Header/`count` ngoài `[1, 24]` ⇒ `0` (consumer **không** đọc rác; `vifast_ring` đã kiểm hình
/// học, đây là lớp kiểm nội dung slot).
pub fn unpack(words: &[u64; 4], out: &mut [u8; CHUNK_BYTES]) -> usize {
    let count = words[3] as usize;
    if count == 0 || count > CHUNK_BYTES {
        return 0;
    }
    let mut index = 0usize;
    while index < count {
        let slot_word = index / 8;
        let slot_byte = index % 8;
        out[index] = ((words[slot_word] >> (8 * slot_byte)) & 0xff) as u8;
        index += 1;
    }
    count
}

/// Số lần gửi tối thiểu để truyền `len` byte (làm tròn lên theo [`CHUNK_BYTES`]) — công thức để
/// evidence in `chunks = ceil(bytes/24)` thay vì số viết tay (D2-106).
pub const fn chunks_for(len: usize) -> usize {
    len.div_ceil(CHUNK_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip_moi_do_dai() {
        let mut src = [0u8; 24];
        for (i, byte) in src.iter_mut().enumerate() {
            *byte = (0xA5u16.wrapping_add(i as u16 * 7) & 0xff) as u8;
        }
        for len in 0..=24usize {
            let (words, count) = pack(&src[..len]);
            assert_eq!(count, len);
            let mut out = [0u8; 24];
            let got = unpack(&words, &mut out);
            assert_eq!(got, len);
            assert_eq!(&out[..len], &src[..len]);
            if len < 24 {
                assert_eq!(out[len], 0, "byte ngoài `count` phải 0");
            }
        }
    }

    #[test]
    fn pack_cat_va_unpack_tu_choi_count_sai() {
        let (words, count) = pack(b"0123456789abcdefghijklmnopqrstuvwxyz");
        assert_eq!(count, 24, "cắt đúng CHUNK_BYTES");
        assert_eq!(words[3], 24);
        let mut out = [0u8; 24];
        assert_eq!(unpack(&words, &mut out), 24);
        assert_eq!(&out[..3], b"012");
        // count = 0 hoặc > 24 ⇒ 0 byte (consumer không đọc rác).
        let mut bad = words;
        bad[3] = 0;
        assert_eq!(unpack(&bad, &mut out), 0);
        bad[3] = 25;
        assert_eq!(unpack(&bad, &mut out), 0);
    }

    #[test]
    fn cong_thuc_suc_chua_va_so_lan_gui() {
        assert_eq!(channel_bytes(), 768);
        assert_eq!(chunks_for(0), 0);
        assert_eq!(chunks_for(1), 1);
        assert_eq!(chunks_for(24), 1);
        assert_eq!(chunks_for(25), 2);
        assert_eq!(chunks_for(channel_bytes()), 32);
    }
}
