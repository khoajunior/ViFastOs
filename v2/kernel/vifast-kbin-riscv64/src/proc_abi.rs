// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-77b** — bố cục cspace + nhãn message **dùng chung** giữa ảnh test (`proc_demo`) và ảnh
//! sản phẩm có shell (`proc_rt`).
//!
//! # Vì sao tệp này tồn tại
//!
//! Ba chương trình U-mode (`procsrv`, `shell`, `child`) được biên dịch **riêng** (`build.rs`) và
//! hardcode số endpoint trong ELF của chính chúng (`init/procsrv_user.rs:29`, `init/shell_user.rs:45`);
//! kernel dựng cspace phải khớp **từng slot** thì `NBSend`/`NBRecv` mới tới đúng đích. Trước
//! KT-77b bảng slot đó chỉ nằm trong `proc_demo.rs` (ảnh test) nên ảnh sản phẩm phải chép lại —
//! hai nguồn dễ lệch. Tệp này là **một** nguồn cho phía kernel.
//!
//! Chương trình U-mode vẫn giữ bản sao hằng số (chúng không link crate kernel): ràng buộc "khớp
//! bảng" được kiểm bằng `v2/agent/JOURNAL/kt77b-harness-scan.sh` §slot (so số trong ELF với
//! `proc_abi`) — không phải bằng lời.
//!
//! Không logic, không `unsafe`, không cấp phát (R1/R5 không áp dụng; tệp chỉ khai báo hằng).

// Mỗi ảnh dùng một tập con (ảnh test dùng `SLOT_CHILD`/`EV_*`, ảnh sản phẩm không) —
// hằng ở đây là **hợp đồng** giữa kernel và chương trình U-mode, không phải code chết.
#![allow(dead_code)]

/// Ô chương trình U-mode trong `user::MAX_PROCS` (bảng dải VA per-slot, KT-65).
pub const SLOT_PROCSRV: usize = 0;
/// Ô của shell.
pub const SLOT_SHELL: usize = 1;
/// Ô của tiến trình con (chỉ ảnh test `ktest-proc` nạp; ảnh sản phẩm giữ ô trống).
pub const SLOT_CHILD: usize = 2;

/// Slot cspace của endpoint fault (core ghi `FaultKind` — D2-25/QD §1 Q-A).
pub const EP_FAULT: usize = 1;
/// Slot endpoint yêu cầu `wait`/`kill` → procsrv.
pub const EP_PROC: usize = 2;
/// Slot endpoint `LINE` của KT-65 (KT-73 bỏ đường này; giữ slot để bố cục không đổi).
pub const EP_UART: usize = 3;
/// Slot endpoint reply của procsrv.
pub const EP_REPLY: usize = 4;
/// Slot endpoint `PS_REQ` → kernel.
pub const EP_PS: usize = 5;
/// Slot cap `SchedControl` (không phải endpoint).
pub const EP_CTL: usize = 6;
/// Slot endpoint sự kiện procsrv → kernel (`EV_*`).
pub const EP_EVENTS: usize = 8;

/// Slot giữ cap `CNode` của chính cspace (shell/procsrv không dùng; bố cục cố định).
pub const SLOT_CSPACE: usize = 20;

/// Slot cap TCB của các tác vụ kernel dựng (theo thứ tự ảnh: procsrv, shell, …).
pub const TCB_SLOTS: [usize; 5] = [10, 11, 12, 13, 14];
/// Slot cap `SchedContext` tương ứng [`TCB_SLOTS`].
pub const SC_SLOTS: [usize; 5] = [30, 31, 32, 33, 34];

/// `PS_REQ` — shell yêu cầu bảng TCB; kernel trả `PS_REPLY` (đọc từ `Machine`, không bảng tĩnh).
pub const PS_REQ: u64 = 0x602;
/// Reply của kernel cho [`PS_REQ`].
pub const PS_REPLY: u64 = 0x603;
/// Sự kiện: procsrv vừa nhận một `FaultKind::Exit`.
pub const EV_FAULT: u64 = 0x502;
/// Sự kiện: kết quả `wait(pid)`.
pub const EV_WAIT: u64 = 0x503;
/// Sự kiện: kết quả `kill(pid)` — kernel thi hành `Tcb.Suspend(Abort)`.
pub const EV_KILL: u64 = 0x504;
/// Sự kiện: procsrv đã chạy (`# proc: up`).
pub const EV_UP: u64 = 0x501;
/// Sự kiện: shell đã in bảng `ps` (evidence).
pub const EV_PS: u64 = 0x505;
/// Sự kiện: shell đã chạy `echo` (evidence).
pub const EV_ECHO: u64 = 0x506;
/// Sự kiện: shell đã chạy `help` (evidence).
pub const EV_HELP: u64 = 0x507;
/// Label message kernel chuyển tiếp `FaultKind::Exit{code}` sang procsrv (cầu nối arch, nợ ABI).
pub const FAULT_EXIT: u64 = 0x608;
