// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-63 — **chuyển cap qua IPC** (port `resolve_caps`/`prepare_caps`/`deliver_caps` của sim,
//! tách khỏi `core_ipc.rs` để giữ R1 ≤ 1.500 dòng): chụp ảnh cap nguồn ở `admit`, lập kế hoạch
//! [`crate::ipc::plan_cap_transfer`] theo receive window của bên nhận, cài cap con **bỏ
//! `RIGHT_GRANT`** vào window và cập nhật `MessageInfo.caps`/`extra_caps_unwrapped`/`badge`.
//!
//! Model-only `[TK]`, không phải ABI. Message **không** copy object — chỉ cap tham chiếu.

use crate::error::{KError, ObjId};
use crate::ipc::{
    plan_cap_transfer, transfer, CapPlan, CapSnapshot, Message, MAX_CAPS as IPC_MAX_CAPS,
};
use crate::machine::{Machine, MAX_CAPS};
use crate::spec::RIGHT_GRANT;

impl Machine {
    /// **KT-63** — chụp ảnh cap nguồn của message: **lỗi cứng** `InvalidCapability` khi cap
    /// chết/không tồn tại, **trước** khi cấp `msg_id` (message không bị giao nửa chừng —
    /// KT-25 DoD 4, như `resolve_caps` của sim).
    pub(crate) fn resolve_caps(&self, msg: &mut Message) -> Result<(), KError> {
        for i in 0..msg.n_caps as usize {
            let c = self.cap(msg.caps[i])?;
            if !c.alive {
                return Err(KError::InvalidCapability);
            }
            msg.snaps[i] = CapSnapshot {
                cap: msg.caps[i],
                obj: c.obj,
                ty: c.ty,
                rights: c.rights,
                badge: c.badge,
            };
        }
        msg.n_snaps = msg.n_caps;
        Ok(())
    }

    /// **KT-63** — lập kế hoạch chuyển cap vào receive window của `receiver` (như sim):
    /// message **không** cap ⇒ `None`; window **chưa bind** mà message có cap **cần cài** ⇒
    /// `InvalidArgument` (lỗi cấu hình, **trước** khi message được giao); ngược lại ⇒
    /// [`plan_cap_transfer`] với ô nào đang có cap (thất bại **mềm**, D2-53 Q6).
    pub(crate) fn prepare_caps(
        &self,
        receiver: ObjId,
        snaps: &[CapSnapshot],
        endpoint: Option<ObjId>,
    ) -> Result<Option<CapPlan>, KError> {
        if snaps.is_empty() {
            return Ok(None);
        }
        let window = self.tcb_data(receiver)?.recv_window;
        let mut occupied = [false; IPC_MAX_CAPS];
        if let Some(window) = window.as_ref() {
            for (i, w) in window.iter().enumerate() {
                occupied[i] = self.cnode_slot(w.cnode, w.slot)?.is_some();
            }
        }
        let plan = plan_cap_transfer(snaps, window.as_ref(), &occupied, endpoint)?;
        // **F1 (reviewer-kt63)** — bảng cap **đầy** thì lỗi **nguyên tử** `NotEnoughMemory`
        // **trước** `admit`/pop/take-waiter: `push_cap` không được panic giữa message.
        // (`derive` cũng cần kiểm tương tự — ghi nợ, không thuộc đường IPC này.)
        if self.n_caps + plan.installed() > MAX_CAPS {
            return Err(KError::NotEnoughMemory);
        }
        // **F2 (reviewer-kt63)** — ô đích phải thuộc CNode **còn sống** + slot hợp lệ: window
        // trỏ CNode đã finalise ⇒ `InvalidCapability` **trước** `admit`, không tạo cap mồ côi
        // rồi `cnode_set_slot` lỗi giữa đường.
        for &(_, dst) in plan.install() {
            if !self.object_alive(dst.cnode) {
                return Err(KError::InvalidCapability);
            }
            self.check_cnode_slot(dst.cnode, dst.slot)?;
        }
        Ok(Some(plan))
    }

    /// **KT-63** — cài cap theo kế hoạch vào receive window và cập nhật `MessageInfo` của
    /// message **nhận**: `caps` = số cap **cài được**, `extra_caps_unwrapped` = số cap unwrap,
    /// `badge` = OR các badge unwrap (port `deliver_caps` của sim). Cap nguồn chết giữa lúc
    /// xếp hàng và lúc giao ⇒ thất bại **mềm** (không tạo cap mồ côi, không mất message).
    /// Cap nhận **mất `RIGHT_GRANT`** (thiết kế §4.6 `:319`), giữ badge nguồn.
    fn deliver_caps(
        &mut self,
        msg: &mut Message,
        snaps: &[CapSnapshot],
        plan: Option<&CapPlan>,
    ) -> Result<(), KError> {
        if snaps.is_empty() {
            return Ok(());
        }
        let Some(plan) = plan else {
            return Err(KError::InvalidArgument);
        };
        let mut installed = 0u8;
        for &(idx, dst) in plan.install() {
            let snap = snaps[idx as usize];
            let alive = snap.cap > 0
                && self
                    .caps
                    .get(snap.cap as usize - 1)
                    .map(|c| c.alive)
                    .unwrap_or(false);
            if !alive {
                self.cap_transfer_failed += 1;
                continue;
            }
            let rights = snap.rights & !RIGHT_GRANT;
            let cap = self.push_cap(snap.obj, snap.ty, Some(snap.cap), rights, snap.badge);
            self.cnode_set_slot(dst.cnode, dst.slot, Some(cap))?;
            self.caps[cap as usize - 1].slot = Some(dst);
            installed += 1;
        }
        self.cap_transfer_failed += plan.skipped() as u64;
        self.cap_transfer_installed += installed as u64;
        self.cap_unwrapped += plan.unwrapped() as u64;
        msg.info = msg
            .info
            .with_caps(installed)?
            .with_extra_caps_unwrapped(plan.unwrapped() as u8)?;
        msg.badge |= plan.badge();
        // Cap đã cài vào window của bên nhận; message nhận **không** còn `CapId` nguồn.
        msg.clear_caps();
        Ok(())
    }

    /// Giao message vào vùng nhận của TCB qua **`ipc::transfer`** (đường copy duy nhất).
    fn deliver_to(&mut self, receiver: ObjId, msg: &Message) -> Result<(), KError> {
        let t = self.tcb_data_mut(receiver)?;
        transfer(msg, &mut t.ipc);
        self.ipc_delivered += 1;
        self.transfer_calls += 1;
        Ok(())
    }

    /// Giao message cho TCB nhận: chuyển cap theo kế hoạch rồi copy message (KT-63).
    pub(crate) fn deliver_message(
        &mut self,
        receiver: ObjId,
        msg: &mut Message,
        plan: Option<&CapPlan>,
    ) -> Result<(), KError> {
        let mut snaps = [CapSnapshot::EMPTY; MAX_CAPS];
        let n = msg.n_snaps as usize;
        snaps[..n].copy_from_slice(&msg.snaps[..n]);
        self.deliver_caps(msg, &snaps[..n], plan)?;
        self.deliver_to(receiver, msg)
    }
}
