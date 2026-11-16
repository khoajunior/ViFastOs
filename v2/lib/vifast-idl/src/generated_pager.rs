// SPDX-License-Identifier: MIT OR Apache-2.0
// SINH TỰ ĐỘNG bởi v2/tools/vifast-gen/gen_idl.py từ v2/lib/vifast-idl/idl/pager.vidl
// — ĐỪNG SỬA TAY (`./v2/bin/vifast-idl --check` sẽ bắt).
//
// Nguồn số method: `spec/abi.toml` KHÔNG có bảng method cho interface mới, nên method id
// sinh TUẦN TỰ từ tệp IDL (0..N-1 theo thứ tự khai báo) — không chép tay số nào (D2-102c).
// Stub thuần encode/decode: không `unsafe`, không syscall, không phụ thuộc crate.
#![allow(clippy::all)]

#[rustfmt::skip]
pub const IFACE_NAME: &str = "pager";
#[rustfmt::skip]
pub const IFACE_VERSION: u32 = 1;
#[rustfmt::skip]
pub const METHOD_COUNT: u32 = 5;

/// Bit reply trong `label` 20 bit (bit 19) — reply của một method cùng số method.
#[rustfmt::skip]
pub const REPLY_BIT: u32 = 0x8_0000;
/// Mặt nạ `label` 20 bit (`spec/abi.toml` `[message_info].label_bits = 20`).
#[rustfmt::skip]
pub const LABEL_MASK: u32 = 0xF_FFFF;
/// Dịch bit `caps` của `MessageInfo` (spec `[message_info].caps:20..22`).
#[rustfmt::skip]
pub const INFO_CAPS_SHIFT: u32 = 20;
/// Dịch bit `length` của `MessageInfo` (spec `[message_info].length:28..34`).
#[rustfmt::skip]
pub const INFO_LENGTH_SHIFT: u32 = 28;

/// Method `fault` — id 0 (sinh tuần tự từ IDL).
#[rustfmt::skip]
pub const METHOD_FAULT: u32 = 0;
/// Method `served` — id 1 (sinh tuần tự từ IDL).
#[rustfmt::skip]
pub const METHOD_SERVED: u32 = 1;
/// Method `ready` — id 2 (sinh tuần tự từ IDL).
#[rustfmt::skip]
pub const METHOD_READY: u32 = 2;
/// Method `finish` — id 3 (sinh tuần tự từ IDL).
#[rustfmt::skip]
pub const METHOD_FINISH: u32 = 3;
/// Method `stop` — id 4 (sinh tuần tự từ IDL).
#[rustfmt::skip]
pub const METHOD_STOP: u32 = 4;

/// Đóng gói `MessageInfo` 35 bit từ `label`/`caps`/`length` (không đổi layout ABI).
#[rustfmt::skip]
pub const fn encode_info(label: u32, caps: u8, length: u8) -> u64 {
    (label as u64 & LABEL_MASK as u64)
        | ((caps as u64 & 0x7) << INFO_CAPS_SHIFT)
        | ((length as u64 & 0x7F) << INFO_LENGTH_SHIFT)
}

/// Số scalar (MR) của từng method — decode kiểm **chính xác** (D2-104: thiếu/thừa ⇒ lỗi).
#[rustfmt::skip]
pub const fn arity(method: u32) -> Option<u8> {
    match method {
        METHOD_FAULT => Some(4),
        METHOD_SERVED => Some(4),
        METHOD_READY => Some(0),
        METHOD_FINISH => Some(3),
        METHOD_STOP => Some(0),
        _ => None,
    }
}

/// Số cap của từng method (0 hoặc 1; cap đi qua receive/send window, không phải MR).
#[rustfmt::skip]
pub const fn cap_count(method: u32) -> Option<u8> {
    match method {
        METHOD_FAULT => Some(0),
        METHOD_SERVED => Some(0),
        METHOD_READY => Some(0),
        METHOD_FINISH => Some(0),
        METHOD_STOP => Some(0),
        _ => None,
    }
}

/// Request đã giải mã từ `label` + `length` + `MR0..MR3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[rustfmt::skip]
pub enum Request {
    Fault { client: u32, file: u32, page: u32, va: u64 },
    Served { client: u32, page: u32, slot: u32, status: u32 },
    Ready,
    Finish { faults: u32, hits: u32, misses: u32 },
    Stop,
}

/// Client encode `fault`: trả `(MessageInfo, MR0..MR3)`.
#[rustfmt::skip]
pub fn encode_fault(client: u32, file: u32, page: u32, va: u64) -> (u64, [u64; 4]) {
    let mut mr = [0u64; 4];
    mr[0] = client as u64;
    mr[1] = file as u64;
    mr[2] = page as u64;
    mr[3] = va as u64;
    (encode_info(METHOD_FAULT, 0, 4), mr)
}

/// Client encode `served`: trả `(MessageInfo, MR0..MR3)`.
#[rustfmt::skip]
pub fn encode_served(client: u32, page: u32, slot: u32, status: u32) -> (u64, [u64; 4]) {
    let mut mr = [0u64; 4];
    mr[0] = client as u64;
    mr[1] = page as u64;
    mr[2] = slot as u64;
    mr[3] = status as u64;
    (encode_info(METHOD_SERVED, 0, 4), mr)
}

/// Client encode `ready`: trả `(MessageInfo, MR0..MR3)`.
#[rustfmt::skip]
pub fn encode_ready() -> (u64, [u64; 4]) {
    let mr = [0u64; 4];
    (encode_info(METHOD_READY, 0, 0), mr)
}

/// Client encode `finish`: trả `(MessageInfo, MR0..MR3)`.
#[rustfmt::skip]
pub fn encode_finish(faults: u32, hits: u32, misses: u32) -> (u64, [u64; 4]) {
    let mut mr = [0u64; 4];
    mr[0] = faults as u64;
    mr[1] = hits as u64;
    mr[2] = misses as u64;
    (encode_info(METHOD_FINISH, 0, 3), mr)
}

/// Client encode `stop`: trả `(MessageInfo, MR0..MR3)`.
#[rustfmt::skip]
pub fn encode_stop() -> (u64, [u64; 4]) {
    let mr = [0u64; 4];
    (encode_info(METHOD_STOP, 0, 0), mr)
}

/// Server/client decode request; `None` = label lạ, arity sai, hoặc tham số tràn kiểu.
#[rustfmt::skip]
pub fn decode(label: u32, length: u8, mr: &[u64; 4]) -> Option<Request> {
    if label & REPLY_BIT != 0 {
        return None;
    }
    match label {
        METHOD_FAULT => {
            if length != 4 {
                return None;
            }
            Some(Request::Fault { client: u32_of(mr[0])?, file: u32_of(mr[1])?, page: u32_of(mr[2])?, va: mr[3] })
        }
        METHOD_SERVED => {
            if length != 4 {
                return None;
            }
            Some(Request::Served { client: u32_of(mr[0])?, page: u32_of(mr[1])?, slot: u32_of(mr[2])?, status: u32_of(mr[3])? })
        }
        METHOD_READY => {
            if length != 0 {
                return None;
            }
            Some(Request::Ready)
        }
        METHOD_FINISH => {
            if length != 3 {
                return None;
            }
            Some(Request::Finish { faults: u32_of(mr[0])?, hits: u32_of(mr[1])?, misses: u32_of(mr[2])? })
        }
        METHOD_STOP => {
            if length != 0 {
                return None;
            }
            Some(Request::Stop)
        }
        _ => None,
    }
}

/// `u32` không mất bit: giá trị > `u32::MAX` ⇒ `None` (fail-closed, không cắt ngầm).
fn u32_of(value: u64) -> Option<u32> {
    if value <= u32::MAX as u64 {
        Some(value as u32)
    } else {
        None
    }
}

/// Reply của `fault`: `returns u32`.
#[rustfmt::skip]
pub fn encode_fault_reply(value: u32) -> (u64, [u64; 4]) {
    (encode_info(METHOD_FAULT | REPLY_BIT, 0, 1), [value as u64, 0, 0, 0])
}

#[rustfmt::skip]
pub fn decode_fault_reply(label: u32, length: u8, mr: &[u64; 4]) -> Option<u32> {
    if label != METHOD_FAULT | REPLY_BIT || length != 1 {
        return None;
    }
    u32_of(mr[0])
}

/// Reply của `served`: `returns unit`.
#[rustfmt::skip]
pub fn encode_served_reply() -> (u64, [u64; 4]) {
    (encode_info(METHOD_SERVED | REPLY_BIT, 0, 0), [0u64; 4])
}

#[rustfmt::skip]
pub fn decode_served_reply(label: u32, length: u8) -> Option<()> {
    if label == METHOD_SERVED | REPLY_BIT && length == 0 {
        Some(())
    } else {
        None
    }
}

/// Reply của `ready`: `returns u32`.
#[rustfmt::skip]
pub fn encode_ready_reply(value: u32) -> (u64, [u64; 4]) {
    (encode_info(METHOD_READY | REPLY_BIT, 0, 1), [value as u64, 0, 0, 0])
}

#[rustfmt::skip]
pub fn decode_ready_reply(label: u32, length: u8, mr: &[u64; 4]) -> Option<u32> {
    if label != METHOD_READY | REPLY_BIT || length != 1 {
        return None;
    }
    u32_of(mr[0])
}

/// Reply của `finish`: `returns u32`.
#[rustfmt::skip]
pub fn encode_finish_reply(value: u32) -> (u64, [u64; 4]) {
    (encode_info(METHOD_FINISH | REPLY_BIT, 0, 1), [value as u64, 0, 0, 0])
}

#[rustfmt::skip]
pub fn decode_finish_reply(label: u32, length: u8, mr: &[u64; 4]) -> Option<u32> {
    if label != METHOD_FINISH | REPLY_BIT || length != 1 {
        return None;
    }
    u32_of(mr[0])
}

/// Reply của `stop`: `returns u32`.
#[rustfmt::skip]
pub fn encode_stop_reply(value: u32) -> (u64, [u64; 4]) {
    (encode_info(METHOD_STOP | REPLY_BIT, 0, 1), [value as u64, 0, 0, 0])
}

#[rustfmt::skip]
pub fn decode_stop_reply(label: u32, length: u8, mr: &[u64; 4]) -> Option<u32> {
    if label != METHOD_STOP | REPLY_BIT || length != 1 {
        return None;
    }
    u32_of(mr[0])
}

/// Reply chung (mọi method): trả `(method, value)`; `length = 0` cho `unit`.
#[rustfmt::skip]
pub fn decode_reply(label: u32, length: u8, mr: &[u64; 4]) -> Option<(u32, u64)> {
    if label & REPLY_BIT == 0 {
        return None;
    }
    let method = label & !REPLY_BIT;
    let want = arity(method)?;
    match want {
        0 => {
            if length == 0 {
                Some((method, 0))
            } else {
                None
            }
        }
        _ => {
            if length == 1 {
                Some((method, mr[0]))
            } else {
                None
            }
        }
    }
}
