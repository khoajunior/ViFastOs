// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-60 — `MessageInfo` 35 bit + `Message` 64 MR + vùng nhận [`IpcArea`] + **một** hàm
//! [`transfer`] copy message — port `no_std` từ `v2/sim/vifast-sim/src/ipc.rs` (oracle).
//!
//! Model-only `[TK]`, **không** phải ABI; layout/number đọc đúng `spec/abi.toml`:
//! `MessageInfo` 35 bit (`label:0..19 | caps:20..22 | no_reply:23 | extra:24..27 |
//! length:28..34`), 64 MR (`4` thanh ghi + `60` IPC buffer), ≤ 4 cap.
//!
//! **Biên KT-60:** `Message` giữ **hình dạng** `caps` của sim nhưng **chưa** chuyển cap —
//! message có cap (`caps > 0`) bị từ chối `InvalidArgument`; chuyển cap (receive window,
//! `plan_cap_transfer`, mất `RIGHT_GRANT`) là nợ ghi trong `docs/k3b-core.md` §8 (KT-61).
//! `badge`/`msg_id` **có** (Notification badge OR + bất biến bảo toàn message).

use crate::error::KError;

/// Số MR tối đa của một message — `spec/abi.toml` `[message].max_words = 64`.
pub const MAX_WORDS: usize = 64;
/// Số cap tối đa của một message — `spec/abi.toml` `[message].max_caps = 4`.
pub const MAX_CAPS: usize = 4;
/// MR0..MR3 đi trong **thanh ghi** (`spec/abi.toml` `words_in_registers = 4`).
pub const WORDS_IN_REGISTERS: usize = 4;
/// MR4..MR63 đi trong **IPC buffer model** (`spec/abi.toml` `words_in_ipc_buffer = 60`).
pub const WORDS_IN_IPC_BUFFER: usize = 60;

/// Số bit của `MessageInfo` — `spec/abi.toml` `[message_info].bits = 35`.
pub const INFO_BITS: u32 = 35;
/// Mặt nạ 35 bit hợp lệ của `MessageInfo`.
pub const INFO_MASK: u64 = (1u64 << INFO_BITS) - 1;
/// Số bit `label`.
pub const LABEL_BITS: u32 = 20;
/// Số bit `caps`.
pub const CAPS_BITS: u32 = 3;
/// Số bit cờ `no_reply`.
pub const NO_REPLY_BITS: u32 = 1;
/// Số bit `extra_caps_unwrapped`.
pub const EXTRA_CAPS_UNWRAPPED_BITS: u32 = 4;
/// Số bit `length`.
pub const LENGTH_BITS: u32 = 7;

/// `MessageInfo` — **một thanh ghi 35 bit** (`spec/abi.toml`, thiết kế §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MessageInfo {
    label: u32,
    caps: u8,
    no_reply: bool,
    extra_caps_unwrapped: u8,
    length: u8,
}

impl MessageInfo {
    /// `MessageInfo` rỗng.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Tạo `MessageInfo` **đã kiểm biên** (biên ABI): `label < 2^20`, `caps <= 4`,
    /// `extra_caps_unwrapped < 16`, `length <= 64`. Vi phạm ⇒ `InvalidArgument`.
    pub fn new(
        label: u32,
        caps: u8,
        no_reply: bool,
        extra_caps_unwrapped: u8,
        length: u8,
    ) -> Result<Self, KError> {
        if label >= (1 << LABEL_BITS) {
            return Err(KError::InvalidArgument);
        }
        if caps as usize > MAX_CAPS {
            return Err(KError::InvalidArgument);
        }
        if extra_caps_unwrapped >= (1 << EXTRA_CAPS_UNWRAPPED_BITS) {
            return Err(KError::InvalidArgument);
        }
        if length as usize > MAX_WORDS {
            return Err(KError::InvalidArgument);
        }
        Ok(Self {
            label,
            caps,
            no_reply,
            extra_caps_unwrapped,
            length,
        })
    }

    /// `label` (20 bit).
    pub fn label(self) -> u32 {
        self.label
    }

    /// Số cap trong message (0..=4).
    pub fn caps(self) -> u8 {
        self.caps
    }

    /// Cờ `no_reply` (Call = Send khi caller đặt; kernel bật khi thiếu Reply).
    pub fn no_reply(self) -> bool {
        self.no_reply
    }

    /// Số extra cap đã unwrap thành badge.
    pub fn extra_caps_unwrapped(self) -> u8 {
        self.extra_caps_unwrapped
    }

    /// Số MR thật (0..=64).
    pub fn length(self) -> u8 {
        self.length
    }

    /// Bản sao có `no_reply = on`.
    pub fn with_no_reply(mut self, on: bool) -> Self {
        self.no_reply = on;
        self
    }

    /// Bản sao có `extra_caps_unwrapped` mới (kiểm biên 4 bit).
    pub fn with_extra_caps_unwrapped(mut self, extra: u8) -> Result<Self, KError> {
        if extra >= (1 << EXTRA_CAPS_UNWRAPPED_BITS) {
            return Err(KError::InvalidArgument);
        }
        self.extra_caps_unwrapped = extra;
        Ok(self)
    }

    /// Bản sao có `caps` mới (biên ABI `max_caps = 4`).
    pub fn with_caps(mut self, caps: u8) -> Result<Self, KError> {
        if caps as usize > MAX_CAPS {
            return Err(KError::InvalidArgument);
        }
        self.caps = caps;
        Ok(self)
    }

    /// Bản sao có `length` mới (kiểm biên như [`MessageInfo::new`]).
    pub fn with_length(mut self, length: u8) -> Result<Self, KError> {
        if length as usize > MAX_WORDS {
            return Err(KError::InvalidArgument);
        }
        self.length = length;
        Ok(self)
    }

    /// Đóng gói thành thanh ghi 35 bit.
    pub fn pack(self) -> u64 {
        (self.label as u64)
            | ((self.caps as u64) << 20)
            | ((self.no_reply as u64) << 23)
            | ((self.extra_caps_unwrapped as u64) << 24)
            | ((self.length as u64) << 28)
    }

    /// Bí danh của [`MessageInfo::pack`].
    pub fn bits(self) -> u64 {
        self.pack()
    }

    /// Giải mã thanh ghi 35 bit; **kiểm lại** mọi biên. Bit ngoài 35 hoặc trường ngoài biên
    /// ⇒ `InvalidArgument` (không cắt im lặng).
    pub fn unpack(raw: u64) -> Result<Self, KError> {
        if raw & !INFO_MASK != 0 {
            return Err(KError::InvalidArgument);
        }
        Self::new(
            (raw & 0xF_FFFF) as u32,
            ((raw >> 20) & 0x7) as u8,
            (raw >> 23) & 1 != 0,
            ((raw >> 24) & 0xF) as u8,
            ((raw >> 28) & 0x7F) as u8,
        )
    }
}

/// Một message IPC: `info` 35 bit + **đủ 64 MR** + `badge` + `msg_id` (kernel cấp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message {
    /// `MessageInfo` 35 bit.
    pub info: MessageInfo,
    /// 64 MR — chỉ `mr[..length]` là thật.
    pub mr: [u64; MAX_WORDS],
    /// Badge (Notification unwrap/OR — KT-27).
    pub badge: u64,
    /// `msg_id` do kernel cấp khi message vào đường IPC (`0` = chưa cấp).
    pub msg_id: u64,
}

impl Message {
    /// `Message` rỗng (mốc const).
    pub const EMPTY: Self = Self {
        info: MessageInfo {
            label: 0,
            caps: 0,
            no_reply: false,
            extra_caps_unwrapped: 0,
            length: 0,
        },
        mr: [0; MAX_WORDS],
        badge: 0,
        msg_id: 0,
    };

    /// Message **không cap**: `info.caps()` phải bằng 0, `words.len()` phải bằng
    /// `info.length()`. Vi phạm ⇒ `InvalidArgument`.
    pub fn new(info: MessageInfo, words: &[u64]) -> Result<Self, KError> {
        if info.caps() != 0 {
            // KT-60: chưa port chuyển cap (nợ §8) — không nhận message có cap để không
            // âm thầm bỏ cap.
            return Err(KError::InvalidArgument);
        }
        Self::build(info, words)
    }

    fn build(info: MessageInfo, words: &[u64]) -> Result<Self, KError> {
        if words.len() != info.length() as usize || words.len() > MAX_WORDS {
            return Err(KError::InvalidArgument);
        }
        let mut mr = [0u64; MAX_WORDS];
        mr[..words.len()].copy_from_slice(words);
        Ok(Self {
            info,
            mr,
            badge: 0,
            msg_id: 0,
        })
    }

    /// Các MR **thật** của message (`mr[..length]`).
    pub fn words(&self) -> &[u64] {
        &self.mr[..self.info.length() as usize]
    }

    /// `length` của message (0..=64).
    pub fn len(&self) -> usize {
        self.info.length() as usize
    }

    /// Message rỗng?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gán `msg_id` do kernel cấp.
    pub fn with_id(mut self, msg_id: u64) -> Self {
        self.msg_id = msg_id;
        self
    }
}

/// Vùng nhận IPC của **một** TCB — hai field **riêng** đúng `spec/abi.toml`:
/// `regs: [u64; 4]` (MR0..MR3) + `buffer: [u64; 60]` (MR4..MR63), cùng metadata message vừa
/// nhận (`info`/`badge`/`msg_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcArea {
    /// `MessageInfo` của message **vừa nhận** (`None` = chưa nhận).
    pub info: Option<MessageInfo>,
    /// MR0..MR3.
    pub regs: [u64; WORDS_IN_REGISTERS],
    /// MR4..MR63.
    pub buffer: [u64; WORDS_IN_IPC_BUFFER],
    /// Badge của message vừa nhận.
    pub badge: u64,
    /// `msg_id` của message vừa nhận.
    pub msg_id: Option<u64>,
}

impl IpcArea {
    /// Vùng nhận rỗng.
    pub const fn new() -> Self {
        Self {
            info: None,
            regs: [0; WORDS_IN_REGISTERS],
            buffer: [0; WORDS_IN_IPC_BUFFER],
            badge: 0,
            msg_id: None,
        }
    }

    /// Xoá sạch vùng nhận.
    pub fn clear(&mut self) {
        self.info = None;
        self.regs = [0; WORDS_IN_REGISTERS];
        self.buffer = [0; WORDS_IN_IPC_BUFFER];
        self.badge = 0;
        self.msg_id = None;
    }

    /// MR thứ `i` (0..64) — ghép hai vùng.
    pub fn mr(&self, i: usize) -> Option<u64> {
        if i < WORDS_IN_REGISTERS {
            self.regs.get(i).copied()
        } else {
            self.buffer.get(i - WORDS_IN_REGISTERS).copied()
        }
    }
}

impl Default for IpcArea {
    fn default() -> Self {
        Self::new()
    }
}

/// **Đường duy nhất** copy nội dung message vào vùng nhận của TCB (thiết kế §4.6): xoá sạch
/// vùng nhận trước, chép `info` + MR (4 word đầu vào `regs`, 60 sau vào `buffer`) + `badge` +
/// `msg_id`. Trả số MR thật đã chép.
pub fn transfer(msg: &Message, area: &mut IpcArea) -> usize {
    area.clear();
    area.info = Some(msg.info);
    let len = msg.info.length() as usize;
    for (i, word) in msg.mr[..len].iter().enumerate() {
        if i < WORDS_IN_REGISTERS {
            area.regs[i] = *word;
        } else {
            area.buffer[i - WORDS_IN_REGISTERS] = *word;
        }
    }
    area.badge = msg.badge;
    area.msg_id = Some(msg.msg_id);
    len
}
