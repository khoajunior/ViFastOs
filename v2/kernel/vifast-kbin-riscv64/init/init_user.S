# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-53 — chương trình `init` **user** (U-mode), nhúng vào ảnh test dưới dạng ELF64.
#
# ABI `spec/abi.toml` `[calling.riscv64]`: a7 = số hiệu syscall, a0/a1 = tham số, trả
# a0 = error (0 = OK), a1 = value. KT-53: ba syscall; **KT-62** thêm DebugGetInfo (15) + Yield (8)
# (đi vào `Machine` thật, không qua shim) và 4 số hiệu ngoài dải (SBI/99/16/17/31 ⇒ 19):
#   - DebugPutStr = 12 (a0 = user ptr, a1 = len)
#   - DebugHalt   = 13 (a0 = code)
#   - Exit        = 14 (a0 = code; QD §1 Q-A)
#
# Kernel hạ `sret` với **a0 = mode**: 0 = lượt syscall bình thường, 1 = lượt cố đọc địa chỉ
# kernel (0x8020_0000) để chứng minh cách ly U-mode. Chương trình chỉ dùng thanh ghi caller-saved
# (a/t) — không đụng s0..s11/gp/tp để đường trả về S-mode của kernel (`user_exit_resume`) khôi
# phục đúng ngữ cảnh.
    .section .text.entry,"ax"
    .globl _start
    .balign 4
_start:
    bnez a0, fault_mode

    # --- 1) DebugPutStr(marker, 26): marker chỉ in được nếu ecall U-mode tới kernel.
    la a0, marker
    li a1, 26
    li a7, 12
    ecall
    bnez a0, bad
    li t0, 26
    bne a1, t0, bad

    # --- 2) Segment .data phải map R|W thật: ghi rồi đọc lại.
    la t0, counter
    li t1, 0x5a5a1234
    sd t1, 0(t0)
    ld t2, 0(t0)
    bne t1, t2, bad

    # --- 3) Ngoài dải 0..15 ⇒ RangeError 19 (D2-79): EID SBI, 99, và **dải reserved** 16/17/31.
    la t3, out_of_range
    li t4, 0
range_loop:
    ld a7, 0(t3)
    li a0, 0
    li a1, 0
    ecall
    li t0, 19
    bne a0, t0, bad
    addi t3, t3, 8
    addi t4, t4, 1
    li t0, 5
    blt t4, t0, range_loop

    # --- 4) DebugGetInfo (15) QUA CORE (KT-62): a0=0, a1=abi_version=1, a3=min_compat=1,
    #        a4=features != 0, a5=num_harts **nền tảng** (D2-88a: 1/2/4 theo `-smp` — chỉ kiểm
    #        trong 1..8, TAP kernel-level pin đúng con số). Đọc trước Yield vì Yield ghi a3..a6.
    li a0, 0
    li a7, 15
    ecall
    bnez a0, bad
    li t0, 1
    bne a1, t0, bad
    bne a3, t0, bad
    beqz a5, bad
    li t0, 8
    bgt a5, t0, bad
    beqz a4, bad

    # --- 5) Yield (8) QUA CORE (KT-62): a0 = 0 (một TCB ⇒ round-trip Running→Ready→Running).
    li a7, 8
    ecall
    bnez a0, bad

    # --- 6) DebugPutStr(badmarker, 25) rồi Exit(42) — mã 42 chứng minh mọi phép kiểm trên ĐẠT.
    la a0, badmarker
    li a1, 25
    li a7, 12
    ecall
    li a0, 42
    li a7, 14
    ecall
    j .

bad:
    # Bất kỳ phép kiểm nào sai ⇒ Exit(99) (TAP đỏ vì mã thoát khác 42).
    li a0, 99
    li a7, 14
    ecall
    j .

fault_mode:
    # mode 1 (KT-56): U-mode đọc địa chỉ kernel identity (leaf U=0) ⇒ load page fault
    # scause=13, stval=0x8020_0000; lượt này **được phép fault** (TAP 5 kiểm FaultKind rồi về
    # S-mode), nên nếu không fault thì Exit(98) để TAP đỏ.
    li t0, 2
    beq a0, t0, wx_mode
    li t0, 0x80200000
    ld t1, 0(t0)
    li a0, 98
    li a7, 14
    ecall
    j .

wx_mode:
    # mode 2 (KT-56): U-mode **thực thi** trang `.data` (RW NX của chính init) ⇒ instruction page
    # fault scause=12; PTE hợp lệ, U=1, W=1, X=0 ⇒ FaultKind::Vm access=Execute reason=Wx.
    # Đây là fault **fatal** cuối boot (chính sách K3: log + halt), có chủ đích.
    la t0, counter
    jr t0

    .section .rodata
    .balign 8
marker:
    .ascii "U: init hello from u-mode\n"
    .balign 8
out_of_range:
    .dword 0x53525354
    .dword 99
    .dword 16
    .dword 17
    .dword 31
badmarker:
    .ascii "U: bad syscall err=19 ok\n"

    .section .data,"aw"
    .balign 8
counter:
    .dword 0
