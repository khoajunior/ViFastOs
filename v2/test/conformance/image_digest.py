#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""KT-55 / D2-80 — BUILD_ID của ảnh: FNV-1a 64 trên nội dung section `.text` của ELF.

Kernel in `# conf image: build=<16 hex> text=<len>` bằng cách băm **vùng `.text` đã nạp** của
chính nó (`linker.ld`: `__text_start`/`__text_end`). Script này băm **cùng section** trong tệp
ELF ⇒ runner/`trace_compare.py` chỉ chấp nhận log có hash **khớp ảnh đang dùng** (D2-80).

Dùng: `python3 image_digest.py <image>` → in `build=<16 hex> text=<len>`.
"""

import struct
import sys

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x00000100000001B3


def fnv1a(data):
    h = FNV_OFFSET
    for b in data:
        h ^= b
        h = (h * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return h


def text_section(path):
    data = open(path, "rb").read()
    if data[:4] != b"\x7fELF":
        raise SystemExit(f"image_digest: {path} không phải ELF")
    if data[4] != 2 or data[5] != 1:
        raise SystemExit("image_digest: chỉ hỗ trợ ELF64 little-endian")
    e_shoff = struct.unpack_from("<Q", data, 0x28)[0]
    e_shentsize = struct.unpack_from("<H", data, 0x3A)[0]
    e_shnum = struct.unpack_from("<H", data, 0x3C)[0]
    e_shstrndx = struct.unpack_from("<H", data, 0x3E)[0]
    if not e_shoff or not e_shnum:
        raise SystemExit("image_digest: ELF không có section header")
    shstr_off = struct.unpack_from("<Q", data, e_shoff + e_shstrndx * e_shentsize + 0x18)[0]
    shstr_size = struct.unpack_from("<Q", data, e_shoff + e_shstrndx * e_shentsize + 0x20)[0]
    names = data[shstr_off : shstr_off + shstr_size]
    for i in range(e_shnum):
        base = e_shoff + i * e_shentsize
        name_off = struct.unpack_from("<I", data, base)[0]
        end = names.find(b"\x00", name_off)
        name = names[name_off:end].decode("ascii", "replace")
        if name == ".text":
            off = struct.unpack_from("<Q", data, base + 0x18)[0]
            size = struct.unpack_from("<Q", data, base + 0x20)[0]
            return data[off : off + size]
    raise SystemExit("image_digest: không thấy section .text")


def main():
    if len(sys.argv) != 2:
        raise SystemExit("dùng: image_digest.py <image>")
    text = text_section(sys.argv[1])
    print(f"build={fnv1a(text):016x} text={len(text)}")


if __name__ == "__main__":
    main()
