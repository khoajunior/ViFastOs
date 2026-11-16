// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-67b** — đọc **header IPC buffer** của U-mode từ bộ nhớ user (D2-98).
//!
//! `spec/abi.toml` dòng 58: *"header chứa receive window (CPTR, ≤4 cap)"*. Arch là nơi **duy
//! nhất** chạm bộ nhớ user (D2-83); core chỉ nhận [`vifast_core::ipc::IpcHeader`] qua hook
//! `SyscallEnv::ipc_header`. Trang IPC buffer (4 KiB tại [`USER_IPC_BUFFER_VA`], `R|W|U|NX`) do
//! loader map cho **mọi** ELF U-mode; shim U-mode (`kernel/vifast-plat-user/lib.rs`) ghi magic
//! `b"KT67b"` + receive window 4 CPTR + 4 CPTR nguồn cap (layout word ở `vifast_core::ipc`).
//!
//! Tách khỏi `user.rs` để giữ luật cấu trúc R1 (≤ 1.500 dòng/tệp). `unsafe` thuộc danh sách đóng
//! `kernel/vifast-plat-*` (D2-17): chỉ một vòng đọc 10 word qword trong trang đã đăng ký.

use vifast_core::ipc::{
    IpcHeader, IPC_HEADER_MAGIC, IPC_HEADER_WORDS, IPC_HEADER_W_INFO, IPC_HEADER_W_SEND_CAPS,
    IPC_HEADER_W_WINDOW,
};

use crate::obj as aobj;
use crate::user::user_range_mapped;

/// VA cố định của trang **IPC buffer** — loader map `R|W|U|NX` + zero cho **mọi** ELF U-mode;
/// shim U-mode (`kernel/vifast-plat-user/lib.rs`) ghi/đọc cùng địa chỉ.
pub(crate) const USER_IPC_BUFFER_VA: u64 = 0x4002_0000;

/// Đọc header IPC buffer của tiến trình **đang chạy** — chỉ gọi trong syscall U-mode (`satp` =
/// root user, trang đã đăng ký ở `active_ranges`). `None` = trang chưa map hoặc magic chưa ghi
/// (ảnh cũ / chương trình không dùng header ⇒ kernel giữ nguyên hành vi trước KT-67b).
pub(crate) fn read_header() -> Option<IpcHeader> {
    let bytes = (IPC_HEADER_WORDS as u64) * 8;
    if !user_range_mapped(USER_IPC_BUFFER_VA, bytes) {
        return None;
    }
    let mut words = [0u64; IPC_HEADER_WORDS];
    aobj::set_sum(true);
    // SAFETY: dải `[USER_IPC_BUFFER_VA, +80)` đã được loader đăng ký là trang `U|R|W` của tiến
    // trình đang chạy; SUM bật nên S-mode đọc được trang U; tắt SUM ngay sau vòng.
    unsafe {
        for (index, slot) in words.iter_mut().enumerate() {
            *slot =
                core::ptr::read_volatile((USER_IPC_BUFFER_VA + (index as u64) * 8) as *const u64);
        }
    }
    aobj::set_sum(false);
    if words[0] != IPC_HEADER_MAGIC {
        return None;
    }
    Some(IpcHeader {
        info: words[IPC_HEADER_W_INFO],
        recv_window: [
            words[IPC_HEADER_W_WINDOW],
            words[IPC_HEADER_W_WINDOW + 1],
            words[IPC_HEADER_W_WINDOW + 2],
            words[IPC_HEADER_W_WINDOW + 3],
        ],
        send_caps: [
            words[IPC_HEADER_W_SEND_CAPS],
            words[IPC_HEADER_W_SEND_CAPS + 1],
            words[IPC_HEADER_W_SEND_CAPS + 2],
            words[IPC_HEADER_W_SEND_CAPS + 3],
        ],
    })
}
