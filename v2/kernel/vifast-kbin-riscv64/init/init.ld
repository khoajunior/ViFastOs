/* SPDX-License-Identifier: MIT OR Apache-2.0 */
/* KT-53 — linker script cho `init` user: VA cố định trong cửa sổ user của KT-53
   (`USER_BASE..USER_TOP` = 0x4000_0000..0x8000_0000; root index 1), không PIE/reloc.
   Tách segment theo quyền để kernel map **W^X**: text RX, rodata R, data RW. */
OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS
{
    . = 0x40000000;

    .text : ALIGN(0x1000) {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }

    .rodata : ALIGN(0x1000) {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    .data : ALIGN(0x1000) {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    .bss : ALIGN(0x1000) {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.riscv.attributes)
    }
}
