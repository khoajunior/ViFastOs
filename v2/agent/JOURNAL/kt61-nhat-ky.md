# KT-61 — nhật ký (K3b cổng: nối core `no_std` vào kernel thật + đo ảnh + soak + conformance)

Ngày: **2026-11-15** · Agent 2 dòng OS · HEAD khi mở: `99d3b7bf` · Nhánh `main` · Máy `ps`
(12 nhân, QEMU 11.1.3, OpenSBI v1.8.1). Báo cáo cổng: [`../reports/K3B-DOD.md`](../reports/K3B-DOD.md).
Chi tiết tầng core: [`../../docs/k3b-core.md`](../../docs/k3b-core.md) §9; platform:
[`../../docs/k3-riscv64.md`](../../docs/k3-riscv64.md) §21.

## 0. Kết luận ngắn

**KT-61 DONE (đề xuất).** Core `no_std` **chạy trên nhân thật**: TAP **43..52** trên QEMU (smp
1/2/4), ảnh sản phẩm chạy một vòng đời TCB mỗi boot ⇒ soak **0/1000**. **Hai ngưỡng** (D2-85a):
`size_of::<Machine>()` **180.168 B** ≤ pin **196.608 B**; ảnh `.data+.bss` **191.416** (product) /
**197.568** (ktest) ≤ trần **262.144**. Static nằm **`.bss`** (không vào file: segment RW
`filesz` 7.760/7.912; file 185.464 / 484.600 B), khởi tạo **tại chỗ** `Machine::init_zeroed`.
Kernel clippy `-D warnings` **đã sạch** (18+31 lint nợ sửa hết, BUILD_ID không đổi). Không đổi
`spec/`, sim semantics, `.scn`, QUEUE/DECISIONS/AGENTS, `viscript/`; `unsafe` chỉ ở
`kernel/vifast-plat-*` (D2-17).

## 1. Trình tự (9/9 bước)

1. Baseline: `gate --fast` **PASS 598/78**, L1 1.000 seed 850 ms (`kt61-baseline-gate.log`);
   `HEAD..origin/main = 0`; `v2/` sạch.
2. Đọc nợ: `k3b-core.md` §8.5 (hai ngưỡng, phương án cắt A..D) + §8.6 (7 nợ KT-60) + K3-PLAN
   §5.2/§5.3 + D2-82..85.
3. **Đỏ trước #1** (tính năng chưa có): build ảnh `ktest` từ **HEAD** trong worktree tạm, boot
   QEMU ⇒ **43 TAP**; runner **mới** (53 TAP + 11 evidence `# core:`) **đỏ**: thiếu
   `tap43..tap53` + `ev72..ev82` (`kt61-red-before-core-tap.log`, `kt61-red-before-ktest-43tap.log`).
4. Code: `core_bridge.rs` (static/init/lock/boot smoke) + `ktest_core.rs` (TAP 43..52) + `main.rs`
   (product smoke) + `ktest.rs` (plan 53) + `ktest.sh`/`expected.txt` (manifest + evidence).
5. **Đỏ thật #2** (hai bản hỏng, đều có log): (a) `static Machine = with_harts(1)` → **`.data`**:
   180 KB vào **file** ảnh (product 365.632, ktest 664.824) — đắt, bỏ; (b) `MaybeUninit` +
   `MACHINE.write(with_harts(1))` → struct lớn trả qua `sret` ⇒ bản tạm **180.168 B trên stack
   64 KiB** ⇒ tràn guard page `PANIC: guard page fault stval=0x80400000`
   (`kt61-red-before-core-link.log`). **Chốt:** `MaybeUninit` (`.bss`) + `write_bytes(0)` +
   `Machine::init_zeroed` tại chỗ — không stack, không byte nào trong file.
6. **Đỏ thật #3**: `init_zeroed` bản đầu để zero thuần ⇒ `HartSched.idle = false` và 8 ô
   `Option<MailboxMsg>` zero = `Some(Wake(0))` ⇒ TAP 46 `check_sched=0` + TAP 52
   `check_mailboxes=0` ("idle lech current", "hop thu mat/them message") — log
   `kt61-red-before-init-zeroed.log`; vá `HartSched::reset`/`HartMailbox::reset` ⇒ xanh
   (`kt61-init-zeroed-fixed.log`). Đây là lý do core **phải** có đường init tại chỗ tường minh.
7. Vá TAP trong quá trình chạy thật: (a) TCB **không có SchedContext** thì `tcb_has_budget` false
   ⇒ không được `pick` — cấp SC riêng cho cả 2 TCB (đúng biên KT-20, không phải mẹo test); (b)
   core **từ chối** `add_kernel_object(Frame)` (`is_core()` false) ⇒ khoá `WaitAddr` dùng **PPN**
   của Frame **obj layer** (một chiều, D2-83) thay vì tạo Frame trong core — biên giới được chính
   core cưỡng chế.
8. Đo (cây đóng băng, BUILD_ID cuối): `ktest.sh 1 matrix` **9/9** (`ktest` **53/53** + guard 3/3
   + user 7/7); `soak.sh 1000 3` **1000/1000, 0 lỗi**, wall 227 s, BUILD_ID `34dc594e0f651ed3`;
   conformance **61/61 / 5/5**, `build=aab21609a80ca7a1`; budget 4 ảnh + `filesz/memsz`
   (`kt61-budget.log`, `kt61-image-size.log`).
9. Gate: (i) đỏ **fmt** ở `ktest_core.rs` → `cargo fmt`; (ii) đỏ **R1** `machine.rs` 1.516 dòng →
   nén doc `init_zeroed` còn **1.500** dòng (BUILD_ID **không đổi** nên mọi log cổng vẫn neo);
   gate cuối xanh (`kt61-gate.log`). Sửa lỗi thật, không retry lấy xanh (D2-07). Kernel clippy
   `-D warnings` sửa **49 lint nợ** (18 + 31 + 2 user.rs) — sạch, BUILD_ID không đổi.
10. Tài liệu: `k3b-core.md` §9/§10 + status/nợ, `k3-riscv64.md` §21, `K3B-DOD.md` D1..D11, journal
   này, `STATE.md`. Review độc lập trước commit (lead spawn `reviewer-kt61`, tối đa 20 phút).

## 2. Quyết định kỹ thuật (có lý do, có log)

| # | Quyết định | Lý do / bằng chứng |
|---|---|---|
| Q1 | **Static `Machine` trong `.bss`** + `init_zeroed` tại chỗ | `.data` = 180 KB vào **file**; `MaybeUninit`+`write` = tràn stack 64 KiB; chốt `.bss` + `init_zeroed` (zero + sentinel tường minh). `.data+.bss` vẫn cộng đủ 180.168 B |
| Q2 | **Không** chuyển `SlotPool` sang arena | làm nửa đường ⇒ hai nguồn sự thật cho cùng cap; giữ `SlotPool` là nguồn duy nhất (nợ K3.5, tiêu chí ở `k3b-core.md` §9.6) |
| Q3 | **Không** tạo Frame/VM trong core | D2-83: VM thuộc object layer; `add_kernel_object(Frame)` panic (core tự chặn); WaitAddr dùng **PPN** obj-layer làm khoá |
| Q4 | **Không** làm cap transfer/finalise IPC/TlbFlush thật | không cần cho cổng K3b; làm nửa đường phá "một nguồn sự thật"; ghi nợ K3.5 §11 DOD |
| Q5 | Ảnh sản phẩm **cũng chạy core** (không chỉ ảnh test) | soak 1.000 boot mới thật sự kiểm core trên kernel sản phẩm; lỗi ⇒ `PANIC: core init product` (soak đỏ) |
| Q6 | TAP core **thêm vào ảnh `ktest`** (không tạo ảnh thứ 4) | giữ 1 nguồn manifest/expected; matrix sẵn có 1/2/4; trap lồng dời 43→53; ít rủi ro hơn thêm bin/label |

## 3. Log trong thư mục này (KT-61)

| Tệp | Nội dung |
|---|---|
| `kt61-baseline-gate.log` | baseline `gate --fast` (598/78, L1 850 ms) |
| `kt61-red-before-ktest-43tap.log` | log boot ảnh `ktest` **HEAD** (43 TAP) trong worktree tạm |
| `kt61-red-before-core-tap.log` | runner **mới** từ chối log 43 TAP (thiếu `tap43..53` + 11 evidence) |
| `kt61-red-before-core-link.log` | PANIC guard page fault của bản `MaybeUninit` (đỏ thật #2) |
| `kt61-ktest-matrix.log` | matrix `smp 1/2/4` × 3 ảnh: **9/9** sạch, TAP 53/53 + 3/3 + 7/7 |
| `kt61-soak.log` + `kt61-soak-results.txt` | soak **0/1000**, 3 luồng, không icount, wall 227 s, BUILD_ID `34dc594e0f651ed3` / sha ảnh-soak `c7cc50b1…` |
| `kt61-trace-compare.log` | conformance **61/61 / 5/5**, `build=aab21609a80ca7a1` |
| `kt61-budget.log` | `.data+.bss` 4 ảnh: 191.416 / 197.568 / 2.680 / 11.752 ≤ 262.144 |
| `kt61-image-size.log` | `wc -c` + `llvm-readobj --program-headers`: file 185.464 / 484.600 / 53.808 / 151.912; segment RW `filesz` 7.760 / 7.912 / 0 / 7.704 ⇒ **`.bss` không vào file** |
| `kt61-red-before-init-zeroed.log`, `kt61-init-zeroed-fixed.log` | đỏ #3 (`init_zeroed` thiếu sentinel → `check_sched`/`check_mailboxes`) và bản vá |
| `kt61-soak-red-threshold.log` | `soak.sh 999 3` ⇒ **exit 2** (không nới N, D2-24) |
| `kt61-core-build.log` | **log sạch cuối**: build 4 ảnh + `cargo clippy -D warnings` toàn kernel (3 crate × mọi feature) — 0 warning |
| `kt61-core-build-before-fix.log` | log **đỏ trước khi sửa** (7 lỗi `plat-riscv64` + 27 lỗi `kbin`) — giữ làm đỏ-trước |
| `kt61-gate.log` | `gate --fast` cuối (sau fmt) |
| `kt61-l1-100k.log` | L1 100.000 seed × smp 1/2/4/8 — **một phần**: `smp 1` xong (`# L1 OK`, dòng 253); `smp 2/4/8` dừng khi hết 3 h, ghi `EXIT=143` (không claim 400k) |

## 4. Commit

- `[KT-61]` = **`1e2455c8`** — kernel + harness + docs (`k3b-core.md`, `k3-riscv64.md`,
  `K3B-DOD.md`) + toàn bộ log `kt61-*`; **chỉ `v2/**`**.
- `[KT-61b]` = `v2/agent/JOURNAL/kt61-nhat-ky.md` + `v2/agent/STATE.md` (tệp này); hash b ghi ở
  bảng lịch sử `STATE.md` (không nhúng được vào chính nó).
- `HEAD..origin/main = 0` trước push; push `origin main`.

## 5. Ghi chú trung thực (không tô hồng)

- `size_of::<Machine>()` = **180.168 B** đã có từ KT-60; KT-61 **không** làm nó nhỏ hơn, chỉ
  chứng minh link thật và đo ảnh. Không có "tối ưu mới" nào được claim.
- Static nằm **`.bss`** + `Machine::init_zeroed` (không phải `.data`): 180.168 B **không** vào
  file (RW segment `filesz` 7.760/7.912; file 185.464/484.600). Bản `.data` từng dùng đã **bỏ**
  (file 365.632/664.824) — log `kt61-image-size.log`. Ngân sách `.data+.bss` vẫn cộng đủ.
- `SlotPool` **vẫn** trong `.bss`; chưa phải arena vật lý — nợ K3.5, không mô tả là "đã xong".
- Kernel clippy `-D warnings` **đã sạch** sau KT-61 (sửa nợ tích tụ: 18 `plat-common`+`plat-riscv64`
  + 27 `kbin` + 2 `user.rs`), BUILD_ID `.text` không đổi;
  log sạch `kt61-core-build.log`, log đỏ trước `kt61-core-build-before-fix.log`. **Nợ quy trình
  còn lại:** `gate --fast` chưa phủ kernel ⇒ đề xuất task riêng (DOD §2.1).
- Soak `0/1000` là **trần** (QD §5.3), không phải bảo đảm; wall 227 s phụ thuộc tải máy, không
  phải ngưỡng. Không retry (D2-07).
- **L1 100k mới xong 1/4 cấu hình** (smp 1, 100.000 seed × 64 op OK); smp 2/4/8 dừng khi chạm
  ngân sách 3 h — **không** viết "400.000 lượt"; `400k × smp 1/2/4/8` là **DoD cổng K6**, không
  phải KT-61; phần còn lại là nợ thời gian có task riêng (DOD §11 dòng 10).
- Ảnh product **rebuild cuối** sau soak (doc/comment-only): `BUILD_ID` `.text` vẫn
  `34dc594e0f651ed3`; sha toàn ảnh đã soak `c7cc50b1…`, bản rebuild `fb68981d…` — neo D2-80 bằng
  **`BUILD_ID` `.text`**, không dùng sha toàn ảnh.
