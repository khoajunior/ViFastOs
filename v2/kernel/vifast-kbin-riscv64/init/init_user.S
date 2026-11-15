# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-53 — chương trình `init` **user** (U-mode), nhúng vào ảnh test dưới dạng ELF64.
#
# ABI `spec/abi.toml` `[calling.riscv64]`: a7 = số hiệu syscall, a0/a1 = tham số, trả
# a0 = error (0 = OK), a1 = value. Ba syscall của KT-53:
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

    # --- 3) EID SBI qua ecall U-mode ⇒ scause=8 + dispatcher trả RangeError (19, D2-79)
    #        vì EID nằm ngoài dải syscall 0..15.
    li a0, 0
    li a1, 0
    li a7, 0x53525354
    ecall
    li t0, 19
    bne a0, t0, bad

    # --- 4) Số hiệu syscall lạ (99) ⇒ RangeError (19), không panic.
    li a0, 0
    li a1, 0
    li a7, 99
    ecall
    li t0, 19
    bne a0, t0, bad

    # --- 5) DebugPutStr(badmarker, 25) rồi Exit(42) — mã 42 chứng minh mọi phép kiểm trên ĐẠT.
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
    # Lượt **cách ly**: U-mode đọc địa chỉ kernel identity (leaf U=0) ⇒ load page fault
    # scause=13, stval=0x8020_0000; kernel in bằng chứng + PANIC + halt sạch.
    li t0, 0x80200000
    ld t1, 0(t0)
    # Không fault nghĩa là kernel bị map U=1 — thoát 98 để TAP đỏ.
    li a0, 98
    li a7, 14
    ecall
    j .

    .section .rodata
    .balign 8
marker:
    .ascii "U: init hello from u-mode\n"
badmarker:
    .ascii "U: bad syscall err=19 ok\n"

    .section .data,"aw"
    .balign 8
counter:
    .dword 0
