// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-58 — parity core `no_std` (`vifast-core`, K3b tầng 1) với **sim** (oracle).
//!
//! Cùng một `Op` chạy trên `vifast_sim::kobj::Machine` và `vifast_core::Machine` qua hai
//! trait [`View`]/[`Mut`]; sau **mỗi** bước so:
//! - kết quả (`Ok`/`Err` + **tên + số** mã lỗi — cùng bộ 19 mã sinh từ spec),
//! - digest toàn trạng thái (object: kiểu/alive/bytes; cap: obj/kiểu/cha/slot/quyền/badge/
//!   alive; từng slot CNode; thống kê byte Untyped),
//! - hai bất biến `check_caps` + `check_all_bytes` của **cả hai** phải `Ok`.
//!
//! Kịch bản (`scripts`): port trực tiếp các ca KT-09 (byte/watermark/Revoke lồng nhau),
//! KT-10 (cây CNode/MDB, reparent, finalise CNode lồng) và KT-11 (quyền/badge/kế thừa);
//! thêm differential 32 seed × 48 op ngẫu nhiên tất định (LCG) để phủ tổ hợp.
//!
//! Đây là test **host** (std) — chạy trong `gate --fast` (`cargo test --workspace`). Core
//! không đổi ngữ nghĩa sim; sim vẫn là oracle.

use vifast_core as kc;
use vifast_sim::kobj as ks;

// ---------------------------------------------------------------------------------------------
// Kiểu chung
// ---------------------------------------------------------------------------------------------

type Res = Result<u64, (&'static str, u16)>;

/// Loại object trong kịch bản (ánh xạ sang enum của sim/core trong impl trait).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    Untyped,
    CNode,
    Tcb,
    SchedContext,
    Endpoint,
    Notification,
    Reply,
    SchedControl,
    VSpace,
    IrqControl,
}

impl Ty {
    fn sim(self) -> ks::ObjType {
        match self {
            Ty::Untyped => ks::ObjType::Untyped,
            Ty::CNode => ks::ObjType::CNode,
            Ty::Tcb => ks::ObjType::Tcb,
            Ty::SchedContext => ks::ObjType::SchedContext,
            Ty::Endpoint => ks::ObjType::Endpoint,
            Ty::Notification => ks::ObjType::Notification,
            Ty::Reply => ks::ObjType::Reply,
            Ty::SchedControl => ks::ObjType::SchedControl,
            Ty::VSpace => ks::ObjType::VSpace,
            Ty::IrqControl => ks::ObjType::IrqControl,
        }
    }

    fn core(self) -> kc::ObjType {
        match self {
            Ty::Untyped => kc::ObjType::Untyped,
            Ty::CNode => kc::ObjType::CNode,
            Ty::Tcb => kc::ObjType::Tcb,
            Ty::SchedContext => kc::ObjType::SchedContext,
            Ty::Endpoint => kc::ObjType::Endpoint,
            Ty::Notification => kc::ObjType::Notification,
            Ty::Reply => kc::ObjType::Reply,
            Ty::SchedControl => kc::ObjType::SchedControl,
            Ty::VSpace => kc::ObjType::VSpace,
            Ty::IrqControl => kc::ObjType::IrqControl,
        }
    }
}

/// Một thao tác của kịch bản — dữ liệu thuần, chạy được trên cả hai máy.
#[derive(Debug, Clone, Copy)]
enum Op {
    AddUntyped {
        base: u64,
        size: usize,
    },
    AddCnode {
        slots: usize,
    },
    AddKernel {
        ty: Ty,
    },
    RootCap {
        obj: u32,
    },
    PutCap {
        cnode: u32,
        slot: usize,
        cap: u32,
    },
    Retype {
        cnode: u32,
        slot: usize,
        ty: Ty,
        bits: u32,
        flags: u32,
    },
    Copy {
        sc: u32,
        ss: usize,
        dc: u32,
        ds: usize,
    },
    CopyRights {
        sc: u32,
        ss: usize,
        dc: u32,
        ds: usize,
        rights: Option<u8>,
    },
    Mint {
        sc: u32,
        ss: usize,
        dc: u32,
        ds: usize,
        rights: u8,
        badge: Option<u64>,
    },
    Delete {
        cnode: u32,
        slot: usize,
    },
    Revoke {
        cnode: u32,
        slot: Option<usize>,
    },
    CheckUntyped {
        u: u32,
        used: usize,
        free: usize,
        allocated: usize,
        gap: usize,
    },
    CheckObject {
        obj: u32,
        bytes: Option<(usize, usize)>,
    },
    CheckObjectAlive {
        obj: u32,
        alive: bool,
    },
    CheckCap {
        cap: u32,
        alive: bool,
        rights: u8,
        badge: Option<u64>,
        parent: Option<u32>,
        slot: Option<(u32, usize)>,
    },
    CheckCapAt {
        cnode: u32,
        slot: usize,
        err: Option<&'static str>,
    },
    CheckChildren {
        cap: u32,
        children: &'static [u32],
    },
}

/// Bước = thao tác + kỳ vọng (id trả về / số cap bị revoke / mã lỗi).
#[derive(Debug, Clone, Copy)]
struct Step {
    op: Op,
    expect: Option<Res>,
}

fn s(op: Op) -> Step {
    Step { op, expect: None }
}

fn ok(op: Op, v: u64) -> Step {
    Step {
        op,
        expect: Some(Ok(v)),
    }
}

fn err(op: Op, e: kc::KError) -> Step {
    Step {
        op,
        expect: Some(Err((e.name(), e.id()))),
    }
}

// ---------------------------------------------------------------------------------------------
// Trait: cùng logic test chạy trên hai bản
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapFp {
    obj: u32,
    ty: &'static str,
    parent: Option<u32>,
    slot: Option<(u32, usize)>,
    rights: u8,
    badge: Option<u64>,
    alive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UntFp {
    size: usize,
    used: usize,
    free: usize,
    allocated: usize,
    gap: usize,
}

trait View {
    fn otype(&self, id: u32) -> Option<&'static str>;
    fn oalive(&self, id: u32) -> bool;
    fn obytes(&self, id: u32) -> Option<(usize, usize)>;
    fn cap_fp(&self, id: u32) -> Option<CapFp>;
    fn cnode_slots(&self, id: u32) -> Option<usize>;
    fn cap_at(&self, cnode: u32, slot: usize) -> Result<u32, &'static str>;
    fn children(&self, cap: u32) -> Vec<u32>;
    fn ustat(&self, id: u32) -> Option<UntFp>;
    fn check_caps(&self) -> Result<(), String>;
    fn check_all_bytes(&self) -> Result<(), String>;
}

trait Mut: View {
    fn add_untyped(&mut self, base: u64, size: usize) -> u32;
    fn add_cnode(&mut self, slots: usize) -> u32;
    fn add_kernel(&mut self, ty: Ty) -> u32;
    fn root_cap(&mut self, obj: u32) -> u32;
    fn put_cap(&mut self, cnode: u32, slot: usize, cap: u32) -> Res;
    fn retype(&mut self, cnode: u32, slot: usize, ty: Ty, bits: u32, flags: u32) -> Res;
    fn copy(&mut self, sc: u32, ss: usize, dc: u32, ds: usize) -> Res;
    fn copy_rights(&mut self, sc: u32, ss: usize, dc: u32, ds: usize, rights: Option<u8>) -> Res;
    fn mint(
        &mut self,
        sc: u32,
        ss: usize,
        dc: u32,
        ds: usize,
        rights: u8,
        badge: Option<u64>,
    ) -> Res;
    fn delete(&mut self, cnode: u32, slot: usize) -> Res;
    fn revoke(&mut self, cnode: u32, slot: Option<usize>) -> Res;
}

fn se(e: ks::KError) -> (&'static str, u16) {
    (e.name(), e.id())
}

fn ce(e: kc::KError) -> (&'static str, u16) {
    (e.name(), e.id())
}

impl View for ks::Machine {
    fn otype(&self, id: u32) -> Option<&'static str> {
        self.object_type(id).map(|t| t.name())
    }
    fn oalive(&self, id: u32) -> bool {
        self.object_alive(id)
    }
    fn obytes(&self, id: u32) -> Option<(usize, usize)> {
        self.object_bytes(id)
    }
    fn cap_fp(&self, id: u32) -> Option<CapFp> {
        self.cap_info(id).ok().map(|c| CapFp {
            obj: c.obj,
            ty: c.ty.name(),
            parent: c.parent,
            slot: c.slot.map(|s| (s.cnode, s.slot)),
            rights: c.rights,
            badge: c.badge,
            alive: c.alive,
        })
    }
    fn cnode_slots(&self, id: u32) -> Option<usize> {
        self.cnode_slot_count(id).ok()
    }
    fn cap_at(&self, cnode: u32, slot: usize) -> Result<u32, &'static str> {
        ks::Machine::cap_at(self, cnode, slot).map_err(|e| e.name())
    }
    fn children(&self, cap: u32) -> Vec<u32> {
        self.cap_children(cap).unwrap_or_default()
    }
    fn ustat(&self, id: u32) -> Option<UntFp> {
        match self.object_type(id) {
            Some(ks::ObjType::Untyped) => Some(UntFp {
                size: self.untyped_size(id),
                used: self.untyped_used(id),
                free: self.untyped_free(id),
                allocated: self.untyped_allocated(id),
                gap: self.untyped_gap(id),
            }),
            _ => None,
        }
    }
    fn check_caps(&self) -> Result<(), String> {
        ks::Machine::check_caps(self)
    }
    fn check_all_bytes(&self) -> Result<(), String> {
        ks::Machine::check_all_bytes(self)
    }
}

impl Mut for ks::Machine {
    fn add_untyped(&mut self, base: u64, size: usize) -> u32 {
        ks::Machine::add_untyped(self, base, size)
    }
    fn add_cnode(&mut self, slots: usize) -> u32 {
        ks::Machine::add_cnode(self, slots)
    }
    fn add_kernel(&mut self, ty: Ty) -> u32 {
        ks::Machine::add_kernel_object(self, ty.sim())
    }
    fn root_cap(&mut self, obj: u32) -> u32 {
        ks::Machine::root_cap(self, obj)
    }
    fn put_cap(&mut self, cnode: u32, slot: usize, cap: u32) -> Res {
        ks::Machine::put_cap(self, cnode, slot, cap)
            .map(|_| 0)
            .map_err(se)
    }
    fn retype(&mut self, cnode: u32, slot: usize, ty: Ty, bits: u32, flags: u32) -> Res {
        ks::Machine::retype(self, ks::SlotRef::new(cnode, slot), ty.sim(), bits, flags)
            .map(u64::from)
            .map_err(se)
    }
    fn copy(&mut self, sc: u32, ss: usize, dc: u32, ds: usize) -> Res {
        ks::Machine::copy(self, ks::SlotRef::new(sc, ss), ks::SlotRef::new(dc, ds))
            .map(u64::from)
            .map_err(se)
    }
    fn copy_rights(&mut self, sc: u32, ss: usize, dc: u32, ds: usize, rights: Option<u8>) -> Res {
        ks::Machine::copy_rights(
            self,
            ks::SlotRef::new(sc, ss),
            ks::SlotRef::new(dc, ds),
            rights,
        )
        .map(u64::from)
        .map_err(se)
    }
    fn mint(
        &mut self,
        sc: u32,
        ss: usize,
        dc: u32,
        ds: usize,
        rights: u8,
        badge: Option<u64>,
    ) -> Res {
        ks::Machine::mint(
            self,
            ks::SlotRef::new(sc, ss),
            ks::SlotRef::new(dc, ds),
            rights,
            badge,
        )
        .map(u64::from)
        .map_err(se)
    }
    fn delete(&mut self, cnode: u32, slot: usize) -> Res {
        ks::Machine::delete(self, ks::SlotRef::new(cnode, slot))
            .map(|_| 0)
            .map_err(se)
    }
    fn revoke(&mut self, cnode: u32, slot: Option<usize>) -> Res {
        ks::Machine::revoke(self, cnode, slot)
            .map(|n| n as u64)
            .map_err(se)
    }
}

impl View for kc::Machine {
    fn otype(&self, id: u32) -> Option<&'static str> {
        self.object_type(id).map(|t| t.name())
    }
    fn oalive(&self, id: u32) -> bool {
        self.object_alive(id)
    }
    fn obytes(&self, id: u32) -> Option<(usize, usize)> {
        self.object_bytes(id)
    }
    fn cap_fp(&self, id: u32) -> Option<CapFp> {
        self.cap_info(id).ok().map(|c| CapFp {
            obj: c.obj,
            ty: c.ty.name(),
            parent: c.parent,
            slot: c.slot.map(|s| (s.cnode, s.slot)),
            rights: c.rights,
            badge: c.badge,
            alive: c.alive,
        })
    }
    fn cnode_slots(&self, id: u32) -> Option<usize> {
        self.cnode_slot_count(id).ok()
    }
    fn cap_at(&self, cnode: u32, slot: usize) -> Result<u32, &'static str> {
        kc::Machine::cap_at(self, cnode, slot).map_err(|e| e.name())
    }
    fn children(&self, cap: u32) -> Vec<u32> {
        self.cap_children(cap)
            .map(|it| it.collect())
            .unwrap_or_default()
    }
    fn ustat(&self, id: u32) -> Option<UntFp> {
        match self.object_type(id) {
            Some(kc::ObjType::Untyped) => Some(UntFp {
                size: self.untyped_size(id),
                used: self.untyped_used(id),
                free: self.untyped_free(id),
                allocated: self.untyped_allocated(id),
                gap: self.untyped_gap(id),
            }),
            _ => None,
        }
    }
    fn check_caps(&self) -> Result<(), String> {
        kc::Machine::check_caps(self).map_err(|e| format!("{e}"))
    }
    fn check_all_bytes(&self) -> Result<(), String> {
        kc::Machine::check_all_bytes(self).map_err(|e| format!("{e}"))
    }
}

impl Mut for kc::Machine {
    fn add_untyped(&mut self, base: u64, size: usize) -> u32 {
        kc::Machine::add_untyped(self, base, size)
    }
    fn add_cnode(&mut self, slots: usize) -> u32 {
        kc::Machine::add_cnode(self, slots)
    }
    fn add_kernel(&mut self, ty: Ty) -> u32 {
        kc::Machine::add_kernel_object(self, ty.core())
    }
    fn root_cap(&mut self, obj: u32) -> u32 {
        kc::Machine::root_cap(self, obj)
    }
    fn put_cap(&mut self, cnode: u32, slot: usize, cap: u32) -> Res {
        kc::Machine::put_cap(self, cnode, slot, cap)
            .map(|_| 0)
            .map_err(ce)
    }
    fn retype(&mut self, cnode: u32, slot: usize, ty: Ty, bits: u32, flags: u32) -> Res {
        kc::Machine::retype(self, kc::SlotRef::new(cnode, slot), ty.core(), bits, flags)
            .map(u64::from)
            .map_err(ce)
    }
    fn copy(&mut self, sc: u32, ss: usize, dc: u32, ds: usize) -> Res {
        kc::Machine::copy(self, kc::SlotRef::new(sc, ss), kc::SlotRef::new(dc, ds))
            .map(u64::from)
            .map_err(ce)
    }
    fn copy_rights(&mut self, sc: u32, ss: usize, dc: u32, ds: usize, rights: Option<u8>) -> Res {
        kc::Machine::copy_rights(
            self,
            kc::SlotRef::new(sc, ss),
            kc::SlotRef::new(dc, ds),
            rights,
        )
        .map(u64::from)
        .map_err(ce)
    }
    fn mint(
        &mut self,
        sc: u32,
        ss: usize,
        dc: u32,
        ds: usize,
        rights: u8,
        badge: Option<u64>,
    ) -> Res {
        kc::Machine::mint(
            self,
            kc::SlotRef::new(sc, ss),
            kc::SlotRef::new(dc, ds),
            rights,
            badge,
        )
        .map(u64::from)
        .map_err(ce)
    }
    fn delete(&mut self, cnode: u32, slot: usize) -> Res {
        kc::Machine::delete(self, kc::SlotRef::new(cnode, slot))
            .map(|_| 0)
            .map_err(ce)
    }
    fn revoke(&mut self, cnode: u32, slot: Option<usize>) -> Res {
        kc::Machine::revoke(self, cnode, slot)
            .map(|n| n as u64)
            .map_err(ce)
    }
}

// ---------------------------------------------------------------------------------------------
// Digest + chạy kịch bản
// ---------------------------------------------------------------------------------------------

type ObjFp = (u32, &'static str, bool, Option<(usize, usize)>);
type CapRow = (u32, CapFp);
type CnRow = (u32, usize, Vec<Option<u32>>);
type UnRow = (u32, UntFp);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Dig {
    objs: Vec<ObjFp>,
    caps: Vec<CapRow>,
    cn: Vec<CnRow>,
    un: Vec<UnRow>,
}

fn digest(m: &impl View) -> Dig {
    let mut objs = Vec::new();
    let mut id = 1u32;
    while let Some(ty) = m.otype(id) {
        objs.push((id, ty, m.oalive(id), m.obytes(id)));
        id += 1;
    }
    let mut caps = Vec::new();
    let mut cid = 1u32;
    while let Some(fp) = m.cap_fp(cid) {
        caps.push((cid, fp));
        cid += 1;
    }
    let mut cn = Vec::new();
    for &(oid, ty, alive, _) in &objs {
        if ty == "CNode" && alive {
            let n = m.cnode_slots(oid).expect("CNode sống phải có số slot");
            let mut slots = Vec::new();
            for slot in 0..n {
                slots.push(m.cap_at(oid, slot).ok());
            }
            cn.push((oid, n, slots));
        }
    }
    let mut un = Vec::new();
    for &(oid, ty, alive, _) in &objs {
        if ty == "Untyped" && alive {
            if let Some(st) = m.ustat(oid) {
                un.push((oid, st));
            }
        }
    }
    Dig { objs, caps, cn, un }
}

/// Bất biến byte luôn phải giữ; `check_caps` chỉ có nghĩa khi **mọi cap con đã nằm trong
/// slot** — sim cho phép trạng thái trung gian "cap Retype chưa đặt vào CNode" (parent có,
/// slot chưa có), và test KT-09/10 cũng không gọi `check_caps` trong khoảng đó.
fn invariant<S: View>(name: &str, m: &S, caps_closed: bool) -> Result<(), String> {
    m.check_all_bytes()
        .map_err(|e| format!("{name}: check_all_bytes: {e}"))?;
    if caps_closed {
        m.check_caps()
            .map_err(|e| format!("{name}: check_caps: {e}"))?;
    }
    Ok(())
}

fn apply<M: Mut>(m: &mut M, op: Op) -> Res {
    match op {
        Op::AddUntyped { base, size } => Ok(u64::from(m.add_untyped(base, size))),
        Op::AddCnode { slots } => Ok(u64::from(m.add_cnode(slots))),
        Op::AddKernel { ty } => Ok(u64::from(m.add_kernel(ty))),
        Op::RootCap { obj } => Ok(u64::from(m.root_cap(obj))),
        Op::PutCap { cnode, slot, cap } => m.put_cap(cnode, slot, cap),
        Op::Retype {
            cnode,
            slot,
            ty,
            bits,
            flags,
        } => m.retype(cnode, slot, ty, bits, flags),
        Op::Copy { sc, ss, dc, ds } => m.copy(sc, ss, dc, ds),
        Op::CopyRights {
            sc,
            ss,
            dc,
            ds,
            rights,
        } => m.copy_rights(sc, ss, dc, ds, rights),
        Op::Mint {
            sc,
            ss,
            dc,
            ds,
            rights,
            badge,
        } => m.mint(sc, ss, dc, ds, rights, badge),
        Op::Delete { cnode, slot } => m.delete(cnode, slot),
        Op::Revoke { cnode, slot } => m.revoke(cnode, slot),
        Op::CheckUntyped {
            u,
            used,
            free,
            allocated,
            gap,
        } => {
            let st = m.ustat(u).expect("Untyped phải tồn tại");
            assert_eq!(st.used, used, "Untyped {u}: used");
            assert_eq!(st.free, free, "Untyped {u}: free");
            assert_eq!(st.allocated, allocated, "Untyped {u}: allocated");
            assert_eq!(st.gap, gap, "Untyped {u}: gap");
            Ok(0)
        }
        Op::CheckObject { obj, bytes } => {
            assert_eq!(m.obytes(obj), bytes, "object {obj}: bytes");
            Ok(0)
        }
        Op::CheckObjectAlive { obj, alive } => {
            assert_eq!(m.oalive(obj), alive, "object {obj}: alive");
            Ok(0)
        }
        Op::CheckCap {
            cap,
            alive,
            rights,
            badge,
            parent,
            slot,
        } => {
            let c = m.cap_fp(cap).expect("cap phải tồn tại");
            assert_eq!(c.alive, alive, "cap {cap}: alive");
            assert_eq!(c.rights, rights, "cap {cap}: rights");
            assert_eq!(c.badge, badge, "cap {cap}: badge");
            assert_eq!(c.parent, parent, "cap {cap}: parent");
            assert_eq!(c.slot, slot, "cap {cap}: slot");
            Ok(0)
        }
        Op::CheckCapAt { cnode, slot, err } => match (err, m.cap_at(cnode, slot)) {
            (None, Ok(id)) => {
                assert!(id > 0, "cap_at ({cnode},{slot}) phải trả cap id");
                Ok(0)
            }
            (Some(want), Err(got)) => {
                assert_eq!(got, want, "cap_at ({cnode},{slot}): mã lỗi");
                Ok(0)
            }
            (None, Err(got)) => panic!("cap_at ({cnode},{slot}): muốn Ok, gặp {got}"),
            (Some(want), Ok(id)) => panic!("cap_at ({cnode},{slot}): muốn {want}, gặp Ok({id})"),
        },
        Op::CheckChildren { cap, children } => {
            assert_eq!(m.children(cap), children.to_vec(), "cap {cap}: children");
            Ok(0)
        }
    }
}

/// Đếm phủ của kịch bản (bằng chứng "không phải toàn lỗi").
#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    steps: usize,
    ok: usize,
    err: usize,
    retype_ok: usize,
    copy_ok: usize,
    mint_ok: usize,
    delete_ok: usize,
    revoke_ok: usize,
    revoke_deleted: u64,
    caps_closed: usize,
}

fn run_script(name: &str, steps: &[Step]) -> Stats {
    let mut sm = ks::Machine::new();
    let mut cm = kc::Machine::new();
    let mut stats = Stats::default();
    for (i, st) in steps.iter().enumerate() {
        let sr = apply(&mut sm, st.op);
        let cr = apply(&mut cm, st.op);
        assert_eq!(
            sr, cr,
            "[{name}#{i}] {:?}: sim {sr:?} != core {cr:?}",
            st.op
        );
        if let Some(want) = st.expect {
            assert_eq!(sr, want, "[{name}#{i}] {:?}: kỳ vọng", st.op);
        }
        let sd = digest(&sm);
        let cd = digest(&cm);
        assert_eq!(sd, cd, "[{name}#{i}] {:?}: digest sim != core", st.op);
        let caps_closed = sd
            .caps
            .iter()
            .all(|(_, c)| c.parent.is_none() || c.slot.is_some());
        invariant("sim", &sm, caps_closed).unwrap();
        invariant("core", &cm, caps_closed).unwrap();
        stats.steps += 1;
        match sr {
            Ok(v) => {
                stats.ok += 1;
                match st.op {
                    Op::Retype { .. } => stats.retype_ok += 1,
                    Op::Copy { .. } | Op::CopyRights { .. } => stats.copy_ok += 1,
                    Op::Mint { .. } => stats.mint_ok += 1,
                    Op::Delete { .. } => stats.delete_ok += 1,
                    Op::Revoke { .. } => {
                        stats.revoke_ok += 1;
                        stats.revoke_deleted += v;
                    }
                    _ => {}
                }
            }
            Err(_) => stats.err += 1,
        }
        if caps_closed {
            stats.caps_closed += 1;
        }
    }
    stats
}

// ---------------------------------------------------------------------------------------------
// Kịch bản port từ KT-09/10/11
// ---------------------------------------------------------------------------------------------

const ALL: u8 = kc::RIGHTS_ALL;
const READ: u8 = kc::RIGHT_READ;
const WRITE: u8 = kc::RIGHT_WRITE;

/// KT-09 `tong_byte_bat_bien_sau_moi_retype`: bốn loại object lõi xếp liền, không gap.
fn script_bytes() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x1000,
                size: 8192,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            2,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 512,
            free: 7680,
            allocated: 512,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Endpoint,
                bits: 0,
                flags: 0,
            },
            3,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 576,
            free: 7616,
            allocated: 576,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Reply,
                bits: 0,
                flags: 0,
            },
            4,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 640,
            free: 7552,
            allocated: 640,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::SchedContext,
                bits: 0,
                flags: 0,
            },
            5,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 896,
            free: 7296,
            allocated: 896,
            gap: 0,
        }),
        s(Op::CheckObject {
            obj: 3,
            bytes: Some((0, 512)),
        }),
        s(Op::CheckObject {
            obj: 4,
            bytes: Some((512, 64)),
        }),
        s(Op::CheckObject {
            obj: 6,
            bytes: Some((640, 256)),
        }),
    ]
}

/// KT-09 `watermark_chi_tien_va_het_cho_khong_ro_byte`: vừa khít 1024 B rồi hết chỗ.
fn script_exhaust() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x2000,
                size: 1024,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            2,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 512,
            free: 512,
            allocated: 512,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Endpoint,
                bits: 0,
                flags: 0,
            },
            3,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 576,
            free: 448,
            allocated: 576,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Reply,
                bits: 0,
                flags: 0,
            },
            4,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 640,
            free: 384,
            allocated: 640,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::SchedContext,
                bits: 0,
                flags: 0,
            },
            5,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 896,
            free: 128,
            allocated: 896,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Endpoint,
                bits: 0,
                flags: 0,
            },
            6,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 960,
            free: 64,
            allocated: 960,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Reply,
                bits: 0,
                flags: 0,
            },
            7,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 1024,
            free: 0,
            allocated: 1024,
            gap: 0,
        }),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Endpoint,
                bits: 0,
                flags: 0,
            },
            kc::KError::NotEnoughMemory,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 1024,
            free: 0,
            allocated: 1024,
            gap: 0,
        }),
    ]
}

/// KT-09 `retype_loi_tra_dung_ma`: mọi lỗi không đốt byte.
fn script_retype_errors() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x3000,
                size: 4096,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 1,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            kc::KError::SlotEmpty,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 9,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            kc::KError::RangeError,
        ),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 3),
        ok(Op::RootCap { obj: 3 }, 2),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 2,
                cap: 2,
            },
            0,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 2,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            kc::KError::WrongObjectType,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 3,
                flags: 0,
            },
            kc::KError::Alignment,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 12,
                flags: 0,
            },
            kc::KError::InvalidArgument,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 31,
                flags: 0,
            },
            kc::KError::InvalidArgument,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::VSpace,
                bits: 0,
                flags: 0,
            },
            kc::KError::NotSupported,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::VSpace,
                bits: 0,
                flags: 1,
            },
            kc::KError::NotSupported,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 0,
                flags: 1,
            },
            kc::KError::InvalidArgument,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Untyped,
                bits: 11,
                flags: 0,
            },
            kc::KError::InvalidArgument,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Untyped,
                bits: 64,
                flags: 0,
            },
            kc::KError::InvalidArgument,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Untyped,
                bits: 13,
                flags: 0,
            },
            kc::KError::NotEnoughMemory,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::IrqControl,
                bits: 0,
                flags: 0,
            },
            kc::KError::NotSupported,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 0,
            free: 4096,
            allocated: 0,
            gap: 0,
        }),
    ]
}

/// KT-17/KT-09 `untyped_con_duoc_can_le_va_tinh_du_byte`: gap căn lề vẫn tính byte.
fn script_nested_align() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x4000,
                size: 8192,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            2,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Untyped,
                bits: 12,
                flags: 0,
            },
            3,
        ),
        s(Op::CheckObject {
            obj: 4,
            bytes: Some((4096, 4096)),
        }),
        s(Op::CheckUntyped {
            u: 2,
            used: 8192,
            free: 0,
            allocated: 4608,
            gap: 3584,
        }),
        err(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Untyped,
                bits: 12,
                flags: 0,
            },
            kc::KError::NotEnoughMemory,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 8192,
            free: 0,
            allocated: 4608,
            gap: 3584,
        }),
    ]
}

/// KT-09 `revoke_untyped_long_nhau_finalise_ca_cay`: finalise cả cây Untyped con.
fn script_revoke_nested() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x5000,
                size: 16384,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Untyped,
                bits: 12,
                flags: 0,
            },
            2,
        ),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 3,
                cap: 2,
            },
            0,
        ),
        s(Op::CheckObject {
            obj: 3,
            bytes: Some((0, 4096)),
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 3,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            3,
        ),
        s(Op::CheckUntyped {
            u: 3,
            used: 512,
            free: 3584,
            allocated: 512,
            gap: 0,
        }),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            2,
        ),
        s(Op::CheckObjectAlive {
            obj: 3,
            alive: false,
        }),
        s(Op::CheckObjectAlive {
            obj: 4,
            alive: false,
        }),
        s(Op::CheckUntyped {
            u: 2,
            used: 0,
            free: 16384,
            allocated: 0,
            gap: 0,
        }),
        s(Op::CheckUntyped {
            u: 3,
            used: 0,
            free: 4096,
            allocated: 0,
            gap: 0,
        }),
        s(Op::CheckCapAt {
            cnode: 1,
            slot: 3,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCapAt {
            cnode: 1,
            slot: 4,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCap {
            cap: 2,
            alive: false,
            rights: ALL,
            badge: None,
            parent: Some(1),
            slot: None,
        }),
        s(Op::CheckCap {
            cap: 3,
            alive: false,
            rights: ALL,
            badge: None,
            parent: Some(2),
            slot: None,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            4,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 512,
            free: 15872,
            allocated: 512,
            gap: 0,
        }),
    ]
}

/// KT-09 `double_free_bi_bat`: revoke lần hai không trả byte lần nữa.
fn script_double_free() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x6000,
                size: 4096,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Endpoint,
                bits: 0,
                flags: 0,
            },
            2,
        ),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            1,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 0,
            free: 4096,
            allocated: 0,
            gap: 0,
        }),
        ok(Op::Delete { cnode: 1, slot: 0 }, 0),
        err(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            kc::KError::SlotEmpty,
        ),
        s(Op::CheckUntyped {
            u: 2,
            used: 0,
            free: 4096,
            allocated: 0,
            gap: 0,
        }),
        s(Op::CheckCap {
            cap: 2,
            alive: false,
            rights: ALL,
            badge: None,
            parent: Some(1),
            slot: None,
        }),
        s(Op::CheckObjectAlive {
            obj: 3,
            alive: false,
        }),
    ]
}

/// KT-10 `delete_cha_reparent_con_cho_ong` + `revoke_none_xoa_moi_cay_con_trong_cnode`.
fn script_mdb_tree() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(Op::AddCnode { slots: 4 }, 2),
        ok(Op::AddCnode { slots: 4 }, 3),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 4),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 5),
        ok(Op::RootCap { obj: 4 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        s(Op::CheckCap {
            cap: 1,
            alive: true,
            rights: ALL,
            badge: None,
            parent: None,
            slot: Some((1, 0)),
        }),
        ok(
            Op::Copy {
                sc: 1,
                ss: 0,
                dc: 1,
                ds: 1,
            },
            2,
        ),
        ok(
            Op::Copy {
                sc: 1,
                ss: 1,
                dc: 2,
                ds: 0,
            },
            3,
        ),
        ok(
            Op::Copy {
                sc: 2,
                ss: 0,
                dc: 2,
                ds: 1,
            },
            4,
        ),
        s(Op::CheckChildren {
            cap: 1,
            children: &[2],
        }),
        s(Op::CheckChildren {
            cap: 2,
            children: &[3],
        }),
        s(Op::CheckChildren {
            cap: 3,
            children: &[4],
        }),
        ok(Op::Delete { cnode: 1, slot: 1 }, 0),
        s(Op::CheckCap {
            cap: 2,
            alive: false,
            rights: ALL,
            badge: None,
            parent: None,
            slot: None,
        }),
        s(Op::CheckCap {
            cap: 3,
            alive: true,
            rights: ALL,
            badge: None,
            parent: Some(1),
            slot: Some((2, 0)),
        }),
        s(Op::CheckChildren {
            cap: 1,
            children: &[3],
        }),
        s(Op::CheckChildren {
            cap: 3,
            children: &[4],
        }),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            2,
        ),
        s(Op::CheckCapAt {
            cnode: 2,
            slot: 0,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCapAt {
            cnode: 2,
            slot: 1,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCap {
            cap: 1,
            alive: true,
            rights: ALL,
            badge: None,
            parent: None,
            slot: Some((1, 0)),
        }),
        ok(Op::RootCap { obj: 5 }, 5),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 2,
                cap: 5,
            },
            0,
        ),
        ok(
            Op::Copy {
                sc: 1,
                ss: 0,
                dc: 3,
                ds: 0,
            },
            6,
        ),
        ok(
            Op::Copy {
                sc: 3,
                ss: 0,
                dc: 3,
                ds: 1,
            },
            7,
        ),
        ok(
            Op::Copy {
                sc: 1,
                ss: 0,
                dc: 3,
                ds: 2,
            },
            8,
        ),
        s(Op::CheckChildren {
            cap: 1,
            children: &[6, 8],
        }),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: None,
            },
            3,
        ),
        s(Op::CheckCap {
            cap: 1,
            alive: true,
            rights: ALL,
            badge: None,
            parent: None,
            slot: Some((1, 0)),
        }),
        s(Op::CheckCap {
            cap: 5,
            alive: true,
            rights: ALL,
            badge: None,
            parent: None,
            slot: Some((1, 2)),
        }),
        s(Op::CheckCap {
            cap: 6,
            alive: false,
            rights: ALL,
            badge: None,
            parent: Some(1),
            slot: None,
        }),
        s(Op::CheckCapAt {
            cnode: 3,
            slot: 0,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCapAt {
            cnode: 3,
            slot: 1,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCapAt {
            cnode: 3,
            slot: 2,
            err: Some("SlotEmpty"),
        }),
    ]
}

/// KT-10 `revoke_untyped_finalise_cnode_va_xoa_cap_trong_slot`.
fn script_finalise_cnode() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x7000,
                size: 16384,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::CNode,
                bits: 4,
                flags: 0,
            },
            2,
        ),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 1,
                cap: 2,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            3,
        ),
        ok(
            Op::PutCap {
                cnode: 3,
                slot: 1,
                cap: 3,
            },
            0,
        ),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 5),
        ok(Op::RootCap { obj: 5 }, 4),
        ok(
            Op::PutCap {
                cnode: 3,
                slot: 0,
                cap: 4,
            },
            0,
        ),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            3,
        ),
        s(Op::CheckObjectAlive {
            obj: 3,
            alive: false,
        }),
        s(Op::CheckObjectAlive {
            obj: 4,
            alive: false,
        }),
        s(Op::CheckObjectAlive {
            obj: 5,
            alive: true,
        }),
        s(Op::CheckCap {
            cap: 4,
            alive: false,
            rights: ALL,
            badge: None,
            parent: None,
            slot: None,
        }),
        s(Op::CheckCapAt {
            cnode: 3,
            slot: 0,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckCapAt {
            cnode: 3,
            slot: 1,
            err: Some("SlotEmpty"),
        }),
        s(Op::CheckUntyped {
            u: 2,
            used: 0,
            free: 16384,
            allocated: 0,
            gap: 0,
        }),
        s(Op::CheckCapAt {
            cnode: 1,
            slot: 1,
            err: Some("SlotEmpty"),
        }),
    ]
}

/// KT-10 `finalise_cnode_long_nhau_xoa_ca_cap_trong_cnode_con`.
fn script_finalise_cnode_nested() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0x8000,
                size: 16384,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::CNode,
                bits: 4,
                flags: 0,
            },
            2,
        ),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 1,
                cap: 2,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::CNode,
                bits: 4,
                flags: 0,
            },
            3,
        ),
        ok(
            Op::PutCap {
                cnode: 3,
                slot: 0,
                cap: 3,
            },
            0,
        ),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 5),
        ok(Op::RootCap { obj: 5 }, 4),
        ok(
            Op::PutCap {
                cnode: 4,
                slot: 0,
                cap: 4,
            },
            0,
        ),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            3,
        ),
        s(Op::CheckObjectAlive {
            obj: 3,
            alive: false,
        }),
        s(Op::CheckObjectAlive {
            obj: 4,
            alive: false,
        }),
        s(Op::CheckObjectAlive {
            obj: 5,
            alive: true,
        }),
        s(Op::CheckCap {
            cap: 4,
            alive: false,
            rights: ALL,
            badge: None,
            parent: None,
            slot: None,
        }),
    ]
}

/// KT-11: `Mint(Some(0))` xoá badge; Copy/Mint không nới quyền; Retype kiểm WRITE và kế thừa.
fn script_rights() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(Op::AddCnode { slots: 8 }, 2),
        ok(
            Op::AddUntyped {
                base: 0x9000,
                size: 16384,
            },
            3,
        ),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 4),
        ok(Op::RootCap { obj: 4 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Mint {
                sc: 1,
                ss: 0,
                dc: 1,
                ds: 1,
                rights: ALL,
                badge: Some(7),
            },
            2,
        ),
        s(Op::CheckCap {
            cap: 2,
            alive: true,
            rights: ALL,
            badge: Some(7),
            parent: Some(1),
            slot: Some((1, 1)),
        }),
        ok(
            Op::Mint {
                sc: 1,
                ss: 1,
                dc: 2,
                ds: 0,
                rights: ALL,
                badge: Some(0),
            },
            3,
        ),
        s(Op::CheckCap {
            cap: 3,
            alive: true,
            rights: ALL,
            badge: None,
            parent: Some(2),
            slot: Some((2, 0)),
        }),
        ok(
            Op::Mint {
                sc: 1,
                ss: 1,
                dc: 2,
                ds: 1,
                rights: ALL,
                badge: None,
            },
            4,
        ),
        s(Op::CheckCap {
            cap: 4,
            alive: true,
            rights: ALL,
            badge: Some(7),
            parent: Some(2),
            slot: Some((2, 1)),
        }),
        ok(
            Op::CopyRights {
                sc: 2,
                ss: 0,
                dc: 2,
                ds: 2,
                rights: Some(READ),
            },
            5,
        ),
        s(Op::CheckCap {
            cap: 5,
            alive: true,
            rights: READ,
            badge: None,
            parent: Some(3),
            slot: Some((2, 2)),
        }),
        err(
            Op::CopyRights {
                sc: 2,
                ss: 2,
                dc: 2,
                ds: 3,
                rights: Some(WRITE),
            },
            kc::KError::InsufficientRights,
        ),
        ok(
            Op::Copy {
                sc: 2,
                ss: 2,
                dc: 2,
                ds: 3,
            },
            6,
        ),
        s(Op::CheckCap {
            cap: 6,
            alive: true,
            rights: READ,
            badge: None,
            parent: Some(5),
            slot: Some((2, 3)),
        }),
        ok(Op::RootCap { obj: 3 }, 7),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 2,
                cap: 7,
            },
            0,
        ),
        ok(
            Op::Mint {
                sc: 1,
                ss: 2,
                dc: 1,
                ds: 3,
                rights: READ,
                badge: None,
            },
            8,
        ),
        err(
            Op::Retype {
                cnode: 1,
                slot: 3,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            kc::KError::InsufficientRights,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 2,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            9,
        ),
        s(Op::CheckCap {
            cap: 9,
            alive: true,
            rights: ALL,
            badge: None,
            parent: Some(7),
            slot: None,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 2,
                ty: Ty::Untyped,
                bits: 12,
                flags: 0,
            },
            10,
        ),
        s(Op::CheckUntyped {
            u: 3,
            used: 8192,
            free: 8192,
            allocated: 4608,
            gap: 3584,
        }),
        ok(
            Op::Mint {
                sc: 1,
                ss: 2,
                dc: 1,
                ds: 4,
                rights: READ | WRITE,
                badge: None,
            },
            11,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 4,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            12,
        ),
        s(Op::CheckCap {
            cap: 12,
            alive: true,
            rights: READ | WRITE,
            badge: None,
            parent: Some(11),
            slot: None,
        }),
        s(Op::CheckUntyped {
            u: 3,
            used: 8704,
            free: 7680,
            allocated: 5120,
            gap: 3584,
        }),
    ]
}

/// KT-09 `lap_lai_retype_va_revoke_khong_ro_byte`: vòng cấp/revoke 20 lần trả đủ byte.
fn script_retype_revoke_loop() -> Vec<Step> {
    let mut steps = vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0xa000,
                size: 4096,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
    ];
    for _ in 0..20 {
        steps.push(s(Op::Retype {
            cnode: 1,
            slot: 0,
            ty: Ty::Endpoint,
            bits: 0,
            flags: 0,
        }));
        steps.push(s(Op::Retype {
            cnode: 1,
            slot: 0,
            ty: Ty::Reply,
            bits: 0,
            flags: 0,
        }));
        steps.push(s(Op::CheckUntyped {
            u: 2,
            used: 128,
            free: 3968,
            allocated: 128,
            gap: 0,
        }));
        steps.push(s(Op::Revoke {
            cnode: 1,
            slot: Some(0),
        }));
        steps.push(s(Op::CheckUntyped {
            u: 2,
            used: 0,
            free: 4096,
            allocated: 0,
            gap: 0,
        }));
    }
    steps
}

/// D2-83: `Revoke`/`put_cap` trên CNode **đã finalise** phải trả `InvalidCapability` như sim
/// (sim đọc body → `Dead`), không đọc metadata trả `Ok(0)`/`SlotEmpty`/`WrongObjectType`.
fn script_dead_cnode_errors() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(
            Op::AddUntyped {
                base: 0xb000,
                size: 16384,
            },
            2,
        ),
        ok(Op::RootCap { obj: 2 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 0,
                ty: Ty::CNode,
                bits: 4,
                flags: 0,
            },
            2,
        ),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 1,
                cap: 2,
            },
            0,
        ),
        ok(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            1,
        ),
        err(
            Op::Revoke {
                cnode: 3,
                slot: Some(0),
            },
            kc::KError::InvalidCapability,
        ),
        err(
            Op::Revoke {
                cnode: 3,
                slot: Some(99),
            },
            kc::KError::InvalidCapability,
        ),
        err(
            Op::Revoke {
                cnode: 3,
                slot: None,
            },
            kc::KError::InvalidCapability,
        ),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 4),
        ok(Op::RootCap { obj: 4 }, 3),
        err(
            Op::PutCap {
                cnode: 3,
                slot: 0,
                cap: 3,
            },
            kc::KError::InvalidCapability,
        ),
        s(Op::CheckCap {
            cap: 3,
            alive: true,
            rights: ALL,
            badge: None,
            parent: None,
            slot: None,
        }),
        s(Op::CheckCapAt {
            cnode: 3,
            slot: 0,
            err: Some("SlotEmpty"),
        }),
    ]
}

/// KT-10 `delete_cap_goc_thi_con_thanh_goc`: xoá cap gốc ⇒ con thành gốc (không mồ côi).
fn script_delete_root_cap() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(Op::AddCnode { slots: 4 }, 2),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 3),
        ok(Op::RootCap { obj: 3 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(
            Op::Copy {
                sc: 1,
                ss: 0,
                dc: 2,
                ds: 0,
            },
            2,
        ),
        s(Op::CheckChildren {
            cap: 1,
            children: &[2],
        }),
        ok(Op::Delete { cnode: 1, slot: 0 }, 0),
        s(Op::CheckCap {
            cap: 1,
            alive: false,
            rights: ALL,
            badge: None,
            parent: None,
            slot: None,
        }),
        s(Op::CheckCap {
            cap: 2,
            alive: true,
            rights: ALL,
            badge: None,
            parent: None,
            slot: Some((2, 0)),
        }),
        s(Op::CheckChildren {
            cap: 2,
            children: &[],
        }),
        err(
            Op::Revoke {
                cnode: 1,
                slot: Some(0),
            },
            kc::KError::SlotEmpty,
        ),
    ]
}

/// Nhánh còn lại: `put_cap` cap đã có slot ⇒ `IllegalOperation`; badge `Some(7)` trên Tcb ⇒
/// `InvalidArgument` còn `Some(0)` hợp lệ; Retype Untyped `len` 8192 > `ARENA_ALIGN` ⇒
/// `Alignment` (sau khi đã qua cổng `len <= u_size`).
fn script_badge_and_align() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(Op::AddCnode { slots: 8 }, 2),
        ok(
            Op::AddUntyped {
                base: 0xc000,
                size: 8192,
            },
            3,
        ),
        ok(Op::AddKernel { ty: Ty::Tcb }, 4),
        ok(Op::RootCap { obj: 4 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        err(
            Op::PutCap {
                cnode: 2,
                slot: 0,
                cap: 1,
            },
            kc::KError::IllegalOperation,
        ),
        err(
            Op::Mint {
                sc: 1,
                ss: 0,
                dc: 2,
                ds: 0,
                rights: ALL,
                badge: Some(7),
            },
            kc::KError::InvalidArgument,
        ),
        s(Op::CheckCapAt {
            cnode: 2,
            slot: 0,
            err: Some("SlotEmpty"),
        }),
        ok(
            Op::Mint {
                sc: 1,
                ss: 0,
                dc: 2,
                ds: 0,
                rights: ALL,
                badge: Some(0),
            },
            2,
        ),
        s(Op::CheckCap {
            cap: 2,
            alive: true,
            rights: ALL,
            badge: None,
            parent: Some(1),
            slot: Some((2, 0)),
        }),
        ok(Op::RootCap { obj: 3 }, 3),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 1,
                cap: 3,
            },
            0,
        ),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 1,
                ty: Ty::Tcb,
                bits: 0,
                flags: 0,
            },
            4,
        ),
        s(Op::CheckUntyped {
            u: 3,
            used: 512,
            free: 7680,
            allocated: 512,
            gap: 0,
        }),
        ok(
            Op::Retype {
                cnode: 1,
                slot: 1,
                ty: Ty::Untyped,
                bits: 12,
                flags: 0,
            },
            5,
        ),
        s(Op::CheckUntyped {
            u: 3,
            used: 8192,
            free: 0,
            allocated: 4608,
            gap: 3584,
        }),
        err(
            Op::Retype {
                cnode: 1,
                slot: 1,
                ty: Ty::Untyped,
                bits: 13,
                flags: 0,
            },
            kc::KError::Alignment,
        ),
        s(Op::CheckUntyped {
            u: 3,
            used: 8192,
            free: 0,
            allocated: 4608,
            gap: 3584,
        }),
    ]
}

#[test]
fn parity_kich_ban_kt09_kt10_kt11() {
    for (name, steps) in [
        ("bytes", script_bytes()),
        ("exhaust", script_exhaust()),
        ("retype_errors", script_retype_errors()),
        ("nested_align", script_nested_align()),
        ("revoke_nested", script_revoke_nested()),
        ("double_free", script_double_free()),
        ("mdb_tree", script_mdb_tree()),
        ("finalise_cnode", script_finalise_cnode()),
        ("finalise_cnode_nested", script_finalise_cnode_nested()),
        ("rights", script_rights()),
        ("retype_revoke_loop", script_retype_revoke_loop()),
        ("dead_cnode_errors", script_dead_cnode_errors()),
        ("delete_root_cap", script_delete_root_cap()),
        ("badge_and_align", script_badge_and_align()),
    ] {
        run_script(name, &steps);
    }
}

// ---------------------------------------------------------------------------------------------
// Differential ngẫu nhiên tất định
// ---------------------------------------------------------------------------------------------

fn pick_slot(rng: &mut vifast_sim::Lcg) -> (u32, usize) {
    // CNode 1 có 8 slot, CNode 2/3 có 4 slot — cố ý chọn slot ngoài dải để phủ RangeError.
    let cnode = [1u32, 2, 3][rng.below(3) as usize];
    (cnode, rng.below(8) as usize)
}

/// Slot nguồn: ưu tiên vùng 0..4 (nơi rig đã đặt cap) để bài học Copy/Retype không rỗng.
fn pick_src(rng: &mut vifast_sim::Lcg) -> (u32, usize) {
    let cnode = [1u32, 2, 3][rng.below(3) as usize];
    (cnode, rng.below(4) as usize)
}

fn pick_ty(rng: &mut vifast_sim::Lcg) -> Ty {
    [
        Ty::Tcb,
        Ty::Endpoint,
        Ty::Notification,
        Ty::Reply,
        Ty::SchedContext,
        Ty::SchedControl,
        Ty::CNode,
    ][rng.below(7) as usize]
}

fn random_op(rng: &mut vifast_sim::Lcg) -> Op {
    match rng.below(10) {
        0..=2 => {
            let (cnode, slot) = pick_src(rng);
            let ty = pick_ty(rng);
            let bits = if ty == Ty::CNode {
                4 + rng.below(2) as u32
            } else if rng.below(8) == 0 {
                [3u32, 12][rng.below(2) as usize]
            } else {
                0
            };
            let flags = if rng.below(16) == 0 { 1 } else { 0 };
            Op::Retype {
                cnode,
                slot,
                ty,
                bits,
                flags,
            }
        }
        3 | 4 => {
            let (sc, ss) = pick_src(rng);
            let (dc, ds) = pick_slot(rng);
            Op::Copy { sc, ss, dc, ds }
        }
        5 => {
            let (sc, ss) = pick_src(rng);
            let (dc, ds) = pick_slot(rng);
            let rights = [ALL, READ, WRITE, READ | WRITE][rng.below(4) as usize];
            let badge = [None, Some(0), Some(9), Some(0xdead)][rng.below(4) as usize];
            Op::Mint {
                sc,
                ss,
                dc,
                ds,
                rights,
                badge,
            }
        }
        6 => {
            let (sc, ss) = pick_src(rng);
            let (dc, ds) = pick_slot(rng);
            let rights = [None, Some(READ), Some(WRITE), Some(ALL)][rng.below(4) as usize];
            Op::CopyRights {
                sc,
                ss,
                dc,
                ds,
                rights,
            }
        }
        7 => {
            let (cnode, slot) = pick_src(rng);
            Op::Delete { cnode, slot }
        }
        _ => {
            let cnode = [1u32, 2, 3][rng.below(3) as usize];
            let slot = if rng.below(2) == 0 {
                None
            } else {
                Some(rng.below(4) as usize)
            };
            Op::Revoke { cnode, slot }
        }
    }
}

fn differential_rig() -> Vec<Step> {
    vec![
        ok(Op::AddCnode { slots: 8 }, 1),
        ok(Op::AddCnode { slots: 4 }, 2),
        ok(Op::AddCnode { slots: 4 }, 3),
        ok(Op::AddKernel { ty: Ty::Endpoint }, 4),
        ok(
            Op::AddKernel {
                ty: Ty::Notification,
            },
            5,
        ),
        ok(
            Op::AddUntyped {
                base: 0x1000,
                size: 32 * 1024,
            },
            6,
        ),
        ok(
            Op::AddUntyped {
                base: 0x9000,
                size: 16 * 1024,
            },
            7,
        ),
        ok(Op::RootCap { obj: 4 }, 1),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 0,
                cap: 1,
            },
            0,
        ),
        ok(Op::RootCap { obj: 5 }, 2),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 1,
                cap: 2,
            },
            0,
        ),
        ok(Op::RootCap { obj: 6 }, 3),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 2,
                cap: 3,
            },
            0,
        ),
        ok(Op::RootCap { obj: 7 }, 4),
        ok(
            Op::PutCap {
                cnode: 1,
                slot: 3,
                cap: 4,
            },
            0,
        ),
    ]
}

fn differential_seed(seed: u64) -> Stats {
    let mut steps = differential_rig();
    let mut rng = vifast_sim::Lcg::new(seed);
    for _ in 0..48 {
        steps.push(s(random_op(&mut rng)));
    }
    run_script(&format!("diff-{seed}"), &steps)
}

#[test]
fn parity_differential_32_seed_x48_op() {
    let mut total = Stats::default();
    for seed in 0..32u64 {
        let st = differential_seed(seed);
        total.steps += st.steps;
        total.ok += st.ok;
        total.err += st.err;
        total.retype_ok += st.retype_ok;
        total.copy_ok += st.copy_ok;
        total.mint_ok += st.mint_ok;
        total.delete_ok += st.delete_ok;
        total.revoke_ok += st.revoke_ok;
        total.revoke_deleted += st.revoke_deleted;
        total.caps_closed += st.caps_closed;
    }
    eprintln!("# parity differential: {total:?}");
    // Ngưỡng phủ tối thiểu (không nới để "cho xanh": chỉ chứng minh kịch bản không rỗng).
    assert!(total.ok >= 300, "quá ít thao tác thành công: {total:?}");
    assert!(total.err >= 300, "quá ít nhánh lỗi: {total:?}");
    assert!(total.retype_ok >= 40, "quá ít Retype thành công: {total:?}");
    assert!(
        total.copy_ok >= 40,
        "quá ít Copy/Mint thành công: {total:?}"
    );
    assert!(
        total.revoke_deleted >= 20,
        "quá ít cap bị Revoke: {total:?}"
    );
    assert!(
        total.caps_closed >= 100,
        "quá ít trạng thái check_caps: {total:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Hằng số ABI + dấu chân tĩnh
// ---------------------------------------------------------------------------------------------

/// D2-83 — biên giới core/VM: core **không** Retype Frame/PageTable/VSpace (object layer
/// `vifast-plat-common::obj` sở hữu); pin để không ai "vá" thành port VM vào core.
#[test]
fn core_d2_83_vm_reserved_not_supported() {
    use kc::{KError, Machine, ObjType, SlotRef};
    let mut m = Machine::new();
    let c = m.add_cnode(8);
    let u = m.add_untyped(0x1000, 8192);
    let cap = m.root_cap(u);
    m.put_cap(c, 0, cap).expect("cap Untyped");
    for ty in [ObjType::Frame, ObjType::PageTable, ObjType::VSpace] {
        assert_eq!(
            m.retype(SlotRef::new(c, 0), ty, 0, 0),
            Err(KError::NotSupported),
            "D2-83: {ty:?} phải do obj layer sở hữu, core trả NotSupported"
        );
    }
    assert_eq!(
        m.retype(SlotRef::new(c, 0), ObjType::IrqControl, 0, 0),
        Err(KError::NotSupported)
    );
    assert_eq!(m.untyped_used(u), 0, "Retype reserved không đốt byte");
    assert_eq!(m.cap_count(), 1, "Retype reserved không tạo cap");
}

#[test]
fn core_hang_so_spec_va_dau_chan_tinh() {
    // D2-11: hằng đọc từ crate sinh (`vifast-abi`), không hardcode.
    assert_eq!(kc::spec::SYSCALL_COUNT, 16);
    assert_eq!(kc::spec::ERROR_COUNT, 19);
    assert_eq!(kc::SLOT_BYTES, 48);
    assert_eq!(kc::CNODE_BASE_BYTES, 64);
    assert_eq!(kc::PAGE_BYTES, 4096);
    assert_eq!(kc::UNTYPED_MIN_BYTES, 4096);
    assert_eq!(kc::RIGHTS_ALL.count_ones(), 4, "đúng 4 quyền");
    assert_eq!(kc::KError::SlotOccupied.id(), 7);
    assert_eq!(kc::KError::RangeError.id(), 19);
    assert_eq!(kc::ObjType::from_name("CNode"), Some(kc::ObjType::CNode));
    assert_eq!(kc::ObjType::cnode_bytes(16), 64 + 16 * 48);
    let size = core::mem::size_of::<kc::Machine>();
    eprintln!(
        "# core: size_of::<Machine>() = {size} B, MAX_OBJECTS={}, MAX_CAPS={}, MAX_SLOT_WORDS={}",
        kc::MAX_OBJECTS,
        kc::MAX_CAPS,
        kc::MAX_SLOT_WORDS
    );
    assert!(size <= 96 * 1024, "core tĩnh phải ≤ 96 KiB (đo {size} B)");
}
