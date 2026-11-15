// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hằng số ABI + quyền lấy **từ nơi sinh** (D2-11): `vifast-abi` sinh từ `spec/abi.toml`.
//!
//! `vifast-core` không chép tay 48 B slot / 64 B cnode_base / 512 B TCB / 4096 B page /
//! 16 syscall / 19 mã lỗi. Bốn bit quyền (READ, WRITE, GRANT, GRANT_REPLY) là hằng **thiết
//! kế** §4.3 + QD §3.12 (không nằm trong `spec/abi.toml`, nên không sinh được); sim
//! (`v2/sim/vifast-sim/src/kobj.rs`) cũng định nghĩa đúng bốn bit này — core giữ nguyên.

pub use vifast_abi::{
    ABI_VERSION, CNODE_BASE_BYTES, DEFAULT_ALIGN_BYTES, ERRORS, ERROR_COUNT, OBJ_ENDPOINT_BYTES,
    OBJ_NOTIFICATION_BYTES, OBJ_REPLY_BYTES, OBJ_SCHED_CONTEXT_BYTES, OBJ_SCHED_CONTROL_BYTES,
    OBJ_TCB_BYTES, PAGE_BYTES, SLOT_ALIGN_BYTES, SLOT_BYTES, SYSCALL_COUNT, UNTYPED_MIN_BYTES,
};

/// Quyền đọc (`spec` không có bảng quyền; thiết kế §4.3, QD §3.12).
pub const RIGHT_READ: u8 = 1 << 0;
/// Quyền ghi — `Retype` cần quyền này trên Untyped nguồn (giống sim KT-10 F3).
pub const RIGHT_WRITE: u8 = 1 << 1;
/// Quyền cấp cap (Copy/Mint; IPC K3b).
pub const RIGHT_GRANT: u8 = 1 << 2;
/// Quyền cấp Reply (IPC K3b).
pub const RIGHT_GRANT_REPLY: u8 = 1 << 3;
/// Cap gốc do kernel/harness cấp giữ đủ **4** quyền (thiết kế §4.3).
pub const RIGHTS_ALL: u8 = RIGHT_READ | RIGHT_WRITE | RIGHT_GRANT | RIGHT_GRANT_REPLY;

/// `flags` bit GUEST của `Untyped.Retype` (D2-32): chỉ VSpace nhận; core trả đúng mã như sim.
pub const FLAG_GUEST: u32 = 1;

/// Căn lề của arena Untyped — bằng `vifast_objmem::ARENA_ALIGN` (4096) = `page_bytes` của
/// `spec/abi.toml`. Untyped con có cỡ > ngưỡng này không thể bảo đảm căn tuyệt đối nên
/// `Retype` trả `Alignment` (đúng giới hạn arena của sim, KT-17).
pub const ARENA_ALIGN: usize = PAGE_BYTES;

/// Cỡ byte của một slot CTE trong CNode — `[object_size].slot.bytes` (QD §3.9).
pub const SLOT_ARRAY_BYTES: usize = SLOT_BYTES;
