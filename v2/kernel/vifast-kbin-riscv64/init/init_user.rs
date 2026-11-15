// SPDX-License-Identifier: MIT OR Apache-2.0
//! Nguồn Rust tối thiểu của chương trình `init` user (KT-53) — chỉ chứa `global_asm!` của
//! [`init_user.S`] và panic handler. Rustc (do `build.rs` gọi) biên dịch + liên kết thành ELF64
//! tại `0x4000_0000` bằng [`init.ld`]; kernel nạp ELF này qua `vifast-elf` (không cần loader
//! động, không reloc).
#![no_std]
#![no_main]

core::arch::global_asm!(include_str!("init_user.S"));

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
