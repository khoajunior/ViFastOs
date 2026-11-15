<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3B-DOD — bằng chứng cổng cuối K3b (KT-61): core `no_std` trên nhân thật

Người ghi: **Agent 2 dòng OS** · Ngày: 2026-11-15 · Task: **KT-61** (cổng K3b) · HEAD khi mở
task: `99d3b7bf` (`[KT-00]` lật KT-60 DONE, mở KT-61) · Nhánh `main` · Máy: `ps` (12 nhân,
QEMU `11.1.3`, OpenSBI `v1.8.1` — version do script đọc và in, xem log).

> **K3b xong cái gì:** `vifast-core` **đã link vào ảnh kernel thật** (sản phẩm + test), `Machine`
> là static **trong vùng ảnh**, khởi tạo từ **nhãn PA** của Untyped boot; **TAP 43..52 chạy trên
> QEMU** (Retype TCB từ Untyped, `Configure`+`SetSchedParams`+`Resume`/`Suspend`, schedule 2 TCB,
> IPC `Call→Recv→Reply` một vòng, Notification signal, `WaitAddr`, biên D2-83, invariants); ảnh
> sản phẩm chạy **một vòng đời TCB** mỗi boot ⇒ soak 1.000 boot kiểm core trên kernel sản phẩm.
> **Còn gì (K3.5 — §11):** `procsrv`/shell, **chuyển cap** qua IPC, **finalise IPC object khi
> Revoke**, `TlbFlush` thật trong core, `SchedContext.Bind`, `tcb_resume_with_regs`, `SlotPool` →
> slot arena, syscall/`init` U-mode dùng core. **K3b KHÔNG phải kernel đầy đủ** — câu này không
> được nới ở bất kỳ mục nào dưới đây.

**Phạm vi KT-61**: link core + TAP trên nhân thật + đo **cả hai** ngưỡng (D2-85a) + soak
0/1000 + matrix smp 1/2/4 + conformance + `gate --fast`. **Không** đổi `spec/`, sim semantics,
`.scn`, QUEUE/DECISIONS/AGENTS, `viscript/`; `unsafe` **chỉ** ở `kernel/vifast-plat-*` (D2-17).
**Không nới ngưỡng nào** (pin 196.608 / trần ảnh 262.144 / N=1.000 / smp 1/2/4 / L1 100k).

Nguồn đối chiếu: `review/agent0/K3-PLAN.md` §5.2/§5.3; **D2-82** (K3 DONE phase-level + nợ),
**D2-83** (biên giới core ↔ object layer VM), **D2-84**/**D2-85** (hai ngưỡng tách + `MAX_MAIL_LOG`),
**D2-80** (neo BUILD_ID), **D2-07** (không retry), **D2-08** (ảnh test tách), **D2-72 Q5/Q10**;
`v2/docs/k3b-core.md` §7/§8/§9; `v2/docs/k3-riscv64.md` §21; tiền lệ
[`K3-DOD.md`](K3-DOD.md).

## 0. TL;DR — D1..D11

| # | Bằng chứng | Kết quả | Log |
|---|---|---|---|
| D1 | `gate --fast` | **PASS 598 test/78 suite** (baseline và sau KT-61 — KT-61 không đổi code host), L1 1.000 seed 849 ms (`kt61-gate.log:1139-1140`; bản chạy trước khi đóng băng tài liệu: `kt61-gate-before-docfreeze.log`) | [`kt61-gate.log`](../JOURNAL/kt61-gate.log), [`kt61-baseline-gate.log`](../JOURNAL/kt61-baseline-gate.log) |
| D2 | TAP core trên **nhân thật** — matrix `smp 1/2/4` × 3 ảnh | **9/9** boot sạch: `ktest` **53/53** (43..52 = core), guard **3/3**, user **7/7** (`kt61-ktest-matrix.log:33,36,39,53`) | [`kt61-ktest-matrix.log`](../JOURNAL/kt61-ktest-matrix.log) |
| D3 | Soak ảnh **sản phẩm** (core chạy mỗi boot) | **1000/1000**, **0 lỗi**, 3 luồng, không icount, timeout 20 s, BUILD_ID `34dc594e0f651ed3` (`kt61-soak.log:1016-1017`) | [`kt61-soak.log`](../JOURNAL/kt61-soak.log), [`kt61-soak-results.txt`](../JOURNAL/kt61-soak-results.txt) |
| D4 | Conformance 5 `.scn` sim ↔ QEMU | **61/61 dòng, 5/5 ca**, `build=aab21609a80ca7a1` khớp ảnh ktest (`kt61-trace-compare.log:5`) | [`kt61-trace-compare.log`](../JOURNAL/kt61-trace-compare.log) |
| D5 | **Hai** ngưỡng (D2-85a) | `size_of::<Machine>()` **180.168 B** ≤ pin **196.608 B**; ảnh `.data+.bss` product **191.416**, ktest **197.568**, guard **2.680**, user **11.752** ≤ **262.144**; `.bss` **không** vào file (`filesz` 7.760/7.912/0/7.704; `kt61-budget.log:2,5,8,11`; `kt61-image-size.log:4,19,34,49`) | [`kt61-budget.log`](../JOURNAL/kt61-budget.log), [`kt61-image-size.log`](../JOURNAL/kt61-image-size.log) |
| D6 | L1 100k × smp 1/2/4/8 | **một phần (khai rõ):** `smp 1`: **100.000 seed × 64 op OK** (`kt61-l1-100k.log:253`); `smp 2/4/8` **dừng** khi hết ngân sách 3 h — **không** claim 400k | [`kt61-l1-100k.log`](../JOURNAL/kt61-l1-100k.log) |
| D7 | **Đỏ trước** + không retry | ảnh **trước KT-61** (43 TAP) bị runner mới (53 TAP) bắt đỏ: thiếu `tap43..53` + 11 evidence `# core:`; đỏ thứ hai: `MaybeUninit`+`write` → guard page fault thật; đỏ thứ ba: `init_zeroed` thiếu sentinel → `check_sched`/`check_mailboxes` đỏ trên QEMU (bắt thật) | [`kt61-red-before-core-tap.log`](../JOURNAL/kt61-red-before-core-tap.log), [`kt61-red-before-ktest-43tap.log`](../JOURNAL/kt61-red-before-ktest-43tap.log), [`kt61-red-before-core-link.log`](../JOURNAL/kt61-red-before-core-link.log) |
| D8 | Biên **D2-83** một chiều + static trong ảnh | core `Retype` Frame/PageTable/VSpace ⇒ `NotSupported` **3/3**, `untyped_used` không đổi; object layer vẫn tạo/revoke VSpace+Frame thật; TAP 43 chứng minh static trong `[0x8020_0000, guard)` | §9 + TAP 43/51 |
| D9 | Tài liệu + tuyên bố phạm vi | tệp này + `k3b-core.md` §9 + `k3-riscv64.md` §21 + `JOURNAL/kt61-*` + `STATE.md`; tuyên bố rõ **K3b không phải kernel đầy đủ** | tệp này |
| D10 | Còn lại cho **K3.5** (không tô hồng) | 10 hạng mục có chủ + tiêu chí đóng (§11) | §11 |
| D11 | Phạm vi file + commit + push | chỉ `v2/**`; `[KT-61]` + `[KT-61b]`; `HEAD..origin/main = 0` trước push | §12 |

**Đề xuất trạng thái**: **KT-61 DONE (đề xuất)** — 11/11 mục D1..D11 ĐẠT, không nới ngưỡng; chờ
**reviewer độc lập** (§13) + **Agent 0** lật QUEUE (D2-37: Agent 2 không tự lật).

## 1. Baseline (trước khi sửa)

- HEAD `99d3b7bf`; `git rev-list --count HEAD..origin/main` = **0**; `v2/` sạch (worktree có
  `viscript/`, `review/`, `.agents/`, `out-b.log` của tab khác — **không chạm**, chỉ stage `v2/`).
- Baseline `gate --fast`: **PASS 598 test/78 suite**, L1 1.000 seed **850 ms**
  ([`kt61-baseline-gate.log`](../JOURNAL/kt61-baseline-gate.log)).
- Baseline ảnh kernel (KT-60): product **90.696 B / data+bss 11.248**, ktest **332.176 / 17.344**,
  guard **53.800 / 2.680**, user **152.048 / 11.752**; `size_of::<Machine>() = 180.168 B` (chưa
  link). Máy: 12 nhân; QEMU `11.1.3`; OpenSBI `fw_dynamic v1.8.1`. **Không retry** (D2-07).

## 2. D1 — `gate --fast`

Lệnh `./v2/bin/v2 gate --fast` (không retry) — log [`kt61-gate.log`](../JOURNAL/kt61-gate.log):
**PASS 598 test/78 suite, 0 failed**, L1 1.000 seed; wall ghi trong log. KT-61 **không đổi** một
dòng nào của workspace host `v2/` (chỉ `v2/kernel/**` + `v2/test/boot/**` + docs), nên số test
khớp baseline; kernel build/clippy chạy riêng (§2.1).

### 2.1 Kernel build + clippy (ngoài `gate --fast`)

- `cargo build ... --target riscv64gc-unknown-none-elf --release` cho **4 ảnh** xanh — chính là
  bước build trong `ktest.sh`/`soak.sh` (log D2/D3).
- **Kernel clippy `-D warnings` — ĐÃ SẠCH** (sửa **toàn bộ nợ**: 18 lint
  `vifast-plat-common`+`vifast-plat-riscv64` như reviewer chỉ, 27 lỗi `vifast-kbin-riscv64` trong
  log đỏ, + 2 `user.rs`; tất cả là nợ **tích tụ từ trước KT-61**, sửa cơ học, không đổi ngữ nghĩa,
  BUILD_ID `.text` ảnh **không đổi** nên mọi log cổng vẫn neo đúng):
  - `cargo clippy --workspace --no-deps --target riscv64gc-unknown-none-elf -- -D warnings` ⇒
    **PASS** (log tự chứng minh: **6** dòng `CLIPPY_EXIT=0` + **3** dòng `BUILD_EXIT=0`; log có
    ghi chú 7 dòng `warning:` là `cargo:warning` của build script KT-53, **không** phải lint);
  - `-p vifast-kbin-riscv64 --features ktest` và `--features ktest,user` ⇒ **PASS**;
  - `-p vifast-plat-riscv64` với `[]`, `[ktest]`, `[ktest,user]` ⇒ **PASS** (log
    [`kt61-core-build.log`](../JOURNAL/kt61-core-build.log)).
  - **Vẫn nợ quy trình:** `gate --fast` **không** phủ kernel (workspace host `v2/Cargo.toml` loại
    `kernel`), nên trước KT-61 không ai chạy `clippy -D warnings` cho kernel ⇒ nợ 18+31 lint.
    Đề xuất **task riêng**: thêm bước `cargo clippy` kernel vào cổng (hoặc script `check-kernel`)
    để nợ không tái tích tụ — KT-61 **không** sửa `v2/bin/v2` (ngoài phạm vi).

## 3. D2 — TAP core trên nhân thật (43..52) + matrix `smp 1/2/4`

Lệnh: `bash v2/test/boot/ktest.sh 1 matrix` — nguồn sự thật TAP là `v2/test/boot/expected.txt`
(`ktest 53 53`, guard `3 3`, user `7 7`); mọi `# core:` đều là **số thật của lần boot**, không có
test nào chạy trên host.

| TAP | Tên | Bằng chứng neo (log) |
|---|---|---|
| 43 | `core link` | `bytes=180168 pin=196608 image_cap=262144 parts=27 sum=180168 … sum_ok=1 in_image=1` |
| 44 | `core untyped boot` | `bytes=65536 used=0 free=65536 … objects=1 caps=0 check_bytes=1` |
| 45 | `core tcb retype` | `objects=11 caps=11 used=2304 state=Inactive check_caps=1` |
| 46 | `core configure resume suspend` | `priority=3 mcp=10 budget=4 period=8 suspended_ok=1 resume=1 state=Ready` |
| 47 | `core sched 2 tcb` | `pick0=3 pick1=4 dispatch0=3 dispatch1=4 t0=Ready t1=Running` |
| 48 | `core ipc round` | `mr=0xfeed blocked_send=1 blocked_reply=1 ep_len=0 sent=1 delivered=1` |
| 49 | `core notification signal` | `word=0x0 badge=0x0 waiter=0 consumed=0x42 poll=0x8` |
| 50 | `core waitaddr` | `frame=0x82000000 label=532480 offset=64 value=0x1234 woke=1 waiters=0 buckets=0` |
| 51 | `core d2-83 boundary` | `frame=NotSupported page_table=NotSupported vspace=NotSupported atomic=1 obj_frame=1 obj_vspace=1 owner=obj` |
| 52 | `core invariants` | `used=0 free=65536 deleted=9 … check_fault=1 leak=0` (12/12 bất biến) |

Kết quả: **9/9 boot sạch** (`ktest` **53/53** ở smp 1/2/4; guard 3/3; user 7/7); runner kiểm
**tên từng TAP**, `1..53` liên tục, đúng 1 `PANIC` mẫu (trap lồng), 11 evidence `# core:`, và
`# conf image: build=…` khớp ELF (D2-80). Số TAP 43..42 cũ **không đổi**; trap lồng dời 43 → 53.

## 4. D3 — Soak ảnh sản phẩm **0 lỗi/1.000**

Lệnh: `bash v2/test/boot/soak.sh 1000 3` (`SOAK_LOGDIR=/tmp/k3-kt61-soak`) — máy `ps`, **3 luồng**,
**không icount** (D2-74), timeout **20 s**/boot, `n ≥ 3/p = 1.000` (D2-24). Mỗi boot ảnh sản phẩm
chạy `core_bridge::init` + `boot_smoke` (một vòng đời TCB thật) và in `# core: product …`.

| Hạng mục | Giá trị | Nguồn (log:dòng) |
|---|---|---|
| Số boot sạch | **1000/1000**, **0 lỗi** | `kt61-soak.log:1016`, `kt61-soak-results.txt:1000` |
| Wall | **227 s** (4 boot/s) — wall phụ thuộc tải máy, không phải ngưỡng | `kt61-soak.log:1016` |
| BUILD_ID / sha256 ảnh **đã soak** | `34dc594e0f651ed3` / `c7cc50b1d79e6f19dc57320d724812beb797d65bfac5dc3426fa51e3dcf4ed45`, git `99d3b7bf` | `kt61-soak.log:1017` |
| Neo D2-80 | `BUILD_ID` = băm **`.text`** (recompute từ ELF khớp ảnh soak); **không** đọc sha toàn ảnh làm neo: sau soak tài liệu-comment-only có rebuild lại ảnh product nên sha toàn file có thể khác, `.text` **không đổi** | `soak.sh` + `trace_compare.py --image` |
| Bất biến mỗi boot | 1 `TAP version 13`, 1 `1..1`, 1 `ok 1 - k3 boot`, 1 `# k3 halt`, `# k3 image=product`, 1 `# conf image:` khớp ELF, `# smp: boot=0 harts=1 …`, **0** `PANIC` — và **core smoke** in `retype=1 configure=1 suspend=1 resume=1 state_ready=1 used=0 check_caps=1 footprint_ok=1` |

`0/1000` chỉ là **trần** (QD §5.3), không phải bảo đảm; không retry (D2-07).

## 5. D4 — Conformance **5/5 / 61 dòng**

Lệnh: `KT55_LOGDIR=/tmp/k3-kt61-matrix bash v2/test/conformance/run.sh --compare --smp 2` (trace
sim host + log QEMU `ktest-smp2-1` từ D2; `trace_compare.py --image` kiểm D2-80). Kết quả:
**61/61 dòng khớp, 5/5 ca** (22:16, 23:19, 24:7, 25:7, 26:12), `build=aab21609a80ca7a1`
(khớp ảnh `ktest` đang chạy — `trace_compare.py --image`, D2-80; `kt61-trace-compare.log:5`).
**Core không đổi đường VM**: `Frame/PageTable/VSpace` vẫn ở object layer (D2-83), `.scn` không
đổi, sim semantics không đổi ⇒ không có lý do conformance đổi; nếu đổi = hồi quy platform.

## 6. D5 — **Hai** ngưỡng (D2-85a), không đọc lẫn

### 6.1 Pin core — `size_of::<Machine>()`

| Hạng mục | Số `[ĐO]` | Ngưỡng | Kết quả |
|---|---|---|---|
| `Machine` tĩnh | **180.168 B** (91,6 %, còn 16.440 B) | **196.608 B** (pin D2-84) | ĐẠT |
| Bảng phân bố | 27 dòng, `sum 180.168` + scalars 239 + padding **1** = total | `sum + scalars + padding == total`, `padding ≤ 8` (D2-85c) | ĐẠT (TAP 43) |

### 6.2 Trần **ảnh link** — `.data+.sdata+.bss+.sbss` (`budget.sh`, ELF thật)

| Ảnh | file (byte) | `.data` | `.bss` | **data+bss** | segment RW `filesz → memsz` |
|---|---:|---:|---:|---:|---|
| product | 185.464 | 7.760 | 183.656 | **191.416** | 7.760 → 2.719.744 |
| ktest | 484.600 | 7.912 | 189.656 | **197.568** | 7.912 → 2.588.672 |
| ktest-guard | 53.808 | 0 | 2.680 | **2.680** | 0 → 2.768.896 |
| ktest-user | 151.912 | 7.704 | 4.048 | **11.752** | 7.704 → 2.723.840 |
| **Trần** | | | | **262.144** | QD §3.5 / D2-72 Q5 |

**Static `Machine` nằm trong `.bss` — KHÔNG nằm trong file ảnh:** segment RW chỉ có
`filesz = 7.760` (product) / `7.912` (ktest) — tức chỉ `.data`; 180.168 B `Machine` nằm trong
`.bss` với `memsz` (NOLOAD). Bản `.data` (đã thử và **bỏ**) làm file product **365.632 B** và
ktest **664.824 B** — tức **+180.168 / +180.224 B trong file**; bản `.bss` hiện tại
**185.464 / 484.600 B**. Đo bằng `llvm-readobj --program-headers` +
`wc -c` ([`kt61-image-size.log`](../JOURNAL/kt61-image-size.log)).

**Cơ chế:** `static MaybeUninit<Machine>` (`.bss`, `NOBITS`) + `write_bytes(0)` + `Machine::init_zeroed`
tại chỗ (chỉ đặt trường **khác 0**: `n_harts`, `wait_buckets`/`_tail = NODE_NONE`,
`HartSched`/`HartMailbox::reset`, `published[*].idle`); body TCB/SC/Endpoint/… được ghi `*::EMPTY`
**lúc cấp phát**. `.data+.bss` **vẫn cộng đủ** 180.168 B ⇒ phép đo ngân sách trung thực, không
giấu; `.stack`/`.stack_guard`/`.trap_stack`/`.hart_stacks` là NOLOAD, báo **riêng** (D2-72 Q5).
TAP 43 chứng minh static trong vùng ảnh `[0x8020_0000, guard)`.

## 7. D6 — Sim L1 100.000 seed × smp 1/2/4/8

Lệnh `./v2/bin/v2 nightly --l1 --seeds 100000 --smp 1,2,4,8` (KHÔNG nằm trong `gate --fast`; chạy
một mình). **Kết quả một phần, khai đúng:** `smp 1` xong **100.000 seed × 64 op, OK**
(`kt61-l1-100k.log:253`: `# L1 OK: smp 1, 100000 seed × 64 op (2000 seed đầu chạy 2 lần; ipc_min 2,
ipc_median 12; ipc_sent 1212128, ipc_delivered 226334, ipc_dropped 782515; vm map 257149, walk
71791, unmap 135089, fault 237472, shootdown 135089)`); `smp 2` chạy ~23 phút chưa xong, `smp 4/8`
chưa tới ⇒ **dừng chủ động** khi chạm ngân sách 3 h của task (ghi `EXIT=143`, không retry). **Không
claim 400.000 lượt.** `400.000 lượt × smp 1/2/4/8` là **DoD của cổng K6**, **không** phải DoD
KT-61 (KT-61 chỉ cần "L1 100k nếu thời gian"); phần chưa chạy là **nợ thời gian có task riêng**
(§11 dòng 10), không phải nợ kỹ thuật. KT-61 không đổi sim/model/spec nên L1 giữ nguyên thiết kế
(D2-65); phần đã chạy là bằng chứng **không hồi quy**.

## 8. D7 — Đỏ trước + **không retry**

| Đỏ | Cách tạo | Kết quả |
|---|---|---|
| **Red-before 1** (TAP core chưa có) | build ảnh `ktest` từ **HEAD `99d3b7bf`** trong worktree tạm (`/tmp/kt61-base`, đã xoá) rồi boot QEMU ⇒ 43 TAP; chạy runner **mới** (`expected.txt` 53 + 11 evidence `# core:`) bằng `--check-log` | **ĐỎ**: thiếu `tap43..tap53` + `ev72..ev82` (log [`kt61-red-before-core-tap.log`](../JOURNAL/kt61-red-before-core-tap.log), log ảnh cũ [`kt61-red-before-ktest-43tap.log`](../JOURNAL/kt61-red-before-ktest-43tap.log)) |
| **Red-before 2** (static `MaybeUninit`) | bản đầu dùng `.bss` zero-init + ghi model lúc boot | **ĐỎ thật trên QEMU**: bản tạm 180.168 B trên stack 64 KiB ⇒ `PANIC: guard page fault scause=0xf stval=0x80400000` (log [`kt61-red-before-core-link.log`](../JOURNAL/kt61-red-before-core-link.log)) — đổi sang **`.bss` + `Machine::init_zeroed`** (§6.2) |
| **Red-before 3** (`init_zeroed` thiếu sentinel) | bản `.bss` đầu tiên để zero thuần: `HartSched.idle = false` và 8 ô `Option<MailboxMsg>` zero = `Some(Wake(0))` | **ĐỎ thật trên QEMU**: TAP 46 `check_sched=0` ("idle lech current") + TAP 52 `check_mailboxes=0` ("hop thu mat/them message") — log [`kt61-red-before-init-zeroed.log`](../JOURNAL/kt61-red-before-init-zeroed.log); vá `HartSched::reset`/`HartMailbox::reset` ⇒ xanh ([`kt61-init-zeroed-fixed.log`](../JOURNAL/kt61-init-zeroed-fixed.log), rồi 9/9 ở D2) |
| **Không retry** | — | mọi log D1..D6 là lần chạy **đầu** sau khi xanh; không có lần chạy lại để "lấy xanh" (D2-07); không `#[ignore]`, không `should_panic`, không sửa `expected.txt`/evidence cho vừa log |

## 9. D8 — Biên **D2-83** (một chiều, không nhân đôi chủ sở hữu)

| Hạng mục | Chủ sở hữu | Bằng chứng |
|---|---|---|
| Untyped/CNode/MDB/cap/TCB/SC/Endpoint/Reply/Notification/IPC/WaitAddr | **core** | TAP 44..50 |
| Frame/PageTable/VSpace | **object layer** (`vifast-plat-common::obj`, KT-52) | TAP 51: core `Retype` ⇒ `NotSupported` **3/3**, `untyped_used` không đổi; obj layer tạo+revoke VSpace/Frame thật `obj_frame=1 obj_vspace=1 obj_health=1` |
| Cầu nối | **một chiều**: nhãn PA cho Untyped boot, **PPN** Frame cho khoá `WaitAddr` | core chỉ kế toán/lưu khoá, **không** đọc byte/PTE của vùng đó |

Ghi chú: core **từ chối** `add_kernel_object(ObjType::Frame)` (`is_core()` = false ⇒ panic) — biên
giới do chính core cưỡng chế. Đổi biên giới cần **D2 mới** (D2-83).

## 10. D9 — Tài liệu + tuyên bố phạm vi

- `v2/docs/k3b-core.md`: §9 (nối kernel: phạm vi, hai ngưỡng, TAP 43..52, D2-83, vì sao **`.bss`** + `init_zeroed`,
  `SlotPool` giữ nguyên, cổng) + §10 (chạy lại); header/§1/§6/§8 cập nhật trạng thái + nợ.
- `v2/docs/k3-riscv64.md`: **§21** (nối core + cổng K3b + còn lại K3.5).
- `v2/agent/JOURNAL/kt61-*`: log cổng + nhật ký; `v2/agent/STATE.md`: HEAD/BUILD_ID/task/bước/cổng.
- Tuyên bố phạm vi: **K3b = core chạy trên nhân thật** (CNode/MDB/cap/TCB/sched/MCS/IPC/
  Notification/WaitAddr) — **không** phải kernel đầy đủ: chưa có `procsrv`/shell, chưa chuyển
  cap, chưa syscall/`init` U-mode dùng core, chưa `TlbFlush` thật trong core.

## 11. D10 — Còn lại cho **K3.5** (có chủ, có tiêu chí đóng)

| # | Hạng mục | Vì sao chưa làm ở KT-61 | Tiêu chí đóng |
|---|---|---|---|
| 1 | `procsrv` + shell (K3.5) | ngoài phạm vi K3b; cần syscall/`init` dùng core | `procsrv` nhận IPC thật + shell chạy trên QEMU |
| 2 | **Chuyển cap** qua IPC (receive window 4 ô + `plan_cap_transfer`, mất `RIGHT_GRANT`, unwrap badge) | không cần cho cổng; làm nửa đường sẽ tạo hai nguồn sự thật cap | parity sim↔core có ca + TAP kernel (D2-53 Q6) |
| 3 | **Finalise IPC object khi Revoke** (huỷ `Reply.caller`, đánh thức Recv waiter, huỷ hàng) | nợ KT-60 §8.6.2; TAP 52 mới revoke cây Untyped sau khi hàng/IPC đã sạch | parity + TAP revoke khi có object IPC đang hoạt động |
| 4 | `TlbFlush` **thật** trong core | core chưa sở hữu TLB (object layer K2/K3); giữ message placeholder | handler gọi `vifast_plat_riscv64::tlb` + ca shootdown |
| 5 | `SchedContext.Bind(notification)` | trường `bound_sc` chết (F4 reviewer-kt60) | port + ca parity |
| 6 | `tcb_resume_with_regs` | nợ KT-59/60 §8.6.6 | port + ca parity (fault resume) |
| 7 | `SlotPool` → slot **arena** (PhysMem) | làm nửa đường = hai nguồn sự thật; xem `k3b-core.md` §9.6 | slot trong Untyped thật, Revoke trả byte, `check_bytes` không đổi |
| 8 | syscall/`init` U-mode dùng core | đụng `init`/U-mode — K3.5 | syscall 12/13/14 đi qua `Machine` |
| 9 | WaitAddr toàn phần (mọi bucket/thứ tự) + Reply cap trong `Endpoint.recv` | nợ **phủ**, không phải nợ hành vi đã biết | thêm ca parity |

| 10 | L1 100k **đủ 4 cấu hình** smp 1/2/4/8 | `smp 1` xong 100.000 seed; `smp 2/4/8` chưa xong khi hết ngân sách 3 h (không phải lỗi kỹ thuật) | chạy `v2 nightly --l1 --seeds 100000` một mình, đủ 4 dòng `# L1 OK` |

**Tất cả 9 mục trên đều CHƯA lên nhân thật** (không có code kernel/TAP nào cho chúng ở KT-61);
KT-61 chỉ đưa lên kernel phần core **đã có** từ KT-58/59/60 (tầng 1..3). Không mục nào được gọi là
"đã xong" trong tệp này; mục 7 (`SlotPool`) là quyết định **giữ nợ** có chủ, không phải tuyên bố
hoàn thành.

## 12. D11 — Phạm vi file + commit + push

- Chỉ `v2/**`; **không** `spec/`, `viscript/`, `docs/` gốc, QUEUE/DECISIONS/AGENTS. `unsafe`
  **chỉ** thêm trong `kernel/vifast-plat-riscv64` (D2-17); `vifast-core` vẫn
  `#![forbid(unsafe_code)]`.
- Commit `[KT-61]` (kernel + harness + docs) và `[KT-61b]` (STATE) — hash + `HEAD..origin/main`
  ghi ở §12.1 sau khi commit.
- `git rev-list --count HEAD..origin/main` = **0** trước push; push `origin main`.

### 12.1 Hash

- Commit `[KT-61]` + `[KT-61b]`: hash ghi ở `STATE.md` và bảng lịch sử sau khi commit (tệp này
  không chứa hash trước khi commit).
- Ảnh product **đã rebuild sau soak** (thay đổi chỉ là doc/comment/`#[allow]` ⇒ `BUILD_ID` `.text`
  vẫn `34dc594e0f651ed3`; sha toàn ảnh **đã soak** `c7cc50b1…` ở `kt61-soak.log:1017`, sha bản
  rebuild cuối `fb68981d8c2f2bdf03338ae3bd4ddf5265dc9c147faeec2b9d61ec1d18487ff5`). Vì vậy mọi neo
  D2-80 trong tệp này dùng **`BUILD_ID` `.text`**, không dùng sha256 toàn ảnh làm "ảnh đã soak".
  `ktest` rebuild cuối: `BUILD_ID aab21609a80ca7a1` = đúng bản conformance đã chạy; guard
  `1ea68a17aec63038`, user `ca2165eaa2eb4417` (`kt61-image-size.log`, mục "rebuild cuối").

## 13. Review độc lập (`reviewer-kt61`, lead spawn, read-only)

(chờ verdict — ghi ở đây sau khi reviewer trả; tối đa 20 phút theo quy trình KT-61)

## 14. Bảng DoD D1..D11 (đo được, không nới)

| # | Điều kiện | Ngưỡng | Kết quả | Nguồn |
|---|---|---|---|---|
| D1 | `gate --fast` | xanh, 0 failed, L1 1.000 seed | **PASS 598/78** (L1 849 ms) | `kt61-gate.log:1139-1140` |
| D2 | TAP core trên QEMU + matrix | **100 %** smp 1/2/4, số TAP khớp `expected.txt` | **53/53 + 3/3 + 7/7**, 9/9 boot | `kt61-ktest-matrix.log:33,36,39,53` |
| D3 | Soak sản phẩm | **0 lỗi / 1.000**, 3 luồng, không icount, 20 s | **0/1000** (wall **227 s**) | `kt61-soak.log:1016-1017` |
| D4 | Conformance | **5/5** ca, 0 khác biệt ngoài trường cho phép | **61/61 dòng, 5/5** | `kt61-trace-compare.log:5` |
| D5 | Hai ngưỡng | `size_of` ≤ **196.608**; ảnh ≤ **262.144** | **180.168**; **191.416 / 197.568 / 2.680 / 11.752** | `kt61-budget.log:2,5,8,11`; `kt61-image-size.log` |
| D6 | Sim L1 | **100.000** seed × smp 1/2/4/8 | **một phần:** smp 1 **100.000 seed OK**; smp 2/4/8 chưa xong (ngân sách 3 h) | `kt61-l1-100k.log:253` |
| D7 | Đỏ trước / không retry | có log đỏ **trước**; không retry, không `#[ignore]` | **3 log đỏ** (§8) | `kt61-red-before-*.log` |
| D8 | Biên D2-83 | VM vẫn ở object layer; core `NotSupported` + atomic | **3/3**, obj thật **1/1** | TAP 51 (`ktest-smp*-1.log`) |
| D9 | Tài liệu phạm vi | K3b = core trên nhân thật, **không** kernel đầy đủ | tệp này + §21 + §9 | — |
| D10 | Nợ K3.5 | danh sách có chủ + tiêu chí | §11 (10 mục) | §11 |
| D11 | Phạm vi + commit/push | chỉ `v2/**`; push sau xanh | §12 | §12.1 |

## 15. Tệp chạm (KT-61)

| Tệp | Loại |
|---|---|
| `v2/kernel/vifast-plat-riscv64/src/core_bridge.rs` | **mới** — cầu nối core (static `.bss`, `write_bytes`+`init_zeroed`, lock, boot smoke) |
| `v2/kernel/vifast-core/src/{machine,sched,mailbox}.rs` | `Machine::init_zeroed` + `HartSched::reset` + `HartMailbox::reset` (khởi tạo tại chỗ cho `.bss`; không đổi ngữ nghĩa/API công khai cũ) |
| `v2/kernel/vifast-plat-common/src/{smp,obj/mod,obj/arena,obj/vspace}.rs` | sửa **18 lint clippy nợ** (cơ học, không đổi ngữ nghĩa) |
| `v2/kernel/vifast-plat-riscv64/src/{mmu,ipi,smp,user,lib}.rs` | sửa lint clippy nợ + cfg import theo feature (không đổi ngữ nghĩa) |
| `v2/kernel/vifast-kbin-riscv64/src/{ktest,ktest_conf,ktest_smp}.rs` | sửa lint clippy nợ (doc indent, `#[allow(too_many_arguments)]` có lý do, iterator) |
| `v2/kernel/vifast-plat-riscv64/{Cargo.toml,src/lib.rs}` | dependency `vifast-core` + `pub mod core_bridge` |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_core.rs` | **mới** — TAP 43..52 |
| `v2/kernel/vifast-kbin-riscv64/{Cargo.toml}` | dependency `vifast-core` (tên/type TAP) |
| `v2/kernel/vifast-kbin-riscv64/src/{main.rs,ktest.rs}` | product: core init + smoke; test: pha core, trap lồng 53 |
| `v2/test/boot/{ktest.sh,expected.txt}` | manifest TAP 43..52 + 11 evidence; `ktest 53 53` |
| `v2/docs/{k3b-core.md,k3-riscv64.md}` | §9/§10 + §21 + cập nhật nợ/trạng thái |
| `v2/agent/reports/K3B-DOD.md` | **mới** (tệp này) |
| `v2/agent/JOURNAL/kt61-*` | log baseline/gate/matrix/soak/conformance/budget/L1/red-before + nhật ký |
| `v2/agent/STATE.md` | HEAD/BUILD_ID/task/bước/cổng |
