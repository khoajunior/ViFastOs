// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-63 — **finalise IPC** khi `Revoke`/`Delete`: huỷ `Reply.caller` (caller `Cancelled +
//! Ready`), đánh thức Recv/Notification waiter của object bị finalise (chống treo), gỡ Reply
//! cap đã chết khỏi waiter, và đếm message bị huỷ (`ipc_dropped`) — port ngữ nghĩa sim
//! (`cancel_reply`/`clear_dead_reply_in_waiters`/`purge_dead_refs`), tách khỏi `core_ipc.rs`
//! để giữ R1 ≤ 1.500 dòng.
//!
//! **Khác biệt có chủ đích so với sim:** sim để Recv/Notification waiter treo trên object đã
//! finalise (không có ca); core đánh thức họ (`Cancelled + Ready`) — chống treo vĩnh viễn.
//! Parity loại trừ đúng đường này; ca core-only pin ở `kt63_core_cap_ipc.rs`.

use crate::error::{KError, ObjId};
use crate::machine::Machine;
use crate::tcb::TcbState;

impl Machine {
    // ------------------------------------------------- finalise IPC (KT-63)

    /// **KT-63** — `Reply` bị xoá/revoke khi caller đang `BlockedOnReply` (D2-53 **Q8**):
    /// lấy `Reply.caller`, đặt lỗi IPC `Cancelled` cho caller rồi đánh thức caller `Ready`
    /// (như `cancel_reply` của sim). Không deadlock, `Reply.caller == None` sau khi huỷ; đi
    /// qua **một** cửa [`Machine::transition_tcb`] ⇒ hook KT-21 tự trả donation (không rò SC).
    /// Trả `Some(caller)` nếu đã đánh thức ai. Idempotent (`caller = None` sau lần đầu).
    pub(crate) fn cancel_reply_object(&mut self, reply_obj: ObjId) -> Option<ObjId> {
        let caller = match self.reply_data_mut(reply_obj) {
            Ok(r) => r.caller.take(),
            Err(_) => return None,
        };
        if let Some(c) = caller {
            let _ = self.set_last_ipc_error(c, KError::Cancelled);
            if matches!(
                self.tcb_state(c),
                Ok(TcbState::BlockedOnReply { reply }) if reply == reply_obj
            ) {
                let _ = self.transition_tcb(c, TcbState::Ready);
            }
        }
        caller
    }

    /// **KT-63** — gỡ **Reply cap đã chết** khỏi Recv waiter của mọi Endpoint (như sim): waiter
    /// vẫn chờ nhưng coi như **không cấp Reply** ⇒ `Call` tới đi nhánh Q7(b) (message vẫn giao
    /// + caller `Cancelled`) thay vì lỗi cứng `InvalidCapability`.
    pub(crate) fn clear_dead_reply_in_waiters(&mut self) {
        for i in 0..self.n_endpoints {
            let Some((tcb, Some(cap))) = self.endpoints[i].recv else {
                continue;
            };
            let alive = self
                .caps
                .get(cap as usize - 1)
                .map(|c| c.alive)
                .unwrap_or(false);
            if !alive {
                self.endpoints[i].recv = Some((tcb, None));
            }
        }
    }

    /// **KT-63** — finalise **body Endpoint** trước khi trả free-list: đánh thức Recv waiter
    /// (`Cancelled + Ready` — chống treo khi Endpoint chết) và đếm message người dùng bị huỷ
    /// vào `ipc_dropped` (bất biến bảo toàn message).
    ///
    /// **Khác biệt có chủ đích so với sim** (ghi `docs/k3b-core.md` §12): sim để waiter treo
    /// trên Endpoint đã finalise (không có ca); core chọn anti-hang. Parity loại trừ đúng
    /// đường này và có ca core-only pin (`kt63_core_recv_waiter_thuc_khi_endpoint_bi_finalise`).
    pub(crate) fn finalise_endpoint_body(&mut self, ep: ObjId) {
        let (recv, dropped) = match self.endpoint_data(ep) {
            Ok(e) => {
                let dropped = e
                    .send
                    .iter()
                    .filter(|mi| self.ipc_msgs[*mi as usize].msg.is_some())
                    .count();
                (e.recv, dropped)
            }
            Err(_) => return,
        };
        self.ipc_dropped += dropped as u64;
        let Some((w, _)) = recv else { return };
        if matches!(self.tcb_state(w), Ok(TcbState::BlockedOnRecv { ep: e2, .. }) if e2 == ep) {
            let _ = self.set_last_ipc_error(w, KError::Cancelled);
            let _ = self.transition_tcb(w, TcbState::Ready);
        }
    }

    /// **KT-63** — finalise **body Notification**: đánh thức waiter (`Cancelled + Ready`) trước
    /// khi body về free-list — cùng lý do chống treo với [`Machine::finalise_endpoint_body`].
    pub(crate) fn finalise_notification_body(&mut self, ntfn: ObjId) {
        let waiter = match self.notification_data(ntfn) {
            Ok(n) => n.waiter,
            Err(_) => return,
        };
        if let Some(w) = waiter {
            if matches!(
                self.tcb_state(w),
                Ok(TcbState::BlockedOnNotification { ntfn: n2 }) if n2 == ntfn
            ) {
                let _ = self.set_last_ipc_error(w, KError::Cancelled);
                let _ = self.transition_tcb(w, TcbState::Ready);
            }
        }
    }
}
