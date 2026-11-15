// SPDX-License-Identifier: MIT OR Apache-2.0
//! `Machine` — model core K3b: Untyped + CNode + MDB + capability (KT-58) **và** TCB +
//! transition + scheduler + hộp thư (KT-59).
//!
//! Port **nguyên ngữ nghĩa** từ `v2/sim/vifast-sim/src/kobj.rs` (oracle): cùng id tuần tự
//! không tái dùng, cùng thứ tự cấp phát, cùng mã lỗi, cùng bất biến `check_caps`/`check_bytes`/
//! `check_sched`/`check_mailboxes`. Khác biệt **duy nhất** so với sim (ghi trong
//! `docs/k3b-core.md`):
//!
//! 1. Bảng tĩnh `.bss` thay `Vec` (không `alloc`); đầy bảng ⇒ `NotEnoughMemory` (harness
//!    `add_*` panic như sim khi arena kernel đầy).
//! 2. Chưa có body Endpoint/Notification/Reply/IPC (KT-60): object vẫn có `ObjType`, `alive`,
//!    `untyped/offset/len`, cap/MDB đầy đủ; TCB/SchedContext **đã có body** (KT-59).
//! 3. Không `Retype` `Frame`/`PageTable`/`VSpace` (K2 object layer `vifast-plat-common::obj`,
//!    KT-52) — trả `NotSupported`; sim (K2) đã mở `Frame`/`PageTable`, xem `docs/k3b-core.md`.
//! 4. **Donation KT-21** là phần của `Call`/`Reply` (KT-60): KT-59 giữ trường
//!    `donated_sc`/`donated_to`/`donor` + công thức `donated_priority`, nhưng chưa có đường tạo
//!    donation — parity KT-59 phủ trạng thái **không donation** (ghi trong docs §7).
//! 5. **Reply/Notification/IPC** chưa có body ⇒ `unlink_tcb_queues` chỉ là mốc (không có hàng
//!    chờ nào của KT-24..28 để gỡ); `TlbFlush` trong hộp thư là no-op (core chưa có TLB).

use crate::arena::Region;
use crate::error::{CapId, CapInfo, KError, ObjId, ObjType, SlotRef};
use crate::mailbox::HartMailbox;
use crate::mcs::ScBudget;
use crate::sched::{HartSched, Published};
use crate::slotpool::SlotPool;
use crate::spec::{
    ARENA_ALIGN, CNODE_BASE_BYTES, DEFAULT_ALIGN_BYTES, FLAG_GUEST, RIGHTS_ALL, RIGHT_WRITE,
};
use crate::tcb::{TcbInfo, TcbState, TCB_STATE_COUNT};

/// Số object tối đa trong bảng tĩnh (id **không tái dùng**, kể cả object đã chết).
pub const MAX_OBJECTS: usize = 256;
/// Số cap tối đa trong bảng tĩnh (id **không tái dùng**, kể cả cap đã chết).
pub const MAX_CAPS: usize = 512;
/// Số Untyped tối đa (mỗi Untyped một arena).
pub const MAX_UNTYPEDS: usize = 16;
/// Số word slot CNode tối đa (1 word = 1 cap id; 48 B slot của spec vẫn được kế toán byte).
pub const MAX_SLOT_WORDS: usize = 4096;
/// Số **body TCB** tối đa (biên tĩnh `[TK]`; sim không biên vì `Box`).
pub const MAX_TCBS: usize = 64;
/// Số **body SchedContext** tối đa (biên tĩnh `[TK]`).
pub const MAX_SCHED_CONTEXTS: usize = 32;
/// Số hart tối đa của mô hình (`MAX_HARTS = 8`, KT-49/D2-72 Q4).
pub const MAX_HARTS: usize = 8;
/// Số dòng log finalise TCB tối đa — **bằng `MAX_OBJECTS`** (không phải `MAX_TCBS`).
///
/// `finalise_tcb` ghi **một** dòng cho mỗi TCB **đang sống** khi finalise; body tái dùng nên
/// một `Machine` có thể finalise nhiều lượt TCB (tối đa bằng số object id từng cấp —
/// `MAX_OBJECTS`). Đầy ⇒ [`Machine::finalise_tcb`] trả `NotEnoughMemory` (không cắt im lặng,
/// không panic); caller phải tái tạo `Machine` nếu cần quá `MAX_OBJECTS` finalise.
pub const MAX_FINALISE: usize = MAX_OBJECTS;

// ---------------------------------------------------------------------------------------------
// Body của object (phần trạng thái KT-59 thêm vào; `.bss`, không cấp phát)
// ---------------------------------------------------------------------------------------------

/// Loại body đã cấp cho một object — `NONE` với các loại chưa port body (Endpoint/...).
pub(crate) const BODY_NONE: u8 = 0;
/// Body TCB (`Machine::tcbs`).
pub(crate) const BODY_TCB: u8 = 1;
/// Body SchedContext (`Machine::scs`).
pub(crate) const BODY_SC: u8 = 2;

/// Tham chiếu body của object — chỉ số + cờ sống.
///
/// sim `destroy` body khi object chết; core đánh dấu `alive = false` và trả chỉ số vào
/// free-list để tái dùng. Truy cập body đã chết trả `InvalidCapability` như sim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BodyRef {
    pub(crate) kind: u8,
    pub(crate) idx: u16,
    pub(crate) alive: bool,
}

impl BodyRef {
    pub(crate) const NONE: Self = Self {
        kind: BODY_NONE,
        idx: 0,
        alive: false,
    };
}

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
    /// Notification đang bind (KT-27 — chưa có ở KT-59, luôn `None`).
    pub(crate) bound_notification: Option<ObjId>,
    /// 60 word "IPC buffer model" (KT-24 — KT-59 chỉ giữ chỗ + accessor; transfer thuộc KT-60).
    pub(crate) ipc_buffer: [u64; 60],
    /// 4 MR "thanh ghi" (KT-24 — như trên).
    pub(crate) regs: [u64; 4],
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
        ipc_buffer: [0; 60],
        regs: [0; 4],
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

/// Một dòng log finalise TCB (KT-18 DoD 9) — port `FinaliseEntry` của sim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinaliseEntry {
    /// TCB.
    pub tcb: ObjId,
    /// Trạng thái ghi log (luôn `Dead` ở sim).
    pub state: TcbState,
    /// SC được trả lại (hook finalise).
    pub released_sched_context: Option<ObjId>,
}
/// Sổ cái một object (metadata; body của object lõi **chưa** port ở tầng này).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Meta {
    pub(crate) ty: ObjType,
    pub(crate) alive: bool,
    /// Chỉ số Untyped chứa object (None với object harness cấp / Untyped gốc).
    pub(crate) untyped: Option<usize>,
    /// Offset logic trong Untyped.
    pub(crate) offset: usize,
    /// Cỡ logic của spec.
    pub(crate) len: usize,
    /// Untyped: chỉ số arena trong `Machine::regions`.
    pub(crate) region: Option<usize>,
    /// CNode: số slot trong **metadata** (đọc được cả sau khi chết; `None` với CNode
    /// `add_kernel_object` như sim `ChoDat::default`).
    pub(crate) slots_meta: Option<usize>,
    /// CNode: số slot **của body** (bảng từ).
    pub(crate) slot_len: usize,
    /// CNode: offset trong pool word.
    pub(crate) slot_base: usize,
    /// Body TCB/SchedContext (KT-59); `NONE` với loại chưa port body.
    pub(crate) body: BodyRef,
}

impl Meta {
    pub(crate) const EMPTY: Self = Self {
        ty: ObjType::Untyped,
        alive: false,
        untyped: None,
        offset: 0,
        len: 0,
        region: None,
        slots_meta: None,
        slot_len: 0,
        slot_base: 0,
        body: BodyRef::NONE,
    };
}

/// Một cap: object + quan hệ cha (MDB) + slot + quyền + badge.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cap {
    pub(crate) obj: ObjId,
    pub(crate) ty: ObjType,
    pub(crate) parent: Option<CapId>,
    pub(crate) slot: Option<SlotRef>,
    pub(crate) rights: u8,
    pub(crate) badge: Option<u64>,
    pub(crate) alive: bool,
}

impl Cap {
    pub(crate) const EMPTY: Self = Self {
        obj: 0,
        ty: ObjType::Untyped,
        parent: None,
        slot: None,
        rights: 0,
        badge: None,
        alive: false,
    };
}

/// Core model K3b — **không** `unsafe`, **không** cấp phát động.
#[derive(Debug, Clone, Copy)]
pub struct Machine {
    pub(crate) regions: [Region; MAX_UNTYPEDS],
    pub(crate) n_regions: usize,
    pub(crate) objects: [Meta; MAX_OBJECTS],
    pub(crate) n_objects: usize,
    pub(crate) caps: [Cap; MAX_CAPS],
    pub(crate) n_caps: usize,
    pub(crate) slots: SlotPool,
    /// Body TCB (KT-59) + free-list chỉ số.
    pub(crate) tcbs: [TcbData; MAX_TCBS],
    pub(crate) n_tcbs: usize,
    pub(crate) tcb_free: [u16; MAX_TCBS],
    pub(crate) n_tcb_free: usize,
    /// Body SchedContext (KT-59) + free-list chỉ số.
    pub(crate) scs: [ScData; MAX_SCHED_CONTEXTS],
    pub(crate) n_scs: usize,
    pub(crate) sc_free: [u16; MAX_SCHED_CONTEXTS],
    pub(crate) n_sc_free: usize,
    /// Bộ đếm cặp chuyển trạng thái 11×11 (KT-18 DoD 4).
    pub(crate) pair_hits: [u32; TCB_STATE_COUNT * TCB_STATE_COUNT],
    /// Bộ đếm cặp **cấm** bị thử (KT-23 DoD 3).
    pub(crate) forbidden_hits: [u32; TCB_STATE_COUNT * TCB_STATE_COUNT],
    /// Số lần `Running → Sleeping{Budget}` (KT-20/KT-23).
    pub(crate) budget_sleeps: u64,
    /// Số lần một `SchedContext` được nạp lại (KT-20/KT-23).
    pub(crate) budget_refills: u64,
    /// Log finalise TCB (KT-18 DoD 9), có biên `MAX_FINALISE`.
    pub(crate) finalise_log: [FinaliseEntry; MAX_FINALISE],
    pub(crate) n_finalise: usize,
    /// Số lần `finalise_tcb` bị từ chối vì log đầy (chẩn đoán; không phải ABI).
    pub(crate) finalise_overflow: u32,
    /// Scheduler theo hart (KT-19/KT-29).
    pub(crate) sched: [HartSched; MAX_HARTS],
    /// Số hart đang dùng (`1..=MAX_HARTS`).
    pub(crate) n_harts: usize,
    /// Hộp thư theo hart (KT-22).
    pub(crate) mail: [HartMailbox; MAX_HARTS],
    /// Trường công bố mỗi hart (KT-29).
    pub(crate) published: [Published; MAX_HARTS],
    /// Hart "đang chạy" của mô hình tại thời điểm gọi (KT-29).
    pub(crate) current_hart: u16,
    /// Số TCB đã di trú thành công (KT-29).
    pub(crate) migrate_count: u64,
    /// Preempt ép tại mọi public op (KT-19, mặc định tắt).
    pub(crate) preempt_every_step: bool,
    /// Thời gian ảo (tick) — **không** đọc wall clock (D2-49(h)).
    pub(crate) now: u64,
}

/// Số thành phần của báo cáo [`Footprint`] (14 bảng/mảng + scalar + padding).
pub const FOOTPRINT_PARTS: usize = 14;

/// Phân bố byte **tĩnh** của [`Machine`] (D2-84) — không cấp phát; dùng cho test pin và
/// KT-61 trước khi link vào ảnh.
///
/// `bytes[i]` là `size_of::<[T; N]>()` của từng bảng; `scalars` là tổng 9 `usize` + 4 `u64` +
/// 1 `u16` + 1 `u32` + 1 `bool` (các trường đếm/cờ); `padding` là byte đệm/đuôi do `repr(Rust)`:
/// `sum(bytes) + scalars + padding == total == size_of::<Machine>()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// Tên 14 thành phần (cùng thứ tự `bytes`).
    pub names: [&'static str; FOOTPRINT_PARTS],
    /// Byte của từng thành phần.
    pub bytes: [usize; FOOTPRINT_PARTS],
    /// Tổng byte của các trường vô hướng.
    pub scalars: usize,
    /// Byte đệm (giữa các trường/đuôi struct).
    pub padding: usize,
    /// `size_of::<Machine>()`.
    pub total: usize,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    /// Model rỗng **1 hart** (Untyped/CNode do harness thêm).
    pub const fn new() -> Self {
        Self::with_harts(1)
    }

    /// Model rỗng `harts` hart (`1..=MAX_HARTS`) — như sim `Machine::with_harts`.
    ///
    /// # Panics
    /// `harts == 0` hoặc `harts > MAX_HARTS` (lỗi init của harness, như sim).
    pub const fn with_harts(harts: u16) -> Self {
        assert!(
            harts > 0 && harts as usize <= MAX_HARTS,
            "vifast-core: số hart phải trong 1..=MAX_HARTS"
        );
        Self {
            regions: [Region::EMPTY; MAX_UNTYPEDS],
            n_regions: 0,
            objects: [Meta::EMPTY; MAX_OBJECTS],
            n_objects: 0,
            caps: [Cap::EMPTY; MAX_CAPS],
            n_caps: 0,
            slots: SlotPool::EMPTY,
            tcbs: [TcbData::EMPTY; MAX_TCBS],
            n_tcbs: 0,
            tcb_free: [0; MAX_TCBS],
            n_tcb_free: 0,
            scs: [ScData::EMPTY; MAX_SCHED_CONTEXTS],
            n_scs: 0,
            sc_free: [0; MAX_SCHED_CONTEXTS],
            n_sc_free: 0,
            pair_hits: [0; TCB_STATE_COUNT * TCB_STATE_COUNT],
            forbidden_hits: [0; TCB_STATE_COUNT * TCB_STATE_COUNT],
            budget_sleeps: 0,
            budget_refills: 0,
            finalise_log: [FinaliseEntry {
                tcb: 0,
                state: TcbState::Dead,
                released_sched_context: None,
            }; MAX_FINALISE],
            n_finalise: 0,
            finalise_overflow: 0,
            sched: [HartSched::new(); MAX_HARTS],
            n_harts: harts as usize,
            mail: [HartMailbox::new(); MAX_HARTS],
            published: [Published {
                idle: true,
                cur_prio: 0,
            }; MAX_HARTS],
            current_hart: 0,
            migrate_count: 0,
            preempt_every_step: false,
            now: 0,
        }
    }

    // ------------------------------------------------------- dấu chân tĩnh (D2-84)

    /// Báo cáo phân bố byte tĩnh của [`Machine`] (D2-84).
    pub fn footprint() -> Footprint {
        use core::mem::size_of;
        let bytes = [
            size_of::<[Region; MAX_UNTYPEDS]>(),
            size_of::<[Meta; MAX_OBJECTS]>(),
            size_of::<[Cap; MAX_CAPS]>(),
            size_of::<SlotPool>(),
            size_of::<[TcbData; MAX_TCBS]>(),
            size_of::<[u16; MAX_TCBS]>(),
            size_of::<[ScData; MAX_SCHED_CONTEXTS]>(),
            size_of::<[u16; MAX_SCHED_CONTEXTS]>(),
            size_of::<[u32; TCB_STATE_COUNT * TCB_STATE_COUNT]>(),
            size_of::<[u32; TCB_STATE_COUNT * TCB_STATE_COUNT]>(),
            size_of::<[FinaliseEntry; MAX_FINALISE]>(),
            size_of::<[HartSched; MAX_HARTS]>(),
            size_of::<[HartMailbox; MAX_HARTS]>(),
            size_of::<[Published; MAX_HARTS]>(),
        ];
        // 9 × usize (n_regions/n_objects/n_caps/n_tcbs/n_tcb_free/n_scs/n_sc_free/
        // n_finalise/n_harts) + 4 × u64 (budget_sleeps/budget_refills/migrate_count/now)
        // + 1 × u16 (current_hart) + 1 × bool (preempt_every_step).
        let scalars = 9 * size_of::<usize>()
            + 4 * size_of::<u64>()
            + size_of::<u16>()
            + size_of::<u32>()
            + size_of::<bool>();
        let total = size_of::<Machine>();
        let used: usize = bytes.iter().sum::<usize>() + scalars;
        Footprint {
            names: [
                "regions[16]",
                "objects[256]",
                "caps[512]",
                "slot_pool",
                "tcbs[64]",
                "tcb_free[64]",
                "scs[32]",
                "sc_free[32]",
                "pair_hits[121]",
                "forbidden_hits[121]",
                "finalise_log[256]",
                "sched[8]",
                "mail[8]",
                "published[8]",
            ],
            bytes,
            scalars,
            padding: total.saturating_sub(used),
            total,
        }
    }

    // ------------------------------------------------------------- cấp phát

    /// Thêm Untyped **do harness cấp** (đáy `base`, `size` byte arena).
    ///
    /// Sim `add_untyped` (KT-16): arena do objmem sở hữu; ở đây arena là `[base, base+size)`
    /// **logic** — `untyped_arena()` trả đúng `(base, size)`, watermark bắt đầu 0.
    ///
    /// # Panics
    /// `size == 0`, hoặc hết bảng `MAX_UNTYPEDS`/`MAX_OBJECTS` (lỗi init của harness; sim
    /// cũng panic khi arena kernel/Untyped hết chỗ).
    pub fn add_untyped(&mut self, base: u64, size: usize) -> ObjId {
        assert!(size > 0, "Untyped phải có cỡ dương");
        assert!(
            self.n_regions < MAX_UNTYPEDS,
            "vifast-core: hết MAX_UNTYPEDS ({MAX_UNTYPEDS})"
        );
        let region = self.n_regions;
        self.regions[region] = Region::new(base, size);
        self.n_regions += 1;
        self.push_meta(
            ObjType::Untyped,
            None,
            0,
            0,
            Some(region),
            None,
            0,
            0,
            BodyRef::NONE,
        )
    }

    /// Thêm CNode **do harness cấp** (init giữ CNode gốc) — như sim `add_cnode`.
    ///
    /// # Panics
    /// `slots == 0` hoặc pool slot/bảng đầy (lỗi init).
    pub fn add_cnode(&mut self, slots: usize) -> ObjId {
        assert!(slots > 0, "CNode phải có ít nhất một slot");
        let base = self
            .slots
            .alloc(slots)
            .expect("vifast-core: pool slot hết chỗ (tăng MAX_SLOT_WORDS)");
        self.push_meta(
            ObjType::CNode,
            None,
            0,
            0,
            None,
            Some(slots),
            base,
            slots,
            BodyRef::NONE,
        )
    }

    /// Thêm object lõi **do harness cấp** (SchedControl, TCB khởi tạo, ...).
    ///
    /// # Panics
    /// `ty` không phải object lõi, hoặc bảng đầy.
    pub fn add_kernel_object(&mut self, ty: ObjType) -> ObjId {
        assert!(ty.is_core(), "chỉ object lõi mới dựng trực tiếp");
        // Sim: `fresh_kind(CNode, 0)` cho body 1 slot dù metadata `slots = None`; giữ đúng
        // hành vi đó để `cnode_slot_count` của CNode harness-cấp là 1 (không dùng trong test).
        let (slot_base, slot_len, slots_meta) = if ty == ObjType::CNode {
            let base = self
                .slots
                .alloc(1)
                .expect("vifast-core: pool slot hết chỗ (CNode harness)");
            (base, 1, None)
        } else {
            (0, 0, None)
        };
        let body = match ty {
            ObjType::Tcb => self
                .alloc_tcb_body()
                .expect("vifast-core: hết MAX_TCBS (harness)"),
            ObjType::SchedContext => self
                .alloc_sc_body()
                .expect("vifast-core: hết MAX_SCHED_CONTEXTS (harness)"),
            _ => BodyRef::NONE,
        };
        self.push_meta(ty, None, 0, 0, None, slots_meta, slot_base, slot_len, body)
    }

    /// Cap gốc cho một object (không cha) — harness đặt vào CNode bằng [`Machine::put_cap`].
    ///
    /// # Panics
    /// Object không tồn tại hoặc bảng cap đầy.
    pub fn root_cap(&mut self, obj: ObjId) -> CapId {
        let ty = self
            .object_type(obj)
            .expect("root_cap: object phải tồn tại");
        self.push_cap(obj, ty, None, RIGHTS_ALL, None)
    }

    /// Đặt cap (chưa nằm trong slot nào) vào slot của CNode.
    pub fn put_cap(&mut self, cnode: ObjId, slot: usize, cap: CapId) -> Result<(), KError> {
        self.check_cnode_slot(cnode, slot)?;
        if self.cap(cap)?.slot.is_some() {
            return Err(KError::IllegalOperation);
        }
        if self.cnode_slot(cnode, slot)?.is_some() {
            return Err(KError::SlotOccupied);
        }
        self.cnode_set_slot(cnode, slot, Some(cap))?;
        if let Some(c) = self.caps.get_mut(cap as usize - 1) {
            c.slot = Some(SlotRef::new(cnode, slot));
        }
        Ok(())
    }

    // ------------------------------------------------------------- truy vấn

    /// Cap trong slot; slot rỗng/ngoài dải/cap chết trả đúng mã của sim.
    pub fn cap_at(&self, cnode: ObjId, slot: usize) -> Result<CapId, KError> {
        let n = self.cnode_slot_count(cnode)?;
        if slot >= n {
            return Err(KError::RangeError);
        }
        match self.cnode_slot(cnode, slot)? {
            None => Err(KError::SlotEmpty),
            Some(cap) => {
                if self.cap(cap)?.alive {
                    Ok(cap)
                } else {
                    Err(KError::InvalidCapability)
                }
            }
        }
    }

    /// Object của cap (cap phải sống).
    pub fn cap_obj(&self, cap: CapId) -> Result<ObjId, KError> {
        let c = self.cap(cap)?;
        if c.alive {
            Ok(c.obj)
        } else {
            Err(KError::InvalidCapability)
        }
    }

    /// Object của cap trong slot.
    pub fn cap_obj_at(&self, cnode: ObjId, slot: usize) -> Result<ObjId, KError> {
        self.cap_obj(self.cap_at(cnode, slot)?)
    }

    /// Ảnh chụp cap; cap **đã chết** vẫn `Ok` với `alive = false` (như sim).
    pub fn cap_info(&self, cap: CapId) -> Result<CapInfo, KError> {
        let c = self.cap(cap)?;
        Ok(CapInfo {
            cap,
            obj: c.obj,
            ty: c.ty,
            parent: c.parent,
            slot: c.slot,
            rights: c.rights,
            badge: c.badge,
            alive: c.alive,
        })
    }

    /// Duyệt toàn bộ bảng cap theo thứ tự cấp phát (gồm cap đã chết).
    pub fn cap_table(&self) -> CapTable<'_> {
        CapTable {
            caps: &self.caps[..self.n_caps],
            idx: 0,
        }
    }

    /// Số cap còn sống.
    pub fn live_cap_count(&self) -> usize {
        self.caps[..self.n_caps].iter().filter(|c| c.alive).count()
    }

    /// Con trực tiếp còn sống của `cap`, theo thứ tự cấp phát (thứ tự MDB của mô hình).
    pub fn cap_children(&self, cap: CapId) -> Result<CapChildren<'_>, KError> {
        self.cap(cap)?;
        Ok(CapChildren {
            caps: &self.caps[..self.n_caps],
            parent: cap,
            idx: 0,
        })
    }

    /// Số slot của CNode (đọc được cả sau khi CNode chết — như `Meta::slots` của sim).
    pub fn cnode_slot_count(&self, cnode: ObjId) -> Result<usize, KError> {
        let m = self.meta(cnode)?;
        if !m.alive {
            return m.slots_meta.ok_or(KError::WrongObjectType);
        }
        if m.ty != ObjType::CNode {
            return Err(KError::WrongObjectType);
        }
        Ok(m.slot_len)
    }

    /// Object còn sống?
    pub fn object_alive(&self, obj: ObjId) -> bool {
        if obj == 0 || obj as usize > self.n_objects {
            return false;
        }
        self.objects[obj as usize - 1].alive
    }

    /// Loại object (kể cả object đã chết — như sim).
    pub fn object_type(&self, obj: ObjId) -> Option<ObjType> {
        if obj == 0 || obj as usize > self.n_objects {
            return None;
        }
        Some(self.objects[obj as usize - 1].ty)
    }

    /// `(offset, len)` trong Untyped; `None` với object harness cấp (như sim `object_bytes`).
    pub fn object_bytes(&self, obj: ObjId) -> Option<(usize, usize)> {
        if obj == 0 || obj as usize > self.n_objects {
            return None;
        }
        let m = &self.objects[obj as usize - 1];
        m.untyped.map(|_| (m.offset, m.len))
    }

    /// Số object đã từng cấp (kể cả đã chết; id = 1..=số này).
    pub fn object_count(&self) -> usize {
        self.n_objects
    }

    /// Số cap đã từng cấp (kể cả đã chết; id = 1..=số này).
    pub fn cap_count(&self) -> usize {
        self.n_caps
    }

    /// Untyped: cỡ byte arena (0 nếu không phải Untyped).
    pub fn untyped_size(&self, obj: ObjId) -> usize {
        if obj == 0 || obj as usize > self.n_objects {
            return 0;
        }
        let m = &self.objects[obj as usize - 1];
        if m.ty != ObjType::Untyped {
            return 0;
        }
        m.region.map(|r| self.regions[r].capacity).unwrap_or(0)
    }

    /// Untyped: watermark byte đã cấp (0 nếu không phải Untyped).
    pub fn untyped_used(&self, obj: ObjId) -> usize {
        if obj == 0 || obj as usize > self.n_objects {
            return 0;
        }
        let m = &self.objects[obj as usize - 1];
        if m.ty != ObjType::Untyped {
            return 0;
        }
        m.region.map(|r| self.regions[r].cursor).unwrap_or(0)
    }

    /// Untyped: byte còn tự do.
    pub fn untyped_free(&self, obj: ObjId) -> usize {
        self.untyped_size(obj)
            .saturating_sub(self.untyped_used(obj))
    }

    /// Tổng `len` của object **sống** cấp từ Untyped (không gồm byte đệm căn lề).
    pub fn untyped_allocated(&self, obj: ObjId) -> usize {
        if obj == 0 || obj as usize > self.n_objects {
            return 0;
        }
        let idx = obj as usize - 1;
        self.objects[..self.n_objects]
            .iter()
            .filter(|m| m.alive && m.untyped == Some(idx))
            .map(|m| m.len)
            .sum()
    }

    /// Byte đệm căn lề nằm giữa watermark và tổng `len` object sống.
    pub fn untyped_gap(&self, obj: ObjId) -> usize {
        self.untyped_used(obj)
            .saturating_sub(self.untyped_allocated(obj))
    }

    /// Tổng watermark của mọi Untyped còn sống.
    pub fn total_used(&self) -> usize {
        self.objects[..self.n_objects]
            .iter()
            .filter(|m| m.alive && m.ty == ObjType::Untyped)
            .map(|m| m.region.map(|r| self.regions[r].cursor).unwrap_or(0))
            .sum()
    }

    /// `(base, capacity)` arena của Untyped (kể cả đã chết — như sim).
    pub fn untyped_arena(&self, obj: ObjId) -> Option<(u64, usize)> {
        if obj == 0 || obj as usize > self.n_objects {
            return None;
        }
        let m = &self.objects[obj as usize - 1];
        if m.ty != ObjType::Untyped {
            return None;
        }
        let r = m.region?;
        Some((self.regions[r].base, self.regions[r].capacity))
    }

    // ------------------------------------------------------------- Retype

    /// Cấp object từ Untyped theo `type/size_bits/flags` (spec `Untyped.Retype`).
    ///
    /// Trả `CapId` cho harness (ABI `returns = []`); cap là con của cap nguồn, **kế thừa**
    /// quyền cap nguồn. Mọi lỗi **nguyên tử**: không đốt byte/cap/object.
    pub fn retype(
        &mut self,
        src: SlotRef,
        ty: ObjType,
        size_bits: u32,
        flags: u32,
    ) -> Result<CapId, KError> {
        let src_cap = self.cap_at(src.cnode, src.slot)?;
        let (u_idx, u_region, u_size, src_rights) = {
            let c = self.cap(src_cap)?;
            if !c.alive {
                return Err(KError::InvalidCapability);
            }
            if c.ty != ObjType::Untyped {
                return Err(KError::WrongObjectType);
            }
            let idx = self.object_index(c.obj)?;
            let m = &self.objects[idx];
            if m.ty != ObjType::Untyped {
                return Err(KError::WrongObjectType);
            }
            let region = m.region.ok_or(KError::WrongObjectType)?;
            (idx, region, self.regions[region].capacity, c.rights)
        };
        if src_rights & RIGHT_WRITE == 0 {
            return Err(KError::InsufficientRights);
        }
        if flags & FLAG_GUEST != 0 {
            return Err(if ty == ObjType::VSpace {
                KError::NotSupported
            } else {
                KError::InvalidArgument
            });
        }
        if !ty.is_core() {
            return Err(KError::NotSupported);
        }

        let (len, align, child_slots) = match ty {
            ObjType::Untyped => {
                if !(12..=30).contains(&size_bits) {
                    return Err(KError::InvalidArgument);
                }
                let len = 1usize << size_bits;
                if len > u_size {
                    return Err(KError::NotEnoughMemory);
                }
                // KT-17: arena của sim chỉ bảo đảm căn `ARENA_ALIGN` (4096 B).
                if len > ARENA_ALIGN {
                    return Err(KError::Alignment);
                }
                (len, len, 0)
            }
            ObjType::CNode => {
                if !(4..=20).contains(&size_bits) {
                    return Err(KError::InvalidArgument);
                }
                let slots = 1usize << size_bits;
                (ObjType::cnode_bytes(slots), CNODE_BASE_BYTES, slots)
            }
            _ => {
                let fixed = ty.fixed_size().ok_or(KError::InvalidArgument)?;
                if size_bits != 0 {
                    if size_bits > 30 {
                        return Err(KError::InvalidArgument);
                    }
                    let requested = 1usize << size_bits;
                    if requested < fixed {
                        return Err(KError::Alignment);
                    }
                    if requested > fixed {
                        return Err(KError::InvalidArgument);
                    }
                }
                (fixed, DEFAULT_ALIGN_BYTES, 0)
            }
        };

        // Hạn mức bảng tĩnh (khác sim chỉ có hạn arena): kiểm **trước** mọi thay đổi để
        // `NotEnoughMemory` nguyên tử.
        let need_region = ty == ObjType::Untyped;
        if self.n_objects >= MAX_OBJECTS
            || self.n_caps >= MAX_CAPS
            || (need_region && self.n_regions >= MAX_UNTYPEDS)
            || (ty == ObjType::Tcb && !self.tcb_body_available())
            || (ty == ObjType::SchedContext && !self.sc_body_available())
        {
            return Err(KError::NotEnoughMemory);
        }
        if ty == ObjType::CNode && !self.slots.can_alloc(child_slots) {
            return Err(KError::NotEnoughMemory);
        }

        let offset = self.regions[u_region]
            .alloc(len, align)
            .ok_or(KError::NotEnoughMemory)?;
        let region = if ty == ObjType::Untyped {
            let base = self.regions[u_region]
                .base
                .checked_add(offset as u64)
                .ok_or(KError::InvalidArgument)?;
            let idx = self.n_regions;
            self.regions[idx] = Region::new(base, len);
            self.n_regions += 1;
            Some(idx)
        } else {
            None
        };
        let (slot_base, slot_len) = if ty == ObjType::CNode {
            let base = self.slots.alloc(child_slots).ok_or_else(|| {
                debug_assert!(false, "Retype: can_alloc đã xác nhận trước đó");
                KError::NotEnoughMemory
            })?;
            (base, child_slots)
        } else {
            (0, 0)
        };
        // Đã kiểm sức chứa body ở trên ⇒ chỉ có thể thành công; lỗi nội bộ là bất biến vỡ.
        let body = match ty {
            ObjType::Tcb => self.alloc_tcb_body().ok_or(KError::NotEnoughMemory)?,
            ObjType::SchedContext => self.alloc_sc_body().ok_or(KError::NotEnoughMemory)?,
            _ => BodyRef::NONE,
        };
        let obj = self.push_meta(
            ty,
            Some(u_idx),
            offset,
            len,
            region,
            if ty == ObjType::CNode {
                Some(child_slots)
            } else {
                None
            },
            slot_base,
            slot_len,
            body,
        );
        Ok(self.push_cap(obj, ty, Some(src_cap), src_rights, None))
    }

    // ------------------------------------------------------------- CSpace

    /// `CNode.Copy`: tạo cap con ở đích; đích phải rỗng. Giữ quyền + badge nguồn.
    pub fn copy(&mut self, src: SlotRef, dst: SlotRef) -> Result<CapId, KError> {
        self.copy_rights(src, dst, None)
    }

    /// `CNode.Copy` với `rights?`: `None` = giữ quyền nguồn; `Some(r)` phải là tập con.
    pub fn copy_rights(
        &mut self,
        src: SlotRef,
        dst: SlotRef,
        rights: Option<u8>,
    ) -> Result<CapId, KError> {
        let src_cap = self.cap_at(src.cnode, src.slot)?;
        let (obj, ty, src_rights, badge) = {
            let c = self.cap(src_cap)?;
            (c.obj, c.ty, c.rights, c.badge)
        };
        let rights = match rights {
            Some(r) => {
                if r & !src_rights != 0 {
                    return Err(KError::InsufficientRights);
                }
                r
            }
            None => src_rights,
        };
        self.derive(src_cap, obj, ty, dst, rights, badge)
    }

    /// `CNode.Mint`: như Copy nhưng thu hẹp quyền và đặt badge (chỉ Endpoint/Notification).
    pub fn mint(
        &mut self,
        src: SlotRef,
        dst: SlotRef,
        rights: u8,
        badge: Option<u64>,
    ) -> Result<CapId, KError> {
        let src_cap = self.cap_at(src.cnode, src.slot)?;
        let (obj, ty, src_rights, src_badge) = {
            let c = self.cap(src_cap)?;
            (c.obj, c.ty, c.rights, c.badge)
        };
        if rights & !src_rights != 0 {
            return Err(KError::InsufficientRights);
        }
        let badge = match badge {
            None => src_badge,
            Some(0) => None,
            Some(b) => {
                if !matches!(ty, ObjType::Endpoint | ObjType::Notification) {
                    return Err(KError::InvalidArgument);
                }
                Some(b)
            }
        };
        self.derive(src_cap, obj, ty, dst, rights, badge)
    }

    /// Tạo cap con của `src_cap` rồi gắn vào `dst` — đã kiểm nguồn/quyền trước khi cấp.
    fn derive(
        &mut self,
        src_cap: CapId,
        obj: ObjId,
        ty: ObjType,
        dst: SlotRef,
        rights: u8,
        badge: Option<u64>,
    ) -> Result<CapId, KError> {
        self.check_cnode_slot(dst.cnode, dst.slot)?;
        if self.cnode_slot(dst.cnode, dst.slot)?.is_some() {
            return Err(KError::SlotOccupied);
        }
        let new_cap = self.push_cap(obj, ty, Some(src_cap), rights, badge);
        self.cnode_set_slot(dst.cnode, dst.slot, Some(new_cap))?;
        self.caps[new_cap as usize - 1].slot = Some(dst);
        Ok(new_cap)
    }

    /// `CNode.Delete`: xoá cap trong slot (object không bị finalise — chỉ Revoke mới).
    ///
    /// Con của cap bị xoá thuộc về **ông** (tổ tiên sống gần nhất) — MDB kiểu seL4.
    pub fn delete(&mut self, slot: SlotRef) -> Result<(), KError> {
        let cap = self.cap_at(slot.cnode, slot.slot)?;
        let mut new_parent = self.cap(cap)?.parent;
        while let Some(p) = new_parent {
            if p != 0 && (p as usize) <= self.n_caps && self.caps[p as usize - 1].alive {
                break;
            }
            new_parent = if p != 0 && (p as usize) <= self.n_caps {
                self.caps[p as usize - 1].parent
            } else {
                None
            };
        }
        for c in &mut self.caps[..self.n_caps] {
            if c.alive && c.parent == Some(cap) {
                c.parent = new_parent;
            }
        }
        self.clear_slot_of(cap);
        if let Some(c) = self.caps.get_mut(cap as usize - 1) {
            c.alive = false;
            c.parent = None;
        }
        Ok(())
    }

    /// `CNode.Revoke`: xoá con/cháu của cap trong `slot` (hoặc mọi cap của CNode khi
    /// `slot = None`). Revoke cap Untyped finalise mọi object trong cây Untyped và đặt
    /// watermark về 0. Trả số cap bị xoá.
    pub fn revoke(&mut self, cnode: ObjId, slot: Option<usize>) -> Result<usize, KError> {
        // D2-83/KT-58: sim đọc **body** CNode trước (`cnode_slots_of`), nên CNode đã finalise
        // trả `InvalidCapability` (body `Dead`), không đọc metadata. `cnode_slot_count` thì
        // vẫn trả số slot của metadata (API truy vấn) — `Revoke` phải đi cửa body như sim.
        let n = {
            let m = self.meta(cnode)?;
            if !m.alive {
                return Err(KError::InvalidCapability);
            }
            if m.ty != ObjType::CNode {
                return Err(KError::WrongObjectType);
            }
            m.slot_len
        };
        let mut deleted = 0;
        match slot {
            Some(s) => {
                if s >= n {
                    return Err(KError::RangeError);
                }
                match self.cnode_slot(cnode, s)? {
                    None => Err(KError::SlotEmpty),
                    Some(cap) => {
                        if self.cap(cap)?.alive {
                            deleted += self.revoke_children(cap)?;
                        }
                        Ok(deleted)
                    }
                }
            }
            None => {
                for s in 0..n {
                    if let Some(cap) = self.cnode_slot(cnode, s)? {
                        if self.cap(cap)?.alive {
                            deleted += self.revoke_children(cap)?;
                        }
                    }
                }
                Ok(deleted)
            }
        }
    }

    /// Xoá cap con/cháu của `root`; nếu `root` là cap Untyped thì finalise object + reset
    /// arena của cả cây Untyped (watermark về 0, thiết kế §4.2).
    fn revoke_children(&mut self, root: CapId) -> Result<usize, KError> {
        if !self.cap(root)?.alive {
            return Err(KError::InvalidCapability);
        }
        let mut doomed = [0u32; MAX_CAPS];
        let mut nd = 0usize;
        for i in 1..=self.n_caps as CapId {
            if !self.caps[i as usize - 1].alive || i == root {
                continue;
            }
            let mut parent = self.caps[i as usize - 1].parent;
            let mut steps = 0usize;
            while let Some(p) = parent {
                if p == root {
                    push_id(&mut doomed, &mut nd, i);
                    break;
                }
                if p == 0 || p as usize > self.n_caps {
                    break;
                }
                parent = self.caps[p as usize - 1].parent;
                steps += 1;
                if steps > self.n_caps {
                    break;
                }
            }
        }

        let root_ty = self.cap(root)?.ty;
        let root_obj = self.cap(root)?.obj;
        if root_ty == ObjType::Untyped {
            let u_idx = self.object_index(root_obj)?;
            let mut dead = [0u32; MAX_OBJECTS];
            let mut ndead = 0usize;
            self.collect_untyped_tree(u_idx, &mut dead, &mut ndead);
            // Finalise CNode (seL4 `finaliseCNode`): cap nằm trong slot của CNode chết phải
            // chết theo **cả cây con của nó**, kể cả khi object đích còn sống.
            for &id in &dead[..ndead] {
                if self.objects[id as usize - 1].ty != ObjType::CNode {
                    continue;
                }
                let len = self.objects[id as usize - 1].slot_len;
                for s in 0..len {
                    if let Some(cap) = self.cnode_slot(id, s)? {
                        if self.cap(cap)?.alive {
                            self.collect_subtree(cap, &mut doomed, &mut nd);
                        }
                    }
                }
            }
            // Slot của CNode chết không giữ cap nào.
            for &id in &dead[..ndead] {
                if self.objects[id as usize - 1].ty == ObjType::CNode {
                    let (base, len) = {
                        let m = &self.objects[id as usize - 1];
                        (m.slot_base, m.slot_len)
                    };
                    self.slots.fill_zero(base, len);
                }
            }
            // KT-18/KT-59: finalise TCB **trước** khi object chết (gỡ ready queue, `Dead`,
            // trả `sched_context` qua hook, ghi log) — như sim `revoke_children`.
            for &id in &dead[..ndead] {
                if self.objects[id as usize - 1].ty == ObjType::Tcb {
                    let _ = self.finalise_tcb(id);
                }
            }
            // Đánh dấu chết + trả word slot pool + trả body TCB/SchedContext về free-list.
            for &id in &dead[..ndead] {
                let idx = id as usize - 1;
                let body = self.objects[idx].body;
                {
                    let m = &mut self.objects[idx];
                    m.alive = false;
                    m.body.alive = false;
                }
                if body.kind == BODY_TCB {
                    self.free_tcb_body(body.idx);
                } else if body.kind == BODY_SC {
                    self.free_sc_body(body.idx);
                }
                let (base, len) = (self.objects[idx].slot_base, self.objects[idx].slot_len);
                if len > 0 {
                    self.slots.free(base, len);
                }
            }
            // Reset arena của **mọi** Untyped trong cây + Untyped gốc.
            for &id in &dead[..ndead] {
                if let Some(r) = self.objects[id as usize - 1].region {
                    self.regions[r].reset();
                }
            }
            if let Some(r) = self.objects[u_idx].region {
                self.regions[r].reset();
            }
            // Cap còn sống trỏ object vừa chết cũng phải chết (không cap nào trỏ object chết).
            for i in 1..=self.n_caps as CapId {
                if self.caps[i as usize - 1].alive
                    && dead[..ndead].contains(&self.caps[i as usize - 1].obj)
                {
                    push_id(&mut doomed, &mut nd, i);
                }
            }
        }

        sort_dedup(&mut doomed, &mut nd);
        let mut deleted = 0;
        for &cap in &doomed[..nd] {
            if self.caps[cap as usize - 1].alive {
                self.clear_slot_of(cap);
                self.caps[cap as usize - 1].alive = false;
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    // ------------------------------------------------------------- nội bộ

    #[allow(clippy::too_many_arguments)]
    fn push_meta(
        &mut self,
        ty: ObjType,
        untyped: Option<usize>,
        offset: usize,
        len: usize,
        region: Option<usize>,
        slots_meta: Option<usize>,
        slot_base: usize,
        slot_len: usize,
        body: BodyRef,
    ) -> ObjId {
        assert!(
            self.n_objects < MAX_OBJECTS,
            "vifast-core: hết MAX_OBJECTS ({MAX_OBJECTS})"
        );
        self.objects[self.n_objects] = Meta {
            ty,
            alive: true,
            untyped,
            offset,
            len,
            region,
            slots_meta,
            slot_len,
            slot_base,
            body,
        };
        self.n_objects += 1;
        self.n_objects as ObjId
    }

    fn push_cap(
        &mut self,
        obj: ObjId,
        ty: ObjType,
        parent: Option<CapId>,
        rights: u8,
        badge: Option<u64>,
    ) -> CapId {
        assert!(
            self.n_caps < MAX_CAPS,
            "vifast-core: hết MAX_CAPS ({MAX_CAPS})"
        );
        self.caps[self.n_caps] = Cap {
            obj,
            ty,
            parent,
            slot: None,
            rights,
            badge,
            alive: true,
        };
        self.n_caps += 1;
        self.n_caps as CapId
    }

    fn tcb_body_available(&self) -> bool {
        self.n_tcb_free > 0 || self.n_tcbs < MAX_TCBS
    }

    fn sc_body_available(&self) -> bool {
        self.n_sc_free > 0 || self.n_scs < MAX_SCHED_CONTEXTS
    }

    fn alloc_tcb_body(&mut self) -> Option<BodyRef> {
        let idx = if self.n_tcb_free > 0 {
            self.n_tcb_free -= 1;
            self.tcb_free[self.n_tcb_free] as usize
        } else if self.n_tcbs < MAX_TCBS {
            let i = self.n_tcbs;
            self.n_tcbs += 1;
            i
        } else {
            return None;
        };
        self.tcbs[idx] = TcbData::EMPTY;
        Some(BodyRef {
            kind: BODY_TCB,
            idx: idx as u16,
            alive: true,
        })
    }

    fn alloc_sc_body(&mut self) -> Option<BodyRef> {
        let idx = if self.n_sc_free > 0 {
            self.n_sc_free -= 1;
            self.sc_free[self.n_sc_free] as usize
        } else if self.n_scs < MAX_SCHED_CONTEXTS {
            let i = self.n_scs;
            self.n_scs += 1;
            i
        } else {
            return None;
        };
        self.scs[idx] = ScData::EMPTY;
        Some(BodyRef {
            kind: BODY_SC,
            idx: idx as u16,
            alive: true,
        })
    }

    fn free_tcb_body(&mut self, idx: u16) {
        if self.n_tcb_free < MAX_TCBS {
            self.tcb_free[self.n_tcb_free] = idx;
            self.n_tcb_free += 1;
        }
    }

    fn free_sc_body(&mut self, idx: u16) {
        if self.n_sc_free < MAX_SCHED_CONTEXTS {
            self.sc_free[self.n_sc_free] = idx;
            self.n_sc_free += 1;
        }
    }

    pub(crate) fn object_index(&self, obj: ObjId) -> Result<usize, KError> {
        if obj == 0 || obj as usize > self.n_objects {
            return Err(KError::InvalidCapability);
        }
        Ok(obj as usize - 1)
    }

    pub(crate) fn meta(&self, obj: ObjId) -> Result<&Meta, KError> {
        Ok(&self.objects[self.object_index(obj)?])
    }

    pub(crate) fn cap(&self, cap: CapId) -> Result<&Cap, KError> {
        if cap == 0 || cap as usize > self.n_caps {
            return Err(KError::InvalidCapability);
        }
        Ok(&self.caps[cap as usize - 1])
    }

    pub(crate) fn cnode_slot(&self, cnode: ObjId, slot: usize) -> Result<Option<CapId>, KError> {
        let m = self.meta(cnode)?;
        if !m.alive {
            let n = m.slots_meta.ok_or(KError::WrongObjectType)?;
            return if slot < n {
                Ok(None)
            } else {
                Err(KError::RangeError)
            };
        }
        if m.ty != ObjType::CNode {
            return Err(KError::WrongObjectType);
        }
        if slot >= m.slot_len {
            return Err(KError::RangeError);
        }
        let word = self.slots.get(m.slot_base, slot);
        Ok(if word == 0 { None } else { Some(word) })
    }

    fn cnode_set_slot(
        &mut self,
        cnode: ObjId,
        slot: usize,
        cap: Option<CapId>,
    ) -> Result<(), KError> {
        let (base, len) = {
            let m = self.meta(cnode)?;
            // Như sim `cnode_set_slot` → `body_mut`: object đã finalise ⇒ `Dead` ⇒
            // `InvalidCapability`; object sống sai kiểu ⇒ `WrongObjectType`.
            if !m.alive {
                return Err(KError::InvalidCapability);
            }
            if m.ty != ObjType::CNode {
                return Err(KError::WrongObjectType);
            }
            (m.slot_base, m.slot_len)
        };
        if slot >= len {
            return Err(KError::RangeError);
        }
        self.slots.set(base, slot, cap.unwrap_or(0));
        Ok(())
    }

    fn check_cnode_slot(&self, cnode: ObjId, slot: usize) -> Result<(), KError> {
        let n = self.cnode_slot_count(cnode)?;
        if slot >= n {
            return Err(KError::RangeError);
        }
        Ok(())
    }

    fn clear_slot_of(&mut self, cap: CapId) {
        let slot = self
            .caps
            .get_mut(cap as usize - 1)
            .and_then(|c| c.slot.take());
        if let Some(SlotRef { cnode, slot: s }) = slot {
            if let Ok(Some(cur)) = self.cnode_slot(cnode, s) {
                if cur == cap {
                    let _ = self.cnode_set_slot(cnode, s, None);
                }
            }
        }
    }

    /// Gom `root` và mọi con/cháu còn sống của nó (theo cạnh cha) vào `out`.
    fn collect_subtree(&self, root: CapId, out: &mut [CapId; MAX_CAPS], n: &mut usize) {
        for i in 1..=self.n_caps as CapId {
            if !self.caps[i as usize - 1].alive {
                continue;
            }
            let mut cur = Some(i);
            let mut steps = 0usize;
            while let Some(p) = cur {
                if p == root {
                    push_id(out, n, i);
                    break;
                }
                if p == 0 || p as usize > self.n_caps {
                    break;
                }
                cur = self.caps[p as usize - 1].parent;
                steps += 1;
                if steps > self.n_caps {
                    break;
                }
            }
        }
    }

    /// Gom mọi object sống nằm trong `u_idx`, đệ quy qua Untyped con.
    fn collect_untyped_tree(&self, u_idx: usize, out: &mut [ObjId; MAX_OBJECTS], n: &mut usize) {
        for i in 0..self.n_objects {
            let (alive, unty, ty) = {
                let m = &self.objects[i];
                (m.alive, m.untyped, m.ty)
            };
            if !alive || unty != Some(u_idx) {
                continue;
            }
            push_id(out, n, (i as u32) + 1);
            if ty == ObjType::Untyped {
                self.collect_untyped_tree(i, out, n);
            }
        }
    }
}

/// Thêm id vào mảng có biên (bỏ nếu đầy — mọi nơi gọi đều ≤ `MAX_*` theo bất biến bảng).
pub(crate) fn push_id<const N: usize>(out: &mut [u32; N], n: &mut usize, id: u32) {
    if *n < N {
        out[*n] = id;
        *n += 1;
    }
}

/// Sắp xếp + khử trùng tại chỗ (mảng nhỏ, không cấp phát).
fn sort_dedup(out: &mut [CapId; MAX_CAPS], n: &mut usize) {
    for i in 1..*n {
        let x = out[i];
        let mut j = i;
        while j > 0 && out[j - 1] > x {
            out[j] = out[j - 1];
            j -= 1;
        }
        out[j] = x;
    }
    let mut w = 0usize;
    for r in 0..*n {
        if w == 0 || out[w - 1] != out[r] {
            out[w] = out[r];
            w += 1;
        }
    }
    *n = w;
}

/// Iterator bảng cap (thay `Vec<CapInfo>` của sim).
#[derive(Debug, Clone)]
pub struct CapTable<'a> {
    caps: &'a [Cap],
    idx: usize,
}

impl Iterator for CapTable<'_> {
    type Item = CapInfo;

    fn next(&mut self) -> Option<CapInfo> {
        let i = self.idx;
        if i >= self.caps.len() {
            return None;
        }
        self.idx += 1;
        let c = &self.caps[i];
        Some(CapInfo {
            cap: (i as u32) + 1,
            obj: c.obj,
            ty: c.ty,
            parent: c.parent,
            slot: c.slot,
            rights: c.rights,
            badge: c.badge,
            alive: c.alive,
        })
    }
}

/// Iterator con trực tiếp của một cap (thay `Vec<CapId>` của sim).
#[derive(Debug, Clone)]
pub struct CapChildren<'a> {
    caps: &'a [Cap],
    parent: CapId,
    idx: usize,
}

impl Iterator for CapChildren<'_> {
    type Item = CapId;

    fn next(&mut self) -> Option<CapId> {
        while self.idx < self.caps.len() {
            let i = self.idx;
            self.idx += 1;
            let c = &self.caps[i];
            if c.alive && c.parent == Some(self.parent) {
                return Some((i as u32) + 1);
            }
        }
        None
    }
}
