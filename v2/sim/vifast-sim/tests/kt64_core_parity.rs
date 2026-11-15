// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-64 — **parity sim↔core** cho hai mảng có mô hình ở sim: `SchedContext.Bind`/`Signal` và
//! `tcb_resume_with_regs`.
//!
//! Cùng một rig slot (cùng thứ tự tạo object ⇒ cùng `ObjId`), cùng một dãy op; sau **từng bước**
//! so (a) **mã lỗi ABI** (`KError::id()`) và (b) **digest** trạng thái quan sát được. Mảng
//! `TlbFlush` và `SlotPool`→arena **không** có parity ở đây vì sim không mô hình hoá flush phần
//! cứng (TLB của sim là mô hình của tầng VSpace K2) và sim giữ slot CNode bằng heap riêng —
//! lý do ghi trong `docs/k3b-core.md` §13; hai mảng đó pin bằng `kt64_core_tlb_bind_resume.rs`
//! và TAP QEMU.
//!
//! `Unbind` cũng **không** có ở sim K1–K2 (chỉ `Bind`); core cài theo `spec/abi.toml` và pin
//! core-only — ghi rõ trong §13.

use kc::{KError as CoreErr, Machine as CoreMachine, ObjId as CoreObj};
use vifast_core as kc;
use vifast_sim::kobj::{
    FaultKind as SimFault, KError as SimErr, Machine as SimMachine, ObjId as SimObj,
    SlotRef as SimSlot, TcbState as SimState,
};

fn sslot(c: u32, s: usize) -> SimSlot {
    SimSlot::new(c, s)
}
fn cslot(c: u32, s: usize) -> kc::SlotRef {
    kc::SlotRef::new(c, s)
}

fn sim_msg(label: u32, words: &[u64]) -> vifast_sim::ipc::Message {
    let info =
        vifast_sim::ipc::MessageInfo::new(label, 0, false, 0, words.len() as u8).expect("info sim");
    vifast_sim::ipc::Message::new(info, words).expect("message sim")
}
fn core_msg(label: u32, words: &[u64]) -> kc::Message {
    let info = kc::MessageInfo::new(label, 0, false, 0, words.len() as u8).expect("info core");
    kc::Message::new(info, words).expect("message core")
}

/// Nhãn trạng thái TCB ổn định (hai enum khác crate, cùng 11 biến thể).
fn sim_state(s: SimState) -> u8 {
    match s {
        SimState::Inactive => 0,
        SimState::Ready => 1,
        SimState::Running { .. } => 2,
        SimState::BlockedOnSend { .. } => 3,
        SimState::BlockedOnRecv { .. } => 4,
        SimState::BlockedOnReply { .. } => 5,
        SimState::BlockedOnNotification { .. } => 6,
        SimState::BlockedOnAddress { .. } => 7,
        SimState::Sleeping { .. } => 8,
        SimState::Stopping => 9,
        SimState::Dead => 10,
    }
}
fn core_state(s: kc::TcbState) -> u8 {
    match s {
        kc::TcbState::Inactive => 0,
        kc::TcbState::Ready => 1,
        kc::TcbState::Running { .. } => 2,
        kc::TcbState::BlockedOnSend { .. } => 3,
        kc::TcbState::BlockedOnRecv { .. } => 4,
        kc::TcbState::BlockedOnReply { .. } => 5,
        kc::TcbState::BlockedOnNotification { .. } => 6,
        kc::TcbState::BlockedOnAddress { .. } => 7,
        kc::TcbState::Sleeping { .. } => 8,
        kc::TcbState::Stopping => 9,
        kc::TcbState::Dead => 10,
    }
}

fn sim_fault_tag(k: SimFault) -> u8 {
    match k {
        SimFault::Exit { .. } => 0,
        SimFault::Vm { .. } => 1,
        SimFault::UnknownSyscall { .. } => 2,
        SimFault::UserException { .. } => 3,
    }
}
fn core_fault_tag(k: kc::FaultKind) -> u8 {
    match k {
        kc::FaultKind::Exit { .. } => 0,
        kc::FaultKind::Vm { .. } => 1,
        kc::FaultKind::UnknownSyscall { .. } => 2,
        kc::FaultKind::UserException { .. } => 3,
    }
}

/// Mã lỗi ABI (1..19) hoặc 0 khi `Ok` — nguồn so duy nhất của "cùng mã lỗi".
fn rc_sim<T>(r: Result<T, SimErr>) -> u16 {
    match r {
        Ok(_) => 0,
        Err(e) => e.id(),
    }
}
fn rc_core<T>(r: Result<T, CoreErr>) -> u16 {
    match r {
        Ok(_) => 0,
        Err(e) => e.id(),
    }
}

// ===========================================================================================
// Kịch bản A — SchedContext.Bind + Signal: 1 CNode 64 slot; 2 SC; 2 Notification; 1 Reply;
// 1 Endpoint; 2 TCB. Slot cố định để hai bên tạo object cùng thứ tự.
// ===========================================================================================

const A_SC: [usize; 2] = [2, 3];
const A_N: [usize; 2] = [4, 5];
const A_REP: usize = 6;
const A_EP: usize = 7;
const A_T: [usize; 2] = [10, 11];
const A_EP_RECV: usize = 9;

struct SimBind {
    m: SimMachine,
    c0: SimObj,
    n_obj: [SimObj; 2],
    t: [SimObj; 2],
    t_s: [SimSlot; 2],
    checks: [bool; 3],
}

struct CoreBind {
    m: CoreMachine,
    c0: CoreObj,
    n_obj: [CoreObj; 2],
    t: [CoreObj; 2],
    t_s: [kc::SlotRef; 2],
    checks: [bool; 3],
}

fn sim_bind() -> SimBind {
    let mut m = SimMachine::new();
    let c0 = m.add_cnode(64);
    for (i, s) in A_SC.iter().enumerate() {
        let sc = m.add_kernel_object(vifast_sim::kobj::ObjType::SchedContext);
        let cap = m.root_cap(sc);
        m.put_cap(c0, *s, cap).expect("cap SC");
        let _ = i;
    }
    let mut n_obj = [0u32; 2];
    for (i, s) in A_N.iter().enumerate() {
        let n = m.add_kernel_object(vifast_sim::kobj::ObjType::Notification);
        let cap = m.root_cap(n);
        m.put_cap(c0, *s, cap).expect("cap Notification");
        n_obj[i] = n;
    }
    let rep = m.add_kernel_object(vifast_sim::kobj::ObjType::Reply);
    let rcap = m.root_cap(rep);
    m.put_cap(c0, A_REP, rcap).expect("cap Reply");
    let ep = m.add_kernel_object(vifast_sim::kobj::ObjType::Endpoint);
    let ecap = m.root_cap(ep);
    m.put_cap(c0, A_EP, ecap).expect("cap Endpoint");
    let ep2 = m.add_kernel_object(vifast_sim::kobj::ObjType::Endpoint);
    let ecap2 = m.root_cap(ep2);
    m.put_cap(c0, A_EP_RECV, ecap2).expect("cap Endpoint recv");
    let mut t = [0u32; 2];
    let mut t_s = [sslot(c0, 0); 2];
    for (i, s) in A_T.iter().enumerate() {
        t[i] = m.add_kernel_object(vifast_sim::kobj::ObjType::Tcb);
        let cap = m.root_cap(t[i]);
        m.put_cap(c0, *s, cap).expect("cap TCB");
        t_s[i] = sslot(c0, *s);
        m.transition_tcb(t[i], SimState::Ready).expect("Ready");
    }
    SimBind {
        m,
        c0,
        n_obj,
        t,
        t_s,
        checks: [false; 3],
    }
}

fn core_bind() -> CoreBind {
    let mut m = CoreMachine::new();
    let c0 = m.add_cnode(64);
    for s in A_SC {
        let sc = m.add_kernel_object(kc::ObjType::SchedContext);
        let cap = m.root_cap(sc);
        m.put_cap(c0, s, cap).expect("cap SC");
    }
    let mut n_obj = [0u32; 2];
    for (i, s) in A_N.iter().enumerate() {
        let n = m.add_kernel_object(kc::ObjType::Notification);
        let cap = m.root_cap(n);
        m.put_cap(c0, *s, cap).expect("cap Notification");
        n_obj[i] = n;
    }
    let rep = m.add_kernel_object(kc::ObjType::Reply);
    let rcap = m.root_cap(rep);
    m.put_cap(c0, A_REP, rcap).expect("cap Reply");
    let ep = m.add_kernel_object(kc::ObjType::Endpoint);
    let ecap = m.root_cap(ep);
    m.put_cap(c0, A_EP, ecap).expect("cap Endpoint");
    let ep2 = m.add_kernel_object(kc::ObjType::Endpoint);
    let ecap2 = m.root_cap(ep2);
    m.put_cap(c0, A_EP_RECV, ecap2).expect("cap Endpoint recv");
    let mut t = [0u32; 2];
    let mut t_s = [cslot(c0, 0); 2];
    for (i, s) in A_T.iter().enumerate() {
        t[i] = m.add_kernel_object(kc::ObjType::Tcb);
        let cap = m.root_cap(t[i]);
        m.put_cap(c0, *s, cap).expect("cap TCB");
        t_s[i] = cslot(c0, *s);
        m.transition_tcb(t[i], kc::TcbState::Ready).expect("Ready");
    }
    CoreBind {
        m,
        c0,
        n_obj,
        t,
        t_s,
        checks: [false; 3],
    }
}

#[derive(Debug, PartialEq, Eq)]
struct BD {
    n_bound: [Option<u32>; 2],
    n_word: [u64; 2],
    n_badge: [u64; 2],
    n_waiter: [Option<u32>; 2],
    st: [u8; 2],
    regs: [[u64; 4]; 2],
    notif: [[u64; 2]; 2],
    sent: u64,
    delivered: u64,
    checks: [bool; 3],
}

fn sim_digest(r: &SimBind) -> BD {
    BD {
        n_bound: [
            r.m.notification_bound_sc(r.n_obj[0]).unwrap(),
            r.m.notification_bound_sc(r.n_obj[1]).unwrap(),
        ],
        n_word: [
            r.m.notification_word(r.n_obj[0]).unwrap(),
            r.m.notification_word(r.n_obj[1]).unwrap(),
        ],
        n_badge: [
            r.m.notification_badge(r.n_obj[0]).unwrap(),
            r.m.notification_badge(r.n_obj[1]).unwrap(),
        ],
        n_waiter: [
            r.m.notification_waiter(r.n_obj[0]).unwrap(),
            r.m.notification_waiter(r.n_obj[1]).unwrap(),
        ],
        st: [
            sim_state(r.m.tcb_state(r.t[0]).unwrap()),
            sim_state(r.m.tcb_state(r.t[1]).unwrap()),
        ],
        regs: [r.m.tcb_regs(r.t[0]).unwrap(), r.m.tcb_regs(r.t[1]).unwrap()],
        notif: [
            match r.m.tcb_notification(r.t[0]) {
                Ok(Some(v)) => [v.word, v.badge],
                _ => [0, 0],
            },
            match r.m.tcb_notification(r.t[1]) {
                Ok(Some(v)) => [v.word, v.badge],
                _ => [0, 0],
            },
        ],
        sent: r.m.ipc_sent(),
        delivered: r.m.ipc_delivered(),
        checks: r.checks,
    }
}

fn core_digest(r: &CoreBind) -> BD {
    BD {
        n_bound: [
            r.m.notification_bound_sc(r.n_obj[0]).unwrap(),
            r.m.notification_bound_sc(r.n_obj[1]).unwrap(),
        ],
        n_word: [
            r.m.notification_word(r.n_obj[0]).unwrap(),
            r.m.notification_word(r.n_obj[1]).unwrap(),
        ],
        n_badge: [
            r.m.notification_badge(r.n_obj[0]).unwrap(),
            r.m.notification_badge(r.n_obj[1]).unwrap(),
        ],
        n_waiter: [
            r.m.notification_waiter(r.n_obj[0]).unwrap(),
            r.m.notification_waiter(r.n_obj[1]).unwrap(),
        ],
        st: [
            core_state(r.m.tcb_state(r.t[0]).unwrap()),
            core_state(r.m.tcb_state(r.t[1]).unwrap()),
        ],
        regs: [r.m.tcb_regs(r.t[0]).unwrap(), r.m.tcb_regs(r.t[1]).unwrap()],
        notif: [
            match r.m.tcb_notification(r.t[0]) {
                Ok(Some(v)) => [v.word, v.badge],
                _ => [0, 0],
            },
            match r.m.tcb_notification(r.t[1]) {
                Ok(Some(v)) => [v.word, v.badge],
                _ => [0, 0],
            },
        ],
        sent: r.m.ipc_sent(),
        delivered: r.m.ipc_delivered(),
        checks: r.checks,
    }
}

fn buoc_a(s: &mut SimBind, c: &mut CoreBind, i: usize, rs: u16, rc: u16, what: &str) {
    assert_eq!(rs, rc, "bước {i} ({what}): mã lỗi sim {rs} vs core {rc}");
    s.checks = [
        s.m.check_notifications().is_ok(),
        s.m.check_ipc().is_ok(),
        s.m.check_sched().is_ok(),
    ];
    c.checks = [
        c.m.check_notifications().is_ok(),
        c.m.check_ipc().is_ok(),
        c.m.check_sched().is_ok(),
    ];
    assert_eq!(
        sim_digest(s),
        core_digest(c),
        "bước {i} ({what}): digest lệch"
    );
}

/// Parity `Bind` + `Signal`: bind 1:1, vi phạm 1:1, rebind gỡ phía cũ, target sai kiểu, xoá cap
/// Notification, signal chỉ đánh thức TCB **đúng** bound.
#[test]
fn kt64_parity_bind_va_signal() {
    let mut s = sim_bind();
    let mut c = core_bind();

    let r =
        s.m.sched_context_bind(sslot(s.c0, A_SC[0]), sslot(s.c0, A_N[0]));
    let rs = rc_sim(r);
    let rc = rc_core(c.m.sched_context_bind(cslot(c.c0, A_SC[0]), cslot(c.c0, A_N[0])));
    buoc_a(&mut s, &mut c, 1, rs, rc, "bind sc0↔n0");

    let rs = rc_sim(s.m.sched_context_bind(sslot(s.c0, A_SC[1]), sslot(s.c0, A_N[0])));
    let rc = rc_core(c.m.sched_context_bind(cslot(c.c0, A_SC[1]), cslot(c.c0, A_N[0])));
    buoc_a(&mut s, &mut c, 2, rs, rc, "vi phạm 1:1 phía notification");

    let rs = rc_sim(s.m.sched_context_bind(sslot(s.c0, A_SC[0]), sslot(s.c0, A_N[1])));
    let rc = rc_core(c.m.sched_context_bind(cslot(c.c0, A_SC[0]), cslot(c.c0, A_N[1])));
    buoc_a(&mut s, &mut c, 3, rs, rc, "SC đổi sang notification khác");

    let rs = rc_sim(s.m.sched_context_bind(sslot(s.c0, A_SC[0]), sslot(s.c0, A_REP)));
    let rc = rc_core(c.m.sched_context_bind(cslot(c.c0, A_SC[0]), cslot(c.c0, A_REP)));
    buoc_a(&mut s, &mut c, 4, rs, rc, "SC bind Reply");

    let rs = rc_sim(s.m.sched_context_bind(sslot(s.c0, A_SC[0]), sslot(s.c0, A_EP)));
    let rc = rc_core(c.m.sched_context_bind(cslot(c.c0, A_SC[0]), cslot(c.c0, A_EP)));
    buoc_a(&mut s, &mut c, 5, rs, rc, "target sai kiểu");

    // Bind lại Notification + Tcb.BindNotification hai TCB; Recv trên Endpoint riêng.
    let rs = rc_sim(s.m.sched_context_bind(sslot(s.c0, A_SC[0]), sslot(s.c0, A_N[0])));
    let rc = rc_core(c.m.sched_context_bind(cslot(c.c0, A_SC[0]), cslot(c.c0, A_N[0])));
    assert_eq!(rs, rc, "bind lại n0");
    let rs = rc_sim(s.m.tcb_bind_notification(s.t_s[0], sslot(s.c0, A_N[0])));
    let rc = rc_core(c.m.tcb_bind_notification(c.t_s[0], cslot(c.c0, A_N[0])));
    assert_eq!(rs, rc, "t0 bind n0");
    let rs = rc_sim(s.m.tcb_bind_notification(s.t_s[1], sslot(s.c0, A_N[1])));
    let rc = rc_core(c.m.tcb_bind_notification(c.t_s[1], cslot(c.c0, A_N[1])));
    assert_eq!(rs, rc, "t1 bind n1");
    let ep_s = sslot(s.c0, A_EP_RECV);
    let rs = rc_sim(s.m.recv(s.t[0], ep_s, None));
    let rc = rc_core(c.m.recv(c.t[0], cslot(c.c0, A_EP_RECV), None));
    assert_eq!(rs, rc, "t0 recv chặn");
    let rs = rc_sim(s.m.recv(s.t[1], ep_s, None));
    let rc = rc_core(c.m.recv(c.t[1], cslot(c.c0, A_EP_RECV), None));
    buoc_a(&mut s, &mut c, 6, rs, rc, "hai TCB chờ Recv + bound");

    let rs = rc_sim(s.m.notification_signal(s.n_obj[0], 0x42));
    let rc = rc_core(c.m.notification_signal(c.n_obj[0], 0x42));
    buoc_a(
        &mut s,
        &mut c,
        7,
        rs,
        rc,
        "signal n0 chỉ đánh thức đúng bound",
    );
    assert_eq!(sim_state(s.m.tcb_state(s.t[0]).unwrap()), 1, "sim: t0 thức");
    assert_eq!(
        core_state(c.m.tcb_state(c.t[0]).unwrap()),
        1,
        "core: t0 thức"
    );
}

// ===========================================================================================
// Kịch bản B — tcb_resume_with_regs: 1 CNode 64; 1 Endpoint; 1 Reply; 2 TCB.
// ===========================================================================================

const B_EP: usize = 1;
const B_REP: usize = 2;
const B_T: [usize; 2] = [10, 11];

struct SimRes {
    m: SimMachine,
    c0: SimObj,
    rep: SimObj,
    t: [SimObj; 2],
    t_s: [SimSlot; 2],
}

struct CoreRes {
    m: CoreMachine,
    c0: CoreObj,
    rep: CoreObj,
    t: [CoreObj; 2],
    t_s: [kc::SlotRef; 2],
}

fn sim_res() -> SimRes {
    let mut m = SimMachine::new();
    let c0 = m.add_cnode(64);
    let ep = m.add_kernel_object(vifast_sim::kobj::ObjType::Endpoint);
    let epc = m.root_cap(ep);
    m.put_cap(c0, B_EP, epc).expect("cap ep");
    let rep = m.add_kernel_object(vifast_sim::kobj::ObjType::Reply);
    let rpc = m.root_cap(rep);
    m.put_cap(c0, B_REP, rpc).expect("cap rep");
    let mut t = [0u32; 2];
    let mut t_s = [sslot(c0, 0); 2];
    for (i, s) in B_T.iter().enumerate() {
        t[i] = m.add_kernel_object(vifast_sim::kobj::ObjType::Tcb);
        let cap = m.root_cap(t[i]);
        m.put_cap(c0, *s, cap).expect("cap tcb");
        t_s[i] = sslot(c0, *s);
        m.transition_tcb(t[i], SimState::Ready).expect("Ready");
    }
    m.tcb_set_fault_ep(t_s[0], sslot(c0, B_EP))
        .expect("fault_ep");
    SimRes { m, c0, rep, t, t_s }
}

fn core_res() -> CoreRes {
    let mut m = CoreMachine::new();
    let c0 = m.add_cnode(64);
    let ep = m.add_kernel_object(kc::ObjType::Endpoint);
    let epc = m.root_cap(ep);
    m.put_cap(c0, B_EP, epc).expect("cap ep");
    let rep = m.add_kernel_object(kc::ObjType::Reply);
    let rpc = m.root_cap(rep);
    m.put_cap(c0, B_REP, rpc).expect("cap rep");
    let mut t = [0u32; 2];
    let mut t_s = [cslot(c0, 0); 2];
    for (i, s) in B_T.iter().enumerate() {
        t[i] = m.add_kernel_object(kc::ObjType::Tcb);
        let cap = m.root_cap(t[i]);
        m.put_cap(c0, *s, cap).expect("cap tcb");
        t_s[i] = cslot(c0, *s);
        m.transition_tcb(t[i], kc::TcbState::Ready).expect("Ready");
    }
    m.tcb_set_fault_ep(t_s[0], cslot(c0, B_EP))
        .expect("fault_ep");
    CoreRes { m, c0, rep, t, t_s }
}

#[derive(Debug, PartialEq, Eq)]
struct RD {
    st: [u8; 2],
    regs: [[u64; 4]; 2],
    fault: [Option<(u32, u8)>; 2],
    reply_caller: Option<u32>,
    sent: u64,
    delivered: u64,
    dropped: u64,
    ok: [bool; 4],
}

fn sim_rd(r: &SimRes) -> RD {
    RD {
        st: [
            sim_state(r.m.tcb_state(r.t[0]).unwrap()),
            sim_state(r.m.tcb_state(r.t[1]).unwrap()),
        ],
        regs: [r.m.tcb_regs(r.t[0]).unwrap(), r.m.tcb_regs(r.t[1]).unwrap()],
        fault: [
            r.m.tcb_last_fault(r.t[0])
                .unwrap()
                .map(|(o, k)| (o, sim_fault_tag(k))),
            r.m.tcb_last_fault(r.t[1])
                .unwrap()
                .map(|(o, k)| (o, sim_fault_tag(k))),
        ],
        reply_caller: r.m.reply_caller(r.rep).unwrap(),
        sent: r.m.ipc_sent(),
        delivered: r.m.ipc_delivered(),
        dropped: r.m.ipc_dropped(),
        ok: [
            r.m.check_ipc().is_ok(),
            r.m.check_sched().is_ok(),
            r.m.check_caps().is_ok(),
            r.m.check_notifications().is_ok(),
        ],
    }
}

fn core_rd(r: &CoreRes) -> RD {
    RD {
        st: [
            core_state(r.m.tcb_state(r.t[0]).unwrap()),
            core_state(r.m.tcb_state(r.t[1]).unwrap()),
        ],
        regs: [r.m.tcb_regs(r.t[0]).unwrap(), r.m.tcb_regs(r.t[1]).unwrap()],
        fault: [
            r.m.tcb_last_fault(r.t[0])
                .unwrap()
                .map(|(o, k)| (o, core_fault_tag(k))),
            r.m.tcb_last_fault(r.t[1])
                .unwrap()
                .map(|(o, k)| (o, core_fault_tag(k))),
        ],
        reply_caller: r.m.reply_caller(r.rep).unwrap(),
        sent: r.m.ipc_sent(),
        delivered: r.m.ipc_delivered(),
        dropped: r.m.ipc_dropped(),
        ok: [
            r.m.check_ipc().is_ok(),
            r.m.check_sched().is_ok(),
            r.m.check_caps().is_ok(),
            r.m.check_notifications().is_ok(),
        ],
    }
}

fn buoc_b(s: &mut SimRes, c: &mut CoreRes, i: usize, rs: u16, rc: u16, what: &str) {
    assert_eq!(rs, rc, "bước {i} ({what}): mã lỗi sim {rs} vs core {rc}");
    assert_eq!(sim_rd(s), core_rd(c), "bước {i} ({what}): digest lệch");
}

/// Parity `tcb_resume_with_regs`: sau fault (`BlockedOnSend`), sau `BlockedOnReply`, `Ready`,
/// `Running` (cấm), message mang cap (InvalidArgument) — so mã lỗi + digest (kể cả thanh ghi,
/// `last_fault`, `reply_caller`, bộ đếm IPC).
#[test]
fn kt64_parity_resume_with_regs() {
    let mut s = sim_res();
    let mut c = core_res();

    // procsrv Recv giữ Reply, rồi fault t0 ⇒ t0 BlockedOnSend (chưa giao).
    let rs = rc_sim(s.m.recv(s.t[1], sslot(s.c0, B_EP), Some(sslot(s.c0, B_REP))));
    let rc = rc_core(c.m.recv(c.t[1], cslot(c.c0, B_EP), Some(cslot(c.c0, B_REP))));
    buoc_b(&mut s, &mut c, 1, rs, rc, "t1 recv + Reply");

    let rs = rc_sim(s.m.inject_fault(s.t[0], SimFault::UnknownSyscall { nr: 7 }));
    let rc = rc_core(c.m.inject_fault(c.t[0], kc::FaultKind::UnknownSyscall { nr: 7 }));
    buoc_b(&mut s, &mut c, 2, rs, rc, "fault t0");

    let rs = rc_sim(s.m.tcb_resume_with_regs(s.t_s[0], sim_msg(0, &[1, 2, 3, 4])));
    let rc = rc_core(c.m.tcb_resume_with_regs(c.t_s[0], core_msg(0, &[1, 2, 3, 4])));
    buoc_b(&mut s, &mut c, 3, rs, rc, "resume sau fault ghi regs");

    // Message mang cap ⇒ `InvalidArgument` (cả hai).
    let sim_cap = s.m.root_cap(s.t[0]);
    let sim_bad = vifast_sim::ipc::Message::with_cap_ids(1, &[9], &[sim_cap]).expect("msg cap");
    let rs = rc_sim(s.m.tcb_resume_with_regs(s.t_s[0], sim_bad));
    let core_cap = c.m.root_cap(c.t[0]);
    let core_bad = kc::Message::with_cap_ids(1, &[9], &[core_cap]).expect("msg cap");
    let rc = rc_core(c.m.tcb_resume_with_regs(c.t_s[0], core_bad));
    buoc_b(&mut s, &mut c, 4, rs, rc, "message mang cap");

    // `BlockedOnReply`: t1 Recv(Reply) rồi t0 Call ⇒ t0 chờ reply; resume ⇒ Ready.
    let rs = rc_sim(s.m.recv(s.t[1], sslot(s.c0, B_EP), Some(sslot(s.c0, B_REP))));
    let rc = rc_core(c.m.recv(c.t[1], cslot(c.c0, B_EP), Some(cslot(c.c0, B_REP))));
    assert_eq!(rs, rc, "t1 recv lần 2");
    let rs = rc_sim(s.m.call_msg(s.t[0], sslot(s.c0, B_EP), sim_msg(2, &[0x55])));
    let rc = rc_core(c.m.call_msg(c.t[0], cslot(c.c0, B_EP), core_msg(2, &[0x55])));
    buoc_b(&mut s, &mut c, 5, rs, rc, "call ⇒ BlockedOnReply");

    let rs = rc_sim(s.m.tcb_resume_with_regs(s.t_s[0], sim_msg(3, &[0x66])));
    let rc = rc_core(c.m.tcb_resume_with_regs(c.t_s[0], core_msg(3, &[0x66])));
    buoc_b(&mut s, &mut c, 6, rs, rc, "resume caller BlockedOnReply");

    // `Running` ⇒ cấm (`IllegalOperation`), digest không đổi (thanh ghi giữ 0x66).
    let rs = rc_sim(s.m.transition_tcb(s.t[0], SimState::Running { hart: 0 }));
    let rc = rc_core(c.m.transition_tcb(c.t[0], kc::TcbState::Running { hart: 0 }));
    assert_eq!(rs, rc, "t0 Running");
    let rs = rc_sim(s.m.tcb_resume_with_regs(s.t_s[0], sim_msg(4, &[0x77])));
    let rc = rc_core(c.m.tcb_resume_with_regs(c.t_s[0], core_msg(4, &[0x77])));
    buoc_b(&mut s, &mut c, 7, rs, rc, "resume khi Running (cấm)");
    assert_eq!(
        rc,
        CoreErr::IllegalOperation.id(),
        "core phải IllegalOperation"
    );
}
