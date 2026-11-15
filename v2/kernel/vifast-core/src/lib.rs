// SPDX-License-Identifier: MIT OR Apache-2.0
//! `vifast-core` — core kernel `no_std` của K3b, **tầng 1** (KT-58): Untyped + CNode + MDB +
//! capability, port **nguyên ngữ nghĩa** từ `v2/sim/vifast-sim/src/kobj.rs` (sim vẫn là
//! oracle; core không thiết kế lại).
//!
//! # Phạm vi KT-58 (D2-82 §12 nợ K3b)
//!
//! - **Có**: `Untyped` (watermark, cỡ/căn lề, Retype trọn bộ object lõi, Revoke trả byte),
//!   `CNode` (slot 48 B theo spec, Copy/Mint/Delete/Revoke, quyền 4 bit, badge),
//!   cây dẫn xuất MDB kiểu seL4 (cha–con, reparent khi xoá, revoke cây con, finalise CNode
//!   lồng nhau), và các bất biến `check_caps`/`check_bytes`.
//! - **Không**: TCB/scheduler/MCS (KT-59), Endpoint/Notification/Reply/IPC/WaitAddr/fault
//!   (KT-60), Frame/PageTable/VSpace (K2 object layer `vifast-plat-common::obj`, KT-52) —
//!   `Retype` các loại reserved trả `NotSupported` như sim (trừ Frame/PageTable mà sim đã
//!   mở ở K2, xem `docs/k3b-core.md` §khác biệt), kết nối syscall/`init` (KT-61).
//!
//! # Vì sao `no_std` + tĩnh
//!
//! Core chạy được **cả host test lẫn bare-metal riscv64**: `#![no_std]`,
//! `#![forbid(unsafe_code)]` (D2-17), **không** `alloc`/`Vec`/`String`. `Vec<Meta>`/`Vec<Cap>`
//! của sim thành bảng tĩnh `.bss` (`MAX_OBJECTS`, `MAX_CAPS`, `MAX_SLOT_WORDS`), body CNode
//! thành pool slot 48 B/slot (vì `CNode` tiêu đúng `cnode_base + slots*48` byte arena); byte
//! Untyped do [`arena::Region`] kế toán. Mọi truy vấn trả iterator/`Copy` thay `Vec`:
//! `cap_table`, `cap_children`.
//!
//! # Hợp đồng ABI (D2-11)
//!
//! `spec::*` tái xuất hằng sinh từ `spec/abi.toml` (`vifast-abi`): 48 B slot, `cnode_base`
//! 64 B, TCB 512 B, page 4096 B, 16 syscall, 19 mã lỗi. Bốn bit quyền là hằng thiết kế
//! §4.3/QD §3.12 (spec không có bảng quyền).
//!
//! # Không `unsafe`
//!
//! Không con trỏ thô, không `transmute`, không trait `PhysMem`: mọi thứ là chỉ số + mảng tĩnh.
//! Kernel QEMU dùng cùng mã qua `vifast-core` (KT-61) — nợ duy nhất là slot nằm trong `.bss`
//! thay vì arena vật lý, đã ghi trong `docs/k3b-core.md`.

#![no_std]
#![forbid(unsafe_code)]

pub mod arena;
mod check;
mod error;
mod machine;
mod slotpool;
pub mod spec;

pub use arena::Region;
pub use error::{CapId, CapInfo, CheckFailure, KError, ObjId, ObjType, SlotRef};
pub use machine::{Machine, MAX_CAPS, MAX_OBJECTS, MAX_SLOT_WORDS, MAX_UNTYPEDS};

/// Số hole tự do tối đa của pool slot CNode (`slotpool`).
pub(crate) const MAX_HOLES: usize = 16;
pub use spec::{
    ARENA_ALIGN, CNODE_BASE_BYTES, DEFAULT_ALIGN_BYTES, FLAG_GUEST, PAGE_BYTES, RIGHTS_ALL,
    RIGHT_GRANT, RIGHT_GRANT_REPLY, RIGHT_READ, RIGHT_WRITE, SLOT_BYTES, UNTYPED_MIN_BYTES,
};
