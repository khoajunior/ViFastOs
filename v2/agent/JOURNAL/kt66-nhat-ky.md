<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# KT-66 — nhật ký (cổng K3.5: ảnh demo tách + stack syscall per-hart + đo cổng)

Người ghi: **Agent 2 dòng OS** · 2026-11-16 · HEAD khi mở: `c607d0a9` (`[KT-65]`) · Nhánh `main`
· Máy `ps` (12 nhân, QEMU `11.1.3`, OpenSBI `v1.8.1`).

## 1. Vào task

- Baseline `gate --fast`: **PASS 649 test/84 suite**, L1 1.000 seed **891 ms**
  (`JOURNAL/kt66-baseline-gate.log`). 3 ảnh test KT-65: `ktest` 60/60, guard 3/3,
  `ktest-user` 24/24 (`/tmp/kt66-base/`).
- Ngân sách 3 h; ưu tiên DoD đo được: tách ảnh, stack per-hart, L1 400k, soak, conformance,
  budget, gate, DOD.

## 2. Đỏ trước (D2-05)

| Việc | Cách tạo đỏ | Log |
|---|---|---|
| Tách ảnh | log 24 TAP cũ kiểm bằng `expected.txt` mới (14) | `kt66-red-before-split.log` |
| Stack dùng chung | cấp cùng block hart 0 cho mọi hart, boot `-smp 2` | `kt66-red-before-stack.log` (`distinct=0`) |
| Canary | ghi đè word canary hart 0 | `kt66-red-before-canary.log` (`canary=1`) |
| **Arena** | cấp stack dispatch từ `Frame` (PA `0x82000000`) | `PANIC: trap StorePageFault … stval=0xffffffc082007ff8` (quan sát trong phiên; **log thô bị ghi đè** khi chuyển NOLOAD — ghi chú, không phải log DoD; bằng chứng kiến trúc `none_physmap=1` ở `JOURNAL/kt66/ktest-smp1-1.log:82`) |

Bài học arena: `copy_kernel_low_root` **không** chép root index ≥ 256, mà trap từ U-mode chạy với
`satp` = root user ⇒ stack phải ở vùng **identity map thấp**. Chọn **NOLOAD per-hart** (đúng chỗ,
không đụng `.data+.bss`).

## 3. Việc làm

1. **Tách ảnh**: `ktest_proc.rs` (TAP 1..10, ánh xạ 15..24), `ktest_user.rs` còn 14 TAP + kết thúc
   sạch (một dòng PANIC in thẳng + `halt`, không qua `panic!` để không sinh `PANIC at …` thứ hai);
   `proc_demo.rs` 15..21 → 1..7; `Cargo.toml` bin thứ tư; `expected.txt` 4 dòng; runner:
   `LABELS` 4, `LABEL_COUNT`, `manifest_proc`, evidence split, `check_stack_lines`, `want_rows`.
2. **Stack per-hart**: record NOLOAD `guard|stack|trap|**syscall 32K**` ⇒ `HART_STRIDE 0x1D000`,
   `HART_AREA_BYTES 0xE8000`; `smp.rs` offset + const asserts; `linker.ld` `.syscall_stack` +
   `.hart_stacks 7×`; `mmu.rs` extern + `syscall_stack_range_of`; `lib.rs` asm `h*0x1D000` (dịch/cộng,
   `0x11000`/`0x15000` offset trong record **không** đổi); `user.rs` dispatch đổi `sp` sang block
   hart hiện tại (asm chỉ `t0/t1`, đọc lại `STACK_HART` hai phía `call`).
3. **Canary + low**: sơn `0xAA` 32 KiB, 2 word canary đáy, kiểm sau mỗi dispatch + lúc in evidence;
   `# stack: noload=1 block=32768 harts=N distinct=1 canary=0 base=0x…` + dòng/hart
   (`base/top/low/used/canary`).
4. **R1**: `v2 check` báo `user.rs` 1.582 dòng ⇒ tách `user_stack.rs` (217 dòng) + `user.rs` 1.406;
   `fmt` lại; build lại **5 ảnh**; **chạy lại** matrix/soak/conformance trên ảnh cuối (lead dặn:
   không dùng số cũ cho ảnh mới). Ảnh cuối: product `ce9a8c7c…`/`eb365a13…`, ktest `9cc6c8f2…`,
   guard `f5afbe3c…`, user `7640ab99…`, proc `fdc8e3bd…`.

## 4. Số đo (nguồn: `K35-DOD.md` §0 + `log:dòng`)

- Matrix **12/12** boot (60+3+14+10 TAP × smp 1/2/4).
- Soak **0/1000**, wall 225 s, BUILD_ID/sha khớp ảnh product.
- **L1 400.000 lượt, 0 vi phạm** — 4 tiến trình **song song** (mỗi cấu hình 100k seed), wall
  **33m30s**; **không** tuần tự. Nói rõ: trong ~20 phút đầu có build kernel + matrix QEMU chạy
  song song; L1 bất biến theo seed, không dùng wall L1 làm số hiệu năng.
- Conformance **61/61 dòng, 5/5 ca**, `build=9cc6c8f295cab617` khớp ảnh ktest.
- Budget: `size_of::<Machine>()` **188.872** ≤ 196.608; data+bss **200.120 / 209.856 / 2.680 /
  202.280 / 206.472** ≤ 262.144; NOLOAD per-hart **950.272 B** (báo riêng).
- `ktest-user` **239.216 → 202.280** (−36.936 B); proc **206.472**.
- `gate --fast` **PASS 649/84**, L1 **911 ms**; clippy kernel 5 tổ hợp `EXIT=0`; `v2 check` PASS; `selftest.sh` **18/18**.

## 5. Vá reviewer vòng 1 (DRIFT, không UNSAFE)

- **M3 (harness false-green, lần 3 trong dự án):** `check_stack_lines` dùng `strtonum()` (chỉ gawk);
  BSD awk báo lỗi ⇒ stdout rỗng ⇒ `bad=""` ⇒ PASS giả, `low`/`block` chưa từng được kiểm. Vá:
  parse `sed` + tính **bash `16#…`**, **fail-closed** (thiếu dòng/hex hỏng ⇒ ĐỎ), thêm
  `ktest.sh --selftest-stack` **5/5** và ca trong `selftest.sh` (**18/18**).
- **M1:** canary hỏng ⇒ in `# stack: canary=1 hart=K` + **PANIC/halt** (fail-closed), không chạy
  tiếp trên trạng thái hỏng.
- **M2:** nợ #9 DOD: stack demo **22.088/32.768 B** (headroom **10,4 KiB**), canary **đã**
  fail-closed, **hart 1..3 chưa từng dispatch** (scheduler demo chỉ hart 0 — nợ IPI #6).
- **M4:** `ktest-user` file **335.520** (rồi 375.512 cho proc); L1 ghi **UTC** `23:56:49Z →
  00:30:19Z`; thêm câu **NOLOAD 950.272 B vẫn chiếm RAM** (mmu map identity toàn vùng).
- **M5:** bỏ dẫn chứng `kt66-red-before-arena-stack.log` (**không tồn tại**) khỏi `user_stack.rs`;
  DOD **đóng băng** + ghi sha ở §13.1 trước commit.
- **D4:** `distinct` so **mọi cặp** hart (không chỉ với hart 0); **D3:** ghi rõ "đã cấp riêng 4
  base, **chưa** dispatch song song hart 1..3"; **D9:** `expected.txt` "bốn nhãn", `ktest.sh` "bốn
  ảnh".
- Ảnh sau vá: product/ktest/guard **không đổi** (`ce9a8c7c`/`9cc6c8f2`/`f5afbe3c`), user
  `fbc03a81…`, proc `333c35bb…` ⇒ matrix/soak/conformance/`gate` chạy lại; soak product vẫn khớp
  sha `eb365a13…`.

## 6. Nợ / bàn giao

- Nợ 9 mục ghi ở `STATE.md` + `K35-DOD.md` §12.2 (fault bridge K3.6, kill chưa Revoke,
  `kill pid sai`, shim không workspace member, shell chưa UART IRQ, core chưa IPI, `TlbFlush`
  `targets/acked`, slot 192 word, stack 32 KiB sát 22.088 B demo).
- Ảnh demo **không** giấu vào ảnh cũ; matrix **không** bỏ slot; không nới ngưỡng nào.
