// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ruột TCB (port `TcbData`) và ruột SchedContext (`SchedData`) — KT-59, tách khỏi
//! `machine.rs` để giữ giới hạn dòng mỗi tệp (QD §3.15); KT-60 thêm vùng nhận IPC + trường
//! `last_ipc_error`/`last_fault`/`notif`.
//!
//! Model-only `[TK]`, **không** phải ABI: `Tcb.ReadState` chỉ trả 4 giá trị; các trường còn
//! lại là model API của sim (oracle) và được parity so từng bước.

use crate::error::{CapId, KError, ObjId};
use crate::ipc::IpcArea;
use crate::mcs::ScBudget;
use crate::tcb::{TcbInfo, TcbState};

/// Ruột TCB — port `TcbData` của sim (D2-33/§4.4), bỏ các trường IPC của KT-24..28
/// (`last_ipc_error`/`recv_window`/`last_fault`/`notif` — KT-60).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TcbData {
    pub(crate) state: TcbState,
    pub(crate) suspended: bool,
    pub(crate) exit_code: Option<u64>,
    /// Cap `fault_ep` (`Tcb.SetFaultEp`).
    pub(crate) fault_ep: Option<CapId>,
    /// CSpace của TCB (object CNode).
    pub(crate) cspace: Option<ObjId>,
    pub(crate) priority: u8,
    pub(crate) max_controlled_priority: u8,
    /// HartMask ≤ 64 bit.
    pub(crate) affinity: u64,
    pub(crate) home: u16,
    /// SchedContext riêng (`set_sched_params`).
    pub(crate) sched_context: Option<ObjId>,
    /// SC đang nhận donation (KT-21 — chưa có đường tạo ở KT-59, luôn `None`).
    pub(crate) donated_sc: Option<ObjId>,
    /// Notification đang bind (KT-27).
    pub(crate) bound_notification: Option<ObjId>,
    /// Vùng nhận IPC: 4 MR thanh ghi + 60 word buffer + message vừa nhận (KT-60).
    pub(crate) ipc: IpcArea,
    /// Lỗi IPC kernel gán ở lần giao gần nhất (`Cancelled` — D2-53 Q7(b), KT-60).
    pub(crate) last_ipc_error: Option<KError>,
    /// `(sender, FaultKind)` của **fault vừa giao** cho TCB làm bên nhận (KT-26, KT-60).
    pub(crate) last_fault: Option<(ObjId, crate::tcb::FaultKind)>,
    /// `(word, badge)` của lần `Wait`/`Poll` Notification gần nhất (KT-27, KT-60).
    pub(crate) notif: Option<(u64, u64)>,
}

impl TcbData {
    pub(crate) const EMPTY: Self = Self {
        state: TcbState::Inactive,
        suspended: false,
        exit_code: None,
        fault_ep: None,
        cspace: None,
        priority: 0,
        max_controlled_priority: 0,
        affinity: 1,
        home: 0,
        sched_context: None,
        donated_sc: None,
        bound_notification: None,
        ipc: IpcArea::new(),
        last_ipc_error: None,
        last_fault: None,
        notif: None,
    };

    pub(crate) fn snapshot(&self) -> TcbInfo {
        TcbInfo {
            state: self.state,
            suspended: self.suspended,
            exit_code: self.exit_code,
            fault_ep: self.fault_ep,
            cspace: self.cspace,
            priority: self.priority,
            max_controlled_priority: self.max_controlled_priority,
            affinity: self.affinity,
            home: self.home,
            sched_context: self.sched_context,
            donated_sc: self.donated_sc,
            bound_notification: self.bound_notification,
        }
    }
}

/// Ruột `SchedContext` — port `SchedData` của sim, bỏ phần `bound` (KT-27).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScData {
    pub(crate) budget: ScBudget,
    /// TCB đang nhận donation (KT-21 — chưa tạo ở KT-59).
    pub(crate) donated_to: Option<ObjId>,
    /// TCB đang cho mượn SC (KT-21 — chưa tạo ở KT-59).
    pub(crate) donor: Option<ObjId>,
}

impl ScData {
    pub(crate) const EMPTY: Self = Self {
        budget: ScBudget::new(),
        donated_to: None,
        donor: None,
    };
}
