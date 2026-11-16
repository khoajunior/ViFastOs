# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-67a — chương trình U-mode **invocation hẹp** (D2-97/D2-99) + memsrv thật:
#   1. giữ cap Untyped được cấp lúc spawn (slot 13), tự Untyped.Retype ⇒ Frame (slot 15);
#   2. Frame.Map (method 0) map Frame đó vào VSpace (slot 14) tại 0x4100_0000;
#   3. ghi/đọc thật 8 B qua vùng vừa map;
#   4. Frame.Unmap (method 1) rồi đọc lại ⇒ load page fault (kernel bắt bằng enter_expect_fault);
#   5. hai invocation ngoài danh sách trắng ⇒ NotSupported 18 (D2-53 Q11), không Ok giả.
#
# Ca **âm** (reviewer-kt67a M1/M2 + D2-99(g)), không được im lặng:
#   - Frame.Map với **VSpace cap chỉ READ** (slot 19) ⇒ InsufficientRights 3;
#   - Frame.Unmap với **Frame cap khác** (slot 18) tại VA đang map Frame A ⇒ InvalidCapability 1,
#     và mapping phải **còn** (đọc lại MAGIC);
#   - Call với `MessageInfo.caps = 1` (chưa cài — KT-67b) ⇒ NotSupported 18.
#
# `MessageInfo` (35 bit, spec/abi.toml `[message_info]`): label = method ở bit 0..19,
# caps ở bit 20..22, length = số MR ở bit 28..34. Invocation dùng length = 4 (MR0..MR3 ở a2..a5).
# Quy ước tham số: **D2-99** (`spec/` sẽ chốt ở KT-67b).
#
# Mã thoát khi hỏng: 90 retype, 91 map, 92 ghi/đọc, 93 unmap, 94/95 method lạ sai mã,
# 96 (không được tới) đọc vùng đã unmap mà không fault, 97 VSpace-readonly map lọt,
# 98 unmap sai Frame không bị từ chối, 99 mapping mất sau unmap sai Frame.
    .section .text.entry,"ax"
    .globl _start
    .balign 4
_start:
    # marker "đã chạy thật trong U-mode"
    la a0, m_start
    li a1, 13
    li a7, 12
    ecall
    bnez a0, f97b

    # --- 1) Untyped.Retype: cap=13, label=0, MR0..3 = 15, 11 (Frame), 12, 0
    li a0, 13
    li a1, 0x40000000
    li a2, 15
    li a3, 11
    li a4, 12
    li a5, 0
    li a7, 0
    ecall
    bnez a0, f90

    # --- 2) M2: Frame.Map qua VSpace cap **chỉ READ** (slot 19) ⇒ phải trả 3
    li a0, 15
    li a1, 0x40000000
    li a2, 19
    li a3, 0x41000000
    li a4, 0x0b
    li a5, 0
    li a7, 0
    ecall
    li t0, 3
    bne a0, t0, f97

    # --- 3) Frame.Map: cap=15, label=0, MR0..3 = 14 (VSpace), VA, R|W|U = 0xb, 0
    li a0, 15
    li a1, 0x40000000
    li a2, 14
    li a3, 0x41000000
    li a4, 0x0b
    li a5, 0
    li a7, 0
    ecall
    bnez a0, f91

    # --- 4) ghi/đọc thật qua PTE vừa cài (MAGIC 0x1122334455667788 trong .rodata)
    la t0, magic
    ld t1, 0(t0)
    li t2, 0x41000000
    sd t1, 0(t2)
    ld t3, 0(t2)
    bne t1, t3, f92

    # --- 5) M1: Frame.Unmap với **Frame cap khác** (slot 18) tại VA của Frame A ⇒ phải trả 1
    li a0, 18
    li a1, 0x40000001
    li a2, 14
    li a3, 0x41000000
    li a4, 0
    li a5, 0
    li a7, 0
    ecall
    li t0, 1
    bne a0, t0, f98

    # --- 6) mapping của Frame A phải **còn** sau ca âm trên
    ld t4, 0(t2)
    bne t1, t4, f99

    la a0, m_rw
    li a1, 17
    li a7, 12
    ecall
    bnez a0, f97b

    # --- 7) Frame.Unmap đúng Frame A: cap=15, label=1, MR0..3 = 14 (VSpace), VA, 0, 0
    li a0, 15
    li a1, 0x40000001
    li a2, 14
    li a3, 0x41000000
    li a4, 0
    li a5, 0
    li a7, 0
    ecall
    bnez a0, f93

    la a0, m_unmap
    li a1, 16
    li a7, 12
    ecall
    bnez a0, f97b

    # --- 8a) method ngoài danh sách: VSpace (9) + method 0 ⇒ NotSupported 18
    li a0, 14
    li a1, 0x40000000
    li a2, 0
    li a3, 0
    li a4, 0
    li a5, 0
    li a7, 0
    ecall
    li t0, 18
    bne a0, t0, f94

    # --- 8b) method 7 trên Untyped ⇒ NotSupported 18
    li a0, 13
    li a1, 0x40000007
    li a2, 0
    li a3, 0
    li a4, 0
    li a5, 0
    li a7, 0
    ecall
    bne a0, t0, f95

    # --- 8c) D2-99(g): `caps = 1` (chưa cài) ⇒ NotSupported 18 (không phải 5)
    li a0, 13
    li a1, 0x40100000
    li a2, 15
    li a3, 11
    li a4, 12
    li a5, 0
    li a7, 0
    ecall
    bne a0, t0, f96b

    la a0, m_deny
    li a1, 15
    li a7, 12
    ecall
    bnez a0, f97b

    # --- 9) đọc lại vùng đã unmap ⇒ load page fault (kernel: enter_expect_fault)
    li t2, 0x41000000
    ld t3, 0(t2)
    li a0, 96
    li a7, 14
    ecall
    j .

f90:
    li a0, 90
    j fail
f91:
    li a0, 91
    j fail
f92:
    li a0, 92
    j fail
f93:
    li a0, 93
    j fail
f94:
    li a0, 94
    j fail
f95:
    li a0, 95
    j fail
f96b:
    li a0, 96
    j fail
f97:
    li a0, 97
    j fail
f98:
    li a0, 98
    j fail
f99:
    li a0, 99
    j fail
f97b:
    li a0, 97
fail:
    li a7, 14
    ecall
    j .

    .section .rodata
    .balign 8
magic:
    .dword 0x1122334455667788
m_start:
    .ascii "U: inv start\n"
m_rw:
    .ascii "U: inv map rw ok\n"
m_unmap:
    .ascii "U: inv unmap ok\n"
m_deny:
    .ascii "U: inv deny ok\n"
