// SPDX-License-Identifier: MIT OR Apache-2.0
//! U-mode trên QEMU `virt` (KT-53) — phần **không viết "safe" được** của
//! [`vifast_plat_common::user`]: nạp `init` ELF vào VSpace thật, dựng trapframe U-mode, `sret`
//! xuống U-mode, bắt `ecall` (`scause=8`) và đọc bộ nhớ user bằng `sstatus.SUM`.
//!
//! # Cơ chế `sscratch` đã chọn (bắt buộc ghi rõ — KT-53)
//!
//! Từ KT-45/KT-49, **`sscratch` luôn giữ đỉnh trap stack per-hart** (giá trị kernel), cả khi
//! đang chạy trong kernel lẫn khi đang chạy trong **U-mode**:
//!
//! - `sret` xuống U-mode **không** đổi `sscratch`; U-mode không đọc/ghi được CSR supervisor.
//! - Trap từ U-mode: `trap_entry` làm `csrrw sp, sscratch, sp` ⇒ `sp = trap stack top` (kernel),
//!   `sscratch = sp user`; frame lưu `x2` = sp user. Đúng một thanh ghi vừa đổi stack vừa giữ
//!   con trỏ kernel — không cần global/`tp` riêng, không lộ địa chỉ kernel cho U-mode.
//! - Trap từ S-mode (timer/PLIC/IPI) dùng **cùng** cơ chế: `sscratch` vẫn là đỉnh trap stack nên
//!   kernel stack bị ngắt được giữ trong `sscratch`, không phụ thuộc đang ở mode nào.
//! - Đường về S-mode khi `Exit` (`user_exit_resume`) khôi phục ngữ cảnh kernel từ
//!   `USER_KERNEL_CTX` (ra/sp/gp/tp/s0..s11) — **không** dùng stack user.
//!
//! Cơ chế này được chọn vì (1) không thêm CSR/global thứ hai, (2) chạy được cho cả trap từ S-mode
//! lẫn U-mode, (3) `sstatus.SPP` phân biệt nguồn trap nên chính sách syscall/fault tách sạch.
//!
//! # W^X khi nạp ELF
//!
//! `vifast-elf::parse` đã từ chối segment `W+X` và segment `PT_DYNAMIC/PT_INTERP/PT_TLS`; loader
//! ở đây kiểm lại `writable && executable` trước khi map và map **đúng quyền** segment:
//! `.text` → `U|R|X` (NX cho W), `.rodata` → `U|R|NX`, `.data` → `U|R|W|NX`; stack user →
//! `U|R|W|NX`, trang **guard** ngay dưới stack **không map**. Object layer (KT-52, D2-67) còn
//! chặn W^X toàn cục theo Frame nên một segment `R|W|X` không thể lọt.
//!
//! # Cách ly
//!
//! Root VSpace của user được chép 512 entry root kernel (test-only, KT-52) để kernel sống khi
//! `satp` trỏ root user; các leaf kernel giữ `U=0`, nên U-mode đọc `0x8020_0000` ⇒ **load page
//! fault** `scause=13` + `stval=0x8020_0000` (không phải đọc được). Fault U-mode là **fatal**:
//! log `# u-mode fault:` rồi `PANIC: u-mode fault …` + `DebugHalt(1)` (SBI SRST), không treo.
//!
//! Mọi `unsafe` ở tệp này thuộc danh sách đóng `kernel/vifast-plat-*` (D2-17): asm
//! `sret`/`sstatus`/`SUM`, đọc bộ nhớ user và ghi frame qua physmap.

use core::arch::{asm, global_asm};
use core::fmt::Write;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use vifast_elf::ElfError;
use vifast_plat_common::fault::{self, FaultKind, PteFacts, TrapFacts, FAULT_STATS};
use vifast_plat_common::mmu::{PTE_EXEC, PTE_READ, PTE_USER, PTE_WRITE};
use vifast_plat_common::obj::{CapId, MapFlags, ObjError, ObjKind};
use vifast_plat_common::trap::{
    tf_reg_offset, Exception, ExceptionKind, TrapFrame, REG_A0, REG_A7, REG_SP, SSTATUS_SIE,
    SSTATUS_SPIE, SSTATUS_SPP, TF_SEPC_OFFSET, TF_SSTATUS_OFFSET,
};
use vifast_plat_common::user::{
    self as abi, SyscallAction, UserMem, UserRange, UserSnapshot, UserTask, ERR_INVALID_ARGUMENT,
    ERR_NOT_MAPPED,
};
use vifast_plat_common::PlatformWriter;

use crate::obj as aobj;
use crate::{debug_put_byte, mmu, Riscv64Virt};

/// `sstatus.SUM` — S-mode được đọc/ghi trang `U=1` (privileged spec).
const SSTATUS_SUM: u64 = 1 << 18;

/// `e_machine` của RISC-V ELF64 (EM_RISCV = 243).
pub const EM_RISCV: u16 = 243;

/// Đáy cửa sổ user `[USER_BASE, USER_TOP)` — root index 1 của Sv39 (kernel root entry 1 rỗng).
pub const USER_BASE: u64 = 0x4000_0000;
/// Root index Sv39 của cửa sổ user (`0x4000_0000 >> 30 = 1`).
pub const USER_ROOT_INDEX: usize = (USER_BASE >> 30) as usize;
/// Đỉnh cửa sổ user — hết root index 1.
pub const USER_TOP: u64 = 0x8000_0000;
/// Đỉnh stack user (mọc xuống).
pub const USER_STACK_TOP: u64 = 0x4001_0000;
/// Cỡ stack user (8 trang 4 KiB).
pub const USER_STACK_BYTES: u64 = 0x8000;
/// Trang guard **không map** ngay dưới stack user.
pub const USER_STACK_GUARD_BOTTOM: u64 = USER_STACK_TOP - USER_STACK_BYTES - 0x1000;
/// Đỉnh trang guard (đáy stack).
pub const USER_STACK_GUARD_TOP: u64 = USER_STACK_TOP - USER_STACK_BYTES;

/// Số dải VA user được phép đọc trong `DebugPutStr`: mỗi `PT_LOAD` một dải + stack user.
const MAX_USER_RANGES: usize = vifast_elf::MAX_LOAD_SEGMENTS + 1;

/// VSpace của lượt U-mode gần nhất (để phân loại fault **sau khi** khôi phục root kernel).
static LAST_VSPACE: AtomicU32 = AtomicU32::new(u32::MAX);
/// ASID nhãn của VSpace đó — nhãn `FaultKind::Vm.vspace`.
static LAST_ASID: AtomicU64 = AtomicU64::new(0);
/// `1` ⇔ lượt U-mode hiện tại **được phép** fault để TAP kiểm `FaultKind` (test-only): handler
/// phân loại + ghi bằng chứng rồi về S-mode, **không** PANIC. Mặc định `0` = chính sách K3
/// (fault U-mode ⇒ log + halt). Đây là deviation **chỉ trong ảnh test**, ghi ở §19 docs.
static USER_FAULT_RECOVER: AtomicU32 = AtomicU32::new(0);

/// Bảng dải VA user **đã map** (ghi lúc nạp, đọc trong syscall) — thay cho `walk` PTE sau khi
/// `satp` trỏ root user (root user **không** có physmap/high alias, xem
/// [`crate::obj::copy_kernel_low_root`]). Mỗi dải là `[base, end)` đã làm tròn trang.
struct UserRanges {
    count: AtomicU32,
    base: [AtomicU64; MAX_USER_RANGES],
    end: [AtomicU64; MAX_USER_RANGES],
}

impl UserRanges {
    const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            base: [const { AtomicU64::new(0) }; MAX_USER_RANGES],
            end: [const { AtomicU64::new(0) }; MAX_USER_RANGES],
        }
    }

    fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }

    fn push(&self, base: u64, end: u64) -> bool {
        let index = self.count.load(Ordering::Relaxed) as usize;
        if index >= MAX_USER_RANGES || end <= base {
            return false;
        }
        self.base[index].store(base, Ordering::Relaxed);
        self.end[index].store(end, Ordering::Relaxed);
        self.count.store(index as u32 + 1, Ordering::Release);
        true
    }

    /// `[ptr, ptr+len)` nằm trọn trong **một** dải đã map (không vắt qua hai dải).
    fn contains(&self, ptr: u64, len: u64) -> bool {
        if len == 0 {
            return true;
        }
        let Some(last) = ptr.checked_add(len - 1) else {
            return false;
        };
        let count = self.count.load(Ordering::Acquire) as usize;
        for index in 0..count {
            let base = self.base[index].load(Ordering::Relaxed);
            let end = self.end[index].load(Ordering::Relaxed);
            if ptr >= base && last < end {
                return true;
            }
        }
        false
    }
}

/// Dải user đang map của tác vụ (một tác vụ/1 hart ở KT-53).
static USER_RANGES: UserRanges = UserRanges::new();

/// Trạng thái tác vụ user (TCB-like tối thiểu) — TAP đọc sau mỗi lượt.
pub static USER_TASK: UserTask = UserTask::new();

/// Lỗi nạp `init` — đủ để TAP in/log, **không panic** trong đường nạp.
#[derive(Clone, Copy, Debug)]
pub enum LoadError {
    /// ELF không hợp lệ (`vifast-elf`).
    Elf(ElfError),
    /// Retype/map qua object layer.
    Obj(ObjError),
    /// PA của Frame không nằm trong physmap (RAM 128 MiB).
    Physmap,
    /// Ảnh ELF chạm vùng guard/stack user.
    StackOverlap,
    /// Segment `W+X` (loader kiểm lại ngoài parser).
    Wx,
    /// File offset/độ dài segment ngoài buffer ELF.
    Image,
    /// `satp` không đổi được sang root user.
    Satp,
}

impl LoadError {
    /// Tên ổn định cho log/TAP.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Elf(_) => "Elf",
            Self::Obj(_) => "Obj",
            Self::Physmap => "Physmap",
            Self::StackOverlap => "StackOverlap",
            Self::Wx => "Wx",
            Self::Image => "Image",
            Self::Satp => "Satp",
        }
    }
}

/// Kết quả nạp `init` — địa chỉ/cờ **thật** của lần boot này.
#[derive(Clone, Copy, Debug)]
pub struct LoadedInit {
    /// Cap VSpace của user process.
    pub vspace: CapId,
    /// PA root page table.
    pub root: u64,
    /// ASID nhãn của VSpace.
    pub asid: u16,
    /// Entry point (`e_entry`).
    pub entry: u64,
    /// Số `PT_LOAD` đã map.
    pub segments: usize,
    /// Số trang segment executable.
    pub text_pages: usize,
    /// Số trang segment chỉ đọc.
    pub ro_pages: usize,
    /// Số trang segment ghi được.
    pub data_pages: usize,
    /// Số trang stack user.
    pub stack_pages: usize,
    /// Cuối vùng ảnh ELF (đã làm tròn trang).
    pub image_end: u64,
    /// Đáy trang guard dưới stack.
    pub stack_guard: u64,
    /// `check_objmem` ĐẠT sau khi nạp.
    pub check_ok: bool,
    /// Số trang leaf có **đồng thời** `W` và `X` (phải 0 — W^X đi lại bằng PTE thật).
    pub wx_pages: usize,
    /// Cờ leaf khớp quyền segment (`R|X` text, `R` rodata, `R|W` data, `U` mọi leaf user).
    pub flags_ok: bool,
    /// Nửa cao (kernel high alias + physmap) **không** map trong VSpace user.
    pub high_half_unmapped: bool,
    /// Trang guard ngay dưới stack user **không** map (walk invalid).
    pub guard_unmapped: bool,
}

/// Nạp `init` ELF vào một VSpace mới: parse `vifast-elf`, map segment **theo quyền**, map stack
/// user + để hở trang guard, rồi `check_objmem`.
///
/// `untyped` là cap Untyped chứa arena/PA cho VSpace/Frame (KT-52).
pub fn load_init(bytes: &[u8], untyped: CapId) -> Result<LoadedInit, LoadError> {
    let image = vifast_elf::parse(bytes, EM_RISCV, USER_BASE, USER_TOP).map_err(LoadError::Elf)?;
    // Ảnh không được chạm guard/stack user; guard nằm ngay dưới stack.
    let mut image_end = USER_BASE;
    for segment in image.segments() {
        let end = segment.memory_end().ok_or(LoadError::Image)?;
        if end > image_end {
            image_end = end;
        }
        if segment.writable() && segment.executable() {
            return Err(LoadError::Wx);
        }
    }
    if image_end > USER_STACK_GUARD_BOTTOM {
        return Err(LoadError::StackOverlap);
    }

    let m = aobj::machine();
    let vspace = m
        .retype(untyped, ObjKind::VSpace, 0)
        .map_err(LoadError::Obj)?;
    let root = m.vspace_root_pa(vspace).map_err(LoadError::Obj)?;
    let asid = m.vspace_asid(vspace).map_err(LoadError::Obj)?;
    // Chỉ chép **nửa thấp** root kernel (index 0..256 trừ cửa sổ user index 1) để kernel sống
    // khi `satp` trỏ root user; **không** chép nửa cao (high alias/physmap) — U-mode không có
    // đường PTE nào tới đó, và syscall không được dùng physmap sau khi đổi `satp`.
    if !aobj::copy_kernel_low_root(root, USER_ROOT_INDEX) {
        return Err(LoadError::Physmap);
    }
    USER_RANGES.reset();

    let mut text_pages = 0usize;
    let mut ro_pages = 0usize;
    let mut data_pages = 0usize;
    for segment in image.segments() {
        let flags = MapFlags {
            read: true,
            write: segment.writable(),
            exec: segment.executable(),
            user: true,
            device: false,
        };
        let first_page = segment.vaddr & !0xfff;
        let pages = segment.page_count();
        let file_end = segment.vaddr + segment.file_size as u64;
        for index in 0..pages {
            let va = first_page + (index as u64) * 4096;
            let frame = m
                .retype(untyped, ObjKind::Frame, 12)
                .map_err(LoadError::Obj)?;
            let pa = m.frame_pa(frame).map_err(LoadError::Obj)?;
            // Phần file rơi vào trang này (phần còn lại zero-fill).
            let page_end = va + 4096;
            let copy_start = va.max(segment.vaddr);
            let copy_end = page_end.min(file_end);
            let (file_start, copy_len) = if copy_end > copy_start {
                let offset = segment.file_offset as u64 + (copy_start - segment.vaddr);
                let len = copy_end - copy_start;
                let start = usize::try_from(offset).map_err(|_| LoadError::Image)?;
                let end = usize::try_from(offset + len).map_err(|_| LoadError::Image)?;
                if end > bytes.len() {
                    return Err(LoadError::Image);
                }
                (start, len as usize)
            } else {
                (0, 0)
            };
            let data = &bytes[file_start..file_start + copy_len];
            if !write_page(pa, data) {
                return Err(LoadError::Physmap);
            }
            m.map(vspace, va, frame, flags).map_err(LoadError::Obj)?;
            if index == 0 && !USER_RANGES.push(va, va + (pages as u64) * 4096) {
                return Err(LoadError::Image);
            }
            if segment.executable() {
                text_pages += 1;
            } else if segment.writable() {
                data_pages += 1;
            } else {
                ro_pages += 1;
            }
        }
    }

    // Stack user: 8 trang RW NX; trang ngay dưới (guard) **không map**.
    let mut stack_pages = 0usize;
    let mut stack_va = USER_STACK_GUARD_TOP;
    while stack_va < USER_STACK_TOP {
        let frame = m
            .retype(untyped, ObjKind::Frame, 12)
            .map_err(LoadError::Obj)?;
        let pa = m.frame_pa(frame).map_err(LoadError::Obj)?;
        if !write_page(pa, &[]) {
            return Err(LoadError::Physmap);
        }
        m.map(vspace, stack_va, frame, MapFlags::RW_NX_USER)
            .map_err(LoadError::Obj)?;
        if stack_pages == 0 && !USER_RANGES.push(USER_STACK_GUARD_TOP, USER_STACK_TOP) {
            return Err(LoadError::Image);
        }
        stack_pages += 1;
        stack_va += 4096;
    }

    // Đi lại PTE thật để chứng minh quyền leaf (không tin cờ lúc map): `W+X` = 0, text có X,
    // data có W, mọi leaf user có `U`; đây là bằng chứng `# init load: … wx=0 flags=1`.
    let mut wx_pages = 0usize;
    let mut flags_ok = true;
    for segment in image.segments() {
        let mut va = segment.vaddr & !0xfff;
        for _ in 0..segment.page_count() {
            let walk = m.walk(vspace, va).map_err(LoadError::Obj)?;
            let raw = walk.raw;
            let read = raw & PTE_READ != 0;
            let write = raw & PTE_WRITE != 0;
            let exec = raw & PTE_EXEC != 0;
            let user = raw & PTE_USER != 0;
            if write && exec {
                wx_pages += 1;
            }
            let shape_ok = walk.valid && walk.leaf && walk.level == 0;
            let expected = shape_ok
                && read
                && user
                && write == segment.writable()
                && exec == segment.executable();
            if !expected {
                flags_ok = false;
            }
            va += 4096;
        }
    }

    // Nửa cao phải **không** map: walk high alias kernel + physmap đều invalid.
    let high_alias_off = m
        .walk(vspace, mmu::KERNEL_HIGH_BASE)
        .map(|walk| !walk.valid)
        .unwrap_or(false);
    let physmap_off = m
        .walk(vspace, mmu::PHYSMAP_BASE + mmu::RAM_PHYS_BASE)
        .map(|walk| !walk.valid)
        .unwrap_or(false);
    let high_half_unmapped = high_alias_off && physmap_off;
    let guard_unmapped = m
        .walk(vspace, USER_STACK_GUARD_BOTTOM)
        .map(|walk| !walk.valid)
        .unwrap_or(false);

    let flush = m.drain_flushes(aobj::run_flush);
    let check_ok = m.check_objmem().is_ok();
    let _ = flush;
    Ok(LoadedInit {
        vspace,
        root,
        asid,
        entry: image.entry,
        segments: image.segment_count(),
        text_pages,
        ro_pages,
        data_pages,
        stack_pages,
        image_end,
        stack_guard: USER_STACK_GUARD_BOTTOM,
        check_ok,
        wx_pages,
        flags_ok,
        high_half_unmapped,
        guard_unmapped,
    })
}

/// Ghi một trang 4 KiB: zero toàn trang rồi chép `data` (≤ 4096) vào đầu trang.
///
/// # Safety
/// Logic: `pa` do object layer cấp trong cửa sổ PA của Untyped, `mmu::physmap_alias` chỉ trả
/// `Some` khi PA nằm trong RAM 128 MiB đã map — nên 4 KiB ghi là hợp lệ.
fn write_page(pa: u64, data: &[u8]) -> bool {
    if data.len() > 4096 {
        return false;
    }
    let Some(va) = mmu::physmap_alias(pa) else {
        return false;
    };
    // SAFETY: `va` là alias physmap của RAM thật (xem trên); ghi 4 KiB + chép `data`.
    unsafe {
        core::ptr::write_bytes(va as *mut u8, 0, 4096);
        if !data.is_empty() {
            core::ptr::copy_nonoverlapping(data.as_ptr(), va as *mut u8, data.len());
        }
    }
    true
}

/// Hạ `sret` xuống U-mode tại `entry` với `sp = USER_STACK_TOP`; trả về **chỉ khi** `init` gọi
/// `Exit` (đường `user_exit_resume` khôi phục ngữ cảnh kernel).
///
/// `mode` được đặt vào `a0` — chương trình `init` dùng để chọn lượt chạy (0 = syscall, 1 = cố
/// đọc địa chỉ kernel cho TAP cách ly).
pub fn enter(loaded: &LoadedInit, mode: u64) -> Result<UserSnapshot, LoadError> {
    enter_with(loaded, mode, false)
}

/// Như [`enter`] nhưng **cho phép một fault** U-mode (TAP `faultkind vm load`): handler phân loại
/// `FaultKind` + ghi bằng chứng rồi trả về S-mode. Chỉ ảnh test dùng; chính sách mặc định của K3
/// vẫn là fatal (`enter`).
pub fn enter_expect_fault(loaded: &LoadedInit, mode: u64) -> Result<UserSnapshot, LoadError> {
    enter_with(loaded, mode, true)
}

fn enter_with(
    loaded: &LoadedInit,
    mode: u64,
    expect_fault: bool,
) -> Result<UserSnapshot, LoadError> {
    LAST_VSPACE.store(u32::from(loaded.vspace.0), Ordering::Relaxed);
    LAST_ASID.store(u64::from(loaded.asid), Ordering::Relaxed);
    USER_FAULT_RECOVER.store(u32::from(expect_fault), Ordering::Relaxed);
    let mut tf = TrapFrame::zeroed();
    tf.x[REG_SP] = USER_STACK_TOP;
    // `a0` = mode; các thanh ghi khác 0 (không lộ địa chỉ kernel cho user).
    tf.x[REG_A0] = mode;
    tf.sepc = loaded.entry;
    let current = read_sstatus();
    // U-mode: SPP=0; tắt SIE/SPIE/SUM (không cho ngắt S-mode chen vào U-phase; syscall không
    // cần ngắt). Giữ nguyên UXL/FS và các bit WARL khác của sstatus.
    tf.sstatus = current & !(SSTATUS_SPP | SSTATUS_SPIE | SSTATUS_SIE | SSTATUS_SUM);
    USER_TASK.begin_run(tf.sstatus & SSTATUS_SPP != 0);
    USER_TASK.record_enter_sstatus(tf.sstatus);
    if !aobj::switch_root(loaded.root, loaded.asid) {
        let _ = aobj::restore_kernel_root();
        return Err(LoadError::Satp);
    }
    // SAFETY: trapframe hợp lệ (sepc trong segment X, sp trong stack đã map U); `user_enter`
    // lưu ngữ cảnh kernel, nạp CSR + 31 thanh ghi từ frame rồi `sret`. Chỉ quay lại qua
    // `user_exit_resume` (Exit) hoặc không quay lại (fault ⇒ halt).
    unsafe { user_enter(&tf as *const TrapFrame) };
    Ok(USER_TASK.snapshot())
}

/// Trap **từ U-mode** đã decode: syscall (`scause=8`) hoặc fault U-mode (mọi loại khác).
///
/// - `ecall` ⇒ dispatch ABI; `Return` ghi `a0/a1` + `sepc += 4`; `Exit` chuyển trapframe về
///   S-mode (`user_exit_resume`), **không** quay lại U-mode; `Halt` log + SRST.
/// - Fault ⇒ ghi `USER_TASK`/`TRAP_STATS`, in `# u-mode fault:` rồi `PANIC: u-mode fault …` +
///   `DebugHalt(1)` — không treo, không resume.
pub fn on_u_exception(tf: &mut TrapFrame, exc: &Exception) {
    let saved = *tf;
    if exc.kind == ExceptionKind::EnvironmentCallFromU {
        // Bằng chứng phần cứng: sstatus **ngay sau trap** từ U-mode (SPP=0, SIE=0, SPIE=0, SUM=0).
        USER_TASK.record_trap_sstatus(read_sstatus());
        USER_TASK.record_ecall(exc.from_supervisor);
        let nr = tf.reg(REG_A7);
        let args = [
            tf.arg(0),
            tf.arg(1),
            tf.arg(2),
            tf.arg(3),
            tf.arg(4),
            tf.arg(5),
        ];
        match abi::dispatch(nr, args, &mut UserMemAccess, &USER_TASK) {
            SyscallAction::Return { error, value } => {
                tf.set_arg(0, error);
                tf.set_arg(1, value);
                tf.advance_pc(4);
                crate::TRAP_STATS.record_exception(&saved, exc.kind, true);
            }
            SyscallAction::Exit { code } => {
                // KT-56/D2-56a: `Exit` là fault kind ABI (đi hàng gửi không Reply — D2-25).
                let kind = FaultKind::Exit { code };
                FAULT_STATS.record(kind);
                {
                    let mut w = PlatformWriter(&Riscv64Virt);
                    let _ = writeln!(w, "# u-mode exit: code={code} faultkind={kind}");
                }
                exit_to_kernel(tf);
                crate::TRAP_STATS.record_exception(&saved, exc.kind, true);
            }
            SyscallAction::Halt { code } => {
                {
                    let mut w = PlatformWriter(&Riscv64Virt);
                    let _ = writeln!(w, "# user debug_halt: code={code} nr=13");
                }
                crate::TRAP_STATS.record_exception(&saved, exc.kind, true);
                crate::debug_halt(code as usize)
            }
        }
        return;
    }

    // Fault U-mode: phân loại `FaultKind` từ **trapframe phần cứng** + PTE thật của VSpace
    // (sau khi khôi phục root kernel — root user không có physmap). Ghi bằng chứng TRƯỚC khi
    // quyết định fatal hay resume (deviation test-only: `enter_expect_fault`).
    let recover = USER_FAULT_RECOVER.swap(0, Ordering::SeqCst) == 1;
    USER_TASK.record_fault(saved.scause, saved.stval, saved.sepc);
    let kind = classify_u_fault(&saved, exc);
    if let Some(kind) = kind {
        FAULT_STATS.record(kind);
    }
    crate::TRAP_STATS.record_exception(&saved, exc.kind, recover);
    {
        let mut w = PlatformWriter(&Riscv64Virt);
        let _ = write!(
            w,
            "# u-mode fault: scause=0x{:x} stval=0x{:x} sepc=0x{:x} from_u=1 kernel_addr={} recover={} faultkind=",
            saved.scause,
            saved.stval,
            saved.sepc,
            is_kernel_addr(saved.stval) as u8,
            u8::from(recover)
        );
        match kind {
            Some(kind) => {
                let _ = writeln!(w, "{kind}");
            }
            None => {
                let _ = writeln!(w, "none");
            }
        }
    }
    if recover {
        // Test-only: về S-mode ngay sau trap (không resume instruction lỗi); caller TAP kiểm
        // `FaultKind` + `scause/stval` từ `FAULT_STATS`.
        exit_to_kernel(tf);
        return;
    }
    vifast_plat_common::trap::report_u_fault(&Riscv64Virt, exc, tf)
}

/// Phân loại fault U-mode **từ trapframe** (scause/stval/sepc) + PTE thật của VSpace đã nạp.
///
/// `restore_kernel_root` trước khi `walk`: sau `sret` xuống U-mode, root đang hoạt động là root
/// user **không** có physmap ([`crate::obj::copy_kernel_low_root`], KT-53), nên không thể đọc
/// PTE; khôi phục root kernel là an toàn vì fault này không quay lại instruction lỗi.
fn classify_u_fault(tf: &TrapFrame, exc: &Exception) -> Option<FaultKind> {
    #[cfg(feature = "ktest")]
    {
        let restored = aobj::restore_kernel_root();
        let vs = LAST_VSPACE.load(Ordering::Relaxed);
        let pte = if restored && vs != u32::MAX {
            aobj::machine()
                .walk(CapId(vs as u16), tf.stval)
                .ok()
                .map(|w| PteFacts::new(w.valid, w.leaf, w.raw))
        } else {
            None
        };
        let facts = TrapFacts {
            scause: exc.scause,
            stval: exc.stval,
            sepc: exc.sepc,
            from_user: true,
        };
        return fault::classify_trap(&facts, LAST_ASID.load(Ordering::Relaxed), pte);
    }
    #[cfg(not(feature = "ktest"))]
    {
        let _ = tf;
        // Không có physmap để walk trong chế độ không-ktest: vẫn phân loại theo **cùng bảng**
        // `scause` (12/13/15 ⇒ `Vm`, còn lại `UserException`), `pte=None` ⇒ `NotMapped`.
        let facts = TrapFacts {
            scause: exc.scause,
            stval: exc.stval,
            sepc: exc.sepc,
            from_user: true,
        };
        fault::classify_trap(&facts, 0, None)
    }
}

/// Địa chỉ trong vùng ảnh kernel (identity 2 MiB đầu) — dùng cho bằng chứng "kernel_addr" của
/// fault cách ly. Không phải kiểm an ninh (an ninh do PTE `U=0` quyết định).
fn is_kernel_addr(addr: u64) -> bool {
    addr >= mmu::KERNEL_PHYS_BASE && addr < mmu::KERNEL_PHYS_BASE + 0x0020_0000
}

/// Lớp đọc bộ nhớ user của syscall `DebugPutStr`: kiểm dải user + PTE (`U|R`), đọc bằng
/// `sstatus.SUM`, in **raw** từng byte ra UART.
struct UserMemAccess;

impl UserMem for UserMemAccess {
    fn put_str(&mut self, ptr: u64, len: u64) -> Result<u64, u64> {
        let range = UserRange::new(USER_BASE, USER_TOP);
        if !range.contains(ptr, len) {
            return Err(ERR_INVALID_ARGUMENT);
        }
        if len == 0 {
            return Ok(0);
        }
        // Sau khi `satp` trỏ root user, kernel **không** có physmap để `walk` PTE (xem
        // `obj::copy_kernel_low_root`). Loader ghi bảng dải `[base,end)` của **mọi** trang đã map
        // U|R (segment + stack); kiểm trọn dải trước khi đọc ⇒ con trỏ xấu nhận `NotMapped`,
        // không biến thành page fault S-mode (fatal).
        if !USER_RANGES.contains(ptr, len) {
            return Err(ERR_NOT_MAPPED);
        }
        // Đọc/ghép từng khối 64 B trong cửa sổ SUM, tắt SUM trước khi in (không giữ SUM lâu hơn
        // cần thiết và không để rò bit SUM sang đường kernel khác).
        let mut copied = 0u64;
        let mut buf = [0u8; 64];
        while copied < len {
            let n = core::cmp::min(64u64, len - copied) as usize;
            aobj::set_sum(true);
            // SAFETY: mọi trang đã kiểm leaf `U|R`; đọc 1 byte trong [0, 4096) của trang đó;
            // SUM bật nên S-mode đọc được trang U; tắt ngay sau vòng.
            unsafe {
                for (i, slot) in buf.iter_mut().enumerate().take(n) {
                    let addr = ptr + copied + i as u64;
                    *slot = core::ptr::read_volatile(addr as *const u8);
                }
            }
            aobj::set_sum(false);
            for byte in buf.iter().take(n) {
                debug_put_byte(*byte);
            }
            copied += n as u64;
        }
        Ok(len)
    }
}

/// Đọc `sstatus` (giữ UXL/FS; loader tự xoá SPP/SPIE/SIE/SUM).
fn read_sstatus() -> u64 {
    let value: u64;
    // SAFETY: chỉ đọc CSR `sstatus` của hart hiện tại.
    unsafe { asm!("csrr {}, sstatus", out(reg) value, options(nomem, nostack)) };
    value
}

/// Chuyển trapframe về S-mode tại [`user_exit_resume`] — **không** quay lại U-mode sau `Exit`.
///
/// Ghi CSR `sstatus.SPP=1` (vì asm resume của `trap_entry` **không** ghi `sstatus` — P2a KT-45),
/// xoá SPIE/SIE/SUM; đặt `sepc`/`sp` kernel để asm resume `sret` về S-mode.
fn exit_to_kernel(tf: &mut TrapFrame) {
    tf.sstatus = (tf.sstatus | SSTATUS_SPP) & !(SSTATUS_SPIE | SSTATUS_SIE | SSTATUS_SUM);
    let resume: unsafe extern "C" fn() = user_exit_resume;
    tf.sepc = resume as usize as u64;
    tf.x[REG_SP] = kernel_ctx_sp();
    // SAFETY: chỉ sửa bit CSR `sstatus`; SIE đang 0 trong trap handler nên sret chưa bật ngắt.
    unsafe {
        asm!(
            "csrs sstatus, {spp}",
            "csrc sstatus, {clear}",
            spp = in(reg) SSTATUS_SPP,
            clear = in(reg) (SSTATUS_SPIE | SSTATUS_SIE | SSTATUS_SUM),
            options(nostack, preserves_flags),
        );
    }
}

/// `sp` kernel đã lưu lúc vào U-mode (dùng làm `sp` tạm trước khi `user_exit_resume` nạp lại).
fn kernel_ctx_sp() -> u64 {
    // SAFETY: `USER_KERNEL_CTX` do asm định nghĩa trong `.bss`; chỉ đọc phần tử `sp`. Một hart ở
    // KT-53 nên không có ghi đồng thời.
    unsafe { USER_KERNEL_CTX[1] }
}

// Ngữ cảnh kernel (ra/sp/gp/tp/s0..s11) được `user_enter` lưu và `user_exit_resume` khôi phục.
extern "C" {
    static USER_KERNEL_CTX: [u64; 16];
    /// Hạ `sret` xuống U-mode theo trapframe tại `a0`; chỉ quay lại qua [`user_exit_resume`].
    fn user_enter(frame: *const TrapFrame);
    /// Đường về S-mode sau `Exit`: khôi phục ngữ cảnh kernel rồi `ret` về caller Rust.
    fn user_exit_resume();
}

// Trapframe x0..x31 + sepc/sstatus/scause/stval (288 B) — asm nạp `sepc/sstatus` và 31 thanh ghi
// (x2 = sp CUỐI CÙNG qua `ld sp, {x2}(a0)`), phần còn lại của frame không bị asm đọc.
global_asm!(
    ".section .bss",
    ".balign 8",
    ".globl USER_KERNEL_CTX",
    "USER_KERNEL_CTX:",
    ".space 128",
    // ------------------------------------------------------------------ vào U-mode
    ".section .text",
    ".balign 4",
    ".globl user_enter",
    "user_enter:",
    // a0 = &TrapFrame. Lưu ngữ cảnh kernel (ra/sp/gp/tp/s0..s11) vào ctx tĩnh trước khi đụng
    // bất cứ thanh ghi callee-saved nào.
    "la t0, USER_KERNEL_CTX",
    "sd ra, 0(t0)",
    "sd sp, 8(t0)",
    "sd gp, 16(t0)",
    "sd tp, 24(t0)",
    "sd s0, 32(t0)",
    "sd s1, 40(t0)",
    "sd s2, 48(t0)",
    "sd s3, 56(t0)",
    "sd s4, 64(t0)",
    "sd s5, 72(t0)",
    "sd s6, 80(t0)",
    "sd s7, 88(t0)",
    "sd s8, 96(t0)",
    "sd s9, 104(t0)",
    "sd s10, 112(t0)",
    "sd s11, 120(t0)",
    // CSR từ frame (a0 làm base). t0 (x5) được khôi phục sau các lệnh csrw.
    "ld t0, {sstatus}(a0)",
    "csrw sstatus, t0",
    "ld t0, {sepc}(a0)",
    "csrw sepc, t0",
    // Khôi phục 31 thanh ghi từ frame; x2 (sp) và x10 (a0) cuối cùng.
    "ld ra, {x1}(a0)",
    "ld gp, {x3}(a0)",
    "ld tp, {x4}(a0)",
    "ld t0, {x5}(a0)",
    "ld t1, {x6}(a0)",
    "ld t2, {x7}(a0)",
    "ld s0, {x8}(a0)",
    "ld s1, {x9}(a0)",
    "ld a1, {x11}(a0)",
    "ld a2, {x12}(a0)",
    "ld a3, {x13}(a0)",
    "ld a4, {x14}(a0)",
    "ld a5, {x15}(a0)",
    "ld a6, {x16}(a0)",
    "ld a7, {x17}(a0)",
    "ld s2, {x18}(a0)",
    "ld s3, {x19}(a0)",
    "ld s4, {x20}(a0)",
    "ld s5, {x21}(a0)",
    "ld s6, {x22}(a0)",
    "ld s7, {x23}(a0)",
    "ld s8, {x24}(a0)",
    "ld s9, {x25}(a0)",
    "ld s10, {x26}(a0)",
    "ld s11, {x27}(a0)",
    "ld t3, {x28}(a0)",
    "ld t4, {x29}(a0)",
    "ld t5, {x30}(a0)",
    "ld t6, {x31}(a0)",
    "ld sp, {x2}(a0)",
    "ld a0, {x10}(a0)",
    "sret",
    // ------------------------------------------------------------------ về S-mode sau Exit
    ".balign 4",
    ".globl user_exit_resume",
    "user_exit_resume:",
    // `sscratch` vẫn là đỉnh trap stack per-hart (trap_entry vừa đặt lại); khôi phục ngữ cảnh
    // kernel rồi `ret` về caller Rust của `user_enter` (a0/ra không cần đúng ABI cho caller).
    "la t0, USER_KERNEL_CTX",
    "ld ra, 0(t0)",
    "ld gp, 16(t0)",
    "ld tp, 24(t0)",
    "ld s0, 32(t0)",
    "ld s1, 40(t0)",
    "ld s2, 48(t0)",
    "ld s3, 56(t0)",
    "ld s4, 64(t0)",
    "ld s5, 72(t0)",
    "ld s6, 80(t0)",
    "ld s7, 88(t0)",
    "ld s8, 96(t0)",
    "ld s9, 104(t0)",
    "ld s10, 112(t0)",
    "ld s11, 120(t0)",
    "ld sp, 8(t0)",
    "ret",
    sstatus = const TF_SSTATUS_OFFSET,
    sepc = const TF_SEPC_OFFSET,
    x1 = const tf_reg_offset(1),
    x2 = const tf_reg_offset(2),
    x3 = const tf_reg_offset(3),
    x4 = const tf_reg_offset(4),
    x5 = const tf_reg_offset(5),
    x6 = const tf_reg_offset(6),
    x7 = const tf_reg_offset(7),
    x8 = const tf_reg_offset(8),
    x9 = const tf_reg_offset(9),
    x10 = const tf_reg_offset(10),
    x11 = const tf_reg_offset(11),
    x12 = const tf_reg_offset(12),
    x13 = const tf_reg_offset(13),
    x14 = const tf_reg_offset(14),
    x15 = const tf_reg_offset(15),
    x16 = const tf_reg_offset(16),
    x17 = const tf_reg_offset(17),
    x18 = const tf_reg_offset(18),
    x19 = const tf_reg_offset(19),
    x20 = const tf_reg_offset(20),
    x21 = const tf_reg_offset(21),
    x22 = const tf_reg_offset(22),
    x23 = const tf_reg_offset(23),
    x24 = const tf_reg_offset(24),
    x25 = const tf_reg_offset(25),
    x26 = const tf_reg_offset(26),
    x27 = const tf_reg_offset(27),
    x28 = const tf_reg_offset(28),
    x29 = const tf_reg_offset(29),
    x30 = const tf_reg_offset(30),
    x31 = const tf_reg_offset(31),
);
