// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mã trạng thái pager trả về — **qua tên** trong bảng `spec/abi.toml` `[error]`
//! (`vifast_abi::ERRORS`, sinh từ spec), không viết số trần (D2-102b).
//!
//! Pager chạy U-mode nên không dùng được `KError` của kernel; nhưng nguồn số vẫn **một**:
//! `Status::id()` tra bảng sinh từ spec. Bằng chứng in `name` + `id` cạnh nhau.

use vifast_abi::ERRORS;

/// Trạng thái pager (tên mã ABI). `0` byte không phải mã — mọi kết quả dùng `Status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    name: &'static str,
}

impl Status {
    /// Tên mã lỗi (chuỗi ổn định trong `spec/abi.toml`).
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Số mã lỗi — tra bảng sinh từ spec theo **tên**; không thấy ⇒ `RangeError` (19).
    pub fn id(self) -> u16 {
        let mut index = 0;
        while index < ERRORS.len() {
            if ERRORS[index].0 == self.name {
                return ERRORS[index].1;
            }
            index += 1;
        }
        19
    }

    /// Đúng ⇔ không phải lỗi.
    pub fn is_ok(self) -> bool {
        self.name.is_empty()
    }

    /// `Status` từ tên mã — `None` nếu tên không có trong spec (không im lặng nhận bừa).
    pub fn named(name: &'static str) -> Option<Self> {
        let mut index = 0;
        while index < ERRORS.len() {
            if ERRORS[index].0 == name {
                return Some(Self { name });
            }
            index += 1;
        }
        None
    }
}

/// Mã dùng trong giao thức pager — tên lấy từ bảng spec, không phải số.
pub mod err {
    use super::Status;

    /// Giao thức thành công (không phải mã lỗi — `name` rỗng).
    pub const OK: Status = Status { name: "" };
    /// Trang ngoài file / tham số sai.
    pub const INVALID_ARGUMENT: Status = Status {
        name: "InvalidArgument",
    };
    /// Frame.Map/Unmap hỏng.
    pub const NOT_MAPPED: Status = Status { name: "NotMapped" };
    /// Backend lỗi I/O.
    pub const IO: Status = Status { name: "RangeError" };
}
