// SPDX-License-Identifier: MIT OR Apache-2.0
//! Nguồn Rust tối thiểu của chương trình `inv` U-mode (**KT-67a**) — chỉ chứa `global_asm!`
//! của [`inv_user.S`] + panic handler (cùng khuôn `init_user.rs`).
//!
//! Vì sao asm: chương trình phải **ghi/đọc thật** vùng vừa map bằng `sd`/`ld`, mà luật cấu trúc
//! R5 (D2-17) chỉ cho `unsafe` trong `kernel/vifast-plat-*`. Asm U-mode giữ chương trình
//! **0 `unsafe`** và không cần lệnh đặc quyền (chỉ `sd`/`ld`/`ecall`).
#![no_std]
#![no_main]

core::arch::global_asm!(include_str!("inv_user.S"));

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
