// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-66 — stack dispatch syscall per-hart + canary + nước thấp** (tách khỏi `user.rs` để giữ
//! luật R1 ≤ 1.500 dòng/tệp).
//!
//! Mỗi hart online có **một** block 32 KiB trong **vùng NOLOAD per-hart** (`__hart_area_base`);
//! record: `guard 4 KiB | kernel stack 64 KiB | trap stack 16 KiB | syscall stack 32 KiB`
//! (`vifast_plat_common::smp::HART_STRIDE = 0x1_D000`). Không nằm trong `.data+.bss` (D2-72 Q5
//! báo NOLOAD riêng) và **không** ở arena/PA thuộc physmap: khi `satp` = root user, nửa cao
//! (physmap/high alias) **không** được chép vào root user, nên stack phải ở vùng identity map
//! **thấp** — đúng vùng NOLOAD per-hart (bằng chứng kiến trúc: `# mmu layout: … none_physmap=1`).
//!
//! Block được **sơn** `0xAA` + hai word **canary** ở đáy (giá trị + nghịch đảo); canary kiểm sau
//! **mỗi** lượt dispatch và lúc in evidence ⇒ tràn/underflow ⇒ `canary=1` (đỏ, không im lặng).
//! `low` = nước thấp **thật** của lần chạy (quét từ đáy tới word đầu tiên khác mẫu sơn).

use core::fmt::Write;

use vifast_plat_common::smp::MAX_HARTS;
use vifast_plat_common::PlatformWriter;

use crate::{mmu, Riscv64Virt};

/// Cỡ stack dispatch syscall. Trap stack per-hart chỉ **16 KiB** (bất biến KT-49); đường IPC
/// `NBRecv`/`Send` với `Message` 64 MR làm khung dispatch sâu hơn 16 KiB ⇒ tràn xuống kernel
/// stack và **ghi đè biến cục bộ** của scheduler (đã quan sát: `LoadedInit`/`steps` bị zero,
/// `ret` về 0).
///
/// **KT-66 — per-hart (thay `SYSCALL_STACK` 32 KiB tĩnh của KT-65):** mỗi hart online nhận
/// **một** block 32 KiB **riêng** trong **vùng NOLOAD per-hart** (`__hart_area_base`), không còn
/// trong `.data+.bss` của ảnh và **không** dùng chung giữa các hart. Mỗi block có **canary** hai
/// word ở đáy (giá trị + nghịch đảo) và được **sơn** `0xAA` để đo nước thấp thật (`low`), nên
/// tràn stack ghi đè canary ⇒ `canary=1` (evidence/TAP đỏ, không im lặng).
///
/// **Vì sao NOLOAD per-hart, không phải Frame từ arena:** `Frame` của object layer nằm trong cửa
/// sổ PA `0x8200_0000` — chỉ chạm được qua **physmap/nửa cao**, mà `copy_kernel_low_root` **không**
/// chép root index ≥ 256 vào root VSpace user. Khi trap từ U-mode, `satp` vẫn là root user ⇒
/// dispatch trên stack ở arena **store page fault thật** (`stval=0xffffffc082007ff8` — quan sát
/// trong phiên, log thô bị ghi đè khi chuyển NOLOAD nên **không** dẫn tệp; bằng chứng kiến trúc là
/// `# mmu layout: … none_physmap=1`) — phát hiện của KT-66, không phải giả thuyết. Vùng
/// NOLOAD per-hart thì identity-mapped **thấp** (root index 2, có trong root user) nên dùng được
/// đúng chỗ; D2-72 Q5 đã quy định báo NOLOAD **riêng**, không cộng vào trần `.data+.bss`.
///
/// Chỉ `static` còn lại là bảng mô tả `[u64; MAX_HARTS]` (~200 B) + scratch dispatch một-lượt.
pub const SYSCALL_STACK_BYTES: usize = 32 * 1024;
/// Block mỗi hart = **đúng** 32 KiB = `HART_SYSCALL_STACK_BYTES`; ngay trên đỉnh là guard page của
/// hart kế (NONE — va vào là fault thật), dưới đáy là trap stack của cùng hart nên **canary** ở
/// đáy là lớp bắt tràn/underflow tường minh (sơn 32 KiB, không sơn quá record).
pub const SYSCALL_STACK_BLOCK: usize = SYSCALL_STACK_BYTES;
/// Mẫu sơn vùng chưa dùng để đo nước thấp (`low`).
const STACK_PAINT: u8 = 0xAA;
/// Canary đáy stack: word0 = `STACK_CANARY`, word1 = `!STACK_CANARY` (bắt ghi lấn cả hai chiều).
const STACK_CANARY: u64 = 0x4b54_3636_5354_4b21; // "KT66STK!"

/// Đáy block (VA physmap) của từng hart; `0` = chưa cấp.
static mut STACK_BASES: [u64; MAX_HARTS] = [0; MAX_HARTS];
/// Đỉnh stack (sp khởi đầu) từng hart.
static mut STACK_TOPS: [u64; MAX_HARTS] = [0; MAX_HARTS];
/// Số lần canary bị phá (phải **0** khi xanh).
static mut STACK_CANARY_BAD: [u32; MAX_HARTS] = [0; MAX_HARTS];
/// Số hart đã cấp stack (0 = chưa `init_syscall_stacks`).
static mut STACK_HARTS: usize = 0;

/// Cấp (khởi tạo) stack dispatch **per-hart** trong vùng NOLOAD: sơn + canary từng block, ghi
/// bảng mô tả. Trả `false` nếu `hart_count` = 0 (không có gì để cấp).
///
/// Gọi **một lần** trên hart boot trước lượt U-mode đầu (ảnh test U-mode). Không cấp phát động
/// trong trap: mọi hart online có sẵn block riêng (32 KiB) trước khi dispatch.
pub fn init_syscall_stacks() -> bool {
    let harts = crate::smp::hart_count().min(MAX_HARTS);
    if harts == 0 {
        return false;
    }
    // SAFETY: chỉ hart boot gọi hàm này một lần (idempotent); đọc `usize` không tạo tham chiếu.
    if unsafe { STACK_HARTS } != 0 {
        return true;
    }
    let mut h = 0;
    while h < harts {
        // Mỗi hart một record NOLOAD riêng ⇒ `base` phân biệt (smp 2/4 **không** dùng chung).
        // Bản đỏ-trước (dùng chung block hart 0 ⇒ `distinct=0`) ở
        // `JOURNAL/kt66-red-before-stack.log`.
        let (block, top) = mmu::syscall_stack_range_of(h);
        // SAFETY: block nằm trong vùng NOLOAD per-hart đã map identity RW NX (mmu::init), mỗi
        // hart một record; ghi 36 KiB đệm + canary 2 word đầu.
        unsafe {
            core::ptr::write_bytes(block as *mut u8, STACK_PAINT, SYSCALL_STACK_BLOCK);
            core::ptr::write_volatile(block as *mut u64, STACK_CANARY);
            core::ptr::write_volatile((block + 8) as *mut u64, !STACK_CANARY);
            STACK_BASES[h] = block;
            STACK_TOPS[h] = top;
            STACK_CANARY_BAD[h] = 0;
            STACK_HARTS = harts;
        }
        h += 1;
    }
    true
}

/// Kiểm canary đáy stack của `hart`.
///
/// **Fail-closed (M1 reviewer KT-66):** canary hỏng nghĩa là stack đã tràn xuống `.trap_stack` —
/// kernel **không** được chạy tiếp trên trạng thái hỏng (đúng cơ chế sự cố KT-65). In evidence
/// `canary=1` rồi `PANIC` + halt ngay; không chỉ cộng biến đếm rồi đi tiếp.
pub fn check_stack_canary(hart: usize) {
    let h = hart.min(MAX_HARTS - 1);
    // SAFETY: chỉ đọc canary/ghi bộ đếm của block đã cấp; hart boot gọi sau mỗi lượt dispatch.
    unsafe {
        let base = STACK_BASES[h];
        if base == 0 {
            return;
        }
        if core::ptr::read_volatile(base as *const u64) != STACK_CANARY
            || core::ptr::read_volatile((base + 8) as *const u64) != !STACK_CANARY
        {
            STACK_CANARY_BAD[h] = STACK_CANARY_BAD[h].saturating_add(1);
            let mut w = PlatformWriter(&Riscv64Virt);
            let _ = writeln!(
                w,
                "# stack: canary=1 hart={h} (tran/underflow — dung dispatch)"
            );
            vifast_plat_common::Platform::put_str(
                &Riscv64Virt,
                "PANIC: stack syscall canary hong (tran stack, kernel dung)\n",
            );
            crate::debug_halt(1)
        }
    }
}

/// Nước thấp (địa chỉ thấp nhất đã bị dùng) của block hart `hart`: quét từ trên canary lên
/// tới word đầu tiên **khác** mẫu sơn; chưa dùng ⇒ trả `top`.
fn scan_stack_low(hart: usize) -> u64 {
    unsafe {
        let base = STACK_BASES[hart.min(MAX_HARTS - 1)];
        let top = STACK_TOPS[hart.min(MAX_HARTS - 1)];
        if base == 0 {
            return 0;
        }
        let paint = u64::from_ne_bytes([STACK_PAINT; 8]);
        let mut p = base + 16;
        while p < top {
            if core::ptr::read_volatile(p as *const u64) != paint {
                return p;
            }
            p += 8;
        }
        top
    }
}

/// Số hart đã cấp stack (0 ⇒ `init_syscall_stacks` chưa chạy).
pub fn stack_harts() -> usize {
    // SAFETY: đọc một `usize` do hart boot ghi một lần trước mọi lượt U-mode.
    unsafe { STACK_HARTS }
}

/// Block NOLOAD đã cấp cho hart: `(base, top)` — `None` nếu chưa cấp.
pub fn stack_block(hart: usize) -> Option<(u64, u64)> {
    // SAFETY: như `stack_harts` (bảng chỉ ghi lúc init).
    unsafe {
        if hart >= STACK_HARTS {
            return None;
        }
        let base = STACK_BASES[hart];
        (base != 0).then(|| (base, STACK_TOPS[hart]))
    }
}

/// Bằng chứng `# stack:` — một dòng tổng + một dòng mỗi hart online:
/// `# stack: noload=1 block=32768 harts=N distinct=1 canary=0 base=0x…`
/// `# stack hart=0 base=0x… top=0x… low=0x… used=… canary=0`
///
/// `distinct=1` ⇔ **mọi cặp** hart có `base` khác nhau (smp 2/4 không dùng chung stack).
pub fn print_stack_evidence() {
    let harts = stack_harts();
    // Copy base ra mảng cục bộ rồi so **mọi cặp** (i<j) — không chỉ so với hart 0: smp 3/4 mà
    // hart 1 và 2 dùng chung vẫn phải bị bắt (reviewer KT-66 mục D4).
    let mut bases = [0u64; MAX_HARTS];
    let mut bad = 0u32;
    let mut i = 0;
    while i < harts {
        check_stack_canary(i);
        // SAFETY: bảng mô tả chỉ ghi lúc init; hart boot đọc sau khi các lượt U-mode xong.
        unsafe {
            bad = bad.saturating_add(STACK_CANARY_BAD[i]);
            bases[i] = STACK_BASES[i];
        }
        i += 1;
    }
    let mut distinct = true; // 1 hart ⇒ không có cặp nào dùng chung (bất biến vacuous)
    let mut i = 0;
    while i < harts {
        let mut j = i + 1;
        while j < harts {
            if bases[i] == bases[j] {
                distinct = false;
            }
            j += 1;
        }
        i += 1;
    }
    let mut w = PlatformWriter(&Riscv64Virt);
    // Base của hart 0 — bằng chứng vùng NOLOAD per-hart (không phải `.bss`/arena).
    let base0 = unsafe {
        if harts > 0 {
            STACK_BASES[0]
        } else {
            0
        }
    };
    let _ = writeln!(
        w,
        "# stack: noload=1 block={} harts={harts} distinct={} canary={} base=0x{base0:x}",
        SYSCALL_STACK_BLOCK,
        u8::from(distinct),
        u8::from(bad != 0)
    );
    let mut h = 0;
    while h < harts {
        let (base, top) = stack_block(h).unwrap_or_default();
        let low = scan_stack_low(h);
        let canary_bad = unsafe { STACK_CANARY_BAD[h] };
        let _ = writeln!(
            w,
            "# stack hart={h} base=0x{base:x} top=0x{top:x} low=0x{low:x} used={} canary={}",
            top.saturating_sub(low),
            u8::from(canary_bad != 0)
        );
        h += 1;
    }
}

/// Đỉnh block (sp khởi đầu) của hart — `0` nếu chưa cấp.
pub fn syscall_stack_top(hart: usize) -> u64 {
    // SAFETY: bảng mô tả chỉ ghi lúc init (hart boot) trước mọi lượt U-mode.
    unsafe {
        if hart >= STACK_HARTS {
            return 0;
        }
        STACK_TOPS[hart]
    }
}
