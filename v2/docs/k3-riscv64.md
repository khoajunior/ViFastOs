<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3 — kernel riscv64 thật trên QEMU `virt` (mốc mở bằng KT-44)

Trạng thái: **KT-53** — **U-mode + 3 syscall + nạp `init` ELF**: `vifast-elf` parse/map `PT_LOAD`
theo quyền (**W^X**), stack user + guard, trapframe U-mode, `sret` (`sstatus.SPP=0`, `sscratch`
giữ trap stack per-hart), `ecall` `scause=8` dispatch `DebugPutStr` (12) / `DebugHalt` (13) /
`Exit` (14), syscall lạ trả mã lỗi ABI (19 mã, không panic), `Exit` ⇒ trạng thái `Exited` + không
quay lại U-mode, fault U-mode ⇒ log + `PANIC` + halt sạch (§16). Trên nền trap KT-45 + MMU Sv39
KT-46 + timer SBI KT-47 + PLIC/UART IRQ KT-48 + SMP KT-49 + IPI KT-50 + ASID/`sfence`/shootdown
KT-51 + object layer KT-52. Tài liệu hằng số nền tảng + quy trình build/boot/trap/MMU/timer/
PLIC/SMP/IPI/TLB/object/U-mode của K3.
Nguồn quyết định: **D2-72** (`v2/agent/DECISIONS.md`) chốt Q1..Q13 của
[`review/agent0/K3-PLAN.md`](../../review/agent0/K3-PLAN.md) §3; chi tiết task ở §4/§4.1 cùng tệp.
Nhãn số liệu theo QD §2.2: `[TK]` thiết kế mới, `[GT]` giả thuyết nền tảng, `[ĐO]` chỉ trong log
do lệnh sinh ra. **Tài liệu này không chứa số đo viết tay** (QD §9 Q12): mọi số đo nằm ở
`v2/agent/JOURNAL/kt45-*.log`, `kt46-*.log`, `kt47-*.log`, `kt48-*.log`, `kt49-*.log` và do
`v2/test/boot/*.sh` / `cargo test` in ra.

## 1. K3 mới là kernel thật

- **K2** là mô hình (`vifast-sim` + `vifast-pte`), **không** phải kernel chạy trên phần cứng (D2-71).
- **K3** (KT-44..57) là lần đầu kernel riscv64 chạy **thật** trên QEMU `virt` qua OpenSBI: trap,
  MMU Sv39 (KT-46), timer SBI (KT-47), PLIC/UART IRQ (KT-48), SMP (KT-49), IPI (KT-50),
  TLB/shootdown (KT-51), object layer (KT-52), U-mode (KT-53).
- K3 **không** phải kernel ABI đầy đủ (D2-72 **Q13**): core CNode/TCB/IPC/scheduler vẫn model-only;
  port core `no_std` là phase **K3b**, mở bằng task port core đầu tiên (KT-58) trước `procsrv`/shell.
- Cổng `.data + .bss ≤ 256 KiB` `[TK]` (QD §3.5) **giữ nguyên** ở K3, áp cho **cả bốn** ảnh
  (sản phẩm + **3** ảnh test; D2-72 Q5) — không nới, không tách ngân sách riêng cho ảnh test.

## 2. Hằng số nền tảng (không mở `spec/kernel.toml` — D2-72 Q1)

| Hạng mục | Giá trị | Nhãn | Nguồn |
|---|---|---|---|
| QEMU | sàn **≥ 9.2**; bản trên máy đo `11.1.3` | `[TK]` / `[GT]` | D2-72 Q2 |
| Máy ảo | `-machine virt,aia=none,aclint=on` (không `virt-<version>`) | `[TK]` | D2-72 Q2/Q8 |
| CPU / RAM / hart | `-cpu rv64 -m 128M -smp N`; sản phẩm mặc định `-smp 1`, **matrix 1/2/4**; `MAX_HARTS = 8` | `[TK]` | D2-72 Q2/Q4, KT-49 §12 |
| SMP / PerHart | **SBI HSM** `hart_start`/`hart_get_status` (EID `0x48534D`, FID 0/2); hart boot `0`; hart `1..N-1` start với `start_addr=_start_secondary`, `opaque="KT49"`; hart ngoài DTB (`hart_get_status = -3`) **bị từ chối** + log, không panic | `[TK]` | D2-72 Q4, KT-49 §12 |
| Vùng NOLOAD per-hart | **8 record** `guard 4 KiB \| stack 64 KiB \| trap stack 16 KiB` (`HART_STRIDE = 0x1_5000`, tổng `0xA_8000`); guard mỗi hart **NONE** ở cả identity/nửa cao/physmap; arena page table **64 KiB** sau `__kernel_end` | `[TK]` | KT-49 §12 |
| Bộ đếm theo hart | `TIMER_STATS`/`PLIC_STATS` là **PerHart** (`get(hart)`), handler chỉ cộng slot của hart đang chạy; `TRAP_STATS`/`GUARD_STATS` vẫn gộp toàn cục | `[TK]` | KT-49 §12 |
| Firmware | OpenSBI `fw_dynamic` nạp **trực tiếp** qua `-bios`, không U-Boot/EDK2; bản đi kèm QEMU `v1.8.1` | `[GT]` | D2-72 Q3 |
| Nạp kernel | S-mode tại `0x8020_0000`, `a0 = hartid`, `a1 = địa chỉ DTB` | `[GT]` | K0.5 (KT-07) |
| UART0 | `0x1000_0000` (16550A), IRQ **10** | `[GT]` | D2-72 Q7 |
| PLIC | `0x0c00_0000` (cổ điển), cửa sổ `0x60_0000`; **KT-48 map 2×2 MiB** (`0x0c00_0000` + `0x0c20_0000`); `ndev` **probe từ register** (ghi 1/đọc lại priority), không giả định 95; context S-mode `2*hart+1` | `[GT]` / `[TK]` | D2-72 Q7/Q8, KT-48 §11 |
| CLINT/ACLINT | `0x0200_0000` (`aclint=on` ⇒ IPI/timer qua ACLINT) | `[GT]` | D2-72 Q7 |
| Trang test | `0x0010_0000` (`test@100000`) | `[GT]` | D2-72 Q7 |
| Timer | **SBI TIME** `set_timer` (EID `0x5449_4D45`) + `sie.STIE`, timebase **10 MHz** (`rdtime`), tick **100 Hz** = 100.000 tick timebase = 10 ms; Sstc hoãn. **KT-47 đã cài** — xem §10 | `[TK]` | D2-72 Q6 |
| Deadline timer | **tuyệt đối** (bội số `TICK_TICKS`, không cộng dồn từ `now`) ⇒ không trôi; một deadline/hart; `missed = late_by / TICK_TICKS` ⇔ mất tick khi trễ trọn một chu kỳ | `[TK]` | KT-47, D2-72 Q6 |
| Tick accounting | `ticks/late/missed/non_monotonic/max_delta/min_delta/max_late` + bằng chứng `sstatus.SPIE/SIE` (`TimerCounters`/`TIMER_STATS`) | `[TK]` | KT-47 |
| Đo drift | Δ tick **∈ [9,11] ms** đo bằng `rdtime`; **chỉ `ktest.sh`** thêm `-icount shift=6` (QEMU TCG không icount phục vụ timer ảo thưa ⇒ đo độ trễ main loop, không đo toán deadline) — **deviation D2-74**, xem §10.5 | `[TK]` | D2-74 |
| Ngắt | **PLIC cổ điển**, AIA (APLIC/IMSIC) hoãn K4'/K5' | `[TK]` | D2-72 Q8 |
| DTB | chỉ kiểm magic nếu OpenSBI truyền ở `a1` (log/cảnh báo, **không** parse đầy đủ) | `[TK]` | D2-72 Q7 |
| Soak | `1000 boot × 3 luồng` trên máy `ps`, timeout **20 s/boot**, `0/1000`, không retry | `[TK]` | D2-72 Q10 |
| Trap | `stvec` **direct mode** (một vector, `MODE=0`, `stvec` là CSR **per-hart**); mỗi hart **trap stack riêng 16 KiB** (`sscratch` per-hart); trapframe **288 B** = 32×8 (x0..x31) + 4×8 (`sepc/sstatus/scause/stval`) | `[TK]` | D2-73, KT-45/49 |
| MMU | **Sv39** (`satp.MODE=8`), 3 cấp × 512 entry, bảng 4 KiB; `VA_BITS=39` từ `vifast-pte`; kernel **identity** + **nửa cao** 2 MiB (image), NOLOAD 4 KiB; physmap RAM **2 MiB NX**; MMIO UART **4 KiB** + PLIC **2×2 MiB Device** (KT-48); guard page **NONE** | `[TK]` | KT-46/48 |
| Địa chỉ map | kernel identity `0x8020_0000`; kernel nửa cao `0xFFFF_FFC0_4000_0000`; physmap `0xFFFF_FFC0_0000_0000 + PA`; vùng NOLOAD per-hart ở `0x8040_0000` (linker `ALIGN(2M)`): guard hart `h` tại `base + h*0x1_5000` | `[TK]` | KT-46/49 |
| Arena page table | **64 KiB** RAM sau `__kernel_end`, bump allocator trong builder; root + bảng trung gian **không** `static`, không cộng trần `.data+.bss` | `[TK]` | KT-46, D2-72 Q5 |
| IRQ | `Platform::enable_irq`/`disable_irq` = **PLIC thật** (KT-48): set/clear bit enable của context `2*hart+1` + đọc lại; cổng CPU `sie.SEIE` + `sstatus.SIE` do `plic::enable_external` (đối xứng `timer::enable_timer`); KT-49: mỗi hart tự đặt threshold/enable trên **context của mình** (`plic::init_hart`) | `[TK]` | D2-72 Q7/Q8, KT-48 §11, KT-49 §12 |
| UART IRQ | UART0 **IRQ 10**; `IER.TX-empty` (mức cao khi THR rỗng); handler `claim → đọc IIR → complete → re-arm`; tự tắt `IER` + enable sau **N = 8** ngắt (`UART_IRQ_LIMIT`) | `[TK]` | KT-48 §11 |
| ASID | **8 bit** (`vifast_pte::ASID_BITS`) ⇒ **256** ASID/thế hệ, cấp từ free-list **không tái dùng trong cùng thế hệ**, `free` = về hưu; cạn ⇒ `generation += 1` + **một** `FlushScope::All`; `satp.ASID` bit 59..44 (kernel dùng 8 bit); đổi ASID **không** `sfence` | `[TK]` | D2-65 Q4, KT-51 §14.1 |
| `sfence.vma` | ba dạng: `va, asid` (một trang) / `zero, asid` (một ASID) / `zero, zero` (toàn bộ); `rs1` = VA, `rs2` = ASID; chỉ TLB hart hiện tại | `[TK]` | privileged spec, KT-51 §14.2 |
| Shootdown | slot yêu cầu/hart `{seq, asid, scope, va}` (4 `AtomicU64`) + kind `TlbFlush` (IPI KT-50); ack `fetch_max` theo `seq`; hart gọi **local flush + ack ngầm**, **không** self-target (D2-68); chờ `ack_mask == active_on`, drain mỗi bước, deadline `rdtime`; **một** shootdown tại một thời điểm (`IN_FLIGHT`) | `[TK]` | D2-65 Q5, D2-68, KT-51 §14.3 |

| Object layer | Untyped boot = arena RAM **64 KiB** sau arena page table KT-46 (không section ELF) + cửa sổ PA `UNTYPED_PA_WINDOW = 1 GiB`; 2 Untyped (`U0` cửa sổ căn 2 MiB, `U1` cửa sổ căn 1 GiB cho Frame 1 GiB); object `Untyped`/`Frame`/`PageTable`/`VSpace`; `Retype` **nội bộ** (không ABI); cap/key tối thiểu trong `.bss` | `[TK]` | D2-66, KT-52 §15 |
| Frame object | 4 KiB / 2 MiB / 1 GiB (`size_bits` 12/21/30 đọc từ `vifast-pte::FrameSize`); `pa = pa_base + offset` là **nhãn PA** trong cửa sổ, **0 byte arena**; `size_bits` khác ba giá trị ⇒ `InvalidArgument`; cấp **giảm dần** (không đệm căn lề ngầm); chồng PA toàn cục ⇒ `AlreadyMapped` | `[TK]` | D2-66, KT-52 §15.4 |
| PageTable | 4 KiB = 512 PTE × 8 B trong arena Untyped, zeroed lúc Retype; hình 3 cấp + `walk_shape`/`PAGE_SIZE_BITS`/`PTE_LEVELS` từ `vifast-pte`; cờ PTE từ `vifast-plat-common::mmu` (không chép bit) | `[TK]` | KT-52 §15.5 |
| VSpace | root PageTable 4 KiB (object nội bộ, không cap) + ASID nhãn 1..255; `map` tự cấp bảng trung gian từ Untyped của VSpace; `unmap`/`protect` đổi PTE rồi đẩy yêu cầu `sfence` cho arch chạy `sfence.vma` (KT-51); W^X **toàn cục** theo Frame (`maps/w/x`, D2-67) | `[TK]` | D2-67, KT-52 §15.5/§15.6 |
| Bất biến object | `Σ byte object sống + Σ hole == watermark ≤ capacity` mỗi Untyped (mọi byte trong watermark là object hoặc hole — đệm căn lề cũng là hole, **không** có gap tự khai); `Σ cỡ Frame + Σ PA hole == pa_used`; không Frame mồ côi; walk PTE khớp mapping; W^X toàn cục — [`check_objmem`] tương đương `check_vm` K2, chạy `no_std` | `[TK]` | KT-52 §15.8 |
| U-mode user | Cửa sổ user **`[0x4000_0000, 0x8000_0000)`** (root index 1 Sv39 — kernel không dùng); stack top `0x4001_0000`, **8 trang** `U|R|W|NX`, **guard** `0x4000_7000` **không map**; `sret` với `sstatus.SPP=0`; **`sscratch` luôn giữ đỉnh trap stack per-hart** (KT-45/49), trap `csrrw sp, sscratch, sp` ⇒ stack kernel + `x2` = sp user; không lộ địa chỉ kernel cho user | `[TK]` | D2-72 Q9, KT-53 §16.3 |
| Nạp `init` ELF | `vifast-elf::parse` (ELF64 LE, `ET_EXEC`, `PT_LOAD`; từ chối `PT_DYNAMIC/INTERP/TLS`, `W+X`, segment ngoài cửa sổ user); map theo quyền **W^X** (`.text` `U|R|X`, `.rodata` `U|R|NX`, `.data` `U|R|W|NX`); `build.rs` dùng **`$RUSTC`** biên dịch `init/init_user.rs` (+`.S`, `.ld`) thành `init.elf` tại `0x4000_0000`, nhúng bằng `include_bytes!` **chỉ ở ảnh test user** | `[TK]` | KT-53 §16.2 |

Ghi chú kiểm chứng: `run.sh`/`ktest.sh` in `# qemu <version>` và `# opensbi <version>` **từ log
boot thật**; QEMU dưới sàn **9.2** (so cả major.minor) ⇒ **FAIL**, không tự hạ (D2-72 Q2). OpenSBI banner
(`Platform Timer Device : aclint-mtimer @ …Hz`, `Platform IPI Device : aclint-mswi`) là log
`[ĐO]`, dùng để đối chiếu timebase ở KT-47 (§10.1). Cửa sổ PLIC `0x60_0000` theo K3-PLAN §1; OpenSBI
domain region phủ `0x0c00_0000–0x0c3f_ffff` `[GT]` — KT-48 map **2×2 MiB** trong cửa sổ đó (đủ
context 15 của `MAX_HARTS = 8`) và đọc `ndev` từ register, không giả định 95 (§11).

## 3. Bản đồ ảnh và linker script

```
0x8020_0000  .text        KEEP(.text.entry) — `_start` trong vifast-plat-riscv64
             .rodata      (chuỗi TAP, chẩn đoán)
             .data / .sdata
             .bss / .sbss / COMMON
             __bss_end
             . = ALIGN(2M)                       ← KT-46: tách vùng NOLOAD sang 2 MiB kế tiếp
0x8040_0000  record hart 0 (NOLOAD, 0x1_5000):
  0x8040_0000  .stack_guard (4 KiB)              ← guard hart 0, MMU cưỡng chế NONE
  0x8040_1000  .stack       (64 KiB)              ← `sp` hart 0 (`__stack_top`), mọc xuống guard
  0x8041_1000  .trap_stack  (16 KiB)              ← `sscratch` hart 0; trapframe 288 B trong vùng này
  0x8041_5000  .hart_stacks (NOLOAD, 7 × 0x1_5000) ← record hart 1..7 cùng khuôn (guard | stack | trap stack)
  0x804A_8000  __kernel_end                        ← arena page table (64 KiB) bắt đầu ở đây, KHÔNG là section
```

Record hart `h` ở `__hart_area_base + h*HART_STRIDE`; guard của hart `h` là **trang đầu record**
(nên tràn stack của hart nào chạm đúng guard hart đó). `__stack_*`/`__trap_stack_*` giữ nghĩa cũ
= **hart 0** (KT-46 dùng); `mmu::stack_range_of(h)`/`guard_range_of(h)`/`trap_stack_range_of(h)`
tính cho mọi `h`. `mmu::init` **panic** nếu linker lệch hằng số `plat-common::smp` (không chạy với
stack hart này đè guard hart khác).

- **Vì sao `ALIGN(2M)`:** KT-46 map **kernel image** `[0x8020_0000, 0x8040_0000)` bằng **một leaf
  2 MiB** ở nửa cao (yêu cầu "kernel nửa cao 2 MiB"). Nếu guard/stack nằm trong chính leaf đó thì
  không thể đặt riêng guard page `NONE` ở granularity 4 KiB. Đưa vùng NOLOAD sang 2 MiB kế tiếp
  giải quyết cả hai: image 2 MiB, guard 4 KiB.
- `.stack_guard` + `.stack` + `.trap_stack` là **NOLOAD**: chỉ chiếm địa chỉ, không chiếm file ảnh,
  **báo riêng** — không cộng vào trần budget (D2-72 Q5). Guard 4 KiB nằm **ngay dưới**
  `__stack_bottom` (`__stack_guard_end == __stack_bottom`).
- **Arena** (KT-46) bắt đầu ở `__kernel_end` (sau record hart 7), 64 KiB, căn 4 KiB. Root page table
  và mọi bảng trung gian được cấp từ đây (bump allocator), **không** `static`/`static mut`; arena
  được identity-map + nửa cao RW NX. Vì không phải section ELF, nó **không** vào `.data+.bss`.
- `_start` nhận `a0 = hartid`, chép vào `tp`; tính `sp`/`sscratch` từ **record của hart đó**
  (`__hart_area_base + hart*HART_STRIDE`), zero `.bss`, lưu `a1` (DTB) rồi gọi `rust_main`.
  `_start_secondary` (SBI HSM) làm cùng phép tính với `a0/a1` do `hart_start` truyền, rồi gọi
  `rust_secondary_main` của ảnh. `Riscv64Virt::hart_id` đọc `tp`, `now` đọc `rdtime`.
- `.stack`/`.trap_stack` là section NOLOAD: **không nằm trong `filesz`** của LOAD (mặc dù LOAD
  `memsz` vẫn phủ tới `__kernel_end`) — đây là bài học KT-45; KT-46 **bắt buộc** map cả hai vùng
  trong page table kernel (TAP 6 ghi/đọc qua nửa cao để chứng minh), nếu thiếu thì trap đầu tiên
  sau khi bật `satp` sẽ page-fault vào chính `.trap_stack` chưa map.

## 4. Bốn ảnh: sản phẩm + ba ảnh test (D2-08) — tách bằng feature, không `cfg(target_arch)`

| Ảnh | Bin | Feature | TAP | Runner |
|---|---|---|---|---|
| **Sản phẩm** | `vifast-kbin-riscv64` | — (mặc định) | `ok 1 - k3 boot` + `# k3 halt`, kèm bằng chứng `# plic:`/`# uart irq:`/`# timer arm:`/`# timer:`/`# smp:` (PLIC + UART IRQ thật, timer 100 Hz + ≥1 tick, **SMP bring-up `N` hart**, đều **không icount**) | `v2/test/boot/run.sh` |
| **Test chính** | `vifast-kbin-ktest` | `ktest` | **37** — 4 TAP trap KT-45 + 3 TAP MMU KT-46 + **3 TAP PLIC/UART IRQ KT-48** + **4 TAP timer KT-47** (chạy **sau** PLIC) + **5 TAP SMP KT-49** + **5 TAP IPI KT-50** + **5 TAP TLB/ASID KT-51** + **7 TAP object layer KT-52** (Retype VSpace, bất biến byte, Frame 3 cỡ, W^X 2 ca, map/unmap 4 KiB, 0 rò) + trap lồng fatal cuối (TAP 37) | `v2/test/boot/ktest.sh` |
| **Test guard** | `vifast-kbin-ktest-guard` | `ktest` | **3** — guard image + load `scause=13` (resume probe) + store `scause=15` fatal | `v2/test/boot/ktest.sh` |
| **Test user** | `vifast-kbin-ktest-user` | `ktest,user` | **5** — **KT-53**: nạp `init` ELF W^X + U-mode (`sret`) + `DebugPutStr` (12) + `Exit` (42) + syscall lạ (mã 5) + U-mode đọc địa chỉ kernel ⇒ `scause=13` + `PANIC: u-mode fault` (boot kết thúc có chủ đích) | `v2/test/boot/ktest.sh` |

- Cùng crate `v2/kernel/vifast-kbin-riscv64`, cùng `linker.ld`; các bin test khai
  `required-features = ["ktest"]` (riêng ảnh user: `["ktest", "user"]`) nên **build mặc định không
  sinh ảnh test**.
- **Vì sao ba ảnh test:** mỗi boot chỉ kết thúc được **một** lần bằng PANIC có chủ đích. Ảnh chính
  dùng lần đó cho **trap lồng** (KT-45 P2b); guard **store fault** (`scause=15`) cần một boot riêng
  để dump + halt sạch. Ảnh guard kiểm thêm **load fault** (`scause=13`) trước đó bằng probe resume
  **chỉ trong test** (cờ `GUARD_RECOVER_LOAD`); chính sách mặc định của kernel vẫn là **fatal**.
- **Vì sao ảnh user riêng (KT-53):** fault U-mode là **fatal** (log + halt), nên test cách ly (TAP 5)
  phải là lần kết thúc boot — cùng lý do guard; 4 TAP U-mode còn lại chạy trọn trong cùng boot
  trước đó. Module U-mode (`vifast-plat-riscv64::user`) nằm sau **feature `user`** (kéo `ktest`), nên
  ảnh sản phẩm + ảnh test chính/guard **không** link nó; 37+3 TAP cũ và budget của chúng giữ nguyên.
  `ktest.sh` build ảnh chính/guard với `--features ktest`, ảnh user với `--features ktest,user`.
- `run.sh` **fail** nếu ảnh sản phẩm còn **một** chuỗi `ktest`; `ktest.sh` **fail** nếu ảnh test
  **không** có chuỗi đó ⇒ chứng minh tách ảnh thật, không chỉ đổi tên.
- **Probe guard chỉ có trong ảnh test**: `probe_guard_load`/`probe_guard_store` nằm sau
  `#[cfg(feature = "ktest")]` ở `vifast-plat-riscv64` (feature `ktest` của kbin bật feature arch).
  Ảnh sản phẩm build **không** feature nên không có probe/chuỗi test; chính sách guard của nó
  **luôn fatal** (`report_guard_fault` + halt).
- `PanicHandler` + trait `Platform` + logic trap/MMU thuần nằm ở `vifast-plat-common` nên **các ảnh
  dùng chung một handler**, không chép đôi code.
- KT-53: ảnh test **user** thêm **5 TAP** (`expected.txt`: `ktest-user 5 5`), boot kết thúc bằng
  `PANIC: u-mode fault scause=0xd stval=0x80200000 …`; runner chỉ chấp nhận đúng dòng PANIC này.
- KT-49/50/51/52: ảnh test chính **37 TAP** (KT-49: 20; KT-50 thêm 5 TAP IPI 20..24; KT-51 thêm 5
  TAP TLB/ASID 25..29; KT-52 thêm 7 TAP object 30..36, trap lồng dời sang **37**) — `expected.txt`
  đọc kế hoạch/số `ok`; `ktest.sh` chạy
  **matrix `-smp 1|2|4`** (`bash ktest.sh 50 matrix`, log riêng từng N); harness TAP đầy đủ cho
  **toàn bộ** `.scn`/conformance vẫn là **KT-54/55**.
- KT-47: ảnh **sản phẩm** arm timer + chờ **≥1 tick** (timeout 1 s) rồi in `# timer: ticks=…` —
  bằng chứng **boot thật, không icount** (D2-74 điều kiện (b)); ảnh test đo drift **tất định** dưới
  `-icount shift=6` (D2-74, §10.5).
- KT-48: **cả hai** ảnh bật PLIC + UART IRQ thật (product **không icount** ⇒ bằng chứng PLIC trên
  đường boot thật; test thêm `# plic:`/`# uart irq:`/`# plic quiet:` và TAP 8..10); handler tự tắt
  `IER` + enable sau N ngắt nên log/TAP không bị ngập (§11).
- KT-49: **cả hai** ảnh bring-up SMP thật (SBI HSM); ảnh test thêm TAP 15..19 + `# smp:`/`# hart h
  alive`/`# stack hart=h`/`# timer hart=h`/`# plic hart=h`/`# barrier:`; ảnh sản phẩm in `# smp:`
  và **PANIC** nếu present-mà-không-online. Chạy `run.sh N 1|2|4` để kiểm ảnh sản phẩm nhiều hart
  (**không icount**, D2-72 Q2).

## 5. Build / boot / đo

```bash
bash v2/test/boot/run.sh 1        # ảnh sản phẩm, 1 boot, 1 hart (D2-72 Q2, KHÔNG icount)
bash v2/test/boot/run.sh 50       # 50 boot 0 lỗi (DoD KT-46/47)
bash v2/test/boot/ktest.sh 1 1    # 3 ảnh test: 37 TAP chính + 3 TAP guard + 5 TAP user (`-icount shift=6`)
bash v2/test/boot/ktest.sh 1 matrix  # matrix smp 1/2/4 (KT-49), log riêng từng N
bash v2/test/boot/ktest.sh 50 matrix # 50 boot **mỗi** N (DoD KT-49)
bash v2/test/boot/run.sh 50 1     # ảnh sản phẩm 1 hart (không icount)
bash v2/test/boot/run.sh 50 4     # ảnh sản phẩm 4 hart (SMP thật, không icount)

bash v2/test/boot/budget.sh \
  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64 product
bash v2/test/boot/budget.sh \
  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest ktest
bash v2/test/boot/budget.sh \
  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-guard ktest-guard
bash v2/test/boot/budget.sh \
  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-user ktest-user
```

Lệnh thô bên trong (không cần nhớ, chỉ để tra):

```bash
cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-kbin-riscv64 \
    --bin vifast-kbin-riscv64 --target riscv64gc-unknown-none-elf --release
cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-kbin-riscv64 \
    --bin vifast-kbin-ktest --bin vifast-kbin-ktest-guard --features ktest \
    --target riscv64gc-unknown-none-elf --release
cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-kbin-riscv64 \
    --bin vifast-kbin-ktest-user --features ktest,user \
    --target riscv64gc-unknown-none-elf --release

# Ảnh sản phẩm (run.sh) — đúng dòng D2-72 Q2:
qemu-system-riscv64 -machine virt,aia=none,aclint=on \
    -bios /opt/homebrew/share/qemu/opensbi-riscv64-generic-fw_dynamic.bin \
    -kernel <ảnh> -nographic -cpu rv64 -m 128M -smp 1 -no-reboot

# Ảnh test (ktest.sh, D2-74): thêm -icount shift=6 để timer ảo QEMU bắn đúng hạn.
qemu-system-riscv64 -icount shift=6 -machine virt,aia=none,aclint=on \
    -bios /opt/homebrew/share/qemu/opensbi-riscv64-generic-fw_dynamic.bin \
    -kernel <ảnh test> -nographic -cpu rv64 -m 128M -smp 1 -no-reboot
```

Cổng đo (do script thực thi, không phải người):

- **Budget** — `budget.sh` cộng `.data .sdata .bss .sbss` bằng `llvm-size -A` (đúng cách đo của
  K0.5/KT-07), trần **262.144 B** `[TK]` cho **cả bốn** ảnh, in
  `# image=… data+bss=… stack=… guard=… trap_stack=…`; **exit 1** nếu vượt (D2-72 Q5). Page table
  nằm trong arena RAM nên **không** đội `.data+.bss`.
- **Boot** — `run.sh`/`ktest.sh`: timeout **20 s**/boot `[TK]` (D2-72 Q10), chỉ `kill <pid>` do
  chính script tạo (AGENTS §9 — cấm `pkill -f qemu-system`), đòi QEMU ≥ sàn **9.2**, đòi TAP đủ
  (`TAP version 13`, `1..K`, mọi dòng `ok`, marker/PANIC có chủ đích), **không** có PANIC lạ.
- **SMP** — `ktest.sh` kiểm `Platform HART Count` của **OpenSBI** khớp `-smp N` (bằng chứng độc lập
  ngoài kernel), `# smp: … harts=N … timeout=0`, `# harts alive: n=N … tickets=1`, số dòng
  `# hart h alive`/`# stack hart=h`/`# timer hart=h`/`# plic hart=h` = N; `run.sh` kiểm
  `# smp: … harts=N … timeout=0` trên ảnh sản phẩm.
- **Cổng repo** — `./v2/bin/v2 gate --fast` **không** build kernel
  (`v2/Cargo.toml` `exclude = ["kernel"]`); kernel kiểm bằng build + `ktest.sh`/`run.sh` + host test
  `cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-plat-common`.

## 6. `vifast-plat-common` + `vifast-plat-riscv64` (KT-44..49)

- `vifast-plat-common` (`no_std` + `#![forbid(unsafe_code)]`):
  - trait **`Platform`**: `put_str`, `halt`, `now_ticks`/`now_ns`, `timebase_hz`, `hart_id`,
    `set_trap_vector`, `enable_irq`/`disable_irq` (**PLIC thật** từ KT-48), và **KT-47**:
    `set_timer_deadline`, `enable_timer`, `disable_timer`; **`PanicHandler`** dùng chung;
    **`PlatformWriter`**.
  - module **`trap`** (KT-45): `TrapFrame` 288 B + offset hằng số + `const` assert `offset_of!`;
    `ExceptionKind`/`Exception`/`Event`; `decode_scause`/`decode_trap`; `handle_trap`;
    `TrapStats`/`TRAP_STATS`; `GuardStats`/`GUARD_STATS`; `report_fatal`/`report_guard_fault`/
    `report_interrupt`; `is_nested_trap_sp`.
  - module **`mmu`** (KT-46): bit PTE Sv39 (`V/R/W/X/U/G/A/D`), `PteFlags`, `MapError`, `Walk`,
    `PteMemory`, `is_canonical`, `walk_with`, `PageTableBuilder` (cấp bảng từ `&mut [u8]`, map
    4 KiB/2 MiB/1 GiB, walk phần mềm), `satp_sv39`/`satp_mode`/`satp_root`; dùng lại
    `vifast_pte::{walk_shape, Pte, VA_BITS=39, PTE_LEVELS=3, PAGE_SIZE_BITS}` — crate nội bộ,
    `no_std`, không dependency ngoài.
  - module **`plic`** (KT-48): hình học PLIC cổ điển (priority `0x0`, pending `0x1000`, enable
    `0x2000 + 0x80*ctx`, context `0x20_0000 + 0x1000*ctx`, threshold `+0`, claim `+4`), context
    `2*hart+1`, `ndev` probe/limit, phân loại claim (UART/spurious/line lạ), `PlicStats` +
    **`PerHartPlicStats`**/`PLIC_STATS.get(hart)` (traps/claims/completions/uart_irqs/spurious/
    unexpected/last_irq/last_iir — **theo hart** từ KT-49).
  - module **`timer`** (KT-47): hằng số `[TK]` `TIMEBASE_HZ=10_000_000`, `TICK_HZ=100`,
    `TICK_TICKS=100_000`, `TICK_NS=10_000_000`, `TICK_DELTA_MIN_NS/MAX_NS` = 9/11 ms;
    `ticks_to_ns`/`ns_to_ticks`; `classify(last, deadline, now) -> TickSample` (delta, late_by,
    `missed`, `next_deadline` — thuần, test host); `TimerCounters` + `TimerSnapshot`
    (tick/late/missed/monotonic/max_delta/min_delta/max_late + `trap_spie_on`/`handler_sie_off`),
    **`PerHartTimerStats`**/`TIMER_STATS.get(hart)` (theo hart từ KT-49).
  - module **`smp`** (KT-49): `MAX_HARTS=8`, hằng số record `guard|stack|trap stack`
    (`HART_STRIDE=0x1_5000`, `HART_AREA_BYTES=0xA_8000`) + offset/guard-owner thuần, `HartMask`
    (`AtomicU64`), hằng SBI HSM + `classify_hart_status`/`start_decision`/`hart_start_ok`
    (hart ngoài DTB `-3` ⇒ `Reject`, không panic).
- `vifast-plat-riscv64` (asm/MMIO, `unsafe` thuộc danh sách đóng `kernel/vifast-plat-*`, D2-17):
  `_start`, `trap_entry` direct mode, `trap_dispatch`, probe KT-45; **module `mmu`** (KT-46): đọc
  linker symbol, arena từ `__kernel_end`, dựng identity + nửa cao + physmap + MMIO + guard NONE,
  `csrw satp` + `sfence.vma` hai bước, đọc lại `satp`, alias self-test, walk phần mềm
  (`PhysMemory`), `probe_guard_load`/`probe_guard_store`, `read_u64`/`write_u64` cho TAP.
  **Module `timer`** (KT-47): `now_ticks` (`rdtime`), `set_timer_deadline` (SBI TIME ecall),
  `enable_timer`/`disable_timer` (`sie.STIE`/`sstatus.SIE`), `read_sie`/`read_sstatus`, `init`
  (arm + bật), `on_timer_interrupt` (đếm tick + trả deadline kế cho `trap_dispatch`).
  **Module `plic`** (KT-48/49): MMIO `plic_read`/`plic_write`, `probe_ndev`, `init` (threshold 0,
  priority 1, enable context `2*hart+1`), `init_hart` (mỗi hart tự threshold + enable round-trip
  trên **context của mình**, kiểm context M-mode `2*h`), `claim`/`complete`, `enable_irq`/
  `disable_irq` + `irq_enabled`, `enable_external`/`disable_external` (`sie.SEIE` + `sstatus.SIE`),
  UART `IER`/`IIR` (`uart_enable_tx_irq`/`uart_disable_tx_irq`), và `on_external_interrupt`
  (claim → IIR → complete → re-arm / tắt sau N) — bộ đếm `PLIC_STATS.get(hart)`.
  **Module `smp`** (KT-49): `_start_secondary` (asm ở `lib`), `hart_start`/`hart_status` (SBI HSM
  `ecall`), `init` (probe + start + chờ online), `secondary_setup` (adopt `satp`, `stvec`, online),
  `release`/`finish`/`wait_release`, `take_ticket`, `note_trap_frame`, barrier
  (`barrier_begin`/`join`/`wait_arrivals`/`release`, timeout) — `unsafe` thuộc D2-17.
- Phần **logic** chạy host (D2-72 Q12): `cargo test --manifest-path v2/kernel/Cargo.toml
  -p vifast-plat-common` — 13 test trap KT-45 + **1 test API `Platform` KT-47** (`now_ns`/
  `timebase_hz`/ba method timer) + **8 test MMU KT-46** (map/walk 4K-2M-1G, từ chối mapping sai,
  VA canonical, guard NONE/physmap NX, `satp` mode 8, guard stats + dump) + **12 test timer KT-47**
  (hằng số/đổi đơn vị, `classify` đúng hẹn/trễ/mất tick/ngắt sớm/saturate, 1.000 tick không mất,
  phát hiện mất tick + tự căn deadline, monotonic, bằng chứng `sstatus`, arm/reset) + **12 test
  PLIC KT-48** (hình học đăng ký, context `2*hart+1` cho hart 0..3, word/bit enable, offset
  threshold/claim, hai cửa sổ phủ `MAX_HARTS = 8`, phân loại claim, claim/complete cân bằng +
  0 IRQ giả, limit tắt sau N, `ndev` không giả định 95, priority > threshold) + **9 test SMP
  KT-49** (`tests/smp.rs`: bố cục record không chồng/lệch, guard owner, `HartMask` + hart ngoài
  dải, phân loại trạng thái HSM + từ chối hart ngoài DTB, `PerHartTimerStats`/`PerHartPlicStats`
  không lẫn) = **55 test**.
  Miri **không** áp ảnh kernel `no_std`/asm/MMIO; Miri vẫn áp `lib/` + sim.

## 7. Trap khung (KT-45) + guard page-fault (KT-46, theo D2-73)

- **`stvec` direct mode**: một vector `trap_entry` (`.balign 4`, `MODE=0`), cài bằng
  `Platform::set_trap_vector`; `read_trap_vector` đọc lại để ảnh in bằng chứng.
- **Trap stack riêng**: `_start`/`_start_secondary` đặt `sscratch =` đỉnh trap stack **của hart
  đang chạy**. `trap_entry` làm `csrrw sp, sscratch, sp` rồi `addi sp, sp, -288` — trapframe 288 B
  nằm **trọn trong** `.trap_stack` của hart đó (P1 của review KT-45; KT-49 làm per-hart). Lưu `x0..x31` + `sepc/sstatus/scause/stval` (offset là
  `const` operand từ `plat-common`), gọi `trap_dispatch`; khôi phục **chỉ** `sepc` + thanh ghi,
  `sp` **cuối cùng**, rồi `sret`. **Không** ghi `sstatus` khi ra (sret tự khôi phục SIE/SPP — P2a).
- **Trap lồng (P2b)**: nếu sp bị ngắt nằm trong trap stack **của hart đang chạy**
  (`[trap_top − 16 KiB, trap_top)`, suy từ `sp` frame — KT-49) ⇒ `nested_trap_halt` in
  `PANIC: nested trap …` + `DebugHalt(1)`, không resume. Ảnh test chính dùng
  đúng đường này làm TAP cuối (fatal có chủ đích).
- **Decode**: bit 63 = interrupt; còn lại `Event::Exception` cho **0/1/2/3/4/5/6/7/8/9/12/13/15**
  và `Unknown(mã)` (host test đủ bảng). Chính sách: `breakpoint`/`illegal`/`ecall` resume đúng cỡ
  instruction (2/4 byte); page fault/access fault/misaligned/`Unknown` **fatal**.
  **KT-47**: `scause = (1<<63) | 5` ⇒ `Event::Timer` (**không** phải exception) — handler đếm tick,
  nạp deadline kế rồi **resume** bằng `sret`. **KT-48**: `scause = (1<<63) | 9` ⇒
  `Event::Interrupt { code: 9 }` đi vào PLIC **claim → xử lý → complete** (không fatal).
  **KT-50**: `scause = (1<<63) | 1` ⇒ `Event::Ipi` (nhánh riêng, không fatal — §13); ngắt mã
  **khác 1/5/9** vẫn fatal.
- **`sstatus.SIE` (P2a, KT-47)**: `trap_entry`/handler **không** ghi `sstatus`. Hardware khi trap:
  `SPIE ← SIE`, `SIE ← 0`, `SPP ← 1` ⇒ trapframe có `SPIE=1` (bị ngắt khi đang bật ngắt) và
  `SIE=0`; handler chạy với `SIE=0` nên **không** trap lồng; `sret` khôi phục `SIE ← SPIE`.
  Cổng `SIE` chỉ bật/tắt ở `timer::enable_timer`/`disable_timer` (`sie.STIE` trước rồi
  `sstatus.SIE`, đảo thứ tự khi tắt) — TAP `timer irq` đếm `trap_spie_on`/`handler_sie_off` bằng
  `ticks` và `sret_sie=1`.
- **Guard page fault (KT-46)**: trước `trap_dispatch`, nếu exception là page fault (12/13/15) và
  `stval ∈ [__stack_guard_start, __stack_guard_end)` thì đi đường riêng:
  - **mặc định fatal**: `GUARD_STATS` ghi nhận, `report_guard_fault` in
    `PANIC: guard page fault scause=0x… stval=0x… sepc=0x…` + `ra/sp/gp/tp` + `a0..a7` rồi
    `DebugHalt(1)` — dump + halt sạch, không treo.
  - **probe resume chỉ trong test**: `probe_guard_load` bật cờ `GUARD_RECOVER_LOAD` **một lần** cho
    đúng load page fault (`scause=13`) để ảnh guard kiểm được cả 13 và 15 trong một boot; handler
    in `# guard fault load scause=0xd …`, cộng `sepc` đúng 4 byte rồi resume. Mọi guard fault khác
    (và store fault) vẫn fatal.
  - Guard page là **NONE** ở cả identity, nửa cao và physmap ⇒ không có cửa sau; TAP 6 (ảnh chính)
    kiểm cả ba; ảnh guard kiểm fault **thật** vào `stvec`.
- **Vì sao page fault thật phải chờ KT-46**: OpenSBI v1.8.1 `delegate_traps()` đặt
  `MEDELEG = 0xf4b509`; bit 2 (illegal) và bit 9 (ecall-S) **không** delegate, nhưng page fault
  12/13/15 **được** delegate — D2-73 chuyển chứng minh `scause=13/15` sang KT-46 guard page. TAP:
  `breakpoint` thật, `ecall-S` qua SBI + resume, `illegal` **tiêm** (ghi rõ inject), trap lồng
  fatal; guard load/store thật.
- **Đo/log**: KT-45 `kt45-*`; KT-46 `v2/agent/JOURNAL/kt46-{red-ktest,host-tests,build,ktest,
  ktest-main-raw,guard-ktest-raw,boot,boot50,budget,gate-fast}.log` (số đo do script in, không chép
  tay — QD §9 Q12).

## 8. MMU Sv39 thật (KT-46)

### 8.1 Bố cục ánh xạ

QEMU `virt`, RAM vật lý `0x8000_0000..0x8800_0000` (`-m 128M`); `Δ = VA − 0x8020_0000`.

| Vùng | VA | PA | Leaf / quyền |
|---|---|---|---|
| kernel identity — image | `0x0000_0000_8020_0000..0x0000_0000_8040_0000` | `= VA` | **2 MiB** `V|R|W|X|A|D` (W^X đầy đủ KT-56) |
| kernel identity — NOLOAD | `0x0000_0000_8040_0000..arena_end` | `= VA` | **4 KiB**; 8 guard per-hart **NONE**; stack/trap stack của **mọi** hart + arena RW NX |
| kernel nửa cao — image | `0xFFFF_FFC0_4000_0000 + Δ` | `0x8020_0000 + Δ` | **2 MiB** RWX |
| kernel nửa cao — NOLOAD | `0xFFFF_FFC0_4000_0000 + Δ`, `Δ ≥ 0x20_0000` | `= 0x8040_0000 + …` | **4 KiB**; guard **NONE**; RW NX |
| physmap RAM | `0xFFFF_FFC0_0000_0000 + PA` | `PA` | **2 MiB NX** (`V|R|W|A|D`); chunk chứa guard tách 4 KiB và **khoét guard của từng hart** |
| UART0 (Device) | `0x0000_0000_1000_0000` | `= VA` | **4 KiB** `V|R|W|A|D`, X=0 |
| PLIC — cửa sổ thấp (Device) | `0x0000_0000_0c00_0000` | `= VA` | **2 MiB** `V|R|W|A|D`, X=0 (priority/pending/enable) |
| PLIC — cửa sổ cao (Device) | `0x0000_0000_0c20_0000` | `= VA` | **2 MiB** `V|R|W|A|D`, X=0 (threshold/claim) |
| guard per-hart | `base + h*0x1_5000 .. +0x1000`, hart 0 = `0x8040_0000` | — | **NONE** (không PTE hợp lệ) ở **cả** identity, nửa cao, physmap |

- Kernel **liên kết thấp** (`linker.ld` `0x8020_0000`) nên nó **chạy** ở identity mapping; nửa cao
  là **alias** để đọc/ghi (và để user root copy entry sau này — K3 chưa có user root). Đây là lựa
  chọn có chủ đích, không phải "quên chạy nửa cao": relocating image sang nửa cao cần linker/entry
  khác và không thuộc KT-46.
- **2 MiB cho image, 4 KiB cho NOLOAD** vì guard page cần granularity 4 KiB; linker đặt vùng NOLOAD
  sang `0x8040_0000` (`ALIGN(2M)`) để leaf 2 MiB của image không phủ guard. Physmap cũng dùng
  leaf 2 MiB (yêu cầu "2 MiB/1 GiB") và khoét guard.
- stack/trap stack per-hart là NOLOAD: **không nằm trong `filesz`** của LOAD (LOAD `memsz` vẫn
  phủ tới `__kernel_end`); KT-46 map **tường minh** cả hai (TAP 6 ghi/đọc qua nửa cao), KT-49 map
  cho **mọi** hart. Thiếu map ⇒ trap đầu tiên sau `satp` sẽ fault vào trap stack.
- MMIO map **4 KiB UART** + **2×2 MiB PLIC** tại đúng PA (D2-72 Q7; KT-48 mở rộng từ 4 KiB để phủ
  enable `0x2000 + 0x80*ctx` và context `0x20_0000 + 0x1000*ctx` cho `MAX_HARTS = 8` — §11).
  CLINT không map (timer qua SBI — KT-47).

### 8.2 Arena page table

- Root page table cấp từ **arena RAM** bắt đầu ở `__kernel_end` (sau record hart 7), cỡ **64 KiB**,
  zero từng bảng 4 KiB. Root = phần tử đầu arena; bảng trung gian cấp tăng dần (bump allocator).
- **Không** `static`/`static mut` cho bảng; không cộng vào trần `.data+.bss` (D2-72 Q5). Arena
  identity-map + nửa cao RW NX; nằm trong vùng NOLOAD 4 KiB nên cũng không đổi file ảnh.
- Nếu arena hết hoặc mapping sai (unaligned/trùng/không canonical/VA ngoài hình), `mmu::init` panic
  qua `PanicHandler` — **không** bật `satp` với bảng dở.

### 8.3 Bật `satp` hai bước + walk khớp

1. Dựng **xong** toàn bộ bảng: identity (image 2 MiB + NOLOAD 4 KiB), nửa cao, physmap NX, MMIO
   Device, guard NONE. Chưa đụng `satp`.
2. Lấy mẫu PTE của 8 VA (kernel identity/nửa cao, physmap, UART, PLIC, `.stack`, `.trap_stack`,
   arena) rồi `csrw satp` (`mode=8 | PPN(root)`) + `sfence.vma` trong **một** asm block.
3. Đọc lại `satp` bằng `csrr` (mode 8 + đúng PPN root); đọc lại **cùng 8 PTE** sau khi bật — phải
   **bằng** mẫu trước; walk phần mềm (`walk_sw`) phải trả đúng PTE đó.
4. **Hardware walk thật**: ghi qua identity rồi đọc qua nửa cao **và** physmap (và ngược lại) với
   một biến `.bss`; nếu MMU không hoạt động/alias sai thì giá trị không khớp. TAP 5/7 in
   `satp`, root, alias và số mẫu khớp.
5. Không có bước "bật thử rồi sửa": mọi mapping phải xong trước `satp`; sau đó chỉ đọc/kiểm, không
   sửa bảng (tránh quên `sfence.vma` — nợ mutation của KT-51).

### 8.4 Guard page và bằng chứng fault thật

- Mỗi hart một guard 4 KiB **ngay dưới** stack của hart đó (`guard_owner(pa)` trả hart); entry
  **không hợp lệ** ở identity, nửa cao và physmap. Chạm vào (dù đọc hay ghi) ⇒ **page fault phần cứng thật** vào `stvec`:
  - đọc ⇒ `scause=13` (`LoadPageFault`), `stval` = địa chỉ trong guard;
  - ghi ⇒ `scause=15` (`StorePageFault`), `stval` = địa chỉ trong guard.
- Handler guard in bằng chứng + (mặc định) `PANIC: guard page fault …` + thanh ghi rồi
  `DebugHalt(1)`. Ảnh `vifast-kbin-ktest-guard` kiểm **cả hai** `scause` trong một boot: load
  `scause=13` qua probe resume-một-lần, store `scause=15` fatal (dump + halt). Đây là page fault
  **thật**, không phải trap tiêm — khác `illegal` của D2-73.
- Guard cũng **NONE** qua physmap (chunk 2 MiB chứa guard được tách 4 KiB) nên không có cửa sau
  đọc/ghi trang bảo vệ bằng alias vật lý.

### 8.5 Phạm vi KT-46 (không làm)

Xem bảng §9. KT-46 **không** timer, PLIC IRQ, SMP/PerHart, IPI, ASID/shootdown, object layer/
Retype, U-mode, W^X đầy đủ, pager/COW, soak 1.000 — các task sau.

## 9. Phạm vi: KT-49 **không** làm gì

| Không làm | Thuộc |
|---|---|
| IPI thật (SBI IPI/ACLINT MSWI), hộp thư drain, `TlbFlush`/`VmKick` | KT-50 |
| Hotplug/suspend hart, `hart_stop`, scheduler đa hart/preemption | K3b + task riêng |
| PLIC ngắt theo hart **thật** cho nguồn khác UART (mỗi hart chỉ init context + round-trip IRQ của mình) | KT-50+ |
| ASID thật, `sfence.vma` theo ASID/VA, shootdown ack + timeout, mutation bỏ flush | KT-51 |
| AIA (APLIC/IMSIC), MSI, EOI/priority động | K4'/K5' (D2-72 Q8) — task riêng |
| Object layer `no_std` + Retype nội bộ (VSpace/PageTable/Frame từ Untyped) | KT-52 |
| U-mode, 3 syscall, nạp `init` ELF, user root copy entry nửa cao | KT-53 |
| Harness TAP **đầy đủ** cho mọi ảnh/`.scn` (KT-49 mới có `expected.txt` + matrix cho 2 ảnh test K3) | KT-54 |
| Conformance 5 `.scn` × sim/QEMU | KT-55 |
| `FaultKind` thật + W^X đầy đủ (tách text/rodata RWX→RX) + một hunk `spec/abi.toml` | KT-56 |
| Soak 1.000 boot + K3-DOD + perf (**soak KHÔNG icount** — D2-74 điều kiện (c)) | KT-57 |
| Port core `no_std` (CNode/TCB/IPC/sched) | **K3b** (D2-72 Q13) |
| Đo **jitter timer thật** (không icount) / preemption thật theo timer | task riêng — D2-74 điều kiện (d) |

## 10. Timer SBI (KT-47) — tick 100 Hz, timebase 10 MHz, deadline tuyệt đối

Nguồn quyết định: **D2-72 Q6** (SBI TIME, 100 Hz, timebase 10 MHz, Sstc hoãn) và **D2-74**
(deviation `-icount shift=6` **chỉ** trong `ktest.sh`, §10.5). Task: K3-PLAN §4.1 KT-47.

### 10.1 Ràng buộc và hằng số `[TK]`

| Hằng số | Giá trị | Nghĩa |
|---|---|---|
| `timer::TIMEBASE_HZ` | **10.000.000** | timebase QEMU `virt`; `rdtime` (Zicntr) đếm theo đơn vị này |
| `timer::TICK_HZ` | **100** | tick kernel = 10 ms |
| `timer::TICK_TICKS` | **100.000** | một tick tính bằng đơn vị timebase (`TIMEBASE_HZ / TICK_HZ`) |
| `timer::TICK_NS` | **10.000.000** | một tick tính bằng ns |
| `timer::TICK_DELTA_MIN_NS` / `TICK_DELTA_MAX_NS` | **9.000.000** / **11.000.000** | DoD KT-47: Δ tick ∈ [9,11] ms — **không nới** |

- **SBI TIME**: EID `0x5449_4D45`, FID `0` = `sbi_set_timer(uint64 deadline)`; deadline **tuyệt
  đối**, đơn vị timebase. `ecall` từ S-mode đi M-mode/OpenSBI (SBI bảo toàn mọi thanh ghi trừ
  `a0`/`a1`). Không dùng CLINT trực tiếp (KT-46 chỉ map UART/PLIC), **không** Sstc (D2-72 Q6).
- **`sie.STIE`** (bit 5) bật nguồn ngắt S-mode timer; **`sstatus.SIE`** (bit 1) mở cổng.
- Một **deadline/hart**: deadline nằm trong slot **PerHart** `TIMER_STATS.get(hart)` (KT-49);
  `sbi_set_timer`/`sie.STIE` là CSR per-hart.
- OpenSBI banner `Platform Timer Device : aclint-mtimer @ 10000000Hz` là log `[ĐO]` để đối chiếu
  timebase — không chép tay vào tài liệu (QD §9 Q12).

### 10.2 Deadline tuyệt đối + phân loại tick

Handler **không** đặt `deadline = now + TICK_TICKS` (kiểu tương đối sẽ cộng dồn độ trễ). Nó dùng
`timer::classify(last, deadline, now)`:

```text
delta       = now − last                          (khoảng cách giữa hai lần quan sát)
late_by     = now − deadline        (0 nếu ngắt đến sớm hơn deadline)
missed      = late_by / TICK_TICKS  (số deadline bị bỏ; 0 ⇔ chỉ là độ trễ ngắt)
next        = deadline + (missed+1)·TICK_TICKS    (luôn > now)
```

- `missed > 0` ⇔ **mất tick** ("trễ trọn một chu kỳ"): deadline kế không thể giao riêng nữa nên
  các deadline bị bỏ được **nhảy qua một lần** (không dồn interrupt).
- `next` là **bội số tuyệt đối** của deadline cũ ⇒ sai số không cộng dồn (không trôi), khác hẳn
  cách cộng dồn từ `now`.
- `TimerCounters` cộng dồn `ticks`, `late`, `missed`, `non_monotonic` (đếm `now <= last`),
  `max_delta`/`min_delta` (đo drift), `max_late`; `record_sstatus` đếm bằng chứng `sstatus`.
- Hàm `classify` **thuần** (không asm/atomic) nên host test phủ cả trường hợp trễ/mất tick mà phần
  cứng khó tạo (`tests/timer.rs`).

### 10.3 API `Platform` + cài đặt arch

`vifast-plat-common::Platform` (KT-47 thêm; chữ ký ổn định cho KT-48+):

| Method | Hợp đồng |
|---|---|
| `now_ticks() -> u64` | `rdtime` thô, đơn vị timebase; monotonic |
| `now_ns() -> u64` | quy đổi từ `now_ticks` (`ticks_to_ns`, 10 MHz ⇒ chính xác tuyệt đối) |
| `timebase_hz() -> u64` | 10 MHz (tên cũ `tick_hz` đã đổi cho khỏi lẫn với `TICK_HZ = 100`) |
| `set_timer_deadline(deadline)` | `sbi_set_timer`; deadline tương lai cũng **xoá** ngắt đang chờ; `u64::MAX` = tắt hữu hiệu |
| `enable_timer() -> bool` | `sie.STIE = 1` **rồi** `sstatus.SIE = 1`; `true` ⇔ đọc lại thấy cả hai |
| `disable_timer()` | `sstatus.SIE = 0` **rồi** `sie.STIE = 0` (đảo thứ tự enable) |

Cài đặt asm/CSR/SBI nằm ở `vifast-plat-riscv64::timer` (`unsafe` thuộc danh sách đóng D2-17):
`now_ticks`, `read_sie`, `read_sstatus`, `set_timer_deadline`, `enable_timer`, `disable_timer`,
`init()` (arm deadline đầu `now + TICK_TICKS` → `set_timer` → bật cổng), `on_timer_interrupt`
(đếm tick + trả deadline kế). Toàn bộ *ngữ nghĩa* (hằng số, `classify`, bộ đếm) ở plat-common để
chạy host (D2-72 Q12).

### 10.4 `sstatus.SIE` khi vào/ra trap (P2a) + đường trap timer

1. **Bật**: chỉ `enable_timer` ghi `sstatus.SIE = 1`, **sau** khi `set_timer(deadline)` đã nạp ⇒
   không có cửa sổ "SIE bật nhưng chưa có deadline". `sie.STIE` bật **trước** `sstatus.SIE`.
2. **Vào trap** (hardware): `SPIE ← SIE`, `SIE ← 0`, `SPP ← 1`; `trap_entry` lưu trapframe rồi
   gọi `trap_dispatch` — **không** ghi `sstatus` (KT-45 P2a).
3. **Trong handler**: `SIE = 0` (không trap lồng). `Event::Timer` ⇒ `on_timer_interrupt` ghi
   `# timer …`? **Không**: handler chỉ đếm + nạp deadline kế, **không in** (in trong handler làm
   trễ tick); `TRAP_STATS.record_interrupt(scause)` ghi `last_scause = (1<<63)|5`.
4. **Ra trap**: asm khôi phục thanh ghi + `sepc`, **không** ghi `sstatus`; `sret` khôi phục
   `SIE ← SPIE` ⇒ sau trap ngắt lại bật. Bằng chứng: `trap_spie_on == ticks` (SPIE=1 trong
   trapframe), `handler_sie_off == ticks` (SIE=0 trong handler), `sret_sie=1` (đọc sau khi về),
   `disabled_sie=0`/`sie_stie=0` sau `disable_timer`.
5. **Tắt trước test trap lồng**: `set_timer_deadline(u64::MAX)` (dập ngắt đang chờ) rồi
   `disable_timer()` — nếu không, timer có thể chen vào probe trap lồng và đổi dòng PANIC.

### 10.5 Đo drift: `-icount shift=6` **chỉ** trong `ktest.sh` (deviation D2-74)

- Với **đúng dòng D2-72 Q2** (không icount), QEMU TCG trên máy đo phục vụ **timer ảo**
  (`QEMU_CLOCK_VIRTUAL`/ACLINT) rất thưa: thời điểm ngắt đến trễ hơn deadline một khoảng lớn và
  thất thường (main loop của QEMU chỉ xử lý timer khi vCPU nhường), nên Δ tick đo được **không**
  phản ánh toán deadline của kernel. Đây là giới hạn của **host/harness**, không phải của kernel:
  cùng một ảnh, cùng kernel, chỉ đổi cách QEMU cấp thời gian ảo.
- Vì vậy **D2-74**: `v2/test/boot/ktest.sh` thêm **`-icount`** (ảnh **test**) ⇒ thời gian ảo tiến
  theo lệnh đã chạy, timer bắn đúng hạn, Δ tick đo bằng `rdtime` nằm trong [9,11] ms và **1.000
  tick không mất**. Ngưỡng **[9,11] ms giữ nguyên** — không nới.
- **Shift icount CỐ ĐỊNH = 6** (64 ns/lệnh), **không** dùng shift tự động (`auto`): `auto` tự đổi
  shift giữa chừng để bám thời gian thật ⇒ thời gian ảo nhảy, Δ tick có lần lệch hẳn khỏi 10 ms
  (đo được: có tick ~3,4 ms và ~16,6 ms trong cùng một lần chạy) ⇒ TAP `deadline drift` đỏ **ngẫu
  nhiên**. Shift cố định giữ Δ = 10 ms ± quantization của icount (mỗi lệnh = 64 ns), vẫn trong
  [9,11] ms và **ổn định** giữa các lần chạy (2 lần liên tiếp PASS). Đây là nội dung **D2-74**
  (đính chính 2026-11-15): vẫn chỉ `ktest.sh`, vẫn không nới ngưỡng.
- **`run.sh` (ảnh sản phẩm) và soak KT-57 KHÔNG icount** (đúng D2-72 Q2): ảnh sản phẩm arm timer
  và chờ **≥1 tick** (timeout 1 s) rồi in `# timer: ticks=…` — bằng chứng boot thật vẫn chạy; vì
  QEMU phục vụ timer ảo thưa, `delta_ns`/`late` của lần boot **không icount** có thể lớn (log
  riêng) và **không** dùng để đo drift.
- Phép đo icount chứng minh: toán deadline tuyệt đối, đếm tick/không mất tick, decode
  `scause=0x8000000000000005` thành `Event::Timer`, đường `sie.STIE`/`sstatus.SIE` và `sret`.
  Nó **không** đo jitter timer thật của host — việc đó (nếu cần) là **task riêng**, không đổi D2
  (D2-74 điều kiện (d)).
- Số đo thật: `v2/agent/JOURNAL/kt47-icount-ktest-run1.log` + `kt47-icount-ktest-run2.log` (2 lần
  liên tiếp, có icount), `kt47-boot50.log` (product, không icount),
  `kt47-qemu-no-icount-vs-icount.log` (A/B cùng ảnh) — tài liệu **không** chép số tay (QD §9 Q12).

### 10.6 TAP timer + bằng chứng (ảnh test chính, `1..30` — timer `11..14`)

| TAP | Điều kiện đo |
|---|---|
| `ok 11 - timer 100 ticks monotonic` | ≥100 ngắt timer **thật** qua `stvec`; `non_monotonic = 0` (mỗi tick `rdtime` tăng); `now > last` |
| `ok 12 - timer no lost tick` | 1.000 tick liên tiếp, `late = missed = 0`, `non_monotonic = 0` |
| `ok 13 - timer irq` | `TRAP_STATS.interrupts ≥ 1.000`, `last_scause = 0x8000000000000005`, `last_code = 5`, **0** exception phát sinh trong lúc chạy; `trap_spie_on = handler_sie_off = ticks`, `sie_stie=1`, `sret_sie=1` |
| `ok 14 - timer deadline drift` | `ticks_to_ns(min_delta)` ≥ 9 ms và `ticks_to_ns(max_delta)` ≤ 11 ms, `late = 0` |

Bằng chứng in ra (runner `grep` đúng các dòng này, `ktest.sh`): `# timer arm: … tick_ticks=100000
tick_hz=100 timebase_hz=10000000 sie_stie=1 sstatus_sie=1`, `# timer ticks: … non_monotonic=0
late=0 missed=0`, `# timer irq: … scause=0x8000000000000005 code=5`, `# timer drift: … in_range=1`,
`# timer sie: … sret_sie=1`, `# timer disable: sie_stie=0 sstatus_sie=0`. Ảnh sản phẩm in
`# timer arm: …` + `# timer: ticks=…` (≥1 tick, không icount).

### 10.7 Không thuộc KT-47

PLIC/UART IRQ (KT-48), SMP/PerHart (KT-49), IPI (KT-50), ASID/shootdown (KT-51), object layer
(KT-52), U-mode/syscall (KT-53), harness matrix/`expected.txt` (KT-54), conformance (KT-55),
`FaultKind`/W^X (KT-56), soak/K3-DOD (KT-57), preemption thật + đo jitter host (task riêng —
D2-74 (d)). Xem bảng §9.

**Giới hạn của phép đo KT-47** (reviewer nêu — ghi để không hiểu quá phạm vi bằng chứng):

1. Δ tick **chỉ đúng dưới icount**: số đo chứng minh toán deadline/đếm tick/đường trap của kernel,
   **không** chứng minh jitter timer thật của host (QEMU không icount phục vụ timer ảo thưa).
2. `late_by` đo được rất nhỏ (≤ vài trăm µs) nên **chưa phân biệt** được deadline tuyệt đối với
   cách `now + TICK`: cả hai đều cho Δ trong [9,11] ms khi drift nhỏ. Việc phân biệt dựa vào
   **host test** `classify` (trễ/mất tick/saturate) + thiết kế tuyệt đối, không dựa vào boot TAP.
3. `shift=6` **chưa** kiểm dưới tải host nặng: guest time là tất định, nhưng wall time tăng theo
   tải (timeout 20 s/boot vẫn còn dư).
4. `smp > 1` + icount **đã kiểm ở KT-49** (§12: matrix 1/2/4, per-hart timer 100 tick/hart).
5. Đo **jitter thật** (không icount) là **task riêng** — D2-74 điều kiện (d); không đổi D2 này.

## 11. PLIC + UART IRQ (KT-48) — claim/complete, context `2*hart+1`, 0 IRQ giả

Nguồn quyết định: **D2-72 Q7/Q8** (địa chỉ hardcode, `ndev` đọc từ register, context `2*hart+1`;
PLIC **cổ điển**, AIA hoãn K4'/K5'), task K3-PLAN §4.1 KT-48. Code: `vifast-plat-common::plic`
(logic an toàn, test host) + `vifast-plat-riscv64::plic` (MMIO/CSR, `unsafe` thuộc danh sách đóng
D2-17); `Platform::enable_irq`/`disable_irq` giờ là **PLIC thật**; `trap_dispatch` có nhánh
`Event::Interrupt { code: 9 }`.

### 11.1 Hằng số `[TK]` (PLIC cổ điển)

| Hằng số | Giá trị | Nghĩa |
|---|---|---|
| `PLIC_BASE` | `0x0c00_0000` | dữ kiện nền tảng QEMU `virt` (D2-72 Q7) |
| cửa sổ map | **2 × 2 MiB** | `0x0c00_0000..0x0c20_0000` (priority/pending/enable) + `0x0c20_0000..0x0c40_0000` (threshold/claim), Device RW NX; phủ context 15 của `MAX_HARTS = 8` |
| `context_of_hart(h)` | `2*h+1` | context **S-mode**; M-mode `2*h` **không** bật (TAP kiểm `enable_mmode=0`) |
| priority UART | **1** | `priority[10]` (PLIC chỉ forward nguồn priority > threshold) |
| threshold | **0** | context S-mode |
| IRQ UART0 | **10** | D2-72 Q7 |
| `IER.TX-empty` | `0x02` | mức cao khi THR rỗng ⇒ nguồn **level** thật, không phải trap tiêm |
| `UART_IRQ_LIMIT` | **8** | số ngắt thật thu rồi handler tự tắt (`IER` + enable) — không ngập |
| `PLIC_MAX_SOURCES` | **1023** | trần probe `ndev` (id hợp lệ `1..=1023`) |

### 11.2 Bản đồ đăng ký + cách đọc `ndev`

```text
0x0000 + 4*irq            priority[irq]          (IRQ 1..=ndev)
0x1000 + 4*(irq/32)       pending                (chỉ đọc)
0x2000 + 0x80*ctx + 4*w   enable word w của context
0x20_0000 + 0x1000*ctx    +0 threshold, +4 claim (đọc = claim, ghi = complete)
```

PLIC spec **không** có thanh ghi `ndev`; QEMU chỉ hiện thực priority cho nguồn `1..=ndev` và trả 0
cho nguồn không tồn tại ⇒ `probe_ndev` ghi 1, đọc lại, dừng ở nguồn đầu tiên không giữ giá trị
(không giả định 95 — D2-72 Q7). Probe là **chẩn đoán không phá huỷ** theo **D2-75** (2026-11-15):
mỗi nguồn `write-1 → read-back → ghi 0 → đọc lại 0`, panic nếu khôi phục thất bại; K3 **sở hữu**
PLIC nên trả priority 0 là an toàn và `init` đặt lại `priority[10] = 1` sau probe. Probe được phép
ở **cả ảnh sản phẩm**. Giá trị `ndev` của lần boot nằm trong log `# plic:`
(`v2/agent/JOURNAL/kt48-*.log`), **không** chép tay vào tài liệu (QD §9 Q12).

### 11.3 Đường ngắt UART (level → claim → complete → re-arm)

1. `plic::init`: probe `ndev` (panic nếu không phủ IRQ 10), `threshold = 0`, `priority[10] = 1`,
   **set** bit enable của context `2*hart+1`; đọc lại kiểm context S-mode bật, **context M-mode
   cùng hart = 0**, claim lúc chưa bật nguồn = 0.
2. Bật `IER.TX-empty` (ghi **đúng** 0x02 để RX/line khác không sinh IRQ) **rồi** `sie.SEIE` +
   `sstatus.SIE` (`plic::enable_external` — nguồn trước, cổng sau, đối xứng
   `timer::enable_timer`; P2a: trap entry/handler **không** ghi `sstatus`).
3. Trap `scause = (1<<63)|9` → `trap_dispatch` → `plic::on_external_interrupt`:
   **claim** context hiện tại → `PLIC_STATS.begin_claim` → line 10: đọc `IIR` (`0x2` = TX-empty),
   đếm, **complete**, rồi **re-arm** `IER` `0 → 2`. PLIC cổ điển là **level-triggered có gateway**:
   sau claim, nguồn còn mức cao **không** tự pending lại (gateway chờ nguồn xuống thấp) ⇒ nếu chỉ
   claim/complete thì TX-empty chỉ cho **đúng một** ngắt; re-arm tạo **level transition** để có
   ngắt kế. Đủ `UART_IRQ_LIMIT` ngắt: tắt `IER` + clear enable (không ngập) rồi complete.
4. Line **lạ**: đếm `unexpected` + **complete** (không treo, K3-PLAN §4.1). Claim 0: spurious —
   **không** complete (PLIC spec).
5. Handler **không in** (in làm trễ/ngập log); TAP đọc `PLIC_STATS` sau. `trap_dispatch` ghi
   `TRAP_STATS.record_interrupt(scause 9)` cho mỗi ngắt external; hardware đã xoá `sstatus.SIE`
   khi vào trap nên không lồng.

### 11.4 TAP (ảnh test chính, `1..15`) + bằng chứng

| TAP | Điều kiện đo |
|---|---|
| `ok 8 - plic window` | 2 cửa sổ PLIC walk thấy leaf **2 MiB Device RW NX** đúng PA; `ndev` **đọc từ register** > 10; context = `2*hart+1`; threshold 0; priority[10] = 1; enable context S-mode = 1, **context M-mode = 0**; claim rỗng = 0; **`Platform::disable_irq(10)` → đọc lại tắt, `Platform::enable_irq(10)` → đọc lại bật** (wrapper thật trên HW) |
| `ok 9 - uart irq` | `IER.TX-empty` + `sie.SEIE` bật; `uart_irqs = UART_IRQ_LIMIT (= 8 `[TK]`) ≥ 1`; `claims == completes`; `last_irq = 10`; `last_iir = 0x2` (TX-empty); handler đã tắt `IER` + enable; `traps_delta == claims`; 0 exception |
| `ok 10 - plic no fake irq` | **0 IRQ giả**: `spurious = unexpected = 0`; claim sau khi tắt = 0; `external_traps == claims + spurious`; `interrupts_delta == external_traps`; cổng external tắt **im lặng** ~1 ms; 0 exception |
| `ok 11..14 - timer …` | 4 TAP timer KT-47 chạy **sau** PLIC trong **cùng boot** ⇒ hồi quy timer sau khi bật PLIC (100 tick đơn điệu, 1.000 tick không mất, `scause=0x8…005`, drift [9,11] ms) |
| `ok 20..24 - ipi …` / `ok 25..29 - tlb …` / `ok 30 - trap nested panic` | KT-50: 5 TAP IPI (20..24, §13.5); KT-51: 5 TAP TLB/ASID/shootdown (25..29, §14.5) rồi trap lồng fatal (KT-45, boot kết thúc `PANIC` có chủ đích); TAP 15..19 là SMP KT-49 (§12) |

Bằng chứng in ra (runner `grep` đúng các dòng, `ktest.sh` — số đo thật nằm trong log, không chép
tay): `# plic: … ndev=… ctx=… plat_off=1 plat_on=1 threshold=0 priority_uart=1 enable_ctx=1
enable_mmode=0 claim_idle=0`; `# plic map: … low_dev=1 high_dev=1 low_2m=1 high_2m=1 low_nx=1 high_nx=1`;
`# uart irq: irqs=… claims=… completes=… traps_delta=… balanced=1 spurious=0 unexpected=0
last_irq=10 last_iir=0x2 ier_off=1 plic_enable=0`; `# plic quiet: claim_idle=0 quiet=1
stats_unchanged=1 external_traps=… interrupts_delta=… spurious=0 unexpected=0`. Ảnh **sản phẩm** cũng in `# plic:` + `# uart irq:` (boot thật, **không**
icount) và `PANIC` nếu sai — cùng một handler/PLIC code với ảnh test.

### 11.5 Vì sao "context đúng hart"

- Context S-mode = `2*hart+1` (D2-72 Q7): hart 0..3 ⇒ 1/3/5/7 — phủ bằng **host test**
  `tests/plic.rs`; lần boot hart 0 in `ctx=1`.
- Init chỉ bật bit enable của **đúng context hiện tại** và kiểm **context M-mode cùng hart = 0**;
  handler claim/complete trên **cùng** context. Sai context ⇒ claim trả 0 (spurious) và TAP 9/10
  đỏ — không có đường "may mắn".
- `smp > 1` đã kiểm ở **KT-49** (§12): mỗi hart tự init context `2*h+1` của mình và TAP 18 đòi
  đủ `N` dòng `# plic hart=h … ctx=2h+1 … ok=1`, context M-mode `2*h` tắt.

### 11.6 Giới hạn phép đo

1. KT-48 chỉ **1 hart**; KT-49 đã boot `N = 1/2/4` và TAP 18 kiểm context `2*h+1` của **từng hart
   trong cùng một boot** (bằng chứng trong log `kt49-*`), nên giới hạn "chỉ hart 0" **hết**.
2. Probe `ndev` dựa vào hành vi QEMU (priority read-back); phần cứng khác cần DTB/khác (chưa có
   `vifast-dtb`, D2-72 Q7).
3. Chỉ **UART0 line 10**; IPI (KT-50), AIA/MSI, EOI/priority động **chưa** làm.
4. "0 IRQ giả" là bằng chứng **đếm** (`spurious`/`unexpected` = 0 + claim rỗng + im lặng sau khi
   tắt cổng), không phải phép đo độc lập của phần cứng; claim sai context sẽ lộ ở `# plic:`/TAP 9
   (claim 0 ⇒ spurious ⇒ đỏ).
5. Timer sau PLIC đo dưới `-icount shift=6` (D2-74) — cùng giới hạn §10.7; ảnh sản phẩm **không**
   icount chỉ đòi ≥1 tick (và PLIC/IRQ không phụ thuộc icount).

## 12. SMP — SBI HSM + PerHart (KT-49)

Nguồn quyết định: **D2-72 Q4** (hart theo DTB, `MAX_HARTS = 8` `[TK]`, matrix `1/2/4`, BSP bật AP
bằng SBI HSM sau khi dựng arena), **D2-72 Q7** (DTB chỉ kiểm magic), task K3-PLAN §4.1 KT-49. Code:
`vifast-plat-common::smp` (hằng số bố cục, `HartMask`, phân loại trạng thái SBI HSM — test host
được) + `vifast-plat-riscv64::smp` (SBI `ecall`, barrier, trạng thái runtime) + asm entry/trap ở
`vifast-plat-riscv64::lib` + linker script vùng NOLOAD per-hart + pha test SMP trong
`vifast-kbin-riscv64/src/ktest_smp.rs` (tách khỏi `ktest.rs` theo R1 ≤ 1.500 dòng).

### 12.1 Vì sao hart thứ cấp cần SBI HSM

OpenSBI `fw_dynamic` với `-smp N` chỉ nhảy vào S-mode ở **hart boot** (hart 0 — OpenSBI banner
`Domain0 Boot HART`); các hart còn lại nằm trong firmware ở trạng thái `STOPPED`. Chúng chỉ chạy khi
hart boot gọi **`sbi_hart_start(hartid, start_addr, opaque)`** (EID `0x48534D`, FID `0`); SBI spec
§8.1: hart được đánh thức vào S-mode với `satp = 0` (Bare), `a0 = hartid`, `a1 = opaque`, ngắt tắt.
Vì vậy:

1. Hart boot làm xong MMU (`mmu::init`) **trước** khi start AP — AP thừa hưởng cùng root page table
   qua `csrw satp` + `sfence.vma` (`mmu::adopt_satp`) khi vào.
2. `_start_secondary` (asm) nhận `a0 = hartid`, `a1 = opaque`; `tp ← a0`; `sp`/`sscratch` tính từ
   **record per-hart**; gọi `rust_secondary_main` của ảnh (bin cung cấp).
3. `smp::secondary_setup(opaque)` kiểm `opaque == "KT49"`, adopt `satp`, cài `stvec` (CSR per-hart),
   set bit `ONLINE` của mình rồi chờ `PHASE` (park bằng **spin**, không `wfi`: K3 chưa có IPI để
   đánh thức — KT-50). BSP phát `release()` khi vào pha SMP.
4. Hart lỗi (start trả lỗi, không online, timeout) ⇒ **log + report**, **không panic cả máy** —
   kể cả ảnh sản phẩm (chỉ in `# smp: WARN …` rồi tiếp tục; `run.sh` đỏ nếu `harts` khác `-smp N`
   nên cấu hình hỏng không bị che). Ảnh **test** để TAP 15 đỏ và các pha sau chỉ chạy trên hart
   online.

### 12.2 Bố cục NOLOAD per-hart `[TK]`

| Hằng số (`plat-common::smp`) | Giá trị | Nghĩa |
|---|---|---|
| `MAX_HARTS` | **8** | trần hart K3 (D2-72 Q4); matrix kiểm `1/2/4` |
| `HART_GUARD_BYTES` | **0x1000** | guard page mỗi hart (NONE) |
| `HART_STACK_BYTES` | **0x10000** | stack mỗi hart, `sp` ở đỉnh, mọc xuống guard của hart đó |
| `HART_TRAP_STACK_BYTES` | **0x4000** | trap stack mỗi hart; trapframe 288 B nằm trọn trong vùng |
| `HART_STRIDE` | **0x1_5000** | `guard + stack + trap stack` (record của một hart) |
| `HART_AREA_BYTES` | **0xA_8000** | `MAX_HARTS × HART_STRIDE` — vùng NOLOAD tổng |
| `ARENA_BYTES` | **64 KiB** | page table (KT-46), bắt đầu ở `__kernel_end` (sau record hart 7) |

```text
record hart h = [base + h*0x1_5000, base + (h+1)*0x1_5000)
  +0x0000 guard  (4 KiB, NONE ở identity / nửa cao / physmap)
  +0x1000 stack  (64 KiB, RW NX; đáy = guard của chính hart này)
  +0x11000 trap stack (16 KiB, RW NX; `sscratch` = đỉnh)
```

- Linker: `.stack_guard`/`.stack`/`.trap_stack` giữ nghĩa **hart 0**; `.hart_stacks` cấp địa chỉ
  cho 7 record còn lại; `__kernel_end` sau record hart 7. `mmu::init` **panic** nếu symbol linker
  không khớp hằng số `plat-common::smp` (`guard_offset`/`stack_top_offset`/`trap_top_offset`/
  `HART_AREA_BYTES`) — chặn rủi ro "stack AP chồng guard hart khác" của K3-PLAN.
- `mmu::map_kernel` map **mọi** trang NOLOAD trừ **guard của từng hart** (4 KiB leaf RW NX, cả
  identity và nửa cao); `mmu::map_physmap` khoét **mọi** guard khỏi chunk 2 MiB ⇒ không có cửa sau
  qua alias vật lý. `mmu::guard_owner(pa)` trả hart sở hữu guard.
- `trap_entry` đổi `sp` sang trap stack **của hart đang chạy** (tính `addi sp, sp, -TF_SIZE` từ
  `sscratch`), đặt lại `sscratch = frame + TF_SIZE`, và phát hiện trap lồng bằng
  `[trap_top − HART_TRAP_STACK_BYTES, trap_top)` — **không** dùng symbol toàn cục một hart như
  KT-45; `trapframe` ghi vào slot `smp::last_trap_frame(hart)` để TAP chứng minh `.trap_stack`
  per-hart.

### 12.3 Hart theo DTB: probe `hart_get_status`, từ chối hart ngoài máy

`plat-common::smp` phân loại mã `sbi_hart_get_status`:

| Mã SBI HSM | `HartStatus` | Quyết định của BSP |
|---|---|---|
| `0 STARTED` | `Started` | `AlreadyStarted` — không start lại, chờ online |
| `1 STOPPED` | `Stopped` | **`Start`** — gọi `sbi_hart_start(h, _start_secondary, "KT49")` |
| `2/3` pending | `Pending` | `AlreadyStarted` — đang chuyển, không start lại |
| `4/5/6` suspend | `Suspended` | `Reject` (K3 không hotplug/suspend) |
| `-3 INVALID_PARAM` | `Invalid` | **`Reject`** — hart **không có trong DTB/domain**; log `# smp: hart h … ngoai DTB — tu choi start`, **không panic cả máy** |
| lỗi khác | `Error(code)` | log mã, không start hart đó |

- Đây là "hart theo DTB" **không cần parse FDT**: firmware (OpenSBI) đã đọc DTB/`-smp` và trả
  `-3` cho hart ngoài máy; D2-72 Q7 vẫn giữ "chỉ kiểm magic DTB". Matrix 1/2/4 kiểm cả hai phía:
  `-smp 1` ⇒ hart 1..7 bị từ chối; `-smp 4` ⇒ chỉ hart 4..7 bị từ chối.
- **DTB magic**: QEMU `virt` đặt FDT ở **cuối RAM** (ngoài miền kernel identity-map), nên
  `dtb_magic_ok` đọc 4 byte đầu qua **physmap** (`mmu::physmap_alias`) — không thêm mapping mới,
  không tin `a1` mù. `a1` được `_start` lưu vào `SMP_DTB` **sau** khi zero `.bss`.
- `smp::init` chờ `ONLINE ⊇ PRESENT` với timeout 2 s; hết hạn ⇒ `timed_out = true`, log hart thiếu,
  trả report — **không treo**. `hart_start` trả `0` hoặc `-6 ALREADY_AVAILABLE` ⇒ coi là thành
  công; `PRESENT` gồm hart probe được (kể cả hart start lỗi, để TAP báo thiếu online).

### 12.4 Bộ đếm **PerHart** (timer/PLIC) — không lẫn giữa các hart

- `timer::TIMER_STATS` (KT-47) và `plic::PLIC_STATS` (KT-48) đổi từ một bộ đếm toàn cục thành
  **PerHart**: `TIMER_STATS.get(hart)`/`PLIC_STATS.get(hart)` là slot riêng của hart (mảng
  `[T; MAX_HARTS]`, `const`-init; `unsafe` không cần). Handler timer/external chạy trên hart nào
  cộng slot `hart_id()` của hart đó; `sbi_set_timer`/`sie.STIE`/`sstatus.SIE` là CSR per-hart nên
  deadline/nhịp của hart này không đụng hart kia.
- TAP 17 đòi mỗi hart `ticks ≥ 100`, `non_monotonic = 0`, `first` đúng bằng `rdtime` lúc **hart đó**
  arm (`snap.first == arm.now`), `armed = 1` ⇒ slot là của chính hart (không phải số gộp).
- TAP 18 đòi hart **boot** (hart chạy các TAP KT-48) vẫn giữ `uart_irqs = UART_IRQ_LIMIT` trong
  khi các hart khác `PLIC_STATS.get(h).traps = 0` (không claim external nào) ⇒ không lẫn. Dùng
  `boot` chứ không hardcode hart 0: OpenSBI có thể chọn boot hart khác 0 (log ảnh sản phẩm
  `-smp 4` không icount từng ghi `Domain0 Boot HART: 2`); kernel xử lý theo `a0` của lần boot.
- `TRAP_STATS`/`GUARD_STATS` **vẫn gộp toàn cục** (atomic): TAP 2..14 của KT-45..48 chạy **trước**
  pha SMP và SMP chỉ so **delta**/slot riêng; đây là phạm vi có chủ đích, ghi ở §12.8.5.

### 12.5 Barrier có timeout — "hart chậm không treo"

API (`vifast-plat-riscv64::smp`, dựng trên `HartMask` của `plat-common`):

```text
barrier_begin(round, expected)   BSP: xoá arrival, đặt expected, công bố round
barrier_join(round, timeout)     mọi hart: chờ round được công bố, set bit arrival của mình
barrier_wait_arrivals(timeout)   BSP: chờ arrivals == expected, trả (mask, timed_out)
barrier_release(round)           BSP: phát (released = round, TĂNG ĐƠN ĐIỆU)
barrier_wait_release(round)      hart: chờ released ≥ round
```

- `released` **không reset về 0** mỗi vòng (bug đo được khi boot `smp=2/4`: BSP sang vòng mới xoá
  release cũ trước khi AP kịp đọc ⇒ AP chờ hết timeout). Nó là số vòng tăng đơn điệu.
- Mỗi vòng barrier đều có timeout (1 s cho vòng thường, **200 ms** cho vòng "có hart không tồn
  tại") và mọi pha đều có timeout riêng (online 2 s, timer 2 s) ⇒ hart lỗi/chậm làm TAP đỏ, **không**
  làm treo cả máy.
- Vòng 5: hart `SLOW_HART = 1` (nếu online) **trễ 5 ms** trước khi join; BSP vẫn thấy đủ arrival
  (`timeout=0`) trong biên 1 s — bằng chứng barrier chờ hart chậm.
- Vòng 6: `expected = online | bit(hart đầu tiên không online)` — hart đó **không tồn tại** nên
  BSP phải `timeout=1`, `missing = bit đó`, rồi `release` và đi tiếp tới TAP 20 (không treo).
- Hart thứ cấp **park bằng spin** (không `wfi`): K3 chưa có IPI để đánh thức `wfi` (KT-50). Trong
  pha SMP, mọi hart chạy đúng chuỗi `alive → guard → timer → PLIC → barrier` với barrier giữa các
  pha; BSP cũng là một thành viên (hart boot).

### 12.6 TAP 15..19 (ảnh test chính) + runner matrix

| TAP | Điều kiện đo |
|---|---|
| `ok 15 - harts alive N` | SBI HSM start đủ hart `PRESENT`; `ONLINE = PRESENT`, `harts = N`; DTB magic hợp lệ; **mỗi hart in một dòng** `# hart h alive ticket=T tp=h …` (ticket là hoán vị `0..N-1`); `# harts alive: n=N … tickets=1` |
| `ok 16 - per-hart stack/guard` | mỗi hart `h`: stack/trap stack RW NX leaf 4 KiB, guard NONE ở identity + nửa cao + physmap, `guard_owner = h`, **tự chạm guard của mình** ⇒ load page fault `scause=13` thật, handler resume đúng **một** instruction, trapframe nằm trong `.trap_stack` của hart `h` (`frame_trap=1`) |
| `ok 17 - per-hart timer` | mỗi hart tự arm SBI TIME, nhận **100 tick** trong slot PerHart của mình, `first = rdtime` lúc arm, `non_monotonic = 0`; số dòng `# timer hart=h … ok=1` = N |
| `ok 18 - plic ctx per hart` | mỗi hart context `2*h+1`, context M-mode `2*h` **không** bật, threshold 0, priority UART 1, enable IRQ UART round-trip (bật → đọc lại 1, tắt → đọc lại 0) trên **context của hart**; các context khác nhau; `PLIC_STATS` PerHart không lẫn; số dòng `# plic hart=h … ok=1` = N |
| `ok 19 - barrier` | vòng 5: đủ arrival, `timeout=0`, hart chậm trễ thật (`slow_delayed=1`); vòng 6: `timeout=1`, `missing` đúng hart không online, `wait_ticks < 1 s`, máy tiếp tục |
| `ok 30 - trap nested panic` | trap lồng vẫn fatal (KT-45 P2b) — boot kết thúc `PANIC: nested trap …` (KT-51 dời 25→30 vì thêm TAP 25..29) |

- `expected.txt` (`v2/test/boot/expected.txt` + sidecar `.license` theo R3) là nguồn duy nhất của
  kế hoạch/số `ok` (KT-49: ktest 20/20; **KT-50: 25/25**, ktest-guard 3/3); `ktest.sh` kiểm
  `1..K`, tập `ok 1..K`
  không thiếu/lặp, đếm số dòng `ok`. Pha SMP test nằm ở `vifast-kbin-riscv64/src/ktest_smp.rs`.
- Matrix: `bash v2/test/boot/ktest.sh 50 matrix` chạy **50 boot mỗi N=1/2/4** (main + guard), log
  riêng `/tmp/k3-ktest-{main,guard}-smp<N>-<i>.log`, tổng kết theo N; `ktest.sh` kiểm thêm banner
  OpenSBI `Platform HART Count : N` (bằng chứng độc lập rằng máy ảo thật sự có N hart).
- Ảnh **sản phẩm**: `# smp: … harts=N … timeout=0`; hart lỗi chỉ log `# smp: WARN` (không panic cả
  máy), `run.sh N 1|2|4` kiểm `harts=N` nên thiếu hart vẫn đỏ — không icount (D2-72 Q2).
- Ảnh **guard** dùng guard của **hart boot thật** (`mmu::guard_range_of(hart_id())`, in
  `# guard image: hart=N`); runner đọc hart đó và đòi dòng fault/load/store đúng hart — không ghim
  hart 0 (đã tái hiện `Domain0 Boot HART: 3` với `-smp 4` không icount, ảnh guard vẫn 3/3 TAP).

### 12.7 Ngân sách và ảnh hưởng kích thước

- Vùng per-hart là **NOLOAD**: không vào `.data/.bss`, không cộng trần 262.144 B (D2-72 Q5);
  `.hart_stacks` báo riêng trong `budget.sh` (`.stack`/`.trap_stack` của hart 0 vẫn như cũ).
- Bộ đếm PerHart + trạng thái SMP là `static` nhỏ trong `.bss` (8 slot `TimerCounters`/`PlicStats`
  + mask/record). Page table per-hart vẫn cấp từ **arena RAM** (64 KiB) — thêm leaf 4 KiB cho
  stack/trap stack các hart nhưng vẫn **không** đội `.data+.bss`; `mmu::init` panic nếu arena/bố cục
  vượt 2 MiB (kiểm tuyến tính giữ nguyên từ KT-46).
- Số đo budget thật của cả ba ảnh nằm trong `v2/agent/JOURNAL/kt49-*.log` (QD §9 Q12 — tài liệu
  không chép số tay).

### 12.8 Giới hạn phép đo (đọc kèm §12.7)

1. **Không IPI** (KT-50): AP park bằng spin; chưa có "đánh thức hart đang ngủ", preemption hay
   scheduler đa hart. Matrix chỉ chứng minh bring-up + trạng thái per-hart, không chứng minh IPC.
2. **PLIC per hart** mới kiểm *đăng ký context + round-trip enable* trên IRQ UART; chưa giao ngắt
   thật cho nhiều hart cùng lúc (nguồn UART chỉ một line; KT-50 mới có IPI/line khác).
3. **Barrier timeout** dùng `rdtime` + spin, không dùng timer ngắt; "hart chậm" là hart trễ 5 ms,
   không phải hart bị treo thật. Hart **không tồn tại** được mô phỏng bằng bit kỳ vọng không có
   hart tương ứng (đúng ngữ nghĩa "expected mà không tới").
4. **QEMU icount chia lượt vCPU thô**: một hart có thể tới barrier muộn hàng chục ms ảo (đo được
   trong log `kt49-*`); vì vậy vòng timeout dùng 200 ms — **không** phải nới ngưỡng DoD nào (DoD
   KT-49 không đặt ngưỡng thời gian barrier; chỉ đòi "hart chậm không treo").
   Cùng lý do, khi `N=4` slot timer **của hart phụ** có thể có `late/missed > 0` (log `kt49-*`;
   giá trị đổi theo lượt chạy) — đây là hạn chế **chia lượt vCPU của icount**, không phải lỗi
   PerHart: TAP 17 chỉ đòi **≥ 100 tick + đúng slot của hart** (`first = rdtime` lúc hart đó arm,
   `non_monotonic = 0`), **không** đòi drift per-hart. Drift Δ tick ∈ [9,11] ms (D2-74) chỉ đo trên
   **BSP** ở TAP 14; chưa tách được biến "scheduling icount" khỏi "toán deadline per-hart" — muốn
   đo jitter timer per-hart là **task riêng**, không mở lại ngưỡng KT-47/49.
5. **`TRAP_STATS`/`GUARD_STATS` vẫn gộp toàn cục**: TAP 2..14 chạy trước pha SMP; trong pha SMP chỉ
   dùng **delta** và slot PerHart của timer/PLIC. Muốn thống kê trap theo hart từng phần ⇒ task sau.
6. **Chưa kiểm `MAX_HARTS = 8`**: matrix `1/2/4` (D2-72 Q4); `-smp 8` vượt yêu cầu K3 nhưng code
   hỗ trợ tới 8 (hằng số + `mmu::init` panic nếu linker lệch); soak 1.000 boot là KT-57.
7. **Không hotplug/suspend/`hart_stop`**: hart ngoài DTB bị từ chối; hart online không tắt giữa
   chừng (K3 chưa cần).

### 12.9 Không thuộc KT-49

IPI thật + hộp thư drain + `VmKick`/`TlbFlush` (KT-50 — **đã làm**, xem §13), ASID/TLB shootdown
(KT-51), object layer (KT-52), U-mode/syscall (KT-53), harness TAP đầy đủ/conformance (KT-54/55),
`FaultKind`/W^X (KT-56), soak 1.000 + K3-DOD (KT-57), port core `no_std` (K3b — D2-72 Q13),
hotplug/suspend, scheduler/preemption đa hart. Xem bảng §9.

## 13. IPI thật + hộp thư per-hart (KT-50)

Nguồn quyết định: task K3-PLAN §4.1 KT-50; **D2-68** (local flush + ack ngầm, **không**
self-target; drain hộp thư khi chờ ack), **D2-70** (bài học KT-43 về ngưỡng đo — KT-50 không đổi
ngưỡng nào), **D2-72 Q4** (`MAX_HARTS = 8`, matrix `1/2/4`), **Q7** (địa chỉ hardcode: ACLINT
`0x0200_0000`; DTB chỉ kiểm magic), **Q5** (budget 262.144 B), **Q12** (Miri không áp kernel).
Code: `vifast-plat-common::ipi` (ngữ nghĩa + bộ đếm + `wait_with_drain`, host test được) +
`vifast-plat-riscv64::ipi` (SBI `ecall`, CSR `sie`/`sip`/`sstatus`) + asm `trap_entry` sẵn có +
pha test `vifast-kbin-riscv64/src/ktest_ipi.rs` (tách khỏi `ktest_smp.rs` theo R1).

### 13.1 Vì sao IPI và vì sao **không** self-target

Pha SMP KT-49 park hart thứ cấp bằng **spin** vì chưa có IPI; KT-50 thêm đường đánh thức/drain
thật để KT-51 (`TlbFlush`/shootdown) và K3b (scheduler) có nền. Hai lỗi làm **chết người** (K3-PLAN
§6.1 R3, D2-68):

1. **Chờ ack mà không drain** hộp thư — hart chờ trong vùng khoá/`SIE = 0` không nhận trap, nên
   message nằm mãi trong hộp thư ⇒ deadlock thật (đúng lỗi AP→BSP của v1). KT-50 đặt
   [`wait_with_drain`] làm **primitive duy nhất** để chờ trong vùng khoá: mỗi bước **drain trước**
   rồi mới kiểm predicate, và **luôn** kết thúc khi `now >= deadline`.
2. **Self-target lặp** — gửi IPI cho chính hart gọi vừa vô nghĩa vừa **che lỗi thật**: mutation
   "bỏ flush cục bộ" sẽ sống vì message tự quay về (D2-68 đính chính 2026-11-14). KT-50 giữ
   **bản không self-target**: `sender == target` ⇒ [`IpiDelivery::Local`] ⇒ **không** gọi SBI IPI,
   chỉ local flush (khung KT-51) + ack ngầm, cộng bộ đếm `local`; `sent`/`traps` không tăng.

### 13.2 SBI IPI + ngắt mã 1 (tách khỏi timer 5 / external 9)

- **`sbi_send_ipi`** EID `0x735049` ("sPI"), FID 0, `hart_mask` + `hart_mask_base = 0` ⇒ bit thứ
  `h` của mask **là** hart `h` (khớp `smp::hart_bit`). Gửi bằng `ecall` từ S-mode (firmware
  OpenSBI v1.8.1 có extension `ipi`); `a0`/`a1` là cặp error/value nên asm khai báo cả hai `inout`.
- **`scause = (1 << 63) | 1`** = *supervisor software interrupt* ⇒ `decode_scause` trả
  **`Event::Ipi`** — nhánh **riêng**, không fatal. Ba mã ngắt được tách **tường minh**:
  `1 → Event::Ipi`, `5 → Event::Timer`, `9 → Event::Interrupt{9}` (PLIC); mã khác vẫn fatal.
  TAP 24 chứng minh trong **cả** pha IPI: `TRAP_STATS.interrupts` tăng đúng bằng số trap IPI,
  `TIMER_STATS.ticks` và `PLIC_STATS.uart_irqs` **0**, decode 1/5/9 ra đúng ba nhánh.
- **Handler**: `trap_dispatch` → `ipi::on_ipi_interrupt()`: đọc `sip.SSIP` **trước** khi xoá (có
  nguồn thật ⇒ không `spurious`), xoá `sip.SSIP`, `swap(0)` hộp thư của hart, cộng
  `delivered`/`kinds`; rồi `TRAP_STATS.record_interrupt(code = 1)`; `sret` resume (SIE=0 trong
  handler nên không trap lồng).

### 13.3 Hộp thư per-hart **coalesce theo kind** + bộ đếm

`Kind` = [`IpiKind`]: `TlbFlush` (khung KT-51 — KT-50 **chưa** làm ASID/`sfence.vma`/ack
shootdown đầy đủ), `VmKick`, `Wake`. Hộp thư mỗi hart là **một bitmask 3 bit** (`AtomicU64`),
không phải hàng FIFO: gửi trùng một kind khi kind đó **đang chờ** bị **gộp** — không mất kind,
không nhân đôi, **không** gửi thêm IPI; kind khác giữ bit riêng nên `drain` một `swap(0)` giao
**đủ** mọi kind đang chờ. Đây là ngữ nghĩa đúng của §4.17 cho shootdown/wake lặp; khác
`vifast-sync::Mailbox` (MPSC 7 message có payload, đầy ⇒ `Busy`) — IPI chỉ cần "đánh thức + kind"
và hàng 3 bit không bao giờ đầy. Bộ đếm `IPI_STATS` (**PerHart**, `const`-init, không `unsafe`):
`sent` (SBI IPI thật, theo hart gửi), `coalesced`, `local`, `errors`/`last_error`, `traps`,
`delivered`, `spurious` (trap mà `sip.SSIP` **không** bật), `kinds[3]`. TAP đọc đúng các số này;
bất biến `posted == delivered + pending` được host test chốt.

### 13.4 Deviation: ACLINT MSWI **không** đọc được từ S-mode ⇒ dùng `sip.SSIP`

K3-PLAN cho phép "ACLINT MSWI nếu cần đọc pending". Đo trên QEMU 11.1.3 + OpenSBI v1.8.1
(`-smp 2`): đọc `0x0200_0004` (MSIP của hart 1) từ S-mode bị **load access fault**
(`scause = 5`, `tval = 0x0200_0004`) — PMP của firmware không mở ACLINT cho S-mode (log
`v2/agent/JOURNAL/kt50-aclint-pmp.log`). Vì vậy arch **không** map/không chạm CLINT; trạng thái
ngắt phần mềm đọc/xoá bằng **`sip.SSIP`** — cùng một trạng thái theo privileged spec, S-mode được
phép đọc/ghi, và đó là bit mà trap `scause = (1<<63)|1` phản ánh. Hằng ACLINT (`MSWI_HART_STRIDE`,
`mswi_offset`, `mswi_address`, `msip_pending`) vẫn ở `plat-common` làm dữ kiện nền tảng + host test;
`mmu::map_mmio` **không** map CLINT (giữ nguyên như KT-47/48). Đây là deviation **đo được**, không
phải nới ngưỡng; ghi ở đây + JOURNAL/STATE. **Agent 0 đã chốt D2-76**: `sip.SSIP` là **nguồn sự
thật** cho IPI S-mode (gửi qua SBI EID `0x735049`, xoá pending bằng `csrc sip, SSIP`), **cấm** map
CLINT vào MMIO kernel (giữ D2-72 Q7); đây là cách đúng theo privileged spec, không phải workaround.

### 13.5 TAP 20..25 (ảnh test chính) + runner theo `-smp`

| TAP | Điều kiện đo (log `# ipi …`) |
|---|---|
| `ok 20 - 100 ipi delivered` / `ok 20 - 100 ipi traps (self)` | `-smp ≥ 2`: BSP gửi **100** IPI tới **một** peer (hart online nhỏ nhất khác BSP), kind luân phiên `TlbFlush/VmKick/Wake` và **đợi từng delivery** trước khi gửi tiếp ⇒ `sent=100`, `delivered=100`, `traps=100`, `coalesced=0`, `spurious=0`, per-kind `34/33/33` (**không mất, không nhân đôi**). `-smp 1`: không có hart đích ⇒ đo 100 IPI **tự gửi** bằng SBI `send_ipi_raw` (vẫn là đường SBI + trap + drain thật) với `traps=100`, log ghi `mode=self-ipi` — **không** giả là 2 hart |
| `ok 21 - ipi drain khi cho ack` | hai hart **tắt `sstatus.SIE`** (vùng "đang giữ khoá"), mỗi hart `post(TlbFlush)` cho peer rồi chờ bằng `wait_drain` (drain trong vòng chờ) cho tới khi hộp thư **của mình** giao ≥ 1 kind; cả hai `ok`, `traps_delta = 0/0` (**không** nhờ ngắt); thêm một lần chờ predicate không bao giờ đúng với timeout 50 ms phải **hết** rồi đi tiếp (`timeout_ended=1`, số `drain` tăng) — hart chết chỉ làm TAP đỏ, không treo máy |
| `ok 22 - self-target khong gui` | **mọi** hart online tự gửi `TlbFlush` cho chính mình: `PostOutcome::Local`, `sent`/`traps` **không** tăng, `local` +1, `delivered` +1, `kinds[TlbFlush]` +1 (local flush + ack ngầm — D2-68) |
| `ok 23 - coalesce khong mat kind` | 5 × `TlbFlush` + 1 × `VmKick` tới peer: `coalesced=4`, chỉ **2** SBI IPI (`sent_delta=2`), hộp thư đích đang chờ **đúng** `{TlbFlush, VmKick}`, một `drain` giao **đủ** 2 kind. `-smp 1`: cùng bất biến kiểm **trên hộp thư tại chỗ** (không có hart đích để gửi IPI), log `mode=mailbox` |
| `ok 24 - ipi khong nham timer/external` | trong **cả** pha: `TRAP_STATS.interrupts` tăng đúng bằng số trap IPI, `TIMER_STATS.ticks` 0, `PLIC_STATS.uart_irqs` 0, `last_scause = 0x8000_0000_0000_0001`, `last_code = 1`, decode 1/5/9 ra `Ipi`/`Timer`/`Interrupt{9}` |
| `ok 30 - trap nested panic` | trap lồng vẫn fatal (P2b) — boot kết thúc `PANIC: nested trap …` + halt (dời 20→25 ở KT-50 vì thêm TAP 20..24; 25→30 ở KT-51 vì thêm TAP 25..29) |

- `expected.txt` là nguồn duy nhất của kế hoạch/số `ok` (KT-51: **ktest 30/30**, guard 3/3).
- `ktest.sh` grep thêm bằng chứng IPI **theo `-smp`**: `-smp ≥ 2` đòi `mode=cross` với
  `delivered=100/traps=100/kinds=34/33/33`, drain `mutual=1`, coalesce `mode=cross sent_delta=2`;
  `-smp 1` đòi `mode=self-ipi` (`delivered=0`, `traps=100`), drain `mode=self-wait`, coalesce
  `mode=mailbox` — hai cấu hình khác nhau **cùng** một bộ bất biến nên không có "xanh hình thức".
- Ở `-smp 4` chỉ **một** peer làm phép đo chéo (đúng phạm vi "100 IPI 2 hart"); các hart còn lại
  tham gia barrier + self-target và không tham gia phép đo chéo (log ghi `peer=…`).
- Runner đọc TAP 15..19 như cũ; TAP 20..24 nằm trong `ktest_ipi.rs`, TAP 25 in trước khi kích hoạt
  nested trap.

### 13.6 Ngân sách và ảnh hưởng kích thước

- Toàn bộ trạng thái IPI là `static` nhỏ trong `.bss` (hộp thư `MAX_HARTS × 6` từ, bộ đếm
  `MAX_HARTS × 9`…), **không** cấp phát, **không** thêm page table/bảng mới (không map CLINT); vì
  vậy `.data+.bss` vẫn dưới trần 262.144 B (D2-72 Q5) và **không** đụng `.stack`/`.hart_stacks`.
- Số đo budget thật của ba ảnh nằm trong `v2/agent/JOURNAL/kt50-budget.log` (QD §9 Q12 — tài liệu
  không chép số tay).

### 13.7 Giới hạn phép đo (đọc kèm §13.8)

1. **`-smp 1` không có phép đo chéo**: đo 100 IPI tự gửi (SBI + trap thật) và các bất biến
   self-target/coalesce/timeout; **không** claim "100 IPI 2 hart" cho cấu hình 1 hart. Log ghi rõ
   `mode=self-ipi`.
2. **QEMU `-icount` chia lượt vCPU thô**: mỗi delivery chéo là một lượt M-mode + `sret` + trap nên
   pha 100 delivery có thể chiếm **hàng giây thời gian ảo** (log `kt50-ipi.log` in `wait_ticks`);
   timeout mỗi delivery là `IPI_DELIVERY_TIMEOUT` (100 ms) và barrier phía AP là
   `IPI_TRAP_PHASE_TIMEOUT` (10 s). Đây là **biên thời gian của máy ảo**, không phải ngưỡng DoD
   (DoD KT-50 chỉ đòi 100/100 không mất/nhân đôi).
3. **Không đo tần suất/latency IPI** so với thời gian thật: `-icount shift=6` (D2-74) là thời gian
   **ảo**; muốn đo jitter IPI là task riêng (như §10.5).
4. **Số IPI 100 là "kind được giao"**, không phải "số lần SBI trả về": với coalesce, một IPI có
   thể giao 1 kind; phép đo tuần tự từng delivery để `delivered = traps = 100` ở `-smp ≥ 2`.
   Bằng chứng ngược (coalesce thật) ở TAP 23: 2 IPI giao 2 kind từ 6 post.
5. **Chỉ một peer**: `-smp 4` không đo 3 cặp đồng thời; drain theo cặp là đủ cho contract
   "MPSC hộp thư per-hart", nhưng **không** chứng minh đa người gửi đồng thời ở mức phần cứng
   (host test kiểm MPSC/atomic; KT-51/57 sẽ dùng nhiều hart hơn).
6. **Không timeout bằng ngắt timer trong handler IPI**: `wait_drain` timeout dùng `rdtime` + spin
   (giống barrier KT-49); timer vẫn tắt trong pha IPI (TAP 24 chứng minh). Timer-based timeout
   đầy đủ là KT-51 (shootdown) khi có deadline thật.
7. **`traps_delta = 0` khi `SIE = 0` chỉ chứng minh ngắt *không được nhận*** (không vào `stvec`),
   **không** chứng minh IPI **không tới**: phần cứng/firmware vẫn có thể đã đặt `sip.SSIP`/pending,
   và kind vẫn tới **hộp thư** — đúng thứ vòng chờ drain rồi giao. Muốn chứng minh "không có IPI
   nào tới trong vùng khoá" phải đọc pending (`sip.SSIP`) **trước/sau** và đếm riêng; KT-50 chỉ
   cần chứng minh tiến trình **không phụ thuộc trap** (TAP 21: cả hai xong, `traps_delta=0`).
   Phép đo "IPI tới nhưng bị che" đầy đủ thuộc KT-51 (shootdown ack + timeout), không claim ở đây.

### 13.8 Không thuộc KT-50

ASID + `sfence.vma` + shootdown ack đầy đủ (KT-51) — KT-50 chỉ đặt **khung** kind `TlbFlush` +
drain; object layer (KT-52), U-mode/syscall (KT-53), harness đầy đủ/conformance (KT-54/55),
`FaultKind`/W^X (KT-56), soak 1.000 + K3-DOD (KT-57), port core `no_std` (K3b), preemption/
scheduler đa hart, hotplug/suspend. Xem bảng §9.

## 14. ASID thật + `sfence.vma` + shootdown ack (KT-51)

Nguồn quyết định: task K3-PLAN §4.1 KT-51; **D2-65 Q4/Q5** (ASID 8 bit + thế hệ; invalidate
`(asid,va)`/`asid`/`gen`; ack `fetch_max`, `Ok` chỉ khi đủ ack, chờ ack phải drain), **D2-68**
(hart gọi **local flush + ack ngầm**, **không** self-target; drain hộp thư trong vòng chờ), **D2-72**
Q4 (`MAX_HARTS = 8`, matrix `1/2/4`), Q5 (budget 262.144 B), Q6 (`rdtime`/SBI TIME), **D2-74**
(`-icount shift=6` chỉ trong `ktest.sh`), **D2-76** (trạng thái ngắt mềm = `sip.SSIP`), **D2-77**
(hart offline ⇒ không giữ kind), **D2-78** (`shootdown` **không** nhận mask từ ngoài — tự lấy
`active_on()`; TAP "ngoài `active_on`" phải gỡ hart khỏi `ACTIVE_ON` **thật** rồi khôi phục). Code:
`vifast-plat-common::tlb` (ASID allocator/thế hệ, `FlushScope`, bất biến ack, bộ đếm — host test
được) + `vifast-plat-riscv64::tlb` (ba dạng `sfence.vma`, `satp.ASID`, slot yêu cầu, shootdown) +
`vifast-riscv64::mmu` (leaf thử nghiệm **chỉ ảnh test**) + pha test
`vifast-kbin-riscv64/src/ktest_tlb.rs` (tách khỏi `ktest_ipi.rs` theo R1).

### 14.1 ASID thật: 8 bit, lease + thế hệ, không tái dùng trong cùng thế hệ

- **`satp.ASID`** (RV64: bit 59..44, privileged spec) dùng **8 bit** (`vifast_pte::ASID_BITS`,
  D2-65 Q4) ⇒ **256** ASID mỗi thế hệ. `mmu::satp_sv39_asid(root, asid)` chỉ thay **trường ASID**,
  giữ nguyên `MODE`/`PPN` (host test chốt `satp & !ASID_FIELD` không đổi).
- **`AsidAllocator`** (`plat-common`): free-list 256 bit (4 × `AtomicU64`), cấp ASID **thấp nhất**
  trước (tất định, dễ đọc log), cạn ⇒ `generation += 1`, nạp lại **đủ** 256, `exhaustions += 1`,
  lease kế tiếp mang `rollover = true` để người gọi phát **một** `FlushScope::All` (Q4: "cạn ⇒
  flush toàn bộ một lần theo `gen`", **không** flush từng ASID).
- **Không tái dùng trong cùng thế hệ**: `free(lease)` chỉ **về hưu** ASID đó (không trả free-list;
  K2-PLAN Q4) và **từ chối** lease sai thế hệ/double-free. Lý do: hart đã rời VSpace có thể còn
  entry TLB mang `(gen, asid)` cũ; phát lại ASID trong cùng thế hệ sẽ khớp nhầm VSpace mới.
- **Đổi ASID không `sfence`**: ghi `satp` với thẻ mới không làm sai translation (entry cũ mang thẻ
  cũ không khớp thẻ mới); chỉ khi ASID **phát lại** ở thế hệ mới mới cần xả — đúng lúc
  `replace_lease` phát `FlushScope::All`. Đây là lý do kỹ thuật, không phải bỏ sót; ghi ở
  `vifast-plat-riscv64/src/tlb.rs` + `mmu::set_satp_asid`.
- **`active_on`** = `HartMask` hart đang nạp ASID kernel: BSP vào [`tlb::init`] (sau `mmu::init`,
  **trước** `smp::init`), hart thứ cấp vào `tlb::activate` **sau** `adopt_satp` (SBI HSM đánh thức
  với `satp = 0`). Hart online nhưng **ngoài** `active_on` không nhận `TlbFlush` và không bị chờ.

### 14.2 `sfence.vma` — ba dạng đúng `rs1`/`rs2`

| Dạng | Lệnh | Dùng cho |
|---|---|---|
| `(asid, va)` | `sfence.vma va, asid` (rs1 = VA, rs2 = ASID) | unmap/protect **một** trang (phủ cả leaf lớn chứa VA) |
| `(asid, *)` | `sfence.vma zero, asid` | teardown một ASID |
| `(*, *)` | `sfence.vma` (rs1 = rs2 = `x0`) | cạn ASID/thế hệ (`generation += 1`) |

`rs1` là **địa chỉ**, `rs2` là **ASID** — đảo thứ tự là lỗi im lặng; ba helper nằm ở
`vifast-plat-riscv64::tlb` (`sfence_page`, `sfence_asid`, `sfence_all`) và **chỉ** hart hiện
tại bị xả (TLB là per-hart). Không dùng `sfence.vma` cho hart khác: đường remote **bắt buộc** qua
`TlbFlush` (14.3) — không có cách nào xả TLB hart khác từ xa ngoài việc hart đó tự chạy `sfence`.

### 14.3 Shootdown: slot yêu cầu + kind `TlbFlush` + ack `fetch_max`

Hộp thư IPI KT-50 chỉ mang **kind** (bitmask 3 bit). KT-51 thêm **một slot yêu cầu/hart**
(`{seq, asid, scope, va}` — 4 `AtomicU64`, `const`-init, không cấp phát):

1. Người gửi ghi `asid/scope/va` (**Relaxed**) → ghi `seq` (**Release**) → `post(TlbFlush)`
   (**AcqRel** + SBI IPI). Gửi trùng bị **coalesce** như KT-50 nhưng slot giữ **seq mới nhất**;
   ack `fetch_max` nên `ack ≥ seq` là đủ — **không mất yêu cầu**, không nhân đôi flush.
2. Hart nhận (trap handler `/` `park_forever` `/` vòng chờ drain — **mọi** đường `drain`) thấy kind
   `TlbFlush` ⇒ đọc `seq` (**Acquire**) rồi đọc tham số, **adopt** `satp.ASID` của yêu cầu nếu
   khác (giữ mọi hart cùng thế hệ ASID — thiếu bước này, hart còn thẻ cũ sẽ **không** bị
   `sfence (asid mới, va)` xả ⇒ stale TLB thật), chạy đúng dạng `sfence.vma`, rồi
   `ACKED[hart].fetch_max(seq)`.
3. Người gửi **luôn** flush cục bộ trước (page table của chính nó vừa đổi), rồi — nếu thuộc
   `active_on` — ghi **ack ngầm** `ACKED[sender] = seq` (**D2-68: không self-target, không gửi
   message cho chính mình**; `targets()` của `plat-common` loại hart gửi khỏi danh sách IPI). Đây
   là bản **chặt hơn**: self-target sẽ làm mutation "bỏ flush cục bộ" sống.
4. Vòng chờ: mỗi bước **drain hộp thư của chính hart** (kể cả `SIE = 0` — luật chống deadlock
   D2-68/§4.10.3) rồi kiểm `ack_mask == active_on`; deadline bằng `rdtime` (timebase 10 MHz).
   Hết deadline ⇒ `TimedOut` + `timeouts += 1`, **không treo**; `Completed` **chỉ** khi
   `acked == active_on` (bất biến Q5). Hart online **ngoài** `active_on` không nằm trong mask nên
   không bị chờ, không làm hỏng kết quả.
5. **API không nhận mask** (D2-78): `shootdown(scope, timeout)` tự đọc `active_on()` **bên trong**
   tại thời điểm phát, nên người gọi không thể chờ/bỏ chờ sai tập hart. TAP 27 chứng minh bằng
   **trạng thái thật**: API test-only gỡ hart khỏi `ACTIVE_ON` (lưu mask cũ) → shootdown chỉ tới
   phần còn lại, hart bị gỡ **không** ack/không bị chờ → **khôi phục** `ACTIVE_ON` rồi shootdown
   lại ⇒ mọi hart online ack.
6. Một shootdown tại một thời điểm (`IN_FLIGHT` CAS): K3 có **một** VSpace; shootdown chồng nhau
   trả `Busy` + `errors` thay vì ghi đè slot đang dùng.

### 14.4 Rollover (cạn ASID) — `generation += 1` + **một** flush toàn bộ

`tlb::replace_lease(lease)` áp ASID mới lên `satp` hart gọi; nếu lease `rollover` thì phát **một**
`FlushScope::All` tới `active_on` và chờ đủ ack. Ở TAP 28, ASID **0** được phát lại (đúng nguy cơ
tái dùng trong cùng thế hệ) nên nếu **thiếu** flush toàn bộ, entry TLB cũ (thẻ ASID 0) vẫn khớp và
hart đọc ra mapping **cũ** — phép đo dựa trên đúng nguy cơ đó, không dựa vào thẻ ASID đổi số.

### 14.5 TAP 25..29 + bằng chứng (ảnh `vifast-kbin-ktest`, trap lồng dời sang TAP 30)

| TAP | Đo |
|---|---|
| `ok 25 - 100 vong shootdown Ok` | 100 vòng: remap leaf `PROBE_VA → PA(r)` + marker mới, shootdown `Page{va}` tới **`active_on()`** (D2-78 — `-smp 2`: 2 hart, `sent=100`; `-smp 4`: 4 hart, `sent=300`), đợi `ack_mask == active_on`; **không self-target** (`sent = 100×(N−1)`, slot hart gọi = 0); hart đích (peer) **prime trước** rồi **đọc lại sau flush** phải thấy marker mới; **mọi** hart online khác BSP phải phục vụ đủ 100 yêu cầu (`served = N−1`). `-smp 1`: 100 vòng self (`mode=self`, `sent=0`), vẫn kiểm ack ngầm + flush cục bộ |
| `ok 26 - stale mapping caught` | **dương tính thật**: map→PA_A, đọc (nạp TLB), remap→PA_B **không** `sfence` ⇒ đọc **phải** ra marker A; sau `sfence.vma` **từng dạng** (`(asid,va)`, `(asid,*)`, `(*,*)`) ⇒ ra marker B. Lặp đủ **3** dạng |
| `ok 27 - hart ngoai active_on` | **D2-78, trạng thái thật**: test-only gỡ khỏi `ACTIVE_ON` 2 hart phụ (`-smp 4`) hoặc hart boot (`-smp 1/2`), lưu mask cũ → shootdown (không truyền mask) ⇒ hart bị gỡ **không** ack, **không** bị chờ, `acked == active_on()` mới; **khôi phục** `ACTIVE_ON` rồi shootdown lại ⇒ **mọi** hart online ack (`restored_acked == online`). Không dùng mask hẹp để giả lập |
| `ok 28 - rollover flush` | cấp cạn 256 ASID ⇒ `generation 0→1`, `exhaustions 1`, **một** `FlushScope::All`; ASID 0 phát lại; đọc lại VA (probe #2) ra marker mới trên **mọi** hart online; `satp_asid` mọi hart = ASID mới |
| `ok 29 - timeout khong treo` | đặt `ACTIVE_ON = {boot, offline}` (hart **offline** thật của máy) ⇒ `Timeout` sau **≥** `probe` (100 ms ảo; có thể vượt do vòng drain — giá trị thật in trong log), `ack_mask ≠ active_on`, **không treo**; yêu cầu/hộp thư của hart offline **rỗng** (D2-77); đặt `ACTIVE_ON = {boot}` rồi shootdown bình thường vẫn `Ok` (máy còn dùng được), cuối cùng khôi phục `ACTIVE_ON` |

Số đo do chính boot in ra: `# tlb init:`, `# tlb:`, `# tlb probe:`, `# shootdown rounds:`,
`# shootdown peer:`, `# stale:` + `# stale form:` ×3, `# outside:`, `# rollover:`, `# timeout:`,
`# tlb asid:` (log `v2/agent/JOURNAL/kt51-{ktest-smp1,ktest-smp2,ktest-smp4,boot50-matrix}.log`).
`ktest.sh` grep **theo `-smp`**: `≥ 2` đòi `mode=cross`, `sent=100`, peer `received/flushes=100`,
`extras=0xc` ở `-smp 4`; `1` đòi `mode=self` (`sent=0`, peer 0) — hai cấu hình khác nhau nhưng
**cùng** bất biến, không có "xanh hình thức".

### 14.6 Leaf thử nghiệm: chỉ ảnh test, không thêm bảng/arena

Phép đo dùng **hai** leaf 2 MiB RW NX, chỉ map trong ảnh `ktest` (`#[cfg(feature = "ktest")]`):

| VA | PA A / PA B (probe #1, TAP 25/26) | PA C / PA D (probe #2, TAP 27/28) |
|---|---|---|
| `0x0000_0000_8080_0000` | `0x8080_0000` / `0x80a0_0000` | — |
| `0x0000_0000_80a0_0000` | — | `0x80c0_0000` / `0x80e0_0000` |

Cả hai leaf nằm trong **bảng cấp 1 đã có** (cùng chỉ số root với leaf image) nên **không** cấp
thêm bảng, **không** tăng arena/budget; ảnh sản phẩm **không** có mapping/không có tham số test.
Probe #2 tách riêng để TAP 27/28 không bị nhiễu bởi entry TLB mà TAP 25/26 đã nạp. `remap_probe*`
chỉ sửa **một** PTE leaf cấp 1 (không cấp bảng, không đụng `satp`), nên nó đúng là thao tác "sửa
page table rồi phải `sfence.vma`".

### 14.7 Giới hạn phép đo (không giấu)

1. **Thời gian là thời gian ảo** (`-icount shift=6`, D2-74): `wait_ticks`/`waited` đo bằng
   `rdtime` dưới icount, **không** phải wall-time; muốn đo latency shootdown thật là task riêng.
2. **Một VSpace** (kernel): mọi hart adopt cùng ASID; nhiều VSpace thật (K3b) cần allocator theo
   VSpace + chọn ASID theo hart đang chạy — KT-51 chỉ chứng minh **cơ chế** ASID/thế hệ/shootdown.
3. **Một shootdown tại một thời điểm** (`IN_FLIGHT`): không đo nhiều shooter đồng thời. TAP 25
   dùng **mọi** hart trong `active_on` (smp2: 2, smp4: 4); TAP 27 **gỡ thật** 2 hart phụ khỏi
   `ACTIVE_ON` ở `-smp 4` (không phải mask hẹp truyền vào), nên "hart ngoài `active_on`" là trạng
   thái thật của kernel VSpace.
4. **`-smp 1` không có phép đo chéo**: đo 100 vòng self + ack ngầm; log ghi `mode=self`, **không**
   claim "2 hart".
5. **`sfence.vma (asid, va)` flush theo VA của mapping** (leaf 2 MiB); không đo trường hợp VA nằm
   **giữa** leaf lớn — QEMU flush trang chứa VA; phần "phủ cả entry lớn" của Q4 là hành vi hardware
   được privileged spec bảo đảm, không phải giả định của kernel.
6. **Không đo TLB hit/miss** phần cứng (không có PMU trong QEMU `-cpu rv64`); phép đo "stale
   mapping" là bằng chứng **hành vi** (đọc ra dữ liệu cũ/mới), không phải đếm entry.

### 14.8 Mutation bắt buộc (KT-51) — log trong `v2/agent/JOURNAL/`

| Mutation | Sửa | Kết quả đỏ (log) |
|---|---|---|
| **M1 — bỏ `sfence.vma`** | ba helper thành no-op | TAP 25/26/27/28 **đỏ**: lần đọc đầu **sau** remap đã ra mapping cũ (`fresh_a` sai) và peer `verify_wrong` tăng — log `kt51-mutation-1-bay-sfence{.log,-qemu.log,.diff}` |
| **M2 — bỏ chờ ack** | không drain/không chờ, trả ngay | TAP 25/27/28/29 **đỏ**: `Ok`/`acked` không đủ, `FlushScope::All` trả `Fail`, timeout "chớp" (< probe) — `kt51-mutation-2-bo-cho-ack*` |
| **M3 — self-target** | hart gọi cũng bị gửi IPI + bỏ ack ngầm | TAP 25/27 **đỏ**: `sent` gấp đôi `sent_expected`, slot yêu cầu của hart gọi khác 0 — `kt51-mutation-3-self-target*` |

### 14.9 Không thuộc KT-51

Object layer/Retype (KT-52), U-mode/3 syscall (KT-53), harness TAP đầy đủ + `expected.txt` cho mọi
ảnh (KT-54), conformance 5 `.scn` (KT-55), `FaultKind`/W^X + `spec/abi.toml` (KT-56), soak 1.000 +
K3-DOD (KT-57), port core `no_std` (K3b), pager/COW, hotplug/suspend, preemption/scheduler đa hart,
nhiều VSpace thật, đo latency shootdown thật (không icount).


## 15. Object layer `no_std` + Retype nội bộ (KT-52)

### 15.1 Phạm vi: nội bộ, **không** ABI, **không** `spec/`

- Code: [`vifast-plat-common::obj`](../kernel/vifast-plat-common/src/obj.rs) — toàn bộ ngữ nghĩa
  object (Retype/Revoke/byte/PA/PTE/W^X/check), `#![no_std]`, **không `unsafe`**, chạy được trên
  host (test `tests/obj.rs`); [`vifast-plat-riscv64::obj`](../kernel/vifast-plat-riscv64/src/obj.rs)
  — `PhysMem` qua physmap + asm `satp`/`sfence`/`sstatus.SUM` (D2-17, `kernel/vifast-plat-*`).
- **Không** mở `spec/`: `Retype`/`VSpace` ở đây là API **nội bộ kernel**, không phải
  `Untyped.Retype` của ABI (`VSpace` vẫn `reserved`/`unsupported` trong `spec/abi.toml`). Task
  KT-56 mới mở `spec/` **một lần** cho `FaultKind` (D2-72 Q1). `git diff spec/` của KT-52 **rỗng**.
- **Không** CNode/MDB/TCB/IPC/scheduler (K3b KT-58..61), **không** U-mode (KT-53). Cap ở đây là
  **handle + key tối thiểu**: bảng phẳng `[Cap; 160]` trong `.bss`, mỗi cap `{obj, kind, rights,
  parent, id}`; `id` là key duy nhất tăng dần; quyền chỉ `R/W/X` và `Retype` cần `W`.
- Object metadata (bảng `Object`/`Cap`/`Mapping`) nằm trong `.bss` có biên; **dữ liệu** object
  (bảng PTE 4 KiB, PA nhãn của Frame) mới là thứ Untyped cấp. Nợ K3b: đưa header object vào
  Untyped (KT-58).

### 15.2 Hai Untyped boot + mô hình byte (D2-66)

`obj::init` (gọi ở **cả** ảnh sản phẩm và ảnh test, sau `mmu::init`/`tlb::init`) lấy
`mmu::arena_range()`, đặt arena object ở `align_up(arena_end, 4 KiB)`, dài **64 KiB** `[TK]` —
vùng RAM **sau `__kernel_end`**, không phải section ELF nên **không** vào trần `.data+.bss`
(D2-72 Q5). Sau đó dựng **2 Untyped**:

| Untyped | Arena byte | Cửa sổ PA (`UNTYPED_PA_WINDOW` = 1 GiB, D2-66) | Dùng cho |
|---|---|---|---|
| `U0` | 64 KiB | `0x8200_0000` (32 MiB trong RAM, căn **2 MiB**) | PageTable/VSpace + Frame 4 KiB/2 MiB (PA thật) |
| `U1` | 4 KiB | `0x8000_0000` (đáy RAM, căn **1 GiB**) | Frame 1 GiB (nhãn PA — xem §15.10) |

Hai không gian **tách biệt** của một Untyped:

1. **Arena byte** — `PageTable` tiêu **4096 B**, root của `VSpace` tiêu **4096 B**;
   `VSpace` (metadata) và `Frame` tiêu **0 B**. Byte đệm căn lề được ghi thành **hole** (không
   có bộ đếm "gap" tự khai) và hole do revoke để lại không đếm hai lần.
2. **Cửa sổ PA** — `Frame` cấp `pa = pa_base + offset` theo watermark + free-list, **không** chồng
   Frame sống khác (toàn cục, kể cả khác Untyped). Đây đúng D2-66: `pa` là **nhãn PA**, không tiêu
   byte arena; nhờ vậy cỡ 1 GiB khả thi trên `-m 128M` (D2-72 Q2) — xem §15.10.

Allocator là **bump + hole free-list** trong bảng tĩnh (`Hole[12]` mỗi không gian): cấp thử hole
trước (first-fit có căn lề), hết mới bump; trả block thì chèn hole, gộp hole dính nhau và rewind
watermark nếu hole cuối chạm watermark. Mọi lỗi (hết chỗ/căn lề/lỗi tham số/W^X) **atomic** —
không đổi object/byte/cap/PTE nào.

### 15.3 Retype/Revoke nội bộ + bất biến byte

| Thao tác | `size_bits` | Tiêu | Trả |
|---|---|---|---|
| `retype(Untyped cap, PageTable)` | `0`/`12` | 4 KiB arena (zeroed) | cap con (`parent` = cap nguồn) |
| `retype(Untyped cap, Frame)` | `12`/`21`/`30` (từ `vifast-pte::FrameSize`) | 0 byte arena; tiêu PA trong cửa sổ | cap con |
| `retype(Untyped cap, VSpace)` | `0` | root PageTable 4 KiB (object **nội bộ**, không cap) | cap VSpace + ASID nhãn |
| `retype(…, Untyped)` | — | — | `NotSupported` (K3b KT-58) |

- `Revoke(Untyped)` finalise **mọi** object cấp từ nó (VSpace teardown trước, gỡ mapping; rồi trả
  PageTable/Frame), reset watermark/hole/PA; cap nguồn **còn sống** và dùng lại được.
- `Revoke(VSpace)` gỡ mapping (đếm Frame trả về), trả root + bảng trung gian, phát một
  `sfence.vma (asid)`; **Frame không bị trả** (Frame có cap riêng, thuộc Untyped khác).
- `Revoke(Frame)` khi còn mapping ⇒ `HasChildren`; `Revoke(PageTable)` khi còn gắn VSpace ⇒
  `HasChildren`.
- Bất biến byte mỗi Untyped (kiểm bởi `check_objmem` **và** TAP 31):
  `Σ len object sống + Σ hole == watermark ≤ capacity` (phủ kín, không gap), và theo định nghĩa
  `used = Σ len object sống`, `free = capacity − used`; Retype chuyển `free → used`, Revoke trả
  ngược lại **đúng** mốc (TAP 31 in `used0/after_retype/after_revoke/free0/free1`).
- Với Frame: `Σ cỡ Frame sống + Σ PA hole == pa_used ≤ 1 GiB` (cửa sổ), `pa` căn đúng cỡ.

### 15.4 Frame 4 KiB / 2 MiB / 1 GiB

- Cỡ đọc từ `vifast_pte::FrameSize::from_size_bits` (12/21/30); giá trị khác ⇒ `InvalidArgument`,
  **atomic** (TAP 32 dùng `13`).
- Cấp **giảm dần** trong một cửa sổ (K2 KT-36): watermark phải **đã** căn đúng cỡ, kernel **không**
  tự đệm căn lề ngầm — lệch ⇒ `Alignment`; muốn trộn 4 KiB/2 MiB thì cấp 2 MiB **trước**.
- Chồng PA với **bất kỳ** Frame sống nào ⇒ `AlreadyMapped` (atomic, trả PA vừa cấp).
- **4 KiB/2 MiB là RAM thật**: TAP 32 ghi/đọc 8 B ngay tại `pa` qua physmap (`PhysMem`), rồi map
  Frame 4 KiB vào VSpace thật ở TAP 35 và đọc/ghi qua VA user — chứng minh cả `pa` lẫn PTE trỏ
  đúng nơi.
- **1 GiB** trên `-m 128M` chỉ kiểm **nhãn**: `pa` căn 1 GiB, `pa + 1 GiB`, nằm trong cửa sổ
  căn 1 GiB của `U1`; **không** đọc/ghi — và `map` Frame 1 GiB bị từ chối `NotSupported` cho tới
  K4/K5 vì cửa sổ `0x8000_0000` trùm RAM thật (xem §15.10 L7).

### 15.5 PageTable 3 cấp Sv39 + VSpace map/unmap/protect

- `PageTable` = đúng 4096 B (512 PTE × 8 B) trong arena Untyped; hình 3 cấp, chỉ số 9/9/9/12 lấy
  từ `vifast_pte::{walk_shape, PAGE_SIZE_BITS, PTE_LEVELS}` — **không** chép tay hằng số hình.
- `map(vspace, va, frame, flags)`:
  1. kiểm cap/loại, `va` canonical (`VA_BITS=39`), căn đúng cỡ Frame (`va % cỡ == 0`), cờ hợp lệ
     (`W ⇒ R`), VA/Vùng chưa map, **W^X** (dưới);
  2. đếm bảng trung gian còn thiếu **trước** mọi thay đổi, cấp slot object/mapping/flush;
  3. cấp block 4 KiB từ Untyped của VSpace, zero, nối PTE bảng từ gốc xuống, ghi leaf;
  4. commit mapping + đếm `maps/w/x` của Frame + đẩy yêu cầu `sfence.vma (asid, va)`.
  Lỗi giữa chừng ⇒ rollback PTE đã nối + block + object slot (không để bảng mồ côi).
- `unmap(vspace, va)` yêu cầu mapping tồn tại (`NotMapped` nếu không), xoá PTE, trả đếm Frame, đẩy
  `sfence` trang. `protect(vspace, va, flags)` đổi cờ leaf, kiểm W^X **trên phần còn lại** của
  Frame (trừ mapping đang sửa), đẩy `sfence` trang.
- **`sfence.vma` do arch chạy**: object layer chỉ **đẩy yêu cầu** (`FlushRequest::{Page, Asid,
  All}`); `vifast_plat_riscv64::obj::run_flush` gọi đúng ba dạng của KT-51 §14.2. TAP 35 drain
  hàng đợi sau mỗi thao tác và đếm `flush` (≥ 3 yêu cầu trang).
- **Chứng minh mapping thật trên phần cứng (TAP 35):** VSpace root mới được **chép 512 entry root
  kernel** vào (giữ kernel identity/nửa cao/physmap/UART/PLIC sống khi đổi `satp`), `csrw satp`
  với ASID nhãn + `sfence.vma`; leaf tại `0x4000_0000` map 4 KiB `U|R|W` (A|D đặt sẵn); S-mode
  đọc/ghi **qua VA user** với `sstatus.SUM=1` (đúng privileged spec) ⇒ dữ liệu đúng; `protect`
  sang `R|NX` ⇒ PTE **mất W** (walk khớp cờ); `unmap` + `sfence` ⇒ walk invalid; khôi phục `satp`
  kernel + `sfence` ⇒ máy còn sống (`# map: … kernel=1`).

### 15.6 W^X toàn cục (D2-67) — hai ca TAP

- Mỗi Frame đếm `maps`/`mapped_w`/`mapped_x` **toàn cục** trên mọi VSpace. **Map/protect W** khi
  `mapped_x > 0`, hoặc **map/protect X** khi `mapped_w > 0` ⇒ `WxViolation`, **atomic** (không PTE,
  không đếm mới). Không miễn trừ PageTable thuần.
- **`Device + EXEC`** ⇒ `WxViolation` từ lúc kiểm cờ: Sv39 **không có bit Device**, nên `device` là
  **cờ chính sách** của mapping (cấm `X`, thường cũng cấm `W+X`); TAP 34 chứng minh ca này.
- TAP 33 (ca `X` rồi `W`) in `# wx: x_then_w=WxViolation … maps/w/x`; TAP 34 (ca `Device+EXEC`) in
  cùng dòng; `check_objmem` **recount** toàn bộ `mapped_w/mapped_x` từ mapping sống và từ chối
  `w>0 ∧ x>0` (không tin bộ đếm).

### 15.7 TAP 30..36 + bằng chứng

| TAP | Tên | Bằng chứng (log, `ktest.sh` grep) |
|---|---|---|
| 30 | `retype vspace` | `# retype: vspace=1 root=0x… asid=… pt=3 frame=1 cap_ok=1 walk_empty=1` |
| 31 | `untyped byte invariant` | `# untyped bytes: cap=… used0=… after_retype=… after_revoke=… free0=… free1=… invariant=1 atomic=1` |
| 32 | `frame 3 sizes` | `# frame: s12=1 s21=1 s30=1 pa4=… pa2=… pa1g=… aligned=1 real4=1 real2=1 window=1 bad13=InvalidArgument atomic=1` |
| 33 | `wx violation x-then-w` | chung dòng `# wx:` (TAP 34) + `maps=1 w=0 x=1` |
| 34 | `wx violation device+exec` | `# wx: x_then_w=WxViolation device_exec=WxViolation atomic=1 …` |
| 35 | `map/unmap 4 KiB` | `# map: va=0x40000000 level=0 pa=0x… satp=1 asid=… sum=1 write=1 read=1 protect=1 unmapped=1 flush=… walk_after=0 kernel=1` |
| 36 | `0 ro` | `# leak: cap=… used=0 free=… holes=0 frames=0 pts=0 vspaces=0 maps=0 orphans=0 check=1` (`check=1` chỉ khi `check_objmem` trả `Ok` — không cờ tự khai) |

Trap lồng fatal dời sang **TAP 37**; ảnh guard vẫn 3 TAP. Log đỏ trước, log TAP xanh, log 50
boot × `smp 1/2/4`, log gate và log budget nằm ở `v2/agent/JOURNAL/kt52-*.log`.

### 15.8 `check_objmem` — tương đương `check_vm` của K2, chạy `no_std`

[`Machine::check_objmem`](../kernel/vifast-plat-common/src/obj.rs) kiểm **mọi** bất biến và trả
`ObjStats` (counts/bytes/walked/orphans + `bytes_ok/wx_ok/walk_ok`):

1. **Byte + arena** mỗi Untyped: hole sắp thứ tự/trong `[0, watermark)`/không chồng; block sống
   căn lề, trong watermark, không chồng hole/block khác; `Σ live + Σ hole == watermark
   ≤ capacity` (byte đệm căn lề là hole).
2. **PA/Frame**: `pa` căn đúng cỡ, trong cửa sổ 1 GiB; **không** chồng Frame sống khác;
   `Σ cỡ Frame + Σ PA hole == pa_used`; mọi Frame sống **có cap sống** (không frame mồ côi).
3. **Bảng trang/VSpace**: VSpace sống có root PageTable sống, `owner` đúng; PageTable nội bộ có
   chủ VSpace sống; object sống khác phải reachable từ cap (hoặc root nội bộ) — `orphans == 0`.
4. **Mapping/Walk**: mỗi mapping đi lại bằng PTE thật qua `PhysMem`: valid/leaf, `level` khớp cỡ
   Frame, `phys == pa + (va & (size−1))`, cờ `R|W|X|U` khớp `MapFlags`; cấp lá lấy từ
   `PAGE_SIZE_BITS` (hình `walk_shape`).
5. **W^X**: recount `mapped_w/mapped_x` từ mapping sống, so với bộ đếm Frame; từ chối
   `w>0 ∧ x>0`; từ chối mapping `device ∧ exec`.
6. **Cap**: mọi cap sống trỏ object sống đúng `kind`/`id`; parent (nếu có) còn sống.

Checker chạy sau **mỗi** pha TAP quan trọng (Retype, byte invariant, W^X, map/unmap, sau revoke)
và trong `init` của **cả hai** ảnh; sai ⇒ TAP đỏ/`PANIC`, không giả xanh.

### 15.9 Ngân sách

Object layer thêm bảng tĩnh `MACHINE` (`Machine<PhysmapMem>`) vào `.data/.bss` — **không** cấp
phát động, **không** đụng trần 262.144 B; arena object/PA là RAM **ngoài** section ELF nên **không**
tính vào budget (giống arena page table KT-46). `budget.sh` in số `data+bss` thật của cả ba ảnh
trong log `kt52-budget*.log`; **không** chép số đo vào tài liệu (QD §9 Q12).

### 15.10 Giới hạn phép đo (không giấu)

1. **Frame 1 GiB là nhãn** trên `-m 128M`: 128 MiB RAM không thể backing 1 GiB. TAP chỉ kiểm
   `size_bits/căn 1 GiB/cửa sổ`; 4 KiB/2 MiB được ghi/đọc thật. Đây đúng D2-66 (`Frame.pa` là nhãn
   PA) — nếu K4/K5 mở nhiều RAM hơn thì cùng mã kiểm được backing thật.
2. **Metadata object trong `.bss`**, không nằm trong Untyped (nợ K3b KT-58); Untyped hiện chỉ cấp
   **dữ liệu** object (bảng PTE) và PA nhãn Frame.
3. **Chưa có child Untyped** (`Retype(Untyped)` ⇒ `NotSupported`) và chưa có CNode/MDB/IPC — K3b.
4. **Single-hart object op** ở KT-52 (BSP); chưa có khoá/MCS cho object layer — K3b.
5. **Không thu hồi bảng trung gian rỗng** sau `unmap` (không cần cho KT-52; walker vẫn đúng).
6. **`Device` không có bit PTE** (Sv39 không định nghĩa) — là cờ chính sách ở mapping.
7. **Không đo pager/COW/shared read-only** — chưa thuộc K3 (D2-71/D2-72 Q13).
8. **Root VSpace chép root kernel là đường test-only** (`#[cfg(feature = "ktest")]`):
   `copy_kernel_root` cho phép đổi `satp` sang root VSpace để TAP 35 chứng minh chuyển dịch phần
   cứng; TAP **chỉ** map ở nhánh root `0x4000_0000` (root index 1 — kernel không dùng), nên
   **không** sửa bảng cấp dưới dùng chung với kernel. VSpace user thật (KT-53) dựng root riêng và
   object layer chưa quản entry kernel chia sẻ; nếu map vào vùng kernel đã có leaf thì
   `missing_tables`/`walk` từ chối `AlreadyMapped` (TAP không làm vậy).
9. **Hai cửa sổ PA của `U0`/`U1` có thể chồng nhau** (đều là **nhãn** 1 GiB): ràng buộc không
   chồng chỉ áp cho **Frame sống** (kiểm toàn cục trong `check_objmem`/`retype_frame`), đúng
   D2-66; TAP 32 revoke Frame `U0` trước khi cấp Frame 1 GiB từ `U1`.
10. **Bảng hole tĩnh `MAX_HOLES = 12`** (mỗi không gian): workload KT-52 (≤ 16 PageTable, cấp/revoke
    LIFO) chưa chạm biên; nếu một lần tách hole cần thêm hole vượt biên, `alloc` trả `None`/lần
    revoke trả `Corrupt` — **fail-closed** (không lần nào lọt qua với byte sai vì `check_objmem`
    kiểm phủ kín watermark); K3b thay bằng free-list động.
11. **L3 — `Device+EXEC` chưa có bộ đếm `mapped_device`** theo Frame (D2-67 "toàn cục"): hiện chặn
    theo **cờ của một mapping**; nếu cần bất biến toàn cục "không Frame nào Device lẫn EXEC" thì
    KT-56 làm rõ D2 + thêm đếm (cùng W^X fault).
12. **L4 — hai nguồn ASID**: VSpace của object layer mang ASID **nhãn** nội bộ (1..255), còn
    `tlb::AsidAllocator` (KT-51) cấp lease cho kernel VSpace — hai bộ đếm chưa hợp nhất; KT-53/K3b
    hợp nhất (lease thật cho mỗi VSpace).
13. **L5 — panic không khôi phục `satp`**: nếu panic xảy ra **trong** cửa sổ TAP 35 (đã đổi
    `satp`), handler chạy với root VSpace đã chép đủ mapping kernel (in/halt được) nhưng **không**
    khôi phục root kernel — chấp nhận vì panic = fatal halt, không resume; không dùng object op
    sau panic.
14. **L6 — `copy_kernel_root` (test-only) chép 512 PTE không tracked** và `map` không giới hạn
    dải VA user: map vào VA đã có **leaf** kernel ⇒ `AlreadyMapped`; map vào **bảng** kernel dùng
    chung có thể ghi PTE của kernel. KT-52 chỉ map nhánh root index 1 (`0x4000_0000`); K3b phải
    theo dõi entry kernel chia sẻ hoặc dựng root riêng hoàn toàn.
15. **L7 — Frame 1 GiB chưa map được**: `map` từ chối `NotSupported` (cửa sổ PA `0x8000_0000` là
    **nhãn** trùm RAM thật trên `-m 128M`); chỉ kiểm nhãn/căn/cửa sổ ở TAP 32. Mở ở K4/K5 khi có
    backing thật (nhiều RAM hơn) hoặc khi có vùng PA riêng ngoài RAM.
16. **L8 — TAP 32 dùng hằng số arch `OBJ_PA_BASE_U1`** (không hardcode trong test) để kiểm đáy cửa
    sổ; không còn phép "kiểm tràn" vô nghĩa.
17. **L9 — nhánh "đệm căn lề thành hole" chưa có test riêng**: mọi cấp phát KT-52 đều 4 KiB/4 KiB
    nên nhánh đó **chưa xảy ra**; giữ cho K3b khi có object cỡ nhỏ hơn/lệch căn (test sẽ thêm cùng
    lúc).

### 15.11 Không thuộc KT-52

CNode/TCB/IPC/scheduler (K3b KT-58..61), U-mode + 3 syscall (KT-53), harness TAP đầy đủ + `expected.txt`
cho mọi ảnh (KT-54), conformance 5 `.scn` (KT-55), `FaultKind`/W^X **fault** + `spec/abi.toml`
(KT-56), soak 1.000 + K3-DOD (KT-57), pager/COW, nhiều VSpace thật cho user, migration/hotplug.

## 16. U-mode + 3 syscall + nạp `init` ELF (KT-53)

### 16.1 Phạm vi: một tác vụ user trong ảnh **test user**, không phải procsrv

- **Code ngữ nghĩa** (bảng số hiệu, 19 mã lỗi, dải user, state machine "TCB-like", dispatch) ở
  [`vifast-plat-common::user`](../kernel/vifast-plat-common/src/user.rs) — `no_std`, **không
  `unsafe`**, host-test `tests/user.rs` (D2-72 Q12). **Code phần cứng** (nạp ELF vào VSpace,
  trapframe, `sret`, đọc user bằng `sstatus.SUM`, asm `user_enter`/`user_exit_resume`) ở
  [`vifast-plat-riscv64::user`](../kernel/vifast-plat-riscv64/src/user.rs), sau **feature `user`**
  (kéo theo `ktest`), thuộc danh sách đóng `unsafe` D2-17.
- **Ảnh riêng** `vifast-kbin-ktest-user`: fault U-mode là **fatal** (log + halt) nên TAP cách ly
  phải kết thúc boot; ảnh chính đã dùng lần kết thúc đó cho trap lồng, ảnh guard cho store fault.
  Bốn TAP U-mode còn lại chạy trước đó trong cùng boot. Vì feature `user` **không** bật ở ảnh sản
  phẩm/ảnh chính/ảnh guard, ba ảnh đó **không** link module U-mode — 37+3 TAP và budget của chúng
  giữ nguyên.
- **Không** CNode/TCB/IPC/scheduler (K3b KT-58..61), **không** procsrv/shell/`wait`/`kill` (K3.5),
  **không** `.scn` (KT-55), **không** `FaultKind` đầy đủ / `spec/abi.toml` (KT-56). Trạng thái
  `UserTask` chỉ là "TCB-like state" tối thiểu (atomically witnessed) — **không phải TCB**, không
  cap, không `fault_ep`.

### 16.2 Nạp `init` ELF: `vifast-elf` + map theo quyền + W^X

- `init` là **ELF64 little-endian, `ET_EXEC`, `e_machine=EM_RISCV`** với các `PT_LOAD`; parser
  [`vifast-elf`](../lib/vifast-elf/src/lib.rs) kiểm header/program header, **từ chối**
  `PT_DYNAMIC`/`PT_INTERP`/`PT_TLS`, segment `W+X`, `vaddr` lệch `p_offset` theo trang, segment
  ngoài cửa sổ user `[0x4000_0000, 0x8000_0000)` (root index 1 — kernel không dùng), chồng lấn,
  entry ngoài segment executable.
- **Nguồn `init`:** [`init/init_user.rs`](../kernel/vifast-kbin-riscv64/init/init_user.rs) (chỉ
  `global_asm!` của [`init/init_user.S`](../kernel/vifast-kbin-riscv64/init/init_user.S)) +
  [`init/init.ld`](../kernel/vifast-kbin-riscv64/init/init.ld). `build.rs` gọi **`$RUSTC`** (cargo
  cấp) với `--target riscv64gc-unknown-none-elf`, `-C panic=abort -C relocation-model=static`,
  `-T init.ld`, `--gc-sections` để sinh `$OUT_DIR/init.elf`; `ktest_user.rs` nhúng bằng
  `include_bytes!`. **Không** toolchain ngoài Rust (không assembler/`objcopy` ngoài), **không**
  sinh file trong cây nguồn, **không** ảnh hưởng ảnh sản phẩm (feature `user` tắt ⇒ build script
  không chạy nhánh này). Chương trình chỉ dùng thanh ghi caller-saved (`a/t`) và một biến `.data`
  — không đụng `s0..s11/gp/tp`, để đường về S-mode khôi phục ngữ cảnh kernel.
- **Layout thật của `init.elf`:** 3 `PT_LOAD` — `.text` `R|X`, `.rodata` `R`, `.data` `R|W` (đủ
  để chứng minh **cả ba** quyền segment, không chỉ text) — entry `0x40000000`. Loader map từng
  trang 4 KiB:
  `.text` → `MapFlags::RX_USER` (U|R|X, **không W**), `.rodata` → U|R|NX, `.data` → `RW_NX_USER`
  (U|R|W, **không X**); Frame 4 KiB zero-fill rồi chép phần file của trang (phần `memsz > filesz`
  đã zero); **stack user** 8 trang `0x40008000..0x40010000` `U|R|W|NX`; **guard** `0x40007000`
  **không map** ngay dưới stack; ảnh ELF phải kết thúc trước guard (`image_end ≤ 0x40007000`).
- **Root VSpace user chỉ chép nửa thấp root kernel** (`index 0..256`, trừ index 1 = cửa sổ user):
  kernel **chạy tại chỗ** khi `satp` trỏ root user cần UART/PLIC (index 0) + image/NOLOAD/arena
  (index 2), tất cả `U=0`. **Nửa cao không được chép** — kernel high alias
  `0xFFFF_FFC0_4000_0000` và physmap `0xFFFF_FFC0_0000_0000` **không tồn tại** trong VSpace user,
  nên U-mode không có PTE nào (kể cả sai `U=1`) tới nửa cao. Hệ quả: sau khi đổi `satp`, kernel
  **không** dùng physmap; syscall kiểm con trỏ bằng **bảng dải VA đã map** (ghi lúc nạp) thay vì
  `walk` PTE. Bằng chứng: `# init load: … high=1` (walk high alias + physmap đều invalid).
- **W^X ba lớp:** parser từ chối `W+X` → loader kiểm lại `writable && executable` → object layer
  (D2-67) chặn W^X **toàn cục** theo Frame. Sau khi map, loader **đi lại PTE thật** để đếm
  `wx_pages` (leaf `W∧X`, phải 0) và `flags_ok` (leaf khớp quyền segment + có `U`), rồi
  `check_objmem`; bằng chứng `# init load: … wx=0 flags=1 check=1`.
- **Cấp phát:** VSpace/Frame lấy từ Untyped `U0` của object layer KT-52 (arena 64 KiB + cửa sổ PA
  1 GiB); bảng trung gian do `VSpace::map` tự cấp; PA của Frame đi qua **physmap** (RAM 128 MiB).
  Không thêm arena/bảng mới.

### 16.3 Trapframe U-mode + `sret` + cơ chế `sscratch` (đã chọn, kèm lý do)

**Cơ chế:** **`sscratch` luôn giữ đỉnh trap stack per-hart** — giống KT-45/KT-49, **không** đổi khi
xuống U-mode:

- `sret` xuống U-mode chỉ đặt `sstatus.SPP=0`, `sepc=entry`, `sp=stack user`; **`sscratch` vẫn là
  đỉnh trap stack** (giá trị kernel). U-mode không đọc/ghi được CSR supervisor nên không thấy và
  không sửa được con trỏ này.
- Trap từ U-mode: `trap_entry` làm `csrrw sp, sscratch, sp` ⇒ `sp` = trap stack top (kernel),
  `sscratch` = sp user; frame lưu `x2` = sp user (asm đọc `sscratch` sau swap). **Một** thanh ghi
  vừa đổi stack vừa giữ con trỏ kernel, không cần global/`tp` phụ.
- Trap từ S-mode (timer/PLIC/IPI) dùng **cùng** đường: `sscratch` vẫn là đỉnh trap stack nên kernel
  stack bị ngắt được giữ trong `sscratch`; `sstatus.SPP` phân biệt nguồn trap nên chính sách
  syscall/fault tách sạch.
- **Đường về S-mode sau `Exit`:** `user_enter` lưu ngữ cảnh kernel `(ra, sp, gp, tp, s0..s11)` vào
  `USER_KERNEL_CTX` (`.bss`, một tác vụ/hart ở KT-53); handler đặt `sstatus.SPP=1` (asm resume của
  `trap_entry` **không** ghi `sstatus` — P2a KT-45), `sepc = user_exit_resume`, rồi asm resume
  `sret` về S-mode; `user_exit_resume` khôi phục ngữ cảnh kernel và `ret` về caller Rust. **Không**
  quay lại U-mode, **không** dùng stack user.

**Vì sao không chọn "`sscratch` = con trỏ user":** (1) sẽ cần thêm một nguồn lưu con trỏ kernel
(global/`tp`) và phân nhánh trap S/U ngay trong asm; (2) trap từ S-mode vẫn phải có đỉnh trap stack
nên `sscratch` kiểu gì cũng phải giữ giá trị kernel trong kernel; (3) cơ chế hiện tại đã chạy cho
cả hai nguồn trap, ít trạng thái hơn, không lộ địa chỉ kernel.

**Trapframe lúc vào U-mode** (`TrapFrame` 288 B của KT-45): `sepc = e_entry`; `x[2] = stack top`;
`x[10] = mode` (0 = lượt syscall, 1 = lượt fault cách ly); `x[3]=x[4]=0` (gp/tp user = 0); các
thanh ghi khác 0; `sstatus = sstatus hiện tại & !(SPP|SPIE|SIE|SUM)` — giữ `UXL/FS` WARL, tắt ngắt
S-mode và SUM trong U-phase. Bằng chứng: `# init run: … u_mode=1 sret_spp=0 ecall_spp=0`.

### 16.4 Ba syscall + mã lỗi ABI (không panic)

ABI đọc từ `spec/abi.toml` `[calling.riscv64]`: `a7` = số hiệu, `a0/a1` = tham số, trả `a0` =
error (0 = OK), `a1` = value.

| # | Syscall | Hành vi KT-53 | Trả |
|---|---|---|---|
| 12 | `DebugPutStr` | kiểm `[ptr, ptr+len)` trọn dải user + **trọn một dải VA đã map** (bảng dải loader ghi: segment + stack), `len ≤ 4096`; đọc raw bằng `sstatus.SUM` (tắt ngay sau mỗi khối 64 B) rồi in ra UART | `a0=0`, `a1=len`; lỗi dải ⇒ 5, ngoài dải đã map ⇒ 11 |
| 13 | `DebugHalt` | log `# user debug_halt: code=N` rồi SBI SRST (`DebugHalt` chỉ cho test — QD §4) | không trả |
| 14 | `Exit(code)` | `UserTask` ⇒ `Exited`, lưu `exit_code`; trapframe chuyển về S-mode (`§16.3`) — **không** quay lại U-mode | không trả |

- **Số hiệu trong dải `0..=15` nhưng chưa cài** (IPC/sched/clock/`DebugGetInfo` 15 — K3b/K3.5) ⇒
  `a0 = NotSupported` (**18**). **Ngoài dải** (16..31 reserved, 99, EID SBI `0x53525354`…) ⇒
  `a0 = RangeError` (**19**, **D2-79**: số syscall là *chỉ số* vào bảng dispatch, spec
  `[error]` 19 = "chỉ số/chỉ số dải ngoài phạm vi"), ghi `bad_calls/last_bad_nr` — **không
  panic** (TAP 4 kiểm 2 lần: EID SBI + 99). Mã 5 `InvalidArgument` chỉ dùng cho tham số sai
  **trong** một syscall hợp lệ (vd `DebugPutStr` quá dài / con trỏ ngoài dải).
- Bảng 19 mã lỗi (1 `InvalidCapability` … 19 `RangeError`) + tên ở
  `vifast_plat_common::user::error_name`, khớp `spec/abi.toml` `[error]` (chỉ **đọc** spec, không
  sửa). `DebugGetInfo` **chưa cài ở KT-53** (không cần cho `init`; ABI đầy đủ là K3b/K3.5).
- **U-mode không gọi được SBI:** `ecall` từ U-mode luôn trap `scause=8` vào dispatcher; EID SBI nằm
  ngoài dải syscall nên chỉ nhận mã lỗi 5 — không có đường nào để U-mode chạy `ecall` S-mode.

### 16.5 Cách ly + chính sách fault U-mode

- **Cách ly địa chỉ:** root VSpace user chỉ chép **nửa thấp** root kernel (**test-only** KT-52
  L6 — `copy_kernel_low_root`) để kernel sống khi `satp` trỏ root user; mọi leaf kernel giữ `U=0`
  và **nửa cao (high alias/physmap) không map**. U-mode đọc `0x80200000` ⇒ **load page fault**
  `scause=13`, `stval=0x80200000`; U-mode **không** đọc/ghi được địa chỉ kernel, không có đường
  PTE tới nửa cao. U-mode chạy với `tp=0` (không lộ hart id/con trỏ kernel); trap handler U-mode
  **không** dùng `hart_id()`/PerHart (chỉ `TRAP_STATS`/`USER_TASK` toàn cục).
- **Chính sách:** mọi exception từ U-mode **không phải** `EnvironmentCallFromU` là **fatal**:
  ghi `USER_TASK.record_fault(scause, stval, sepc)` + `TRAP_STATS` (fatal), in
  `# u-mode fault: scause=… stval=… sepc=… from_u=1 kernel_addr=… fatal=1`, rồi
  `PANIC: u-mode fault …` + `DebugHalt(1)` — **không** treo, **không** resume, **không** pager
  (`FaultKind` đầy đủ là KT-56). Đọc/ghi kernel từ U-mode đều cùng chính sách; TAP kiểm **read**
  (`scause=13`) theo đúng tên test, còn store U-mode (`scause=15`) cùng PTE `U=0` — ghi nợ đo riêng
  (§16.8).
- **Bẫy lỗi con trỏ user:** `DebugPutStr` kiểm dải + walk PTE **trước** khi chạm bộ nhớ, nên con
  trỏ user xấu trả mã lỗi ABI (5/11) — **không** biến thành page fault S-mode (fatal).

### 16.6 TAP 1..5 (ảnh `vifast-kbin-ktest-user`) + runner

| TAP | Tên | Bằng chứng (`ktest.sh` grep) |
|---|---|---|
| 1 | `init u-mode` | `# init load: elf=1 entry=0x40000000 segs=3 text=1 ro=1 data=1 stack=8 image_end=0x… guard=0x40007000 guard_none=1 wx=0 flags=1 high=1 root=0x… asid=… check=1` + `# init run: … u_mode=1 sret_spp=0 ecall_spp=0 entries=1 syscalls=5 exits=1 exit=42` + `# u sstatus: enter=0x… trap=0x… spp=0 spie=0 sie=0 sum=0 uxl=2` |
| 2 | `syscall debug_put_str` | `# user put_str: calls=2 bytes=51 sum_off=1` + **byte user** in ra: `U: init hello from u-mode`, `U: bad syscall err=19 ok` |
| 3 | `syscall exit` | `# user exit: code=42 state=Exited exits=1` |
| 4 | `syscall la ma loi` | `# user badsys: nr=99 err=19 calls=2 panic=0` |
| 5 | `u-mode kernel read fault` | `ok 5` in **trước**, rồi `# u-mode read: … addr=0x80200000 expect=scause=0xd stval=0x80200000`, `# u-mode fault: scause=0xd stval=0x80200000 … from_u=1 kernel_addr=1 fatal=1`, `PANIC: u-mode fault scause=0xd stval=0x80200000 … from_u=1` |

- `expected.txt` thêm `ktest-user 5 5`; `ktest.sh` build ảnh user riêng (`--features ktest,user`),
  kiểm budget + chuỗi `ktest`, boot `-machine virt,aia=none,aclint=on -cpu rv64 -m 128M -smp N
  -icount shift=6`, đòi `Platform HART Count` của OpenSBI khớp N, TAP 100 %, **chỉ** dòng PANIC
  `u-mode fault` được phép. Mỗi vòng `NBOOT` boot **3 ảnh test** × matrix `1/2/4`; summary
  `N_BOOT*3`.
- Ảnh chính vẫn **37 TAP**, ảnh guard **3 TAP** — không đổi số/không renumber (feature `user` chỉ
  bật ở ảnh user).

### 16.7 Ngân sách và tệp

- Không thêm section ELF vào ảnh sản phẩm/ảnh chính/guard: module U-mode nằm sau feature `user`;
  ảnh user thêm `init.elf` (nhúng `.rodata`) + bảng `USER_TASK`/`USER_KERNEL_CTX` nhỏ trong
  `.data/.bss`; trần **262.144 B** `[TK]` vẫn áp cho **cả bốn** ảnh (D2-72 Q5).
  `budget.sh`/`ktest.sh` in số thật trong log `kt53-budget.log`/`kt53-ktest.log` — **không** chép
  số đo vào tài liệu (QD §9 Q12).
- Tệp: `v2/kernel/vifast-plat-common/src/user.rs` + `tests/user.rs`;
  `v2/kernel/vifast-plat-riscv64/src/user.rs`; `v2/kernel/vifast-kbin-riscv64/{build.rs,init/*,
  src/ktest_user.rs,Cargo.toml}`; `v2/kernel/vifast-plat-riscv64/lib.rs` (nhánh trap U-mode),
  `v2/kernel/vifast-plat-common/{lib.rs,trap.rs}` (`report_u_fault`);
  `v2/test/boot/{ktest.sh,expected.txt}`.

### 16.8 Giới hạn phép đo (không giấu)

1. **Một tác vụ user, một hart (BSP)**; ảnh user không `smp::init` nên hart phụ không chạy; chưa
   có scheduler/TCB/IPC — K3b.
2. **Không có TCB/cap/fault_ep:** `Exit` chỉ chuyển `UserTask` sang `Exited` + lưu mã; không gửi
   `FaultKind::Exit{code}` tới `fault_ep`, không `BlockedOnSend`, không `wait`/`kill` — K3.5
   (QD §1 Q-A nói rõ các phần đó là procsrv).
3. **Fault U-mode là fatal** (không pager/COW/fault endpoint); `FaultKind` đầy đủ + `spec/abi.toml`
   là KT-56.
4. **Store U-mode (`scause=15`) không có TAP riêng**: dùng chung PTE `U=0` với load (13, TAP 5);
   ảnh guard đã đo store fault 15 từ S-mode. Nếu cần đo U-mode 15 phải thêm boot/ảnh riêng.
5. **`DebugGetInfo` (15) chưa cài** — trả `NotSupported`; chưa kiểm `UXL`/FPU/CSR user khác.
6. **Loader test-only, lỗi giữa chừng không dọn object** (VSpace/Frame đã cấp có thể còn); đo
   `check_objmem` chỉ ở đường thành công. Đường lỗi in `not ok` + halt; K3.5 cần loader có
   teardown/commit hai pha.
7. **VSpace user dùng ASID nhãn nội bộ (1..255)** của object layer, chưa hợp nhất lease
   `tlb::AsidAllocator` (L4 KT-52) — một nguồn ASID thật là K3b/K4.
8. **Root VSpace chép nửa thấp root kernel (test-only)** và chỉ map root index 1; **không** chép
   nửa cao (high/physmap) nên handler U-mode **không** được dùng physmap — `DebugPutStr` dùng bảng
   dải VA tĩnh (tối đa `MAX_LOAD_SEGMENTS+1` dải) thay cho `walk` PTE; chưa theo dõi entry kernel
   chia sẻ (L6 KT-52); `sstatus.SUM` chỉ bật trong cửa sổ đọc `DebugPutStr` (tắt ngay sau mỗi khối,
   bằng chứng `sum_off=1`).
9. **Guard stack U-mode**: TAP 1 xác nhận **leaf guard invalid** bằng `walk` (`guard_none=1`),
   nhưng **không** có test tràn stack thật; tràn stack sẽ fault U-mode ⇒ fatal (policy đã có).
10. **Không kiểm chữ ký `init`/file system**: ELF nhúng từ build; nạp từ FS/pager là K3.5/K4.
11. **`SUM` mới đo một chiều**: bằng chứng `enter/trap sstatus` + `sum_off=1` chứng minh SUM tắt
    ở hai đầu (vào U và sau `DebugPutStr`); chưa đo SUM trong lúc đọc (chỉ suy từ thứ tự
    `set_sum(true) … set_sum(false)` + trap sstatus).
12. **`sscratch` chứng minh gián tiếp**: trap từ U-mode lưu `x2` = sp user và handler chạy trên
    trap stack, nhưng chưa có TAP đọc `sscratch` trực tiếp trong U-mode (U-mode không đọc được
    CSR supervisor; đo trực tiếp cần trap tiêm/host model).
13. **Cách ly mới đo 1 địa chỉ kernel**: TAP 5 đọc `0x80200000` (leaf identity). Chưa đo U-mode
    chạm root index 0 (UART/PLIC) hay arena/page-table RAM; có thể thêm nếu cần (rẻ: thêm lượt
    fault/boot riêng).
14. **U-phase `SIE=0`** (cố ý): `init` lỗi lặp vô hạn sẽ treo hart tới timeout 20 s của runner
    (`T-INFRA`/treo, không panic); chưa có watchdog/preemption — K3.5.
15. **Sau `Exit`, `satp` vẫn root user + `SIE=0`**: ảnh test kết thúc bằng fault nên chấp nhận;
    K3.5 cần chuyển về root kernel/TCB idle trước khi chạy tiếp (fragile nếu tái dùng đường này).
16. **`USER_RANGES` thay `walk`**: sau khi đổi `satp`, handler U-mode không có physmap nên kiểm
    con trỏ bằng bảng dải tĩnh (`MAX_LOAD_SEGMENTS+1`); nếu loader map thêm trang ngoài segment/
    stack (tương lai) phải cập nhật bảng. `DebugGetInfo` (15) chưa cài.
17. **Store `scause=15` từ U-mode chưa có TAP**: TAP 5 là load `scause=13`; store cùng PTE `U=0`
    nhưng chưa đo riêng từ U-mode (ảnh guard đo store 15 từ S-mode).

### 16.9 Không thuộc KT-53

CNode/TCB/Untyped child/MDB/IPC/Reply/Notification/WaitAddr/scheduler (K3b KT-58..61), procsrv +
shell Rust + `wait`/`kill` + `FaultKind::Exit` tới `fault_ep` (K3.5), harness TAP matrix/`expected.txt`
đầy đủ (KT-54), conformance 5 `.scn` × sim/QEMU (KT-55), `FaultKind` đầy đủ + W^X **fault** +
`spec/abi.toml` (KT-56), soak 1.000/K3-DOD (KT-57), pager/COW/shared memory, U-mode đa hart, nạp
ELF từ FS.
