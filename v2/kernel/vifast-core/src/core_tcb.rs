// SPDX-License-Identifier: MIT OR Apache-2.0
//! Phần `impl Machine` của **KT-59**: TCB + transition + scheduler + ngân sách MCS + hộp thư.
//!
//! Tách khỏi `machine.rs` để giữ luật cấu trúc **R1** (mỗi tệp ≤ 1.500 dòng); toàn bộ trạng
//! thái vẫn nằm trong [`Machine`] (`machine.rs`), hai tệp chỉ là hai khối `impl Machine` của
//! **cùng một kiểu** — không có kiểu/trạng thái thứ hai. Port nguyên ngữ nghĩa sim (xem
//! `docs/k3b-core.md` §7); biên giới KT-59: chưa có IPC/Reply/Notification (KT-60), donation
//! KT-21 chỉ giữ trường + công thức.

use crate::error::{KError, ObjId, ObjType, SlotRef};
use crate::machine::{
    FinaliseEntry, Machine, ScData, TcbData, BODY_SC, BODY_TCB, MAX_FINALISE, MAX_SCHED_CONTEXTS,
    MAX_TCBS,
};
use crate::mcs::{donated_priority, ScInfo};
use crate::tcb::{SleepReason, SuspendMode, TcbInfo, TcbSnapshot, TcbState, TCB_STATE_COUNT};

impl Machine {
    // ------------------------------------------------------- body TCB/SchedContext

    /// Body TCB của object — lỗi đúng như sim: object ngoài bảng/body đã chết ⇒
    /// `InvalidCapability`; object sống sai kiểu ⇒ `WrongObjectType`.
    pub(crate) fn tcb_data(&self, obj: ObjId) -> Result<&TcbData, KError> {
        let idx = self.object_index(obj)?;
        let m = &self.objects[idx];
        match m.body.kind {
            BODY_TCB => {
                if m.body.alive {
                    Ok(&self.tcbs[m.body.idx as usize])
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            BODY_SC => {
                if m.body.alive {
                    Err(KError::WrongObjectType)
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            _ => Err(KError::WrongObjectType),
        }
    }

    pub(crate) fn tcb_data_mut(&mut self, obj: ObjId) -> Result<&mut TcbData, KError> {
        let idx = self.object_index(obj)?;
        let m = &self.objects[idx];
        let body = m.body;
        match body.kind {
            BODY_TCB => {
                if body.alive {
                    Ok(&mut self.tcbs[body.idx as usize])
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            BODY_SC => {
                if body.alive {
                    Err(KError::WrongObjectType)
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            _ => Err(KError::WrongObjectType),
        }
    }

    /// Body SchedContext của object — cùng quy ước lỗi như [`Machine::tcb_data`].
    pub(crate) fn sc_data(&self, obj: ObjId) -> Result<&ScData, KError> {
        let idx = self.object_index(obj)?;
        let m = &self.objects[idx];
        match m.body.kind {
            BODY_SC => {
                if m.body.alive {
                    Ok(&self.scs[m.body.idx as usize])
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            BODY_TCB => {
                if m.body.alive {
                    Err(KError::WrongObjectType)
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            _ => Err(KError::WrongObjectType),
        }
    }

    fn sc_data_mut(&mut self, obj: ObjId) -> Result<&mut ScData, KError> {
        let idx = self.object_index(obj)?;
        let body = self.objects[idx].body;
        match body.kind {
            BODY_SC => {
                if body.alive {
                    Ok(&mut self.scs[body.idx as usize])
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            BODY_TCB => {
                if body.alive {
                    Err(KError::WrongObjectType)
                } else {
                    Err(KError::InvalidCapability)
                }
            }
            _ => Err(KError::WrongObjectType),
        }
    }

    /// `require_type` của sim: cap phải sống và đúng **kiểu cap**, trả `ObjId`.
    fn require_type(&self, slot: SlotRef, ty: ObjType) -> Result<ObjId, KError> {
        let cap = self.cap_at(slot.cnode, slot.slot)?;
        let c = self.cap(cap)?;
        if c.ty != ty {
            return Err(KError::WrongObjectType);
        }
        Ok(c.obj)
    }

    // =========================================================================================
    // KT-59 — TCB / transition / scheduler / hộp thư (port từ sim `kobj.rs`)
    // =========================================================================================

    // ------------------------------------------------------------- cấu hình TCB

    /// `Tcb.Configure`: gắn CSpace; **xoá `exit_code`** (D2-25: `Configure`/`Resume` xoá).
    pub fn tcb_configure(&mut self, tcb: SlotRef, cspace: Option<SlotRef>) -> Result<(), KError> {
        let tcb_obj = self.require_type(tcb, ObjType::Tcb)?;
        let cspace_obj = match cspace {
            Some(s) => Some(self.require_type(s, ObjType::CNode)?),
            None => None,
        };
        {
            let t = self.tcb_data_mut(tcb_obj)?;
            t.cspace = cspace_obj;
            t.exit_code = None;
        }
        self.sched_after_op(false);
        Ok(())
    }

    /// `Tcb.SetFaultEp`: cap Endpoint; ghi `fault_ep`.
    pub fn tcb_set_fault_ep(&mut self, tcb: SlotRef, ep: SlotRef) -> Result<(), KError> {
        let tcb_obj = self.require_type(tcb, ObjType::Tcb)?;
        let ep_cap = self.cap_at(ep.cnode, ep.slot)?;
        if self.cap(ep_cap)?.ty != ObjType::Endpoint {
            return Err(KError::WrongObjectType);
        }
        self.tcb_data_mut(tcb_obj)?.fault_ep = Some(ep_cap);
        self.sched_after_op(false);
        Ok(())
    }

    /// `SchedControl.Configure(sc, budget, period)` — `budget > 0`, `period > 0`,
    /// `budget ≤ period`; cấu hình lại giữa chu kỳ đặt `consumed = 0` (model-only `[TK]`).
    pub fn sched_control_configure(
        &mut self,
        sched_control: SlotRef,
        sched: SlotRef,
        budget: u64,
        period: u64,
    ) -> Result<(), KError> {
        self.require_type(sched_control, ObjType::SchedControl)?;
        let sched_obj = self.require_type(sched, ObjType::SchedContext)?;
        if budget == 0 || period == 0 || budget > period {
            return Err(KError::InvalidArgument);
        }
        let now = self.now;
        self.sc_data_mut(sched_obj)?
            .budget
            .configure(now, budget, period);
        self.sched_after_op(false);
        Ok(())
    }

    /// `Tcb.SetPriority` `[TK]`: `max_controlled_priority ≥ priority` (thiết kế §4.5 `:311`).
    pub fn tcb_set_priority(
        &mut self,
        tcb: SlotRef,
        priority: u8,
        max_controlled_priority: u8,
    ) -> Result<(), KError> {
        let tcb_obj = self.require_type(tcb, ObjType::Tcb)?;
        if max_controlled_priority < priority {
            return Err(KError::InvalidArgument);
        }
        let info = self.tcb_data(tcb_obj)?.snapshot();
        // KT-21: donor đang donate không đổi priority — KT-59 chưa tạo donation nên nhánh
        // `is_donation_donor` của sim là vô hiệu (KT-60 nối lại cùng chỗ này).
        if info.state == TcbState::Ready || info.donated_sc.is_some() {
            self.sched_remove_all(tcb_obj);
        }
        {
            let t = self.tcb_data_mut(tcb_obj)?;
            t.priority = priority;
            t.max_controlled_priority = max_controlled_priority;
        }
        self.sched_sync(tcb_obj)?;
        self.sched_after_op(true);
        Ok(())
    }

    /// `Tcb.SetAffinity` `[TK]`: mask ≤ 64 bit, khác 0, chỉ hart có thật, chứa `home`.
    pub fn tcb_set_affinity(&mut self, tcb: SlotRef, affinity: u64) -> Result<(), KError> {
        let tcb_obj = self.require_type(tcb, ObjType::Tcb)?;
        let hart_mask = if self.n_harts >= 64 {
            u64::MAX
        } else {
            (1u64 << self.n_harts) - 1
        };
        if affinity == 0 || affinity & !hart_mask != 0 {
            return Err(KError::InvalidArgument);
        }
        let home = self.tcb_data(tcb_obj)?.home;
        if affinity & (1u64 << home) == 0 {
            return Err(KError::InvalidArgument);
        }
        if self.tcb_state(tcb_obj)? == TcbState::Ready {
            self.sched_remove_all(tcb_obj);
        }
        self.tcb_data_mut(tcb_obj)?.affinity = affinity;
        self.sched_sync(tcb_obj)?;
        self.sched_after_op(true);
        Ok(())
    }

    /// `Tcb.SetHome` `[TK]`: home phải có thật và nằm trong affinity; **KT-29**: không đổi
    /// `home` của TCB đang `Running` khi `n > 1` (`Running{hart} ⇒ home == hart`).
    pub fn tcb_set_home(&mut self, tcb: SlotRef, home: u16) -> Result<(), KError> {
        let tcb_obj = self.require_type(tcb, ObjType::Tcb)?;
        if home as usize >= self.n_harts {
            return Err(KError::InvalidArgument);
        }
        let info = self.tcb_data(tcb_obj)?.snapshot();
        if self.n_harts > 1 && matches!(info.state, TcbState::Running { .. }) {
            return Err(KError::IllegalOperation);
        }
        if info.affinity & (1u64 << home) == 0 {
            return Err(KError::InvalidArgument);
        }
        if info.state == TcbState::Ready {
            self.sched_remove_all(tcb_obj);
        }
        self.tcb_data_mut(tcb_obj)?.home = home;
        self.sched_sync(tcb_obj)?;
        self.sched_after_op(true);
        Ok(())
    }

    /// Model API `set_sched_params(tcb, sc)` — **D2-49(b)**: gắn/gỡ `SchedContext` của TCB.
    ///
    /// KT-21: nhánh "TCB đang nhận/cho donation" của sim chưa kích hoạt ở KT-59 (chưa có
    /// `Call`/`Reply`); giữ đúng mã lỗi `Busy` khi SC đang bị donate cho TCB khác.
    pub fn set_sched_params(&mut self, tcb: SlotRef, sched: Option<SlotRef>) -> Result<(), KError> {
        let tcb_obj = self.require_type(tcb, ObjType::Tcb)?;
        let sched_obj = match sched {
            Some(s) => Some(self.require_type(s, ObjType::SchedContext)?),
            None => None,
        };
        let info = self.tcb_data(tcb_obj)?.snapshot();
        if info.donated_sc.is_some() {
            return Err(KError::IllegalOperation);
        }
        if let Some(sc) = sched_obj {
            if self.sc_data(sc)?.donated_to.is_some() {
                return Err(KError::Busy);
            }
        }
        self.tcb_data_mut(tcb_obj)?.sched_context = sched_obj;
        self.enforce_budget(tcb_obj)?;
        self.sched_after_op(false);
        Ok(())
    }

    /// Ảnh chụp toàn bộ trường TCB.
    pub fn tcb_info(&self, tcb: SlotRef) -> Result<TcbInfo, KError> {
        let obj = self.require_type(tcb, ObjType::Tcb)?;
        Ok(self.tcb_data(obj)?.snapshot())
    }

    /// Như [`Machine::tcb_info`] nhưng nhận thẳng `ObjId` (scheduler/nội bộ).
    pub fn tcb_info_obj(&self, tcb: ObjId) -> Result<TcbInfo, KError> {
        Ok(self.tcb_data(tcb)?.snapshot())
    }

    /// `Tcb.ReadState` — trả **4 giá trị** `{state, suspended, blocked_on_kind, exit_code}`.
    pub fn tcb_read_state(&self, tcb: SlotRef) -> Result<TcbSnapshot, KError> {
        let obj = self.require_type(tcb, ObjType::Tcb)?;
        let t = self.tcb_data(obj)?;
        Ok(TcbSnapshot {
            state: t.state,
            suspended: t.suspended,
            blocked_on_kind: t.state.blocked_on_kind(),
            exit_code: t.exit_code,
        })
    }

    /// Trạng thái TCB theo `ObjId`.
    pub fn tcb_state(&self, tcb: ObjId) -> Result<TcbState, KError> {
        Ok(self.tcb_data(tcb)?.state)
    }

    /// 60 word "IPC buffer model" của TCB (KT-59 chỉ đọc; transfer thuộc KT-60).
    pub fn tcb_ipc_buffer(&self, tcb: ObjId) -> Result<[u64; 60], KError> {
        Ok(self.tcb_data(tcb)?.ipc_buffer)
    }

    /// 4 MR "thanh ghi" của TCB (KT-59 chỉ đọc).
    pub fn tcb_regs(&self, tcb: ObjId) -> Result<[u64; 4], KError> {
        Ok(self.tcb_data(tcb)?.regs)
    }

    /// **Một cửa** đổi trạng thái TCB (D2-49(d)): tra bảng [`crate::tcb::ALLOWED`]; cặp cấm
    /// trả `IllegalOperation`, **không** đổi trạng thái, đếm vào `forbidden_hits`.
    ///
    /// KT-21/25/27/34 hook của sim (`release_donation_of`, liên kết `Reply.caller`) chưa có
    /// vật thể tương ứng ở KT-59 — KT-60 nối lại **tại đúng cửa này** (một cửa duy nhất).
    pub fn transition_tcb(&mut self, tcb: ObjId, to: TcbState) -> Result<(), KError> {
        let from = self.tcb_state(tcb)?;
        if !crate::tcb::allowed(from, to) {
            self.forbidden_hits[from.index() * TCB_STATE_COUNT + to.index()] += 1;
            return Err(KError::IllegalOperation);
        }
        // Bất biến ≤1 `Running`/hart (KT-19 DoD 2): TCB vào `Running{hart}` khi hart đang có
        // TCB chạy ⇒ đưa TCB cũ về `Ready` **trước**, qua đúng cửa này.
        if let TcbState::Running { hart } = to {
            if hart as usize >= self.n_harts {
                return Err(KError::InvalidArgument);
            }
            // KT-29: `Running{hart}` chỉ hợp lệ khi `home == hart`.
            if self.tcb_data(tcb)?.home != hart {
                return Err(KError::IllegalOperation);
            }
            if let Some(cur) = self.sched[hart as usize].current() {
                if cur != tcb {
                    self.transition_tcb(cur, TcbState::Ready)?;
                }
            }
        }
        self.tcb_data_mut(tcb)?.state = to;
        self.pair_hits[from.index() * TCB_STATE_COUNT + to.index()] += 1;
        // KT-23: đếm hoạt động ngân sách ở **một cửa**.
        if matches!(from, TcbState::Running { .. })
            && matches!(
                to,
                TcbState::Sleeping {
                    reason: SleepReason::Budget
                }
            )
        {
            self.budget_sleeps += 1;
        }
        self.sched_sync(tcb)?;
        Ok(())
    }

    /// Số lần cặp `(from, to)` đã đi qua cổng.
    pub fn pair_hit_count(&self, from: usize, to: usize) -> u32 {
        self.pair_hits[from * TCB_STATE_COUNT + to]
    }

    /// `(số cặp ALLOWED đã có hit, 48)`.
    pub fn transition_coverage(&self) -> (usize, usize) {
        crate::tcb::coverage(&self.pair_hits)
    }

    /// Số lần cặp **cấm** `(from, to)` bị thử.
    pub fn forbidden_attempt_count(&self, from: usize, to: usize) -> u32 {
        self.forbidden_hits[from * TCB_STATE_COUNT + to]
    }

    /// `(số cặp thiết kế đã có hit, 46)` — bỏ 2 cặp `Stopping` reserved.
    pub fn design_coverage(&self) -> (usize, usize) {
        crate::tcb::coverage_design(&self.pair_hits)
    }

    /// Log finalise TCB (theo thứ tự finalise).
    pub fn finalise_log(&self) -> &[FinaliseEntry] {
        &self.finalise_log[..self.n_finalise]
    }

    /// Số lần finalise bị từ chối vì log đầy (`MAX_FINALISE`) — chẩn đoán, không phải ABI.
    pub fn finalise_overflow(&self) -> u32 {
        self.finalise_overflow
    }

    /// `Tcb.Suspend(mode)` (thiết kế §4.4 `:278-283`).
    ///
    /// - `Abort`: gỡ khỏi hàng, `BlockedOn*`/`Running` → `Ready`, `Ready` giữ `Ready`,
    ///   `Sleeping` giữ `Sleeping`; `exit_code` giữ nguyên (D2-25).
    /// - `Defer`: K1b chưa có `Restart` ⇒ **luôn** `InvalidArgument`, trạng thái không đổi.
    /// - `Inactive`/`Stopping`/`Dead`: `IllegalOperation`.
    pub fn tcb_suspend(&mut self, tcb: SlotRef, mode: SuspendMode) -> Result<(), KError> {
        let obj = self.require_type(tcb, ObjType::Tcb)?;
        if mode == SuspendMode::Defer {
            return Err(KError::InvalidArgument);
        }
        let state = self.tcb_state(obj)?;
        if !state.is_live() || state == TcbState::Inactive {
            return Err(KError::IllegalOperation);
        }
        self.unlink_tcb_queues(obj);
        if state.is_blocked() || matches!(state, TcbState::Running { .. }) {
            self.transition_tcb(obj, TcbState::Ready)?;
        }
        self.set_tcb_suspended(obj, true)?;
        self.sched_after_op(true);
        Ok(())
    }

    /// `Tcb.Resume`: chỉ xoá cờ `suspended` + `exit_code` (D2-25); **không** phát lại
    /// Call/Send/Recv/ReplyRecv. Idempotent khi TCB không treo.
    pub fn tcb_resume(&mut self, tcb: SlotRef) -> Result<(), KError> {
        let obj = self.require_type(tcb, ObjType::Tcb)?;
        if self.tcb_state(obj)? == TcbState::Dead {
            return Err(KError::IllegalOperation);
        }
        let dang_treo = self.tcb_data(obj)?.suspended;
        {
            let t = self.tcb_data_mut(obj)?;
            t.suspended = false;
            t.exit_code = None;
        }
        if dang_treo {
            self.sched_sync(obj)?;
        }
        self.sched_after_op(true);
        Ok(())
    }

    /// Finalise TCB (thiết kế §4.3 bảng Finalise; KT-18 DoD 9): gỡ khỏi hàng, chuyển `Dead`
    /// qua một cửa, trả `sched_context` bằng hook, ghi log (biên [`MAX_FINALISE`]).
    pub fn finalise_tcb(&mut self, tcb: ObjId) -> Result<(), KError> {
        let state = self.tcb_state(tcb)?;
        if state == TcbState::Dead {
            return Ok(());
        }
        // **F1 (reviewer-kt59)**: log có biên `MAX_OBJECTS`; đầy ⇒ `NotEnoughMemory` **trước**
        // mọi thay đổi (atomic) — không `debug_assert`, không cắt im lặng như bản trước.
        if self.n_finalise >= MAX_FINALISE {
            self.finalise_overflow = self.finalise_overflow.saturating_add(1);
            return Err(KError::NotEnoughMemory);
        }
        self.unlink_tcb_queues(tcb);
        let released = {
            let t = self.tcb_data_mut(tcb)?;
            let released = t.sched_context.take();
            t.bound_notification = None;
            released
        };
        self.transition_tcb(tcb, TcbState::Dead)?;
        self.finalise_log[self.n_finalise] = FinaliseEntry {
            tcb,
            state: TcbState::Dead,
            released_sched_context: released,
        };
        self.n_finalise += 1;
        self.sched_after_op(false);
        Ok(())
    }

    // ------------------------------------------------------------- ngân sách MCS (KT-20)

    /// Thời gian ảo hiện tại (tick) — **không** đọc wall clock (D2-49(h)).
    pub fn virtual_now(&self) -> u64 {
        self.now
    }

    /// `tick(n)` — chạy đồng hồ ảo `n` đơn vị; tất định. Mỗi đơn vị: charge TCB đang `Running`
    /// cho `[now, now+1)`, `now += 1`, rồi xử lý biên refill (charge trước — refill sau).
    /// **Không** tự `dispatch`.
    pub fn tick(&mut self, n: u64) -> Result<(), KError> {
        for _ in 0..n {
            self.tick_one()?;
        }
        self.sched_after_op(false);
        Ok(())
    }

    fn tick_one(&mut self) -> Result<(), KError> {
        for h in 0..self.n_harts {
            if let Some(cur) = self.sched[h].current() {
                self.charge_tick(cur)?;
            }
        }
        self.now = self.now.saturating_add(1);
        self.process_refills()
    }

    /// Charge **một** tick cho TCB đang chạy; hết budget ⇒ `Running → Sleeping{Budget}`.
    fn charge_tick(&mut self, tcb: ObjId) -> Result<(), KError> {
        let info = self.tcb_data(tcb)?.snapshot();
        if !matches!(info.state, TcbState::Running { .. }) {
            return Ok(());
        }
        let Some(sc) = self.funding_sc(tcb) else {
            self.transition_tcb(
                tcb,
                TcbState::Sleeping {
                    reason: SleepReason::Budget,
                },
            )?;
            return Ok(());
        };
        if !self.sc_data(sc)?.budget.is_configured() {
            self.transition_tcb(
                tcb,
                TcbState::Sleeping {
                    reason: SleepReason::Budget,
                },
            )?;
            return Ok(());
        }
        let charged = self.sc_data_mut(sc)?.budget.charge();
        if !charged || !self.tcb_has_budget(tcb) {
            self.transition_tcb(
                tcb,
                TcbState::Sleeping {
                    reason: SleepReason::Budget,
                },
            )?;
        }
        Ok(())
    }

    /// Xử lý biên nạp lại tại `now` cho mọi `SchedContext` sống; `Sleeping{Budget}` +
    /// `!suspended` được đánh thức `Ready`.
    fn process_refills(&mut self) -> Result<(), KError> {
        let now = self.now;
        let mut refilled = [0u32; MAX_SCHED_CONTEXTS];
        let mut n_refilled = 0usize;
        for i in 0..self.n_objects {
            let obj = (i as u32) + 1;
            if !self.objects[i].alive || self.objects[i].ty != ObjType::SchedContext {
                continue;
            }
            let body = self.objects[i].body;
            if body.kind != BODY_SC || !body.alive {
                continue;
            }
            if self.scs[body.idx as usize].budget.refill_due(now) {
                self.budget_refills += 1;
                if n_refilled < MAX_SCHED_CONTEXTS {
                    refilled[n_refilled] = obj;
                    n_refilled += 1;
                }
            }
        }
        if n_refilled == 0 {
            return Ok(());
        }
        let mut wake = [0u32; MAX_TCBS];
        let mut n_wake = 0usize;
        for i in 0..self.n_objects {
            let obj = (i as u32) + 1;
            if !self.objects[i].alive || self.objects[i].ty != ObjType::Tcb {
                continue;
            }
            let info = self.tcb_data(obj)?.snapshot();
            if matches!(
                info.state,
                TcbState::Sleeping {
                    reason: SleepReason::Budget
                }
            ) && !info.suspended
            {
                if let Some(sc) = self.funding_sc(obj) {
                    if refilled[..n_refilled].contains(&sc) && n_wake < MAX_TCBS {
                        wake[n_wake] = obj;
                        n_wake += 1;
                    }
                }
            }
        }
        for &tcb in &wake[..n_wake] {
            self.transition_tcb(tcb, TcbState::Ready)?;
        }
        Ok(())
    }

    /// Gắn/gỡ SC khiến TCB đang `Running` **không còn budget** ⇒ ngủ ngay `Sleeping{Budget}`.
    fn enforce_budget(&mut self, tcb: ObjId) -> Result<(), KError> {
        let info = self.tcb_data(tcb)?.snapshot();
        if matches!(info.state, TcbState::Running { .. }) && !self.tcb_has_budget(tcb) {
            self.transition_tcb(
                tcb,
                TcbState::Sleeping {
                    reason: SleepReason::Budget,
                },
            )?;
        }
        Ok(())
    }

    /// Ảnh chụp ngân sách của `SchedContext`.
    pub fn sched_info(&self, sched: SlotRef) -> Result<ScInfo, KError> {
        let obj = self.require_type(sched, ObjType::SchedContext)?;
        let s = self.sc_data(obj)?;
        let mut info = s.budget.info();
        info.donated_to = s.donated_to;
        info.donor = s.donor;
        Ok(info)
    }

    /// `SchedContext.Consumed` `[TK]`: số tick đã charge trong chu kỳ hiện tại.
    pub fn sched_consumed(&self, sched: SlotRef) -> Result<u64, KError> {
        Ok(self.sched_info(sched)?.consumed)
    }

    /// Budget còn lại của chu kỳ hiện tại; `0` khi SC chưa cấu hình.
    pub fn sched_remaining(&self, sched: SlotRef) -> Result<u64, KError> {
        let info = self.sched_info(sched)?;
        Ok(if info.configured {
            info.budget - info.consumed
        } else {
            0
        })
    }

    /// SC đang **cấp ngân sách** cho `tcb`: SC được donate (KT-21, chưa có ở KT-59) hoặc SC
    /// riêng không bị donate; `None` ⇒ không có budget ⇒ không được `pick` (D2-50).
    fn funding_sc(&self, tcb: ObjId) -> Option<ObjId> {
        let info = self.tcb_data(tcb).ok()?.snapshot();
        if let Some(sc) = info.donated_sc {
            return Some(sc);
        }
        let own = info.sched_context?;
        match self.sc_data(own) {
            Ok(s) if s.donated_to.is_none() => Some(own),
            _ => None,
        }
    }

    /// Model API `[TK]`: SC đang cấp ngân sách cho `tcb`.
    pub fn sc_of(&self, tcb: ObjId) -> Result<Option<ObjId>, KError> {
        Ok(self.funding_sc(tcb))
    }

    /// TCB có được phép chạy theo ngân sách không (SC đã cấu hình và `consumed < budget`).
    pub fn tcb_has_budget(&self, tcb: ObjId) -> bool {
        let Some(sc) = self.funding_sc(tcb) else {
            return false;
        };
        self.sc_data(sc)
            .map(|s| s.budget.has_budget())
            .unwrap_or(false)
    }

    /// Priority **hiệu lực** của `tcb` (KT-21 DoD 6): khi nhận donation =
    /// [`donated_priority`]; ngược lại priority gốc. KT-59 chưa tạo donation nên luôn là
    /// priority gốc; công thức + trường đã sẵn cho KT-60.
    pub fn effective_priority(&self, tcb: ObjId) -> Result<u8, KError> {
        let info = self.tcb_data(tcb)?.snapshot();
        let Some(sc) = info.donated_sc else {
            return Ok(info.priority);
        };
        let donor = self.sc_data(sc)?.donor;
        let Some(donor) = donor else {
            return Ok(info.priority);
        };
        let oi = self.tcb_data(donor)?.snapshot();
        Ok(donated_priority(
            oi.priority,
            oi.max_controlled_priority,
            info.max_controlled_priority,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{KError, ObjType, SlotRef};
    use crate::machine::{Machine, MAX_FINALISE};

    /// **F1**: log finalise có biên `MAX_OBJECTS` (256), đầy ⇒ `NotEnoughMemory` **atomic**
    /// (không đổi trạng thái TCB), không panic; 257 lần gọi liên tiếp đều trả lỗi và đếm
    /// `finalise_overflow`.
    #[test]
    fn finalise_log_day_tra_not_enough_memory_257_lan() {
        let mut m = Machine::new();
        let t = m.add_kernel_object(ObjType::Tcb);
        m.n_finalise = MAX_FINALISE;
        for i in 1..=257 {
            assert_eq!(
                m.finalise_tcb(t),
                Err(KError::NotEnoughMemory),
                "lần {i}: log đầy phải NotEnoughMemory"
            );
            assert_eq!(
                m.tcb_state(t),
                Ok(TcbState::Inactive),
                "lỗi atomic, state không đổi"
            );
        }
        assert_eq!(m.finalise_log().len(), MAX_FINALISE);
        assert_eq!(m.finalise_overflow(), 257);
    }

    /// Reviewer-kt59 tái hiện: **2 vòng × 64 TCB** (body tái dùng) phải ghi đủ 128 dòng
    /// finalise, không panic — chứng minh `MAX_FINALISE = MAX_TCBS` cũ là sai.
    #[test]
    fn finalise_hai_vong_64_tcb_du_128_dong() {
        let mut m = Machine::new();
        let c0 = m.add_cnode(128);
        let u = m.add_untyped(0x1000, 1 << 16);
        let cap = m.root_cap(u);
        m.put_cap(c0, 0, cap).expect("cap Untyped");
        for vong in 0..2 {
            for i in 0..64 {
                let cap = m
                    .retype(SlotRef::new(c0, 0), ObjType::Tcb, 0, 0)
                    .expect("retype TCB");
                m.put_cap(c0, i + 1, cap).expect("dat cap TCB");
            }
            let deleted = m.revoke(c0, Some(0)).expect("revoke 64 TCB");
            assert_eq!(deleted, 64, "vòng {vong}: phải revoke đủ 64 TCB");
        }
        assert_eq!(m.finalise_log().len(), 128);
        assert_eq!(m.finalise_overflow(), 0);
        assert!(m.finalise_log().iter().all(|e| e.state == TcbState::Dead));
    }
}
