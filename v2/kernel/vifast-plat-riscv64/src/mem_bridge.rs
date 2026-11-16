// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-67** — cầu nối D2-83 cho `memsrv`: **core giữ cap, object layer giữ Frame/PA**.
//!
//! # Biên giới (một chiều, đúng D2-83)
//!
//! - `Frame`/`PageTable`/`VSpace` **chỉ** do object layer (`vifast_plat_common::obj`, KT-52) cấp
//!   và thu hồi; core **không** Retype ba loại này (pin TAP `core d2-83 boundary`).
//! - Khi cấp một Frame cho client, `vifast-core` giữ một **object đại diện**
//!   ([`vifast_core::Machine::add_foreign_object`]) + cap trỏ nó; `key` của object là danh tính
//!   bên object layer (`(object_id << 16) | cap_index`). Chiều tra cứu là **một chiều**:
//!   cap core → key → cap object layer → `(pa, size_bits)`; core **không** đọc byte/PTE nào của
//!   Frame và **không** kế toán PA (D2-83).
//! - Byte của cửa sổ PA do **một** allocator Frame (`vifast_alloc::FrameArena`) trong object layer
//!   quản; [`budget`] chỉ là **view** của cùng trạng thái ấy (không phải allocator thứ hai).
//!
//! # Vì sao quyết định nằm ở đây, chưa phải trong `memsrv` U-mode
//!
//! KT-67 ưu tiên phần chắc chắn đo được: allocator + cầu nối + TAP trên QEMU (theo chỉ đạo
//! "ưu tiên `vifast-alloc` + TAP alloc/revoke trên kernel"). Phần **memsrv U-mode** bị **BLOCKED**
//! vì ABI hiện tại **không** chở được cap trong message từ U-mode (`NBSend` chỉ có label + 4 MR —
//! `v2/kernel/vifast-plat-user/lib.rs` `send()`), và U-mode **không** có invocation để tạo
//! `Frame` (bảng dispatch `v2/kernel/vifast-core/src/syscall.rs` chỉ 0..15; Retype VM trong core
//! trả `NotSupported` tại `v2/kernel/vifast-core/src/machine.rs`). Vì vậy hàm [`grant`] ở đây là
//! **bước quyết định + thi hành** mà vòng `NBRecv` của memsrv sẽ gọi khi có request (cùng mẫu
//! "kernel làm scheduler/bridge" của KT-65), và **cơ chế chuyển cap KT-63**
//! (`prepare_caps`/`deliver_caps`) là đường đưa cap Frame tới client — TAP `mem bridge cap`
//! chạy đúng đường đó trên 2 TCB thật.
//!
//! `unsafe`: **không** — tệp này chỉ gọi API safe của object layer và core (D2-17).

use vifast_core::{CapId as CoreCap, KError, ObjId, ObjType, RIGHT_WRITE};
use vifast_plat_common::obj::{CapId, FrameBudget, ObjError, ObjKind};

use crate::core_bridge;
use crate::obj as aobj;

/// Mask phần thấp của `key` (chỉ số cap object layer).
const KEY_CAP_MASK: u64 = 0xFFFF;

/// Lỗi của cầu nối: lỗi object layer hoặc lỗi core (giữ nguyên nguồn, không bọc mã mới).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemError {
    /// Lỗi từ object layer (`Retype`/`Revoke`/truy vấn Frame).
    Obj(ObjError),
    /// Lỗi từ core (cap người gửi, bảng object/cap, MDB).
    Core(KError),
}

impl MemError {
    /// Mã lỗi ABI (`spec/abi.toml`) — `None` với lỗi nội bộ không có mã.
    pub const fn abi_code(self) -> Option<u16> {
        match self {
            MemError::Obj(error) => error.abi_code(),
            MemError::Core(error) => Some(error.id()),
        }
    }

    /// Tên ổn định cho log/TAP (trùng `spec/abi.toml` khi có).
    pub const fn name(self) -> &'static str {
        match self {
            MemError::Obj(error) => error.code_name(),
            MemError::Core(error) => error.name(),
        }
    }
}

/// Một Frame đã cấp cho client: cap object layer + object/cap đại diện trong core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameGrant {
    /// Object đại diện trong core (`ObjType::Frame`, không thuộc Untyped nào).
    pub core_obj: ObjId,
    /// Cap gốc của object đại diện trong core (chưa nằm trong slot — caller `put_cap`).
    pub core_cap: CoreCap,
    /// Cap Frame của object layer (nguồn sự thật PA/byte).
    pub obj_cap: CapId,
    /// Danh tính bên object layer: `(object_id << 16) | cap_index`.
    pub key: u64,
    /// PA nhãn của Frame (D2-66).
    pub pa: u64,
    /// `size_bits` (`12`/`21`/`30`).
    pub size_bits: u8,
}

/// Ghép `key` từ danh tính object layer: `object_id` (không tái dùng) ở cao, chỉ số cap ở thấp.
pub const fn make_key(cap: CapId, object_id: u32) -> u64 {
    (object_id as u64) << 16 | cap.0 as u64
}

/// Tách `key` thành `(cap, object_id)` — dùng để phát hiện cap đã chết/tái dùng chỉ số.
pub const fn decode_key(key: u64) -> (CapId, u32) {
    (CapId((key & KEY_CAP_MASK) as u16), (key >> 16) as u32)
}

/// **Quyết định + thi hành** một yêu cầu cấp Frame (bước **kernel-mediated** — D2-96; `memsrv`
/// U-mode chỉ là **policy façade**, chưa cài): kiểm **cap client gửi** (phải là `Untyped` còn
/// sống, có `RIGHT_WRITE`) rồi cấp Frame thật từ Untyped nguồn của object layer và dựng đại diện
/// trong core.
///
/// **Giới hạn có chủ đích (reviewer-kt67 #6)**: kiểm cap client là **quyền thao tác**, **không**
/// phải uỷ quyền/ngân sách — KT-67 **không** thi hành quota theo cap client (`CLIENT_TOKEN_BYTES`
/// trong TAP chỉ là cỡ nhãn của Untyped dựng cho test, không phải hạn mức được trừ). Ngân sách
/// thật của Frame là **cửa sổ PA của Untyped nguồn** (một allocator — `vifast-alloc`).
///
/// Lỗi **nguyên tử**: cap client sai ⇒ không cấp byte nào; core không dựng được đại diện ⇒
/// object layer được **rollback** (`revoke`) ngay, không để Frame mồ côi.
pub fn grant(client: CoreCap, source: CapId, size_bits: u8) -> Result<FrameGrant, MemError> {
    // 1) Quyền của **cap client gửi**: sai loại/thiếu `RIGHT_WRITE` ⇒ từ chối trước khi cấp.
    core_bridge::with_machine(|m| {
        let info = m.cap_info(client).map_err(MemError::Core)?;
        if info.ty != ObjType::Untyped {
            return Err(MemError::Core(KError::WrongObjectType));
        }
        if info.rights & RIGHT_WRITE == 0 {
            return Err(MemError::Core(KError::InsufficientRights));
        }
        Ok(())
    })?;
    // 2) Object layer cấp Frame **thật** (tầng duy nhất sở hữu Frame — D2-83).
    let obj_cap = aobj::machine()
        .retype(source, ObjKind::Frame, size_bits)
        .map_err(MemError::Obj)?;
    let (pa, bits, key) = {
        let m = aobj::machine();
        let pa = m.frame_pa(obj_cap).map_err(MemError::Obj)?;
        let bits = m.frame_size_bits(obj_cap).map_err(MemError::Obj)?;
        let id = m.cap_key(obj_cap).map_err(MemError::Obj)?;
        (pa, bits, make_key(obj_cap, id))
    };
    // 3) Core: object đại diện + cap gốc; lỗi ⇒ rollback object layer (nguyên tử).
    let made = core_bridge::with_machine(|m| {
        let obj = m.add_foreign_object(ObjType::Frame, key)?;
        let cap = m.root_cap(obj);
        Ok::<(ObjId, CoreCap), KError>((obj, cap))
    });
    match made {
        Ok((core_obj, core_cap)) => Ok(FrameGrant {
            core_obj,
            core_cap,
            obj_cap,
            key,
            pa,
            size_bits: bits,
        }),
        Err(error) => {
            match aobj::machine().revoke(obj_cap) {
                Ok(()) => Err(MemError::Core(error)),
                // Rollback hỏng là lỗi **nặng hơn** lỗi gốc (Frame mồ côi) — báo đúng nó,
                // không nuốt (reviewer-kt67 #6).
                Err(rollback) => Err(MemError::Obj(rollback)),
            }
        }
    }
}

/// Tra ngược **một chiều** `core obj → (cap object layer, pa, size_bits)` (D2-83).
///
/// `None` khi object không phải đại diện Frame, cap bên object layer đã chết, hoặc `object_id`
/// không khớp (chỉ số cap bị tái dùng) — fail-closed, không đoán.
pub fn resolve(core_obj: ObjId) -> Option<(CapId, u64, u8)> {
    let key = core_bridge::with_machine(|m| m.foreign_key(core_obj).ok())?;
    let (cap, want_id) = decode_key(key);
    let m = aobj::machine();
    if m.cap_kind(cap).ok()? != ObjKind::Frame {
        return None;
    }
    if m.cap_key(cap).ok()? != want_id {
        return None;
    }
    let pa = m.frame_pa(cap).ok()?;
    let bits = m.frame_size_bits(cap).ok()?;
    Some((cap, pa, bits))
}

/// Trả Frame về allocator (revoke) — thứ tự **an toàn**: kiểm hết điều kiện có thể hỏng **trước**
/// mọi thay đổi, rồi `revoke` (object layer = nguồn sự thật Frame), cuối cùng đánh dấu đại diện
/// core chết. Lỗi giữa đường vì vậy **không** để lại "core đã chết object, Frame còn sống"
/// (reviewer-kt67 #6).
///
/// Caller phải `delete` mọi cap (core) trỏ tới object đại diện trước — giữ bất biến
/// `check_caps` "cap sống không trỏ object chết".
pub fn release(grant: FrameGrant) -> Result<(), MemError> {
    let (maps, writes, execs) = aobj::machine()
        .frame_counts(grant.obj_cap)
        .map_err(MemError::Obj)?;
    if maps != 0 || writes != 0 || execs != 0 {
        return Err(MemError::Obj(ObjError::HasChildren));
    }
    if core_bridge::with_machine(|m| m.foreign_object_has_caps(grant.core_obj))
        .map_err(MemError::Core)?
    {
        return Err(MemError::Core(KError::IllegalOperation));
    }
    // 1) Trả Frame về allocator (object layer là nguồn sự thật byte).
    aobj::machine()
        .revoke(grant.obj_cap)
        .map_err(MemError::Obj)?;
    // 2) Đánh dấu object đại diện chết (đã chắc không còn cap sống).
    core_bridge::with_machine(|m| m.kill_foreign_object(grant.core_obj)).map_err(MemError::Core)?;
    Ok(())
}

/// Ảnh chụp cửa sổ PA mà `memsrv`/TAP dùng để **đối chiếu** với object layer — view của
/// allocator Frame duy nhất, không phải allocator thứ hai.
pub fn budget(source: CapId) -> Option<FrameBudget> {
    aobj::machine().frame_budget(source).ok()
}

/// Cấp key mới cho một Frame **đã có** (chỉ dùng cho test/chẩn đoán cap cũ): `None` nếu hỏng.
pub fn key_of(core_obj: ObjId) -> Option<u64> {
    core_bridge::with_machine(|m| m.foreign_key(core_obj).ok())
}
