// SPDX-License-Identifier: MIT OR Apache-2.0
//! `Machine` — model core tầng 1: Untyped + CNode + MDB + capability (KT-58).
//!
//! Port **nguyên ngữ nghĩa** từ `v2/sim/vifast-sim/src/kobj.rs` (oracle): cùng id tuần tự
//! không tái dùng, cùng thứ tự cấp phát, cùng mã lỗi, cùng bất biến `check_caps`/`check_bytes`.
//! Khác biệt **duy nhất** so với sim ở tầng này (ghi trong `docs/k3b-core.md`):
//!
//! 1. Bảng tĩnh `.bss` thay `Vec` (không `alloc`); đầy bảng ⇒ `NotEnoughMemory` (harness
//!    `add_*` panic như sim khi arena kernel đầy).
//! 2. Chưa có body TCB/IPC/Notification/Reply (KT-59/60): object vẫn có `ObjType`, `alive`,
//!    `untyped/offset/len`, cap/MDB đầy đủ — đủ cho parity CNode/MDB/Untyped.
//! 3. Không `Retype` `Frame`/`PageTable`/`VSpace` (K2 object layer `vifast-plat-common::obj`,
//!    KT-52) — trả `NotSupported`; sim (K2) đã mở `Frame`/`PageTable`, xem `docs/k3b-core.md`.

use crate::arena::Region;
use crate::error::{CapId, CapInfo, KError, ObjId, ObjType, SlotRef};
use crate::slotpool::SlotPool;
use crate::spec::{
    ARENA_ALIGN, CNODE_BASE_BYTES, DEFAULT_ALIGN_BYTES, FLAG_GUEST, RIGHTS_ALL, RIGHT_WRITE,
};

/// Số object tối đa trong bảng tĩnh (id **không tái dùng**, kể cả object đã chết).
pub const MAX_OBJECTS: usize = 256;
/// Số cap tối đa trong bảng tĩnh (id **không tái dùng**, kể cả cap đã chết).
pub const MAX_CAPS: usize = 512;
/// Số Untyped tối đa (mỗi Untyped một arena).
pub const MAX_UNTYPEDS: usize = 16;
/// Số word slot CNode tối đa (1 word = 1 cap id; 48 B slot của spec vẫn được kế toán byte).
pub const MAX_SLOT_WORDS: usize = 4096;
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

/// Core model (tầng 1) — **không** `unsafe`, **không** cấp phát động.
#[derive(Debug, Clone, Copy)]
pub struct Machine {
    pub(crate) regions: [Region; MAX_UNTYPEDS],
    pub(crate) n_regions: usize,
    pub(crate) objects: [Meta; MAX_OBJECTS],
    pub(crate) n_objects: usize,
    pub(crate) caps: [Cap; MAX_CAPS],
    pub(crate) n_caps: usize,
    pub(crate) slots: SlotPool,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    /// Model rỗng (Untyped/CNode do harness thêm).
    pub const fn new() -> Self {
        Self {
            regions: [Region::EMPTY; MAX_UNTYPEDS],
            n_regions: 0,
            objects: [Meta::EMPTY; MAX_OBJECTS],
            n_objects: 0,
            caps: [Cap::EMPTY; MAX_CAPS],
            n_caps: 0,
            slots: SlotPool::EMPTY,
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
        self.push_meta(ObjType::Untyped, None, 0, 0, Some(region), None, 0, 0)
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
        self.push_meta(ObjType::CNode, None, 0, 0, None, Some(slots), base, slots)
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
        self.push_meta(ty, None, 0, 0, None, slots_meta, slot_base, slot_len)
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
            // Đánh dấu chết + trả word slot pool (body TCB/IPC là KT-59/60 — không có ở đây).
            for &id in &dead[..ndead] {
                let idx = id as usize - 1;
                {
                    let m = &mut self.objects[idx];
                    m.alive = false;
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

    fn object_index(&self, obj: ObjId) -> Result<usize, KError> {
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
