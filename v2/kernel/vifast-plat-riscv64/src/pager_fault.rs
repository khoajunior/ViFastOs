// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-70** — trạng thái fault VM giao cho **pager server U-mode** (tách khỏi `user.rs` để giữ
//! luật cấu trúc R1: mỗi tệp kernel/lib ≤ 1500 dòng).
//!
//! Chính sách mặc định (KT-53/KT-56) vẫn là fault U-mode **fatal**. Chỉ ảnh test `ktest-user`
//! bật [`set_pager_faults`] trong pha pager; khi bật, `on_u_exception` giữ `sepc` tại instruction
//! lỗi, ghi ảnh chụp fault ở đây rồi trả scheduler (`Stop::Fault`), scheduler giao fault cho
//! pager; pager map trang xong thì `run_task` chạy lại instruction lỗi.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// `1` ⇔ pha **pager** (KT-70): fault VM U-mode trả về scheduler thay vì PANIC.
static PAGER_FAULTS: AtomicU32 = AtomicU32::new(0);
/// Số fault VM đã giao cho pager (một chiều).
static PAGER_FAULT_COUNT: AtomicU64 = AtomicU64::new(0);
/// `stval` của fault VM gần nhất (= VA client chạm).
static PAGER_FAULT_VA: AtomicU64 = AtomicU64::new(0);
/// ASID nhãn của VSpace bị fault (từ `FaultKind::Vm.vspace` của nền tảng).
static PAGER_FAULT_ASID: AtomicU64 = AtomicU64::new(0);
/// Mã quyền truy cập (`FaultAccess::code`: Read=0/Write=1/Execute=2).
static PAGER_FAULT_ACCESS: AtomicU32 = AtomicU32::new(0);
/// Mã lý do (`VmFaultReason::code`: NotMapped=0/Permission=1/Wx=2).
static PAGER_FAULT_REASON: AtomicU32 = AtomicU32::new(0);

/// Bật/tắt đường fault→pager (chỉ pha pager của ảnh test `ktest-user`).
pub fn set_pager_faults(on: bool) {
    PAGER_FAULTS.store(u32::from(on), Ordering::Release);
}

/// Đường fault→pager có bật?
pub fn pager_faults() -> bool {
    PAGER_FAULTS.load(Ordering::Acquire) != 0
}

/// Số fault VM đã giao cho pager.
pub fn pager_fault_count() -> u64 {
    PAGER_FAULT_COUNT.load(Ordering::Acquire)
}

/// Ảnh chụp fault VM gần nhất — **giá trị đo** từ trapframe + `FaultKind`, không phải hằng.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagerFault {
    /// Địa chỉ gây fault (`stval`).
    pub va: u64,
    /// ASID nhãn VSpace (`FaultKind::Vm.vspace`).
    pub asid: u64,
    /// `FaultAccess::code` (Read=0/Write=1/Execute=2).
    pub access: u64,
    /// `VmFaultReason::code` (NotMapped=0/Permission=1/Wx=2).
    pub reason: u64,
}

/// Fault VM gần nhất đã giao cho pager.
pub fn pager_fault() -> PagerFault {
    PagerFault {
        va: PAGER_FAULT_VA.load(Ordering::Acquire),
        asid: PAGER_FAULT_ASID.load(Ordering::Acquire),
        access: u64::from(PAGER_FAULT_ACCESS.load(Ordering::Acquire)),
        reason: u64::from(PAGER_FAULT_REASON.load(Ordering::Acquire)),
    }
}

/// Ghi một fault VM vừa phân loại (gọi từ `on_u_exception` khi [`pager_faults`] bật).
/// `#[allow(dead_code)]`: build không bật feature `user` không có call site (cùng một crate).
#[allow(dead_code)]
pub(crate) fn record(va: u64, asid: u64, access: u32, reason: u32) {
    PAGER_FAULT_COUNT.fetch_add(1, Ordering::AcqRel);
    PAGER_FAULT_VA.store(va, Ordering::Release);
    PAGER_FAULT_ASID.store(asid, Ordering::Release);
    PAGER_FAULT_ACCESS.store(access, Ordering::Release);
    PAGER_FAULT_REASON.store(reason, Ordering::Release);
}
