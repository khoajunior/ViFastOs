// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ảnh **test U-mode** riscv64 (KT-53) — bin riêng, feature `ktest`, nạp qua OpenSBI.
//!
//! Vì sao ảnh riêng: mỗi boot chỉ kết thúc được **một** lần bằng PANIC có chủ đích. Ảnh
//! `vifast-kbin-ktest` đã dùng lần kết thúc đó cho trap lồng (KT-45 P2b), ảnh guard dùng cho
//! store fault (KT-46) — ảnh này dùng cho **fault U-mode** (`scause=13` khi U-mode đọc địa chỉ
//! kernel), nên 5 TAP U-mode chạy trọn trong một boot rồi kết thúc bằng `PANIC: u-mode fault`.
//!
//! TAP (5 test):
//!
//! 1. `ok 1 - init u-mode` — nạp `init` ELF bằng `vifast-elf` (parse + map `PT_LOAD` theo quyền,
//!    **W^X**: `.text` `U|R|X`, `.rodata` `U|R|NX`, `.data` `U|R|W|NX`), stack user 8 trang +
//!    guard hở; `sret` xuống U-mode (`sstatus.SPP=0`, `sscratch` giữ trap stack per-hart), `init`
//!    chạy thật và in marker qua `ecall`.
//! 2. `ok 2 - syscall debug_put_str` — 2 lần `DebugPutStr` (12) in **đúng byte user**
//!    (`U: init hello from u-mode` / `U: bad syscall err=5 ok`), 50 byte.
//! 3. `ok 3 - syscall exit` — `Exit(42)` (14): tác vụ sang `Exited`, lưu `exit_code=42`, **không**
//!    quay lại U-mode (entries=1); đường về S-mode qua `user_exit_resume`.
//! 4. `ok 4 - syscall la ma loi` — 2 `ecall` ngoài dải ABI (EID SBI `0x53525354`, số 99) ⇒
//!    `InvalidArgument` (5), **không panic**; `init` kiểm `a0==5` rồi mới `Exit(42)`.
//! 5. `ok 5 - u-mode kernel read fault` — in `ok 5` **trước**, rồi U-mode đọc `0x80200000` (leaf
//!    kernel `U=0`) ⇒ **load page fault** `scause=13`, `stval=0x80200000`; handler ghi
//!    `# u-mode fault:` rồi `PANIC: u-mode fault …` + `DebugHalt(1)`. Runner đòi đúng dòng PANIC
//!    này và **chỉ** dòng này.
//!
//! Chương trình `init` do `build.rs` biên dịch từ `init/init_user.rs` (+ `.S`) thành ELF64 tại
//! `0x4000_0000` rồi nhúng qua `include_bytes!` — không loader động, không reloc, không cần
//! toolchain ngoài Rust.
#![no_std]
#![no_main]

use core::fmt::Write;

use vifast_plat_common::user::TaskState;
use vifast_plat_common::{PanicHandler, Platform, PlatformWriter};
use vifast_plat_riscv64::mmu;
use vifast_plat_riscv64::user::{self, USER_BASE, USER_STACK_GUARD_BOTTOM};
use vifast_plat_riscv64::{read_trap_vector, trap_vector, Riscv64Virt};

static PLATFORM: Riscv64Virt = Riscv64Virt;
static PANIC: PanicHandler<Riscv64Virt> = PanicHandler::new(Riscv64Virt);

/// ELF64 `init` do `build.rs` sinh (`init/init_user.rs` + `.S` + `.ld`), nhúng vào ảnh test.
static INIT_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.elf"));

/// Số byte của 2 marker trong `init` — `init` kiểm `a1` sau `DebugPutStr`, host test/runner kiểm
/// lại tổng byte (26 + 24 = 50).
const MARKER_BYTES: u64 = 51;

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    PLATFORM.put_str("TAP version 13\n1..5\n");
    PLATFORM.set_trap_vector(trap_vector());
    let stvec = read_trap_vector();
    // Bảng trang + arena/guard trước (mmu::init bật satp); ASID lease rồi object layer.
    let mmu = mmu::init();
    let _tlb = vifast_plat_riscv64::tlb::init();
    let obj = vifast_plat_riscv64::obj::init();
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# user image: hart={} stvec=0x{:x} direct={} satp=0x{:x} mode={} root=0x{:x} obj_arena=0x{:x}-0x{:x} untyped={}",
            PLATFORM.hart_id(),
            stvec,
            (stvec & 0b11 == 0) as u8,
            mmu.satp,
            mmu.satp >> 60,
            mmu.root,
            obj.arena_base,
            obj.arena_end,
            obj.untypeds
        );
        let _ = writeln!(
            w,
            "# init elf: bytes={} entry=0x40000000 user=[0x{:x},0x{:x}) stack_guard=0x{:x}",
            INIT_ELF.len(),
            USER_BASE,
            vifast_plat_riscv64::user::USER_TOP,
            USER_STACK_GUARD_BOTTOM
        );
    }

    let loaded = match user::load_init(INIT_ELF, obj.untyped0) {
        Ok(loaded) => loaded,
        Err(error) => {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 1 - init u-mode: nap ELF that bai kind={} ({:?})",
                error.name(),
                error
            );
            PLATFORM.put_str("# ktest halt\n");
            PLATFORM.halt(1)
        }
    };
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# init load: elf=1 entry=0x{:x} segs={} text={} ro={} data={} stack={} image_end=0x{:x} guard=0x{:x} guard_none={} wx={} flags={} high={} root=0x{:x} asid={} check={}",
            loaded.entry,
            loaded.segments,
            loaded.text_pages,
            loaded.ro_pages,
            loaded.data_pages,
            loaded.stack_pages,
            loaded.image_end,
            loaded.stack_guard,
            loaded.guard_unmapped as u8,
            loaded.wx_pages,
            loaded.flags_ok as u8,
            loaded.high_half_unmapped as u8,
            loaded.root,
            loaded.asid,
            loaded.check_ok as u8
        );
    }
    if loaded.entry & 0xfff != 0
        || loaded.segments != 3
        || loaded.text_pages != 1
        || loaded.ro_pages != 1
        || loaded.data_pages != 1
        || loaded.stack_pages != 8
        || !loaded.guard_unmapped
        || loaded.wx_pages != 0
        || !loaded.flags_ok
        || !loaded.high_half_unmapped
        || !loaded.check_ok
    {
        PLATFORM.put_str("not ok 1 - init u-mode: plan ELF/mapping khong dung\n");
        PLATFORM.put_str("# ktest halt\n");
        PLATFORM.halt(1)
    }

    // ---------------------------------------------------------------- 1..4) lượt U-mode chuẩn
    let snap = match user::enter(&loaded, 0) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 1 - init u-mode: enter that bai kind={} ({:?})",
                error.name(),
                error
            );
            PLATFORM.put_str("# ktest halt\n");
            PLATFORM.halt(1)
        }
    };
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# init run: entry=0x{:x} u_mode=1 sret_spp={} ecall_spp={} entries={} syscalls={} exits={} exit={}",
            loaded.entry,
            snap.enter_spp,
            snap.ecall_spp,
            snap.entries,
            snap.syscalls,
            snap.exits,
            snap.exit_code
        );
    }
    let mut fails = 0usize;

    // TAP 1: đã hạ sret xuống U-mode thật, mọi ecall đều từ U-mode (SPP=0), và đã exit sạch.
    // Bằng chứng `sstatus` hai đầu: giá trị ghi CSR lúc vào U (SUM/SIE/SPIE/SPP=0) và giá trị
    // phần cứng để lại ở trap `ecall` đầu tiên (SPP=0, SIE=0, SPIE=0, SUM=0).
    const SSTATUS_MASK: u64 = (1 << 8) | (1 << 5) | (1 << 1) | (1 << 18);
    let sstatus_ok = snap.enter_sstatus & SSTATUS_MASK == 0
        && snap.trap_sstatus & SSTATUS_MASK == 0
        && snap.trap_sstatus >> 32 & 0x3 == 2; // UXL=2 (64-bit) — hardware thật
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# u sstatus: enter=0x{:x} trap=0x{:x} spp=0 spie=0 sie=0 sum=0 uxl=2",
            snap.enter_sstatus, snap.trap_sstatus
        );
    }
    let ok1 = snap.entries == 1
        && snap.enter_spp == 0
        && snap.ecalls == 5
        && snap.ecall_spp == 0
        && snap.syscalls == 5
        && snap.state == TaskState::Exited
        && snap.exits == 1
        && sstatus_ok;
    if ok1 {
        PLATFORM.put_str("ok 1 - init u-mode\n");
    } else {
        PLATFORM.put_str("not ok 1 - init u-mode: entries/spp/ecall khong dung\n");
        fails += 1;
    }

    // TAP 2: DebugPutStr in đúng 2 marker, 50 byte, do chính byte user quyết định; cửa sổ đọc
    // user dùng `sstatus.SUM` phải đóng lại (`SUM=0`) sau khi in.
    let sum_off = vifast_plat_riscv64::obj::sum_off();
    let ok2 = snap.put_calls == 2 && snap.put_bytes == MARKER_BYTES && sum_off;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# user put_str: calls={} bytes={} sum_off={}",
            snap.put_calls, snap.put_bytes, sum_off as u8
        );
    }
    if ok2 {
        PLATFORM.put_str("ok 2 - syscall debug_put_str\n");
    } else {
        PLATFORM.put_str("not ok 2 - syscall debug_put_str: so lan/byte khong dung\n");
        fails += 1;
    }

    // TAP 3: Exit(42) — Exited, đúng mã, không quay lại U-mode.
    let ok3 = snap.state == TaskState::Exited
        && snap.exits == 1
        && snap.exit_code == 42
        && snap.entries == 1;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# user exit: code={} state={} exits={}",
            snap.exit_code,
            snap.state.name(),
            snap.exits
        );
    }
    if ok3 {
        PLATFORM.put_str("ok 3 - syscall exit\n");
    } else {
        PLATFORM.put_str("not ok 3 - syscall exit: ma/trang thai khong dung\n");
        fails += 1;
    }

    // TAP 4: syscall lạ (ngoài dải, gồm EID SBI) trả mã lỗi ABI 5, không panic; `init` chỉ
    // Exit(42) khi cả hai lần đều thấy a0=5.
    let ok4 = snap.bad_calls == 2
        && snap.last_bad_nr == 99
        && snap.last_bad_error == 19
        && snap.exit_code == 42;
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# user badsys: nr={} err={} calls={} panic=0",
            snap.last_bad_nr, snap.last_bad_error, snap.bad_calls
        );
    }
    if ok4 {
        PLATFORM.put_str("ok 4 - syscall la ma loi\n");
    } else {
        PLATFORM.put_str("not ok 4 - syscall la ma loi: ma loi/so lan khong dung\n");
        fails += 1;
    }

    if fails > 0 {
        PLATFORM.put_str("# ktest halt\n");
        PLATFORM.halt(1)
    }

    // ------------------------------------------------ 5) U-mode đọc địa chỉ kernel (fatal)
    // In `ok 5` TRƯỚC khi kích hoạt: boot kết thúc bằng `PANIC: u-mode fault …` + SBI SRST,
    // không có `# ktest halt`. Runner đòi đúng dòng PANIC này và chỉ dòng này.
    PLATFORM.put_str("ok 5 - u-mode kernel read fault\n");
    {
        let mut w = PlatformWriter(&PLATFORM);
        let _ = writeln!(
            w,
            "# u-mode read: hart={} addr=0x80200000 expect=scause=0xd stval=0x80200000",
            PLATFORM.hart_id()
        );
    }
    match user::enter(&loaded, 1) {
        // Không được tới đây: đọc địa chỉ kernel mà không fault nghĩa là leaf kernel bị `U=1`.
        Ok(_) => {
            PLATFORM.put_str("not ok 5 - u-mode kernel read fault: khong fault\n");
            PLATFORM.put_str("# ktest halt\n");
            PLATFORM.halt(1)
        }
        Err(error) => {
            let mut w = PlatformWriter(&PLATFORM);
            let _ = writeln!(
                w,
                "not ok 5 - u-mode kernel read fault: enter loi kind={} ({:?})",
                error.name(),
                error
            );
            PLATFORM.put_str("# ktest halt\n");
            PLATFORM.halt(1)
        }
    }
}

/// Entry hart **thứ cấp** (KT-49): ảnh user chỉ cần symbol để link; không gọi `smp::init` nên
/// không hart thứ cấp nào chạy (nếu có, vào đây rồi park — không panic).
#[no_mangle]
pub extern "C" fn rust_secondary_main(_hart: usize, opaque: usize) -> ! {
    let _ = vifast_plat_riscv64::smp::secondary_setup(opaque);
    vifast_plat_riscv64::park_forever()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    PANIC.handle(info)
}
