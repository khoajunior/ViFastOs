// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pha **core kernel thật** của ảnh test `vifast-kbin-ktest` (KT-61) — TAP **43..52**; trap lồng
//! dời sang **53**.
//!
//! Mọi thao tác đi qua [`vifast_plat_riscv64::core_bridge::with_machine`] — tức `Machine` **static
//! trong `.bss` của ảnh kernel** (không phải model host, không phải sim). Bằng chứng `# core:`
//! in số object/cap/TCB **thật** của lần boot này + trạng thái các bất biến
//! (`check_caps`/`check_bytes`/`check_sched`/`check_ipc`/…).
//!
//! | TAP | Điều kiện đo |
//! |---|---|
//! | `ok 43 - core link` | `Machine` là static trong **vùng ảnh** `[0x8020_0000, guard)`; `size_of::<Machine>()` ≤ pin D2-84 **196.608 B**; bảng phân bố 27 dòng khớp `sum + scalars + padding == total`, padding ≤ 8 (D2-85c) |
//! | `ok 44 - core untyped boot` | Untyped boot dựng từ **vùng nhãn PA** ngay sau arena object layer (D2-83: không dereference); byte `used=0 free=size`, `check_bytes`/`check_all_bytes` xanh |
//! | `ok 45 - core tcb retype` | `Retype` **từ Untyped**: 1 CNode gốc (32 slot) + 3 TCB + Endpoint + Reply + Notification + SchedControl + SchedContext; cap vào slot, TCB `Inactive`, `check_caps`/`check_bytes` xanh |
//! | `ok 46 - core configure resume suspend` | `Tcb.Configure` (cspace) + `SchedControl.Configure(4,8)` + `SetSchedParams` + `SetPriority(3,10)` + `Resume`/`Suspend(Abort)`; `suspended` đúng, priority/cspace/SC đọc lại khớp, `check_sched`/`check_sc_refs`/`check_budget` xanh |
//! | `ok 47 - core sched 2 tcb` | 2 TCB Ready khác priority: `sched_pick` chọn đúng; `dispatch` chạy TCB 1; `Suspend` TCB 1 ⇒ **không** còn trong hàng đợi ⇒ `dispatch` chuyển sang TCB 2; `Resume` đưa lại |
//! | `ok 48 - core ipc round` | `Call → Recv → Reply` **một vòng** trên Endpoint/Reply thật; MR tới đích, `BlockedOnSend{call}`→`BlockedOnReply`→`Ready`, `Reply.caller` sạch, `check_ipc`/`check_reply_links` xanh |
//! | `ok 49 - core notification signal` | bind + `Wait` (chặn) + `Signal(0x42)` (đánh thức, giao word/badge) + `Signal` không waiter rồi `Poll`; `check_notifications` xanh |
//! | `ok 50 - core waitaddr` | `WaitAddr(nhãn Frame, +64, 0)` chặn, `SetAddr`/`WakeAddr(1)` đánh thức; nhãn **không có byte** (D2-83), bucket rỗng, `check_waitaddr` xanh |
//! | `ok 51 - core d2-83 boundary` | `Retype` `Frame`/`PageTable`/`VSpace` trong core ⇒ **`NotSupported` 3/3**, `untyped_used` không đổi (atomic); **cùng lúc** object layer tạo/revoke VSpace+Frame thật ⇒ biên giới một chiều giữ nguyên |
//! | `ok 52 - core invariants` | Revoke cây Untyped ⇒ byte về 0 (`used=0 free=size gap=0`), cap còn đúng 1; **12/12** bất biến xanh, 0 rò |
//!
//! Số đo do chính boot in (`# core:`); `ktest.sh` grep đúng các dòng đó — tài liệu không chép số
//! tay (QD §9 Q12). Xem `v2/docs/k3b-core.md` §9.

use core::fmt::Write;

use vifast_core::{KError, Machine, ObjId, ObjType, SlotRef, SuspendMode, TcbState};
use vifast_plat_common::obj::ObjKind;
use vifast_plat_common::{Platform, PlatformWriter};

use vifast_plat_riscv64::core_bridge;
use vifast_plat_riscv64::obj as aobj;
use vifast_plat_riscv64::Riscv64Virt;

static PLATFORM: Riscv64Virt = Riscv64Virt;

/// MR của vòng `Call` (hằng để TAP grep được giá trị cụ thể).
const IPC_MR: u64 = 0xfeed;
/// Giá trị futex của TAP 50.
const WA_VALUE: u64 = 0x1234;
/// Offset futex của TAP 50 (byte trong Frame).
const WA_OFFSET: u32 = 64;

/// Bộ định danh dựng từ Untyped cho 10 TAP core.
struct Rig {
    root: ObjId,
    us: SlotRef,
    t0s: SlotRef,
    t1s: SlotRef,
    t2s: SlotRef,
    eps: SlotRef,
    rs: SlotRef,
    ns: SlotRef,
    ctl: SlotRef,
    sc: SlotRef,
    sc1: SlotRef,
    cspace: SlotRef,
    t0: ObjId,
    t1: ObjId,
    t2: ObjId,
    ep: ObjId,
    reply: ObjId,
    ntfn: ObjId,
    sc_obj: ObjId,
    sc1_obj: ObjId,
}

/// Tên trạng thái TCB ổn định để in evidence.
fn state_name(s: TcbState) -> &'static str {
    match s {
        TcbState::Inactive => "Inactive",
        TcbState::Ready => "Ready",
        TcbState::Running { .. } => "Running",
        TcbState::BlockedOnSend { .. } => "BlockedOnSend",
        TcbState::BlockedOnRecv { .. } => "BlockedOnRecv",
        TcbState::BlockedOnReply { .. } => "BlockedOnReply",
        TcbState::BlockedOnNotification { .. } => "BlockedOnNotification",
        TcbState::BlockedOnAddress { .. } => "BlockedOnAddress",
        TcbState::Sleeping { .. } => "Sleeping",
        TcbState::Stopping => "Stopping",
        TcbState::Dead => "Dead",
    }
}

/// `true` ⇔ mọi bất biến của core xanh (12 nhóm; gọi **sau** mỗi pha để bắt lệch sớm).
fn invariants_ok(m: &Machine) -> bool {
    m.check_caps().is_ok()
        && m.check_all_bytes().is_ok()
        && m.check_sched().is_ok()
        && m.check_sc_refs().is_ok()
        && m.check_budget().is_ok()
        && m.check_mailboxes().is_ok()
        && m.check_ipc().is_ok()
        && m.check_reply_links().is_ok()
        && m.check_notifications().is_ok()
        && m.check_waitaddr().is_ok()
        && m.check_fault().is_ok()
}

/// Dựng CNode gốc + 8 object lõi từ **Untyped của core** (harness cấp, như `build()` parity
/// KT-60). Lỗi ở bước này là lỗi model ⇒ panic có chuỗi ổn định để runner đỏ.
fn build_rig(m: &mut Machine, u: ObjId) -> Rig {
    let root = m.add_cnode(32);
    let us = SlotRef::new(root, 0);
    let uc = m.root_cap(u);
    m.put_cap(root, 0, uc).expect("core rig: cap Untyped");
    // 9 object lõi — **hai** SchedContext để cả 2 TCB có ngân sách chạy được
    // (`tcb_has_budget` chỉ đúng khi TCB có SC cấu hình — KT-20).
    let tys = [
        ObjType::Tcb,
        ObjType::Tcb,
        ObjType::Tcb,
        ObjType::Endpoint,
        ObjType::Reply,
        ObjType::Notification,
        ObjType::SchedControl,
        ObjType::SchedContext,
        ObjType::SchedContext,
    ];
    let mut obj = [0u32; 9];
    for (i, ty) in tys.iter().enumerate() {
        let cap = m
            .retype(us, *ty, 0, 0)
            .expect("core rig: retype object lõi");
        m.put_cap(root, 1 + i, cap)
            .expect("core rig: đặt cap object");
        obj[i] = m.cap_obj(cap).expect("core rig: cap_obj");
    }
    // Cap trỏ chính CNode gốc để làm CSpace của TCB (self-reference như sim).
    let rc = m.root_cap(root);
    m.put_cap(root, 10, rc).expect("core rig: cap cspace");
    Rig {
        root,
        us,
        t0s: SlotRef::new(root, 1),
        t1s: SlotRef::new(root, 2),
        t2s: SlotRef::new(root, 3),
        eps: SlotRef::new(root, 4),
        rs: SlotRef::new(root, 5),
        ns: SlotRef::new(root, 6),
        ctl: SlotRef::new(root, 7),
        sc: SlotRef::new(root, 8),
        sc1: SlotRef::new(root, 9),
        cspace: SlotRef::new(root, 10),
        t0: obj[0],
        t1: obj[1],
        t2: obj[2],
        ep: obj[3],
        reply: obj[4],
        ntfn: obj[5],
        sc_obj: obj[7],
        sc1_obj: obj[8],
    }
}

/// TAP **43**: `Machine` static trong ảnh + bảng phân bố byte (D2-84/D2-85c).
fn tap43_link(report: &core_bridge::CoreReport, fails: &mut usize) {
    let f = report.footprint;
    let mut sum = 0usize;
    for i in 0..vifast_core::FOOTPRINT_PARTS {
        sum += f.bytes[i];
    }
    let sum_ok = sum + f.scalars + f.padding == f.total;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# core: link addr=0x{:x} bytes={} pin={} image_cap={} parts={} sum={} scalars={} padding={} sum_ok={} in_image={}",
            report.addr,
            report.static_bytes,
            report.pin,
            core_bridge::IMAGE_CAP_BYTES,
            vifast_core::FOOTPRINT_PARTS,
            f.total,
            f.scalars,
            f.padding,
            sum_ok as u8,
            report.in_image as u8
        );
    }
    let ok = sum_ok
        && f.padding <= 8
        && report.static_bytes == f.total
        && report.static_bytes <= report.pin
        && report.in_image;
    if ok {
        PLATFORM.put_str("ok 43 - core link\n");
    } else {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "not ok 43 - core link: sum_ok={} padding={} static={} pin={} in_image={}",
            sum_ok as u8, f.padding, report.static_bytes, report.pin, report.in_image as u8
        );
        *fails += 1;
    }
}

/// TAP **44..52** — toàn bộ chạy trong **một** lượt giữ khoá `Machine` (không re-entrant).
fn core_taps(
    report: &core_bridge::CoreReport,
    obj: &vifast_plat_riscv64::obj::ObjReport,
    fails: &mut usize,
) {
    core_bridge::with_machine(|m| {
        let u = report.untyped;
        // ------------------------------------------------------------ TAP 44: Untyped boot
        let size = m.untyped_size(u);
        let used = m.untyped_used(u);
        let free = m.untyped_free(u);
        let allocated = m.untyped_allocated(u);
        let gap = m.untyped_gap(u);
        let check_bytes = m.check_bytes(u).is_ok();
        let check_all = m.check_all_bytes().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: untyped id={} base=0x{:x} bytes={} used={} free={} allocated={} gap={} objects={} caps={} check_bytes={} check_all_bytes={}",
                u,
                report.base,
                size,
                used,
                free,
                allocated,
                gap,
                m.object_count(),
                m.live_cap_count(),
                check_bytes as u8,
                check_all as u8
            );
        }
        let ok44 = size == core_bridge::CORE_ARENA_BYTES
            && used == 0
            && free == size
            && allocated == 0
            && gap == 0
            && check_bytes
            && check_all;
        if ok44 {
            PLATFORM.put_str("ok 44 - core untyped boot\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(w, "not ok 44 - core untyped boot: byte/check khong khop");
            *fails += 1;
        }

        let rig = build_rig(m, u);

        // ------------------------------------------------------------ TAP 45: Retype TCB
        let t0_state = m.tcb_state(rig.t0).unwrap_or(TcbState::Dead);
        let used45 = m.untyped_used(u);
        let caps45 = m.live_cap_count();
        let objects45 = m.object_count();
        let ok45 = t0_state == TcbState::Inactive
            && used45 > 0
            && caps45 == 11
            && m.check_caps().is_ok()
            && m.check_bytes(u).is_ok()
            && m.check_all_bytes().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: tcb retype tcb={} slot=1 objects={} caps={} used={} state={} check_caps={} check_bytes={}",
                rig.t0,
                objects45,
                caps45,
                used45,
                state_name(t0_state),
                m.check_caps().is_ok() as u8,
                m.check_bytes(u).is_ok() as u8
            );
        }
        if ok45 {
            PLATFORM.put_str("ok 45 - core tcb retype\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 45 - core tcb retype: state={} caps={} used={}",
                state_name(t0_state),
                caps45,
                used45
            );
            *fails += 1;
        }

        // ------------------------------------- TAP 46: Configure + SetSchedParams + Resume/Suspend
        let mut ok46 = m.tcb_configure(rig.t0s, Some(rig.cspace)).is_ok();
        ok46 &= m.sched_control_configure(rig.ctl, rig.sc, 4, 8).is_ok();
        ok46 &= m.set_sched_params(rig.t0s, Some(rig.sc)).is_ok();
        ok46 &= m.tcb_set_priority(rig.t0s, 3, 10).is_ok();
        ok46 &= m.transition_tcb(rig.t0, TcbState::Ready).is_ok();
        ok46 &= m.tcb_suspend(rig.t0s, SuspendMode::Abort).is_ok();
        let after_suspend = m.tcb_read_state(rig.t0s).ok();
        let suspended_ok = after_suspend.map(|s| s.suspended).unwrap_or(false);
        ok46 &= suspended_ok;
        ok46 &= m.tcb_resume(rig.t0s).is_ok();
        let info = m.tcb_info(rig.t0s).ok();
        let sc = m.sched_info(rig.sc).ok();
        let state46 = info.map(|i| i.state).unwrap_or(TcbState::Dead);
        let ok46_fields = info
            .map(|i| {
                !i.suspended
                    && i.state == TcbState::Ready
                    && i.priority == 3
                    && i.max_controlled_priority == 10
                    && i.cspace == Some(rig.root)
                    && i.sched_context == Some(rig.sc_obj)
            })
            .unwrap_or(false)
            && sc
                .map(|s| s.configured && s.budget == 4 && s.period == 8)
                .unwrap_or(false);
        ok46 &= ok46_fields
            && m.check_sched().is_ok()
            && m.check_sc_refs().is_ok()
            && m.check_budget().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: configure cspace=1 sc=1 priority=3 mcp=10 budget=4 period=8 suspended_ok={} resume=1 state={} check_sched={} check_sc_refs={} check_budget={}",
                suspended_ok as u8,
                state_name(state46),
                m.check_sched().is_ok() as u8,
                m.check_sc_refs().is_ok() as u8,
                m.check_budget().is_ok() as u8
            );
        }
        if ok46 {
            PLATFORM.put_str("ok 46 - core configure resume suspend\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 46 - core configure resume suspend: state={} fields={}",
                state_name(state46),
                ok46_fields as u8
            );
            *fails += 1;
        }

        // -------------------------------------------------- TAP 47: schedule 2 TCB + suspend
        // Cả hai TCB phải có SchedContext đã cấu hình — `tcb_has_budget` (KT-20) chỉ cho TCB
        // có ngân sách vào `pick`; đây là biên của mô hình, không phải mẹo test.
        let mut ok47 = m.sched_control_configure(rig.ctl, rig.sc1, 4, 8).is_ok();
        ok47 &= m.set_sched_params(rig.t1s, Some(rig.sc1)).is_ok();
        ok47 &= m.tcb_set_priority(rig.t1s, 2, 10).is_ok();
        ok47 &= m.transition_tcb(rig.t1, TcbState::Ready).is_ok();
        let pick0 = m.sched_pick_on(0);
        let d0 = m.dispatch(0).ok().flatten();
        ok47 &= pick0 == Some(rig.t0) && d0 == Some(rig.t0);
        ok47 &= m.tcb_state(rig.t0) == Ok(TcbState::Running { hart: 0 });
        ok47 &= m.tcb_state(rig.t1) == Ok(TcbState::Ready);
        ok47 &= m.tcb_suspend(rig.t0s, SuspendMode::Abort).is_ok();
        let pick1 = m.sched_pick_on(0);
        let d1 = m.dispatch(0).ok().flatten();
        ok47 &= pick1 == Some(rig.t1) && d1 == Some(rig.t1);
        ok47 &= m.tcb_state(rig.t1) == Ok(TcbState::Running { hart: 0 });
        // TCB 2 chạy bằng **SC riêng** đã cấu hình (không mượn SC của TCB 1).
        let info1 = m.tcb_info(rig.t1s).ok();
        ok47 &= info1
            .map(|i| i.sched_context == Some(rig.sc1_obj))
            .unwrap_or(false);
        ok47 &= m.tcb_resume(rig.t0s).is_ok();
        let info0 = m.tcb_info(rig.t0s).ok();
        let st0 = info0.map(|i| i.state).unwrap_or(TcbState::Dead);
        let sus0 = info0.map(|i| i.suspended).unwrap_or(true);
        let st1 = m.tcb_state(rig.t1).unwrap_or(TcbState::Dead);
        ok47 &= st0 == TcbState::Ready && !sus0 && st1 == TcbState::Running { hart: 0 };
        ok47 &= m.check_sched().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: sched harts=1 pick0={} pick1={} dispatch0={} dispatch1={} t0={} t1={} suspended={} check_sched={}",
                pick0.unwrap_or(0),
                pick1.unwrap_or(0),
                d0.unwrap_or(0),
                d1.unwrap_or(0),
                state_name(st0),
                state_name(st1),
                sus0 as u8,
                m.check_sched().is_ok() as u8
            );
        }
        if ok47 {
            PLATFORM.put_str("ok 47 - core sched 2 tcb\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 47 - core sched 2 tcb: pick={:?}/{} dispatch={:?}/{} t0={} t1={}",
                pick0,
                rig.t0,
                d0,
                rig.t1,
                state_name(st0),
                state_name(st1)
            );
            *fails += 1;
        }

        // ------------------------------------------------------------ TAP 48: IPC một vòng
        let mut ok48 = m.call(rig.t0, rig.eps, IPC_MR).is_ok();
        let st_send = m.tcb_state(rig.t0).unwrap_or(TcbState::Dead);
        let blocked_send = matches!(
            st_send,
            TcbState::BlockedOnSend {
                ep,
                call: true,
                ..
            } if ep == rig.ep
        );
        ok48 &= blocked_send;
        ok48 &= m.recv(rig.t1, rig.eps, Some(rig.rs)).is_ok();
        let st_reply = m.tcb_state(rig.t0).unwrap_or(TcbState::Dead);
        let blocked_reply = st_reply == TcbState::BlockedOnReply { reply: rig.reply };
        ok48 &= blocked_reply;
        let mr = m.endpoint_last_mr(rig.ep).ok().flatten();
        ok48 &= mr == Some(IPC_MR);
        ok48 &= m.reply(rig.t1, rig.rs).is_ok();
        let st_after = m.tcb_state(rig.t0).unwrap_or(TcbState::Dead);
        let caller = m.reply_caller(rig.reply).ok().flatten();
        let ep_len = m.endpoint_send_len(rig.ep).unwrap_or(usize::MAX);
        let sent = m.ipc_sent();
        let delivered = m.ipc_delivered();
        ok48 &= st_after == TcbState::Ready && caller.is_none() && ep_len == 0;
        ok48 &= sent >= 1 && delivered >= 1;
        ok48 &= m.check_ipc().is_ok() && m.check_reply_links().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: ipc call=1 recv=1 reply=1 mr=0x{:x} blocked_send={} blocked_reply={} t0={} ep_len={} reply_caller={} sent={} delivered={} check_ipc={} check_reply_links={}",
                mr.unwrap_or(0),
                blocked_send as u8,
                blocked_reply as u8,
                state_name(st_after),
                ep_len,
                caller.map(|_| 1).unwrap_or(0),
                sent,
                delivered,
                m.check_ipc().is_ok() as u8,
                m.check_reply_links().is_ok() as u8
            );
        }
        if ok48 {
            PLATFORM.put_str("ok 48 - core ipc round\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 48 - core ipc round: send={} reply={} mr={:?} state={} ep_len={}",
                blocked_send as u8,
                blocked_reply as u8,
                mr,
                state_name(st_after),
                ep_len
            );
            *fails += 1;
        }

        // ------------------------------------------------------ TAP 49: Notification signal
        let mut ok49 = m.tcb_bind_notification(rig.t2s, rig.ns).is_ok();
        ok49 &= m.notification_wait(rig.t2, rig.ntfn).is_ok();
        ok49 &= m.tcb_state(rig.t2) == Ok(TcbState::BlockedOnNotification { ntfn: rig.ntfn });
        ok49 &= m.notification_signal(rig.ntfn, 0x42).is_ok();
        ok49 &= m.tcb_state(rig.t2) == Ok(TcbState::Ready);
        let consumed = m.take_tcb_notification(rig.t2).ok().flatten();
        ok49 &= consumed
            .map(|v| v.word == 0x42 && v.badge == 0x42)
            .unwrap_or(false);
        // Signal không có waiter ⇒ tích word/badge; Poll nhận rồi xoá cả hai.
        ok49 &= m.notification_signal(rig.ntfn, 0x8).is_ok();
        let polled = m.notification_poll(rig.ntfn).ok().flatten();
        let word = m.notification_word(rig.ntfn).unwrap_or(u64::MAX);
        let badge = m.notification_badge(rig.ntfn).unwrap_or(u64::MAX);
        let waiter = m.notification_waiter(rig.ntfn).ok().flatten();
        ok49 &= polled
            .map(|v| v.word == 0x8 && v.badge == 0x8)
            .unwrap_or(false);
        ok49 &= word == 0 && badge == 0 && waiter.is_none();
        ok49 &= m.check_notifications().is_ok() && m.check_ipc().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: ntfn signal=1 wake=1 word=0x{:x} badge=0x{:x} waiter={} consumed=0x{:x} poll=0x{:x} state={} check_notifications={}",
                word,
                badge,
                waiter.is_some() as u8,
                consumed.map(|v| v.word).unwrap_or(0),
                polled.map(|v| v.word).unwrap_or(0),
                state_name(m.tcb_state(rig.t2).unwrap_or(TcbState::Dead)),
                m.check_notifications().is_ok() as u8
            );
        }
        if ok49 {
            PLATFORM.put_str("ok 49 - core notification signal\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 49 - core notification signal: word=0x{:x} badge=0x{:x} consumed={:?}",
                word,
                badge,
                consumed.map(|v| v.word)
            );
            *fails += 1;
        }

        // ------------------------------------------------------------ TAP 50: WaitAddr
        // D2-83: Frame **thuộc object layer**; core chỉ nhận **nhãn** (PPN) làm khoá futex —
        // một chiều, không đọc byte/PTE của Frame. Đây là cầu nối duy nhất cho WaitAddr.
        let om = aobj::machine();
        let ofr = om
            .retype(obj.untyped0, ObjKind::Frame, 12)
            .expect("core TAP 50: obj layer Frame");
        let frame_pa = om.frame_pa(ofr).unwrap_or(0);
        let label = (frame_pa >> 12) as u32;
        let mut ok50 = frame_pa != 0;
        ok50 &= m.wait_addr(rig.t2s, label, WA_OFFSET, 0).is_ok();
        ok50 &= m.tcb_state(rig.t2)
            == Ok(TcbState::BlockedOnAddress {
                frame: label,
                offset: WA_OFFSET,
            });
        ok50 &= m.set_addr_word(label, WA_OFFSET, WA_VALUE).is_ok();
        let woke = m.wake_addr(label, WA_OFFSET, 1).unwrap_or(u32::MAX);
        ok50 &= woke == 1;
        let value = m.addr_word(label, WA_OFFSET);
        let waiter = m.address_waiter(label, WA_OFFSET);
        let buckets = m.waitaddr_nonempty_buckets();
        let st50 = m.tcb_state(rig.t2).unwrap_or(TcbState::Dead);
        ok50 &= value == WA_VALUE && waiter.is_none() && buckets == 0;
        ok50 &= st50 == TcbState::Ready;
        ok50 &= m.check_waitaddr().is_ok() && m.check_sched().is_ok();
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: waitaddr frame=0x{:x} label={} offset={} value=0x{:x} woke={} waiters={} buckets={} state={} check_waitaddr={}",
                frame_pa,
                label,
                WA_OFFSET,
                value,
                woke,
                waiter.is_some() as u8,
                buckets,
                state_name(st50),
                m.check_waitaddr().is_ok() as u8
            );
        }
        let _ = om.revoke(ofr);
        if ok50 {
            PLATFORM.put_str("ok 50 - core waitaddr\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 50 - core waitaddr: woke={} value=0x{:x} buckets={} state={}",
                woke,
                value,
                buckets,
                state_name(st50)
            );
            *fails += 1;
        }

        // ----------------------------------------------------- TAP 51: biên giới D2-83
        let used_before = m.untyped_used(u);
        let f51 = m.retype(rig.us, ObjType::Frame, 0, 0);
        let pt51 = m.retype(rig.us, ObjType::PageTable, 0, 0);
        let vs51 = m.retype(rig.us, ObjType::VSpace, 0, 0);
        let used_after = m.untyped_used(u);
        let core_not_supported = [f51, pt51, vs51]
            .iter()
            .all(|r| matches!(r, Err(KError::NotSupported)));
        // Cùng lúc: object layer (chủ sở hữu VM) vẫn tạo/revoke VSpace + Frame **thật**.
        let obj = aobj::init();
        let om = aobj::machine();
        let ovs = om.retype(obj.untyped0, ObjKind::VSpace, 0);
        let ofr = om.retype(obj.untyped0, ObjKind::Frame, 12);
        let obj_ok = ovs.is_ok() && ofr.is_ok() && om.check_objmem().is_ok();
        let obj_counts = om.counts();
        if let Ok(cap) = ofr {
            let _ = om.revoke(cap);
        }
        if let Ok(cap) = ovs {
            let _ = om.revoke(cap);
        }
        let obj_clean = om.check_objmem().is_ok();
        let ok51 = core_not_supported && used_after == used_before && obj_ok && obj_clean;
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: d2-83 frame={} page_table={} vspace={} atomic={} untyped_used={} obj_frame={} obj_vspace={} obj_health={} owner=obj",
                err_name(&f51),
                err_name(&pt51),
                err_name(&vs51),
                (used_after == used_before) as u8,
                used_after,
                obj_counts.frames,
                obj_counts.vspaces,
                obj_clean as u8
            );
        }
        if ok51 {
            PLATFORM.put_str("ok 51 - core d2-83 boundary\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 51 - core d2-83 boundary: frame={} pt={} vspace={} atomic={} obj_ok={}",
                err_name(&f51),
                err_name(&pt51),
                err_name(&vs51),
                (used_after == used_before) as u8,
                obj_ok as u8
            );
            *fails += 1;
        }

        // ---------------------------------------------------------- TAP 52: invariants
        // Đưa mọi TCB về Ready trước khi revoke (Running→Revoke sẽ làm current treo).
        let _ = m.transition_tcb(rig.t1, TcbState::Ready);
        let _ = m.delete(rig.cspace);
        let deleted = m.revoke(rig.root, Some(0)).unwrap_or(0);
        let used52 = m.untyped_used(u);
        let free52 = m.untyped_free(u);
        let gap52 = m.untyped_gap(u);
        let caps52 = m.live_cap_count();
        let inv = invariants_ok(m);
        let ok52 = used52 == 0
            && free52 == core_bridge::CORE_ARENA_BYTES
            && gap52 == 0
            && caps52 == 1
            && deleted >= 9
            && inv;
        {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "# core: invariants caps={} objects={} used={} free={} deleted={} check_caps={} check_bytes={} check_all_bytes={} check_sched={} check_sc_refs={} check_budget={} check_mailboxes={} check_ipc={} check_reply_links={} check_notifications={} check_waitaddr={} check_fault={} leak={}",
                caps52,
                m.object_count(),
                used52,
                free52,
                deleted,
                m.check_caps().is_ok() as u8,
                m.check_bytes(u).is_ok() as u8,
                m.check_all_bytes().is_ok() as u8,
                m.check_sched().is_ok() as u8,
                m.check_sc_refs().is_ok() as u8,
                m.check_budget().is_ok() as u8,
                m.check_mailboxes().is_ok() as u8,
                m.check_ipc().is_ok() as u8,
                m.check_reply_links().is_ok() as u8,
                m.check_notifications().is_ok() as u8,
                m.check_waitaddr().is_ok() as u8,
                m.check_fault().is_ok() as u8,
                (!(used52 == 0 && gap52 == 0)) as u8
            );
        }
        if ok52 {
            PLATFORM.put_str("ok 52 - core invariants\n");
        } else {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 52 - core invariants: used={} free={} gap={} caps={} deleted={} inv={}",
                used52, free52, gap52, caps52, deleted, inv as u8
            );
            *fails += 1;
        }
    });
}

/// Tên mã lỗi hoặc `ok` cho evidence của TAP 51.
fn err_name<T>(r: &Result<T, KError>) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(e) => e.name(),
    }
}

/// Pha core: TAP **43..52**; trả số TAP đỏ.
pub fn core_phase() -> usize {
    let mut fails = 0usize;
    // D2-83: object layer **vẫn** là chủ sở hữu Frame/PageTable/VSpace; `init` cho report +
    // cap Untyped để TAP 51 chứng minh VM sống ở đó, không nhân đôi chủ sở hữu.
    let obj = aobj::init();
    let report = core_bridge::init(obj.arena_end, core_bridge::CORE_ARENA_BYTES);
    tap43_link(&report, &mut fails);
    core_taps(&report, &obj, &mut fails);
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# core: phase fails={} static={} pin={} image_cap={}",
            fails,
            report.static_bytes,
            report.pin,
            core_bridge::IMAGE_CAP_BYTES
        );
    }
    fails
}
