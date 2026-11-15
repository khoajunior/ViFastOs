// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-62 (K3.5 tầng 1) — **16 syscall `0..15` đi vào `Machine` thật**.
//!
//! Trước KT-62, ảnh test U-mode gọi `vifast-plat-common::user::dispatch` — một *shim* KT-53
//! biết đúng 3 syscall (`DebugPutStr` 12, `DebugHalt` 13, `Exit` 14) và trả `NotSupported` cho
//! phần còn lại. KT-62 xoá đường tạm đó: mọi số hiệu `0..15` đi qua **một** bảng dispatch ở
//! đây rồi vào API công khai của [`Machine`] (TCB/scheduler/IPC/Notification/WaitAddr/fault) —
//! không nhánh giả, không `todo!`.
//!
//! # Bảng 16 syscall → core (nguồn: `spec/abi.toml` `[syscall]`, D2-29/D2-79)
//!
//! | nr | tên | đường vào core |
//! |---|---|---|
//! | 0 | `Call` | [`Machine::call_msg`] (Endpoint; cap khác ⇒ `NotSupported`, D2-53 Q11) |
//! | 1 | `Send` | [`Machine::send`] (Endpoint/Reply/Notification) |
//! | 2 | `NBSend` | [`Machine::nb_send`] |
//! | 3 | `Recv` | [`Machine::recv`] |
//! | 4 | `NBRecv` | [`Machine::nb_recv`] (`Busy` khi không có message) |
//! | 5 | `ReplyRecv` | [`Machine::reply_recv`] |
//! | 6 | `NBSendRecv` | [`Machine::nb_send_recv`] (D2-53 Q15a) |
//! | 7 | `NBSendWait` | [`Machine::nb_send_wait`] (D2-53 Q15a) |
//! | 8 | `Yield` | [`Machine::yield_current`] + ngân sách còn lại |
//! | 9 | `WaitAddr` | [`Machine::wait_addr`] qua hook [`SyscallEnv::resolve_addr`] |
//! | 10 | `WakeAddr` | [`Machine::wake_addr`] qua cùng hook |
//! | 11 | `ClockRead` | hook [`SyscallEnv::now_ns`] (đồng hồ đơn điệu của nền tảng) |
//! | 12 | `DebugPutStr` | hook [`SyscallEnv::put_str`] (đọc bộ nhớ user bằng `SUM`) |
//! | 13 | `DebugHalt` | trả [`SyscallOutcome::Halt`] — arch quyết định dừng máy |
//! | 14 | `Exit` | [`Machine::exit`] (fault_ep ⇒ `BlockedOnSend{call:false, fault:Exit}`) |
//! | 15 | `DebugGetInfo` | hằng sinh từ spec (`vifast_abi`) + `hart_count` + `features` |
//!
//! **Ngoài dải `0..15` (kể cả EID SBI) ⇒ `RangeError` 19** — D2-79, giữ nguyên hành vi đã chốt
//! từ KT-53; đây là chỗ **duy nhất** quyết định (arch/plat không kiểm lại).
//!
//! # Biên giới (D2-83, một chiều)
//!
//! Core **không** đọc/ghi bộ nhớ user, không biết VSpace/PTE: chỗ nào cần (12 `DebugPutStr`,
//! 9/10 futex theo **địa chỉ user**) thì đi qua trait [`SyscallEnv`] do arch cài. `resolve_addr`
//! trả **nhãn PPN + offset trong trang** — đúng tinh thần D2-83 (core chỉ giữ *khoá* WaitAddr,
//! không sở hữu Frame/PTE).
//!
//! # Chưa làm (nợ K3.5, có chủ)
//!
//! - **Chuyển cap** trong message (`MessageInfo.caps > 0`) vẫn `InvalidArgument` — KT-63.
//! - **MR > 4** (MR4..MR63 qua IPC buffer) chưa đọc được ra/ghi vào user buffer ⇒ `InvalidArgument`
//!   ghi nợ; 4 MR thanh ghi `a3..a6` chạy đầy đủ.
//! - **Invocation** (Call trên cap không phải Endpoint) trả `NotSupported` theo D2-53 Q11, chưa
//!   mở bảng method.
//!
//! `#![forbid(unsafe_code)]` của crate áp dụng nguyên vẹn: tệp này chỉ gọi API an toàn.

use crate::error::{KError, ObjId, SlotRef};
use crate::ipc::{Message, MessageInfo, WORDS_IN_REGISTERS};
use crate::machine::Machine;
use crate::spec::SYSCALL_COUNT;
use crate::tcb::TcbState;

// ---------------------------------------------------------------------------------------------
// Hằng ABI — sinh từ `spec/abi.toml` (D2-11), không chép tay
// ---------------------------------------------------------------------------------------------

// D2-88(c): tên + số hiệu syscall **sinh từ** `spec/abi.toml` `[syscall]` (`vifast-gen`), không
// chép tay. Danh sách đủ 16 hằng ở [`crate::spec`]; ở đây tái xuất những số hiệu có nhánh riêng.
pub use crate::spec::{
    SYS_CLOCK_READ, SYS_DEBUG_GET_INFO, SYS_DEBUG_HALT, SYS_DEBUG_PUT_STR, SYS_EXIT, SYS_YIELD,
};

/// Số hiệu syscall lớn nhất của ABI (`[syscall]` 0..15; dải 16..31 reserved).
pub const SYSCALL_MAX: u64 = SYSCALL_COUNT as u64 - 1;

/// `abi_version` trả bởi `DebugGetInfo` — sinh từ spec.
pub const ABI_VERSION: u64 = crate::spec::ABI_VERSION as u64;
/// `min_compat` — sinh từ spec (`[abi].min_compat`).
pub const MIN_COMPAT: u64 = crate::spec::MIN_COMPAT as u64;

/// Bit `features` của `DebugGetInfo`: `DebugPutStr` (12) có mặt (spec không định nghĩa bitmap —
/// KT-62 dùng bit 0/1, ghi ở `docs/k3b-core.md` §11).
pub const FEATURE_DEBUG_PUT_STR: u64 = 1 << 0;
/// Bit `features`: `DebugHalt` (13) có mặt.
pub const FEATURE_DEBUG_HALT: u64 = 1 << 1;
/// Bit `features`: `WaitAddr`/`WakeAddr` dùng hook địa chỉ thật.
pub const FEATURE_WAIT_ADDR: u64 = 1 << 2;
/// Bit `features`: `Yield` có ngân sách MCS thật.
pub const FEATURE_MCS_YIELD: u64 = 1 << 3;

/// Trần số byte một lần `DebugPutStr` (chặn user giữ kernel trong vòng đọc vô hạn; `[TK]`).
pub const MAX_DEBUG_PUT: u64 = 4096;

/// `a1` của `Recv` / `NBSendRecv` khi **không** có cap Reply.
pub const REPLY_NONE: u64 = u64::MAX;

// ---------------------------------------------------------------------------------------------
// Kết quả dispatch
// ---------------------------------------------------------------------------------------------

/// Kết quả trả về user qua `a0/a1/a2/a3..a6` — đúng kênh `spec/abi.toml`
/// (`return_error = a0`, `return_value = a1`, `message_info = a2`, `mr = a3..a6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyscallReturn {
    /// `a0` — 0 = OK, 1..19 = mã lỗi ABI ([`KError::id`]).
    pub error: u64,
    /// `a1`.
    pub value: u64,
    /// `a2` — `MessageInfo` **đã đóng gói** (Recv/NBRecv/ReplyRecv/NBSendRecv/NBSendWait) hoặc 0.
    pub info: u64,
    /// `a3..a6` — MR0..MR3 (hoặc `min_compat`/`features`/`num_harts` của `DebugGetInfo` ở
    /// `mr[0..3]`).
    pub mr: [u64; 4],
}

impl SyscallReturn {
    /// Trả lỗi ABI `error` (`a1..a6` = 0).
    pub const fn err(error: u64) -> Self {
        Self {
            error,
            value: 0,
            info: 0,
            mr: [0; 4],
        }
    }

    /// Trả `(error, value)`.
    pub const fn new(error: u64, value: u64) -> Self {
        Self {
            error,
            value,
            info: 0,
            mr: [0; 4],
        }
    }
}

/// Việc arch phải làm sau khi core xử lý xong một syscall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyscallOutcome {
    /// Ghi `a0/a1/a2/a3..a6` rồi `sepc += 4` — TCB **vẫn** `Running` trên hart này.
    Return(SyscallReturn),
    /// `Exit(code)` — TCB đã vào `BlockedOnSend{fault_ep}` hoặc `Inactive`; **không** quay lại
    /// U-mode (QD §1 Q-A/D2-25).
    Exit {
        /// Mã thoát.
        code: u64,
    },
    /// `DebugHalt(code)` — arch log rồi dừng máy (chỉ ảnh test dùng).
    Halt {
        /// Mã halt.
        code: u64,
    },
    /// TCB **không còn** `Running` (chặn/ngủ/sẵn sàng) — arch phải lập lịch; KT-62 chưa có
    /// scheduler U-mode nên ảnh test coi đây là lỗi có chẩn đoán, TAP kernel đọc trạng thái.
    Switched {
        /// Trạng thái TCB sau syscall (đã qua `dispatch` nếu là `Ready`).
        state: TcbState,
    },
}

/// Hook **arch** cho những gì core không được/không thể làm (D2-83, D2-17).
///
/// `no_std`, không `unsafe`: arch cài bằng asm/`SUM`/CSR của chính nó.
pub trait SyscallEnv {
    /// Đọc `len` byte user tại `ptr` và in ra console; trả số byte đã in hoặc **mã lỗi ABI**
    /// (1..19).
    fn put_str(&mut self, ptr: u64, len: u64) -> Result<u64, u64>;
    /// Dịch địa chỉ user `va` thành **nhãn** `(ppn, offset_trong_trang)` cho WaitAddr — D2-83:
    /// core chỉ giữ khoá, không sở hữu Frame/PTE. Lỗi ⇒ mã ABI (vd 11 `NotMapped`).
    fn resolve_addr(&mut self, va: u64) -> Result<(u32, u32), u64>;
    /// Đồng hồ đơn điệu (ns) — `ClockRead` 11.
    fn now_ns(&mut self) -> u64;
    /// Bitmap `features` trả bởi `DebugGetInfo` (bit 0/1/2/3 — xem hằng `FEATURE_*`).
    fn features(&mut self) -> u64;
    /// Số hart **nền tảng đang online** (≤ 8) — D2-88(a): `DebugGetInfo.num_harts` là số hart
    /// phần cứng (arch đọc SBI HSM), **không** phải số hart mà core model đang chạy.
    fn num_harts(&mut self) -> u64;
}

// ---------------------------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------------------------

/// `slot` của con trỏ cap user trong **CSpace của TCB**; TCB chưa cấu hình cspace hoặc cap index
/// quá lớn ⇒ `InvalidCapability` (1).
fn cap_slot(m: &Machine, tcb: ObjId, cptr: u64) -> Result<SlotRef, KError> {
    let cspace = m
        .tcb_info_obj(tcb)
        .map_err(|_| KError::InvalidCapability)?
        .cspace
        .ok_or(KError::InvalidCapability)?;
    let slot = usize::try_from(cptr).map_err(|_| KError::InvalidCapability)?;
    // Chỉ số cap ngoài bảng slot của CSpace = **cap không hợp lệ** (1), không phải `RangeError`:
    // 19 dành cho **số hiệu syscall** ngoài dải (D2-79). `cap_at` của core trả `RangeError` cho
    // chỉ số ngoài bảng nên chặn ở đây để ngữ nghĩa ABI không lẫn (ghi docs §11).
    if slot >= m.cnode_slot_count(cspace)? {
        return Err(KError::InvalidCapability);
    }
    Ok(SlotRef::new(cspace, slot))
}

/// Dựng `Message` từ `MessageInfo` thô + tối đa 4 MR thanh ghi.
///
/// `length > mr.len()` ⇒ `InvalidArgument` (MR4..MR63 qua IPC buffer là nợ K3.5 — ghi ở đầu tệp);
/// `info.caps > 0` ⇒ `InvalidArgument` (chuyển cap = KT-63).
fn build_message(raw_info: u64, mrs: &[u64]) -> Result<Message, KError> {
    let info = MessageInfo::unpack(raw_info)?;
    let len = info.length() as usize;
    if len > mrs.len() {
        return Err(KError::InvalidArgument);
    }
    Message::new(info, &mrs[..len])
}

/// `true` ⇔ cap tại `slot` là **Notification** (nhánh trả `word/badge`, không có `MessageInfo`).
fn cap_is_notification(m: &Machine, slot: SlotRef) -> bool {
    m.cap_at(slot.cnode, slot.slot)
        .ok()
        .and_then(|cap| m.cap_info(cap).ok())
        .map(|info| info.ty == crate::error::ObjType::Notification)
        .unwrap_or(false)
}

/// Payload Notification **vừa nhận** vào TCB (`word`/`badge`): Notification không có
/// `MessageInfo` ⇒ `a1 = value = word`, `mr[0] = word`, `mr[1] = badge` (docs §11.2). `None` nếu
/// TCB chưa nhận payload nào.
fn notification_return(m: &mut Machine, tcb: ObjId, error: u64) -> Option<SyscallReturn> {
    let value = m.take_tcb_notification(tcb).ok().flatten()?;
    let mut r = SyscallReturn::new(error, value.word);
    r.mr[0] = value.word;
    r.mr[1] = value.badge;
    Some(r)
}

/// `true` ⇔ TCB đang `Running` — syscall trả về user được.
fn is_running(m: &Machine, tcb: ObjId) -> bool {
    matches!(m.tcb_state(tcb), Ok(TcbState::Running { .. }))
}

/// Trả `info` + MR0..MR3 của message **vừa nhận** trong vùng nhận IPC của TCB.
fn received(m: &Machine, tcb: ObjId, error: u64, value: u64) -> SyscallReturn {
    let Ok(area) = m.tcb_ipc_area(tcb) else {
        return SyscallReturn::new(error, value);
    };
    let mut out = SyscallReturn::new(error, value);
    if let Some(info) = area.info {
        out.info = info.pack();
    }
    for i in 0..out.mr.len() {
        out.mr[i] = area.mr(i).unwrap_or(0);
    }
    out
}

/// Sau một thao tác IPC: TCB còn `Running` ⇒ trả về user; `Ready` ⇒ `dispatch` lại (Reply/
/// Signal làm TCB về `Ready` — mô hình một-hart, nếu chính nó được chọn thì tiếp tục); ngược lại
/// ⇒ [`SyscallOutcome::Switched`].
/// Số MR của message **vừa nhận** (0 nếu vùng nhận chưa có `MessageInfo`).
fn recv_len(m: &Machine, tcb: ObjId) -> u64 {
    m.tcb_ipc_area(tcb)
        .ok()
        .and_then(|a| a.info)
        .map(|i| u64::from(i.length()))
        .unwrap_or(0)
}

fn after_ipc(m: &mut Machine, tcb: ObjId, hart: u16, r: SyscallReturn) -> SyscallOutcome {
    if is_running(m, tcb) {
        return SyscallOutcome::Return(received(m, tcb, r.error, r.value));
    }
    if matches!(m.tcb_state(tcb), Ok(TcbState::Ready)) {
        let _ = m.dispatch(hart);
        if is_running(m, tcb) {
            return SyscallOutcome::Return(received(m, tcb, r.error, r.value));
        }
    }
    let state = m.tcb_state(tcb).unwrap_or(TcbState::Inactive);
    SyscallOutcome::Switched { state }
}

/// Một syscall đi vào [`Machine`]. **Không panic** với mọi `nr`/tham số.
///
/// `tcb` là TCB gọi (đang `Running` trên `hart`); `args` = `a0..a5`.
pub fn dispatch(
    m: &mut Machine,
    tcb: ObjId,
    hart: u16,
    nr: u64,
    args: [u64; 6],
    env: &mut impl SyscallEnv,
) -> SyscallOutcome {
    // D2-79: số syscall là **chỉ số** vào bảng dispatch ⇒ ngoài 0..15 là `RangeError` 19.
    if nr >= SYSCALL_COUNT as u64 {
        return SyscallOutcome::Return(SyscallReturn::err(KError::RangeError.id() as u64));
    }
    match nr {
        // ---------------------------------------------------------------- IPC 0..7
        0 => match cap_slot(m, tcb, args[0])
            .and_then(|ep| build_message(args[1], &args[2..6]).map(|msg| (ep, msg)))
        {
            Ok((ep, msg)) => match m.call_msg(tcb, ep, msg) {
                Ok(()) => after_ipc(m, tcb, hart, SyscallReturn::new(0, 0)),
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            },
            Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
        },
        1 => match cap_slot(m, tcb, args[0])
            .and_then(|cap| build_message(args[1], &args[2..6]).map(|msg| (cap, msg)))
        {
            Ok((cap, msg)) => match m.send(tcb, cap, msg) {
                Ok(()) => after_ipc(m, tcb, hart, SyscallReturn::new(0, 0)),
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            },
            Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
        },
        2 => match cap_slot(m, tcb, args[0])
            .and_then(|cap| build_message(args[1], &args[2..6]).map(|msg| (cap, msg)))
        {
            Ok((cap, msg)) => match m.nb_send(tcb, cap, msg) {
                Ok(()) => after_ipc(m, tcb, hart, SyscallReturn::new(0, 0)),
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            },
            Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
        },
        3 => {
            let reply = if args[1] == REPLY_NONE {
                None
            } else {
                match cap_slot(m, tcb, args[1]) {
                    Ok(slot) => Some(slot),
                    Err(e) => {
                        return SyscallOutcome::Return(SyscallReturn::err(e.id() as u64));
                    }
                }
            };
            let (target, is_ntfn) = match cap_slot(m, tcb, args[0]) {
                Ok(slot) => (slot, cap_is_notification(m, slot)),
                Err(e) => return SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            };
            match m.recv(tcb, target, reply) {
                Ok(()) => {
                    // Notification: payload `(word, badge)` trong `tcb.notif` — **không** đọc
                    // `ipc_area` (có thể là message Endpoint cũ ⇒ stale).
                    if is_ntfn {
                        if is_running(m, tcb) {
                            if let Some(r) = notification_return(m, tcb, 0) {
                                return SyscallOutcome::Return(r);
                            }
                        }
                        return SyscallOutcome::Switched {
                            state: m.tcb_state(tcb).unwrap_or(TcbState::Inactive),
                        };
                    }
                    // Endpoint: giao ngay ⇒ `value` = số MR; chặn ⇒ 0.
                    let value = if is_running(m, tcb) {
                        recv_len(m, tcb)
                    } else {
                        0
                    };
                    after_ipc(m, tcb, hart, SyscallReturn::new(0, value))
                }
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            }
        }
        4 => {
            let (target, is_ntfn) = match cap_slot(m, tcb, args[0]) {
                Ok(slot) => (slot, cap_is_notification(m, slot)),
                Err(e) => return SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            };
            match m.nb_recv(tcb, target) {
                Ok(Some(msg)) => {
                    if is_ntfn {
                        // `notification_message`: MR0 = word, `badge` = badge.
                        let mut r = SyscallReturn::new(0, msg.mr[0]);
                        r.mr[0] = msg.mr[0];
                        r.mr[1] = msg.badge;
                        SyscallOutcome::Return(r)
                    } else {
                        SyscallOutcome::Return(received(m, tcb, 0, msg.info.length() as u64))
                    }
                }
                Ok(None) => SyscallOutcome::Return(SyscallReturn::err(KError::Busy.id() as u64)),
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            }
        }
        5 => {
            let built = cap_slot(m, tcb, args[0]).and_then(|reply| {
                cap_slot(m, tcb, args[1])
                    .and_then(|ep| build_message(args[2], &args[3..6]).map(|msg| (reply, ep, msg)))
            });
            match built {
                Ok((reply, ep, msg)) => match m.reply_recv(tcb, reply, ep, msg, None) {
                    Ok(()) => after_ipc(m, tcb, hart, SyscallReturn::new(0, 0)),
                    Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
                },
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            }
        }
        6 => {
            // 3 cap nên `MessageInfo` đi `a3`, MR0..MR2 ở `a4..a6` (ghi ở docs §11).
            let recv_reply = if args[2] == REPLY_NONE {
                None
            } else {
                match cap_slot(m, tcb, args[2]) {
                    Ok(slot) => Some(slot),
                    Err(e) => {
                        return SyscallOutcome::Return(SyscallReturn::err(e.id() as u64));
                    }
                }
            };
            let built = cap_slot(m, tcb, args[0]).and_then(|send| {
                cap_slot(m, tcb, args[1]).and_then(|recv| {
                    build_message(args[3], &args[4..6]).map(|msg| (send, recv, msg))
                })
            });
            match built {
                Ok((send, recv, msg)) => match m.nb_send_recv(tcb, send, msg, recv, recv_reply) {
                    Ok(send_err) => {
                        let error = send_err.map(|e| e.id() as u64).unwrap_or(0);
                        after_ipc(m, tcb, hart, SyscallReturn::new(error, 0))
                    }
                    Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
                },
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            }
        }
        7 => {
            let built = cap_slot(m, tcb, args[0]).and_then(|send| {
                cap_slot(m, tcb, args[1]).and_then(|ntfn| {
                    build_message(args[2], &args[3..6]).map(|msg| (send, ntfn, msg))
                })
            });
            match built {
                Ok((send, ntfn, msg)) => match m.nb_send_wait(tcb, send, msg, ntfn) {
                    Ok(send_err) => {
                        let error = send_err.map(|e| e.id() as u64).unwrap_or(0);
                        // Wait giao ngay ⇒ payload `(word, badge)`; chặn ⇒ Switched.
                        if is_running(m, tcb) {
                            if let Some(r) = notification_return(m, tcb, error) {
                                return SyscallOutcome::Return(r);
                            }
                            return SyscallOutcome::Return(SyscallReturn::new(error, 0));
                        }
                        SyscallOutcome::Switched {
                            state: m.tcb_state(tcb).unwrap_or(TcbState::Inactive),
                        }
                    }
                    Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
                },
                Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
            }
        }
        // ---------------------------------------------------------------- scheduler 8
        SYS_YIELD => match m.yield_current(hart) {
            Ok(next) => {
                let value = m.tcb_remaining(tcb);
                if next == Some(tcb) || is_running(m, tcb) {
                    SyscallOutcome::Return(SyscallReturn::new(0, value))
                } else {
                    let state = m.tcb_state(tcb).unwrap_or(TcbState::Inactive);
                    SyscallOutcome::Switched { state }
                }
            }
            Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
        },
        // ---------------------------------------------------------------- futex 9/10
        9 => match env.resolve_addr(args[0]) {
            Ok((frame, offset)) => {
                let expected = args[1];
                match m.wait_addr_obj(tcb, frame, offset, expected) {
                    Ok(()) => SyscallOutcome::Switched {
                        state: TcbState::BlockedOnAddress { frame, offset },
                    },
                    Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
                }
            }
            Err(code) => SyscallOutcome::Return(SyscallReturn::err(code)),
        },
        10 => match env.resolve_addr(args[0]) {
            Ok((frame, offset)) => {
                let n = u32::try_from(args[1]).unwrap_or(u32::MAX);
                match m.wake_addr(frame, offset, n) {
                    Ok(woken) => SyscallOutcome::Return(SyscallReturn::new(0, u64::from(woken))),
                    Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
                }
            }
            Err(code) => SyscallOutcome::Return(SyscallReturn::err(code)),
        },
        // ---------------------------------------------------------------- clock/debug 11..15
        11 => SyscallOutcome::Return(SyscallReturn::new(0, env.now_ns())),
        SYS_DEBUG_PUT_STR => {
            let (ptr, len) = (args[0], args[1]);
            if len > MAX_DEBUG_PUT {
                return SyscallOutcome::Return(SyscallReturn::err(
                    KError::InvalidArgument.id() as u64
                ));
            }
            match env.put_str(ptr, len) {
                Ok(bytes) => SyscallOutcome::Return(SyscallReturn::new(0, bytes)),
                Err(code) => SyscallOutcome::Return(SyscallReturn::err(code)),
            }
        }
        SYS_DEBUG_HALT => SyscallOutcome::Halt { code: args[0] },
        SYS_EXIT => match m.exit(tcb, args[0]) {
            Ok(()) => SyscallOutcome::Exit { code: args[0] },
            Err(e) => SyscallOutcome::Return(SyscallReturn::err(e.id() as u64)),
        },
        SYS_DEBUG_GET_INFO => {
            let mut r = SyscallReturn::new(0, ABI_VERSION);
            r.mr[0] = MIN_COMPAT;
            r.mr[1] = env.features();
            r.mr[2] = env.num_harts();
            SyscallOutcome::Return(r)
        }
        // `nr < SYSCALL_COUNT` bao trùm mọi nhánh ở trên; nhánh này chỉ để compiler thấy đủ.
        _ => SyscallOutcome::Return(SyscallReturn::err(KError::RangeError.id() as u64)),
    }
}

/// `true` ⇔ TCB có `cspace` (dùng cho chẩn đoán/TAP).
pub fn tcb_has_cspace(m: &Machine, tcb: ObjId) -> bool {
    m.tcb_info_obj(tcb)
        .map(|i| i.cspace.is_some())
        .unwrap_or(false)
}

/// `WORDS_IN_REGISTERS` — số MR đi trong thanh ghi (`spec/abi.toml` = 4).
pub const MR_IN_REGS: usize = WORDS_IN_REGISTERS;
