<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K35-DOD — bằng chứng cổng cuối K3.5 (KT-66): procsrv + shell thật, ảnh demo tách, stack per-hart

Người ghi: **Agent 2 dòng OS** · Ngày: 2026-11-16 · Task: **KT-66** (cổng K3.5) · HEAD khi mở
task: `3f02d72d` (`[KT-00]` lật KT-65 DONE, mở KT-66) · Nhánh `main` · Máy: `ps` (12 nhân,
QEMU `11.1.3`, OpenSBI `v1.8.1` — version do script đọc và in, xem log).

> **K3.5 xong cái gì:** kernel chạy **chương trình U-mode thật có vòng đời** — `procsrv` (Rust,
> nhận fault chuyển tiếp `Exit{code}` qua endpoint), **shell** tối thiểu trên UART (message
> `LINE`), **con** U-mode, `wait`/`kill` là giao thức procsrv, `ps` đọc từ `Machine` của core;
> **demo chạy trên QEMU** (TAP 1..10 của ảnh `ktest-proc`, ảnh **tách** khỏi ảnh syscall), cổng
> K3.5 đo được: TAP 100 %, soak 0/1000, L1 400k, conformance 5/5, budget 2 ngưỡng, stack syscall
> **per-hart + canary**. **Còn gì (không tô hồng, §12):** fault bridge ABI cho K3.6 (fault đi qua
> "message chuyển tiếp" của arch, chưa phải trường fault trong `Recv`), `kill` chưa `Revoke cap
> TCB` (chỉ `Suspend`), `kill <pid sai>` chưa bị từ chối, `vifast-plat-user` **không** là thành
> viên workspace, shell **chưa** đọc UART IRQ (dòng do kernel bơm), core **chưa** nối IPI
> multi-hart, `TlbFlush` `targets/acked` **chưa quan sát được** từ user, slot 192 word/Region.
> **K3.5 KHÔNG phải kernel đầy đủ** — câu này không được nới ở bất kỳ mục nào dưới đây.

**Phạm vi KT-66**: tách ảnh demo (`vifast-kbin-ktest-proc`), stack syscall **per-hart** (thay
`SYSCALL_STACK` tĩnh) + canary + nước thấp, matrix **4 ảnh × smp 1/2/4**, soak **0/1000**, L1
**100k × smp 1/2/4/8 = 400.000 lượt**, conformance **5/5**, budget **2 ngưỡng** cho **mọi** ảnh
(gồm ảnh demo), clippy kernel sạch, `v2 check` + `gate --fast`, tệp này + `v2/docs/k35-procsrv.md`
§9/§10 + `JOURNAL/kt66-*` + `STATE.md`. **Không**: K6 (memsrv/IDN/virtio), sửa `spec/`,
QUEUE/DECISIONS/AGENTS/`viscript/`; **không nới ngưỡng nào** (pin `size_of::<Machine>()` ≤
196.608 / trần ảnh 262.144 / N=1.000 / smp 1/2/4 / L1 100k / 32 KiB stack / 20 s/boot).

Nguồn đối chiếu: **D2-82** (K3 DONE + nợ), **D2-83** (biên giới core ↔ object layer VM),
**D2-84/D2-85** (hai ngưỡng tách + `MAX_MAIL_LOG`), **D2-86** (K3b DONE + 10 nợ), **D2-87**
(roadmap K3.5), **D2-89/D2-90** (finalise IPC anti-hang), **D2-91** (kỷ luật commit đa agent),
**D2-92** (bốn chốt KT-64), **D2-93** (`vifast-plat-user` trong danh sách đóng D2-17);
`v2/docs/k35-procsrv.md` §8 (nợ) + §9/§10 (KT-66); `v2/docs/k3b-core.md` §8; tiền lệ
[`K3B-DOD.md`](K3B-DOD.md), [`K3-DOD.md`](K3-DOD.md).

## 0. TL;DR — D1..D12

| # | Bằng chứng | Kết quả | Nguồn `log:dòng` |
|---|---|---|---|
| D1 | `gate --fast` (+ `v2 check`) | **PASS 649 test/84 suite**, 0 failed; L1 1.000 seed **911 ms**; `v2 check` **PASS** (fmt + luật cấu trúc + clippy host + gen-check) | `kt66-gate.log:66,1226,1227`; `kt66-check.log` |
| D2 | Matrix TAP **4 ảnh × smp 1/2/4** | **12/12 boot sạch**: `ktest` **60/60** ×3, guard **3/3** ×3, user **14/14** ×3, proc **10/10** ×3 | `kt66-ktest-matrix.log:27-38,67` |
| D3 | Soak ảnh **sản phẩm** | **1000/1000, 0 lỗi**, wall **225 s**, 3 luồng, **không icount**, timeout 20 s/boot; BUILD_ID `ce9a8c7cff7c68a3` + sha256 `eb365a13…` **khớp** ảnh product đang có | `kt66-soak.log:1016,1017,1019,1020` |
| D4 | Sim L1 **100k × smp 1/2/4/8** | **400.000 lượt, 0 vi phạm**: `# L1 OK: smp 1/2/4/8, 100000 seed × 64 op` ×4, cả 4 `EXIT=0`; **4 tiến trình song song** (mỗi cấu hình một tiến trình — **không** tuần tự), wall **33m30s** (23:56:49Z → 00:30:19Z, UTC) | `kt66-l1-smp1.log:291`; `smp2:291`; `smp4:290`; `smp8:289` |
| D5 | Conformance 5 `.scn` sim ↔ QEMU | **61/61 dòng, 5/5 ca** (22:16, 23:19, 24:7, 25:7, 26:12), `build=9cc6c8f295cab617` **khớp ảnh ktest đang chạy** (D2-80) | `kt66-trace-compare.log:5-6` |
| D6 | **Hai** ngưỡng (D2-85a) cho **mọi** ảnh | `size_of::<Machine>()` **188.872 B** ≤ pin **196.608**; data+bss: product **200.120**, ktest **209.856**, guard **2.680**, user **202.280**, proc **206.472** ≤ **262.144**; NOLOAD per-hart **950.272 B** báo **riêng** (D2-72 Q5) | `kt66-budget.log:1,4,7,10,13`; `kt66-image-size.log:3-7` |
| D7 | **Ảnh demo tách** + headroom | `ktest-user` **239.216 → 202.280 B** (**−36.936 B**); ảnh mới `ktest-proc` **206.472 B**; 10 ca cũ 15..24 thành TAP **1..10**; matrix **9 → 12** boot | `kt65-budget.log:10`; `kt66-budget.log:10,13` |
| D8 | **Stack syscall per-hart + canary** | smp 1/2/4: `harts=1/2/4 distinct=1 canary=0`; `base` mỗi hart **khác nhau** (`0x80415000/0x80432000/0x8044f000/0x8046c000`); hart 0 dùng **3.720 B** (ảnh user), **22.088 B** (ảnh demo — chứng minh 16 KiB **không đủ**); không còn 32 KiB trong `.data+.bss` | `JOURNAL/kt66/ktest-user-smp4-1.log:129-133`; `JOURNAL/kt66/ktest-proc-smp1-1.log:122-123` |
| D9 | **Đỏ trước** + không retry | 3 log đỏ: ảnh tách (24 vs 14 TAP), stack dùng chung (`distinct=0`), canary bị ghi đè (`canary=1`) | `kt66-red-before-split.log`, `kt66-red-before-stack.log`, `kt66-red-before-canary.log`; QEMU: `kt66-red-before-shared-stack-qemu.log` |
| D10 | Clippy kernel `-D warnings` + `v2 check` | workspace + `[ktest]` + `[ktest,user]` + plat `[]`/`[ktest,user]` **PASS**; `v2 check` **PASS** | `kt66-core-build.log`; `kt66-gate.log:1-66` |
| D11 | Tuyên bố **phạm vi** (xong gì / còn gì) | §12: 8 nợ có chủ + tiêu chí; **không** claim kernel đầy đủ | §12 |
| D12 | Phạm vi file + commit + push | chỉ `v2/**`; `[KT-66]` + `[KT-66b]`; `HEAD..origin/main = 0` trước push | §13 |

**Đề xuất trạng thái**: **KT-66 DONE (đề xuất)** và **K3.5 DONE (đề xuất)** — D1..D12 ĐẠT, không
nới ngưỡng; chờ **reviewer độc lập** (§14) + **Agent 0** lật QUEUE (D2-37: Agent 2 không tự lật).

## 1. Baseline (trước khi sửa)

- HEAD `3f02d72d`; `git rev-list --count HEAD..origin/main` = **0**; `v2/` sạch (worktree có
  `viscript/`, `review/`, `.agents/`, `out-b.log`, `v2/agent/reports/kt64/` của tab khác —
  **không chạm**; D2-91: **không** `git add`).
- Baseline `gate --fast`: **PASS** (log [`kt66-baseline-gate.log`](../JOURNAL/kt66-baseline-gate.log)
  — `v2 check: PASS` dòng 66, `L1: OK (891 ms)` dòng 1224, `v2 gate --fast: PASS` dòng cuối).
- Baseline boot 3 ảnh ở `smp=1` (KT-65): `ktest` **60/60**, guard **3/3**, `ktest-user` **24/24**
  — log 24 TAP nhúng trong [`kt66-red-before-split.log`](../JOURNAL/kt66-red-before-split.log)
  (chính log đó bị `expected.txt` mới 14 TAP bắt đỏ).
- Baseline budget KT-65: product **200.120**, ktest **209.856**, guard **2.680**, user
  **239.216** ([`kt65-budget.log`](../JOURNAL/kt65-budget.log):1,4,7,10); `size_of::<Machine>()`
  **188.872** (TAP `# core: link` — không đổi ở KT-66).
- Máy: 12 nhân; QEMU `11.1.3` (sàn 9.2, D2-72 Q2); OpenSBI `fw_dynamic v1.8.1`. **Không retry**
  (D2-07); flake không có.

## 2. D1 — `gate --fast` + `v2 check`

- Lệnh: `./v2/bin/v2 check` rồi `./v2/bin/v2 gate --fast` (không retry) — log
  [`kt66-check.log`](../JOURNAL/kt66-check.log), [`kt66-gate.log`](../JOURNAL/kt66-gate.log).
- `v2 check`: **PASS** — `fmt: OK` (`:3`), `luật cấu trúc: OK` (`:8`, R1 ≤ 1.500 dòng/tệp: sau
  KT-66 `user.rs` **1.406 dòng** + `user_stack.rs` **217 dòng**, tách theo R1), `clippy: OK`,
  `scn-check: PASS (27 tệp, 0 blocked, 16 nhóm)` (`:27`), `gen-check: PASS` (`:53`),
  `report-check: PASS` (`:31`), `abi-check: PASS` (`:21`).
- `gate --fast`: **PASS 649 test/84 suite**, 0 failed; L1 1.000 seed **911 ms** (`:1226` —
  ngưỡng QD §5.2 ≤ 60 s chỉ áp cho L1, giữ nguyên); `v2 gate --fast: PASS` (`:1227`).
- Baseline trước KT-66 cũng **PASS** ([`kt66-baseline-gate.log`](../JOURNAL/kt66-baseline-gate.log):
  `v2 check: PASS` `:66`, `L1: OK (891 ms)` `:1224`) ⇒ không hồi quy.
- `git diff --stat -- spec/` **rỗng**; `gen-check` xác nhận code/docs sinh từ `spec/` khớp.

## 3. D2 — Matrix TAP `smp 1/2/4` × **4 ảnh** (12 boot)

Lệnh: `KT54_LOGDIR=/tmp/k3-kt66-matrix3 bash v2/test/boot/ktest.sh 1 matrix` — nguồn sự thật TAP là
`v2/test/boot/expected.txt` (`ktest 60 60`, `ktest-guard 3 3`, `ktest-user 14 14`,
`ktest-proc 10 10`); runner kiểm **mọi slot** (`want_rows = NBOOT × smp × LABEL_COUNT`), dãy
`ok 1..N` liên tục, đúng **một** PANIC theo mẫu từng ảnh, mọi evidence bắt buộc, và **stack
per-hart** (`check_stack_lines`). Log: [`kt66-ktest-matrix.log`](../JOURNAL/kt66-ktest-matrix.log),
12 log boot thô ở [`JOURNAL/kt66/`](../JOURNAL/kt66/).

| Ảnh | `smp 1` | `smp 2` | `smp 4` | TAP / evidence |
|---|---|---|---|---|
| `ktest` | 60/60 | 60/60 | 60/60 | 60 TAP + **91** evidence (`ktest-smp{1,2,4}-1.log`) |
| `ktest-guard` | 3/3 | 3/3 | 3/3 | 3 TAP + **5** evidence (guard page fault thật) |
| `ktest-user` | 14/14 | 14/14 | 14/14 | **30** evidence (gồm `# stack:` + `# ktest-user:`) |
| `ktest-proc` | 10/10 | 10/10 | 10/10 | **28** evidence (gồm `# stack:` + init/core/exitkind) |
| **Tổng** | **4/4** | **4/4** | **4/4** | **12/12 boot sạch** (`kt66-ktest-matrix.log:85`) |

- Dòng bảng tổng kết theo `(ảnh, smp)` ở `kt66-ktest-matrix.log:54-65`; dòng `ok` từng boot ở
  `:27-30` (smp 1), `:31-34` (smp 2), `:35-38` (smp 4); tổng kết `:67`.
- Matrix **trước KT-66** là 3 ảnh × 3 = 9 boot; nay **4 ảnh × 3 = 12** — ảnh thứ tư là demo
  (D7). Runner **không** bỏ slot nào (`want_rows` tính từ `LABEL_COUNT`).
- `ktest-user` không còn fault: kết thúc bằng đúng một dòng
  `PANIC: KT-66 u-mode pha syscall xong (14 TAP, 0 fail)`; `ktest-proc` kết thúc bằng
  `PANIC: u-mode fault scause=0xc …` (chính sách fatal K3) — mẫu PANIC tách theo ảnh trong
  `panic_allow_re` (`ktest-user` **khác** `ktest-proc`).
- `# stack:` của mỗi boot là **số thật của lần chạy** (D8), không hardcode.
- **Harness — vá false-green (reviewer KT-66 M3, lần thứ 3 trong dự án):** bản đầu của
  `check_stack_lines` dùng `strtonum()` (chỉ gawk) ⇒ **BSD awk** báo `calling undefined function
  strtonum`, stdout rỗng ⇒ `bad=""` ⇒ **PASS giả**; `low ∈ [base,top]` và `block ≥ 32 KiB`
  **chưa từng** được kiểm. Đã sửa: parse bằng `sed` + tính bằng **bash `16#…`** (portable), và
  **fail-closed** — không parse đủ `smp` dòng / hex hỏng / thiếu dòng ⇒ thêm `MISSING` (ĐỎ).
  Ca tự kiểm mới: `bash v2/test/boot/ktest.sh --selftest-stack` **5/5** (log đúng xanh; `low`
  ngoài, `block` nhỏ, thiếu dòng, hex hỏng đều **ĐỎ**), được gọi trong `selftest.sh`
  (**18/18** ca). Log matrix sau vá **0** dòng lỗi awk (`kt66-ktest-matrix.log`).

## 4. D3 — Soak ảnh sản phẩm **0 lỗi/N = 1.000**

Lệnh: `SOAK_LOGDIR=/tmp/k3-kt66-soak3 bash v2/test/boot/soak.sh 1000 3` — **ảnh sản phẩm**
(`vifast-kbin-riscv64`, không phải ảnh test), **không icount** (D2-72 Q2), 3 tiến trình QEMU song
song, timeout 20 s/boot, neo **BUILD_ID** .text (D2-80). Log:
[`kt66-soak.log`](../JOURNAL/kt66-soak.log).

```
# soak: xong wall=225s (4 boot/s); ok=1000 fail=0 tổng=1000/1000        (:1016)
# soak: BUILD_ID=ce9a8c7cff7c68a3 sha256=eb365a13… git=3f02d72d…        (:1017)
# soak: 0/1000 lỗi (PASS)                                               (:1019)
# soak.sh: PASS                                                         (:1020)
```

- **0 lỗi / 1.000 boot** — 0/N chỉ là **trần** (QD §5.3), không retry (D2-07).
- BUILD_ID + sha256 **khớp đúng ảnh product hiện tại**
  ([`kt66-image-size.log`](../JOURNAL/kt66-image-size.log):3: `build=ce9a8c7cff7c68a3
  sha256=eb365a13…`) ⇒ soak thật sự đo ảnh đem bàn giao, không phải ảnh cũ.
- Log boot từng cái ở `JOURNAL/kt66/soak*` (nếu cần đối chiếu); script kiểm boot không PANIC +
  dòng `# k3 image` + `# boot ok` theo mẫu của KT-57, và **chỉ** `kill <pid>` của chính nó.

## 5. D4 — Sim L1 **100.000 seed × smp 1/2/4/8 = 400.000 lượt**

Cổng K3.5 đòi **100.000 seed × smp 1/2/4/8 = 400.000 lượt, 0 vi phạm** (KT-61/KT-64 chưa
chạy đủ). Lệnh gốc là `./v2/bin/v2 nightly --l1 --seeds 100000 --smp 1,2,4,8`, nhưng driver chạy
**tuần tự** từng cấu hình: đo thực tế ~25–30 phút/cấu hình ⇒ ~3,5 giờ cho 400k, vượt ngân sách 3 h
của task. Vì vậy KT-66 chạy **4 tiến trình song song**, **mỗi cấu hình một tiến trình**, cùng
`VIFAST_SEEDS=100000`, cùng driver `driver_smp`, cùng bất biến — **không phải** chạy tuần tự (ghi
rõ để không hiểu nhầm):

| Cấu hình | Lệnh | Kết quả | `log:dòng` |
|---|---|---|---|
| smp 1 | `VIFAST_SEEDS=100000 VIFAST_SMP=1 cargo test -p vifast-sim driver_smp -- --nocapture` | `# L1 OK: smp 1, 100000 seed × 64 op …` + `# L1 OK: 100000 lượt (smp [1], seeds 100000)`; `EXIT=0` | [`kt66-l1-smp1.log`](../JOURNAL/kt66-l1-smp1.log):291-292,297 |
| smp 2 | như trên, `VIFAST_SMP=2` | `# L1 OK: smp 2 …`; `EXIT=0` | [`kt66-l1-smp2.log`](../JOURNAL/kt66-l1-smp2.log):291-292,297 |
| smp 4 | `VIFAST_SMP=4` | `# L1 OK: smp 4 …`; `EXIT=0` | [`kt66-l1-smp4.log`](../JOURNAL/kt66-l1-smp4.log):290-291,296 |
| smp 8 | `VIFAST_SMP=8` | `# L1 OK: smp 8 …`; `EXIT=0` | [`kt66-l1-smp8.log`](../JOURNAL/kt66-l1-smp8.log):289-290,295 |

- **0 vi phạm**: mỗi lượt driver kiểm `check_sched`/`check_harts`/`check_ipc`/`check_waitaddr`/
  `check_mailboxes`/`check_caps`/`check_reply_links` sau **mỗi** op + bảo toàn
  `ipc_sent == ipc_delivered + ipc_pending + ipc_dropped` (≥1 mỗi seed); không panic, không seed
  lệch trace (cùng seed ⇒ cùng trace).
- **Wall 33m30s** = **23:56:49Z → 00:30:19Z (UTC)** cho **cả 4** tiến trình (mốc ghi ở
  `/tmp/kt66-l1-start.txt`/`end.txt`, dán trong
  [`kt66-l1-timing.txt`](../JOURNAL/kt66-l1-timing.txt)). Không cộng dồn wall từng tiến trình để
  giả thành "tuần tự".
- **Trung thực về "chạy một mình"**: 4 tiến trình L1 **không** chạy song song với `gate --fast`,
  `soak`, hay nightly khác; nhưng trong ~20 phút đầu có **build kernel** + **một vòng matrix 12
  boot QEMU** của chính task (đo ở 09:12–09:14). L1 là **bất biến theo seed**, không có ngưỡng
  wall-clock, nên kết quả không phụ thuộc tải máy; **không** dùng wall L1 làm số hiệu năng.
  Nếu reviewer muốn số "sạch tuyệt đối", chạy lại tuần tự theo lệnh gốc (~3,5 h) — phần đó **không**
  phải điều kiện của KT-66 và đã ghi rõ ở đây thay vì che.
- Số op: `ipc_sent` 1.212.128 / 1.165.183 / 1.137.251 / 1.125.024; `vm map` ~257k; `shootdown`
  ~135k mỗi cấu hình (chi tiết trong 4 dòng `# L1 OK`).

## 6. D5 — Conformance **5/5 / 61 dòng**

Lệnh: `KT55_LOGDIR=/tmp/k3-kt66-matrix4 bash v2/test/conformance/run.sh --compare --smp 2` (trace
sim host + log QEMU `ktest-smp2-1.log` từ D2; `trace_compare.py --image` kiểm D2-80). Kết quả:
**61/61 dòng khớp, 5/5 ca** (22:16, 23:19, 24:7, 25:7, 26:12), `build=9cc6c8f295cab617` — đúng
BUILD_ID ảnh `ktest` của lần build KT-66 ([`kt66-image-size.log`](../JOURNAL/kt66-image-size.log):4)
⇒ neo D2-80 **khớp ảnh đang chạy**. Log:
[`kt66-trace-compare.log`](../JOURNAL/kt66-trace-compare.log):5-6; `scn-check: PASS (27 tệp, 0
blocked, 16 nhóm)` cùng log.

**Vì sao trace không đổi:** KT-66 **không** đổi ngữ nghĩa sim/model/`.scn` — chỉ kernel (tách ảnh +
vị trí stack). 5 ca conformance đi qua object layer VM + core, không đi qua stack syscall U-mode;
nếu trace đổi thì đó là hồi quy platform. `build=` **do runner tính lại từ ảnh** rồi so
(không hardcode); với ảnh ktest cuối, `build=9cc6c8f295cab617` khớp
[`kt66-image-size.log`](../JOURNAL/kt66-image-size.log):4.

## 7. D6 — **Hai** ngưỡng (D2-85a), không đọc lẫn, cho **mọi** ảnh

### 7.1 Pin core — `size_of::<Machine>()`

| Hạng mục | Số `[ĐO]` | Ngưỡng | Kết quả |
|---|---|---|---|
| `Machine` tĩnh | **188.872 B** (96,1 % pin, còn 7.736 B) | **196.608 B** (pin D2-84) | ĐẠT |
| Bảng phân bố | 29 phần, `sum=188872`, `padding` ≤ 8 | `sum + scalars + padding == total` (D2-85c) | ĐẠT — TAP 43 ảnh `ktest` (`JOURNAL/kt66/ktest-smp1-1.log`, `# core: link`) |

### 7.2 Trần **ảnh link** — `.data+.sdata+.bss+.sbss` (`budget.sh`, ELF thật)

| Ảnh | file (byte) | **data+bss** | Trần | Headroom |
|---|---:|---:|---:|---:|
| product | 188.184 | **200.120** | 262.144 | 62.024 |
| ktest | 574.032 | **209.856** | 262.144 | 52.288 |
| ktest-guard | 54.048 | **2.680** | 262.144 | 259.464 |
| ktest-user | 335.520 | **202.280** | 262.144 | 59.864 |
| **ktest-proc** (mới) | 375.512 | **206.472** | 262.144 | 55.672 |

Nguồn: [`kt66-budget.log`](../JOURNAL/kt66-budget.log):1 (product), `:4` (ktest), `:7` (guard),
`:10` (user), `:13` (proc); BUILD_ID + kích thước tệp:
[`kt66-image-size.log`](../JOURNAL/kt66-image-size.log):3-7.

### 7.3 NOLOAD per-hart — báo **riêng**, không cộng trần (D2-72 Q5)

`budget.sh` nay in thêm `syscall_stack=`, `hart_stacks=`, `noload_hart=` (KT-66):

| Hạng mục | Byte | Ghi chú |
|---|---:|---|
| `.stack` hart 0 | 65.536 | NOLOAD |
| `.stack_guard` hart 0 | 4.096 | NONE |
| `.trap_stack` hart 0 | 16.384 | NOLOAD |
| **`.syscall_stack` hart 0** | **32.768** | **mới KT-66** — stack dispatch per-hart |
| `.hart_stacks` (7 record) | 831.488 | `7 × HART_STRIDE = 7 × 0x1_D000` |
| **Tổng NOLOAD per-hart** | **950.272** (928 KiB) | `MAX_HARTS × HART_STRIDE`; **không** vào `.data+.bss` |

Nguồn: `kt66-budget.log:1` (cùng số cho mọi ảnh vì linker dùng chung); bố cục kiểm chéo ở
`JOURNAL/kt66/ktest-smp1-1.log:104` (`# smp: … stride=0x1d000 area=0xe8000`).

**NOLOAD vẫn chiếm RAM:** `mmu::init` map identity **toàn** vùng per-hart (928 KiB), nên con số
950.272 B **không** phải "miễn phí" — nó chỉ **không** nằm trong tiêu chí trần `.data+.bss` của
D2-72 Q5. Tăng `HART_STRIDE` là tăng RAM thật: báo cáo ở đây để người đọc không hiểu nhầm là
"thêm stack mà ảnh không to hơn". `.syscall_stack` 32 KiB/hart × 8 = 256 KiB trong tổng đó.

**Vì sao stack không nằm trong khoảng 262.144:** tiêu chí D2-72 Q5 đo `.data+.bss` (phần **file**
ảnh); stack/trap stack/NOLOAD được khai **riêng** từ KT-46/KT-49 — KT-66 đặt stack syscall vào
cùng loại NOLOAD per-hart, **không** giấu vào `.bss` (bản KT-65 nằm trong `.bss` và **làm mất**
32 KiB headroom; nay trả lại). Trần 262.144 **không đổi**; `size_of::<Machine>()` **không đổi**.

## 8. D7 — Tách ảnh demo (`vifast-kbin-ktest-proc`)

- **Trước:** `ktest-user` = `init` ELF + 3 ELF demo (procsrv/shell/con) + `proc_demo` + stack
  dispatch 32 KiB trong `.bss` ⇒ **239.216 B** (`kt65-budget.log:10`), headroom 22.928 B.
- **Sau:** ảnh syscall/core **14 TAP** — **202.280 B** (`kt66-budget.log:10`), headroom
  **59.864 B**; **trả về 36.936 B** (≈36,1 KiB). Brief ghi "~40 KiB" là **ước lượng**; phần chênh
  là code/`.text`+`.rodata` của `init`/rig vẫn ở lại ảnh này (không nằm trong `.data+.bss`) —
  **không** hạ ngưỡng, không cắt tính năng nào.
- **Ảnh thứ tư:** `vifast-kbin-ktest-proc` (bin `src/ktest_proc.rs`, `required-features =
  ["ktest","user"]`) — **206.472 B** (`kt66-budget.log:13`) ≤ trần.
- **Mười ca cũ 15..24 → TAP 1..10** của ảnh mới (bảng ánh xạ ở `v2/docs/k35-procsrv.md` §9.1);
  bằng chứng từng ca **giữ nguyên** để đối chiếu dễ: `ktest-proc` in lại các dòng `# proc:`/
  `# shell:`/`# child:`/`# u-mode …` như ảnh cũ.
- **Runner + expected + manifest sửa cùng lúc** (luật KT-54): `LABELS` 4 ảnh, `LABEL_COUNT`,
  `manifest_proc`, `check_stack_lines`, `panic_allow_re` tách theo ảnh, `want_rows` tính động.
- So sánh trực tiếp `JOURNAL/kt66/ktest-user-smp{1,2,4}-1.log` (14 TAP) và
  `JOURNAL/kt66/ktest-proc-smp{1,2,4}-1.log` (10 TAP): cả hai đều sạch.

## 9. D8 — Stack syscall **per-hart** + canary + nước thấp

- Thay `static SYSCALL_STACK` 32 KiB (KT-65 — **một** stack dùng chung) bằng **một block 32 KiB
  cho mỗi hart online** trong vùng **NOLOAD per-hart**: record mới
  `guard 4 KiB | kernel stack 64 KiB | trap stack 16 KiB | **syscall stack 32 KiB**` ⇒
  `HART_STRIDE = 0x1_D000`, `HART_AREA_BYTES = 0xE_8000`; `smp`/`linker.ld`/macro `trap_entry`/
  `mmu::init` (kiểm khớp) cùng đổi. Hằng số nằm trong code, không phải số đo tay.
- **Vì sao không cấp Frame từ arena** (đã thử, **đỏ thật**, nhưng **không** phải bằng chứng DoD):
  `Frame` của object layer ở cửa sổ PA `0x8200_0000`, chỉ chạm được qua **physmap/nửa cao**;
  `copy_kernel_low_root` **không** chép root index ≥ 256 vào root user, mà trap từ U-mode chạy với
  `satp` = root user ⇒ dispatch trên stack arena **store page fault**
  `stval=0xffffffc082007ff8` (PANIC quan sát trong phiên; **log thô đã bị ghi đè** khi chuyển
  sang NOLOAD nên không dẫn tệp — ghi để minh bạch, **không** tính là log DoD). Bằng chứng kiến
  trúc cho cùng kết luận: `# mmu layout: … none_high=1 none_physmap=1`
  ([`JOURNAL/kt66/ktest-smp1-1.log`](../JOURNAL/kt66/ktest-smp1-1.log):82). Vùng NOLOAD per-hart
  identity-mapped **thấp** (root index 2, có trong root user) nên dùng đúng chỗ.
- **Canary + low:** hai word canary ở đáy + sơn `0xAA` toàn block 32 KiB; `low` = word đầu tiên
  khác sơn; canary kiểm **sau mỗi lượt dispatch** + lúc in evidence ⇒ tràn/underflow ⇒ `canary=1`.
- Bằng chứng đo được (số **thật**, không hardcode):

  | Ảnh / smp | Dòng evidence | `distinct` | `canary` | hart 0 `low`/`used` |
  |---|---|---:|---:|---|
  | `ktest-user` smp 1 | `JOURNAL/kt66/ktest-user-smp1-1.log:129-130` | 1 | 0 | low `0x8041c178`, used **3.720** |
  | `ktest-user` smp 2 | `JOURNAL/kt66/ktest-user-smp2-1.log:129-131` | 1 | 0 | used 3.720; hart 1 `base=0x80432000`, used 0 |
  | `ktest-user` smp 4 | `JOURNAL/kt66/ktest-user-smp4-1.log:129-133` | 1 | 0 | 4 `base` khác nhau |
  | `ktest-proc` smp 1 | `JOURNAL/kt66/ktest-proc-smp1-1.log:122-123` | 1 | 0 | low `0x804179b8`, used **22.088** |
  | `ktest-proc` smp 4 | `JOURNAL/kt66/ktest-proc-smp4-1.log:122-126` | 1 | 0 | 4 `base` khác nhau |

- **22.088 B** trên block 32 KiB ở ảnh demo = bằng chứng định lượng rằng stack 16 KiB của KT-65
  **không đủ** cho đường dispatch IPC sâu (khớp sự cố tràn trap stack ở `k35-procsrv.md` §6) —
  đây là lý do giữ 32 KiB, **không** phải nới ngưỡng (ngưỡng stack không có trong QD; KT-65 chọn
  32 KiB, KT-66 giữ nguyên).
- `ktest-user` và `ktest-proc` gọi `user::init_syscall_stacks()` trước `load_init`; `smp 2/4`
  **không** dùng chung stack — runner `check_stack_lines` đòi **tập** hart 0..N-1, `base` phân
  biệt **theo mọi cặp** (không chỉ so với hart 0), `low ∈ [base, top]`, `top-base ≥ 32 KiB`.
- **Phạm vi đo thật (D3/D4 reviewer):** smp 4 **đã cấp** 4 block `base` khác nhau, nhưng **chưa**
  có hart 1..3 nào **dispatch** (scheduler demo chỉ chạy hart 0) — cột `used=0` ở hart 1..3 phản
  ánh đúng điều đó, **không** phải "đã chạy song song". Việc nối IPI multi-hart là nợ #6 §12.2.

## 10. D9 — **Đỏ trước** + không retry

| Đỏ | Cách tạo | Kết quả | Log |
|---|---|---|---|
| **Ảnh tách** | log QEMU **24 TAP cũ** của `ktest-user` kiểm bằng `expected.txt` **mới** (14) — tức ảnh cũ không còn thoả runner mới | **ĐỎ**: `ok-count=24/14`, `tap-day-1..14`, `panic-khac=1`, thiếu evidence stack | [`kt66-red-before-split.log`](../JOURNAL/kt66-red-before-split.log) |
| **Stack dùng chung** | cấp stack với **cùng** `base` cho mọi hart (tái hiện khuyết điểm KT-65) rồi boot `-smp 2` | **ĐỎ**: `stack-distinct=1/2`, thiếu evidence `distinct=1` (log QEMU: `distinct=0`, hai hart cùng `base=0x80415000`) | [`kt66-red-before-stack.log`](../JOURNAL/kt66-red-before-stack.log); QEMU: [`kt66-red-before-shared-stack-qemu.log`](../JOURNAL/kt66-red-before-shared-stack-qemu.log):129-131 |
| **Canary** | ghi đè word canary hart 0 (giả lập tràn) rồi in evidence | **ĐỎ**: log QEMU `canary=1`, thiếu evidence `canary=0` | [`kt66-red-before-canary.log`](../JOURNAL/kt66-red-before-canary.log) |
| **Arena (phát hiện, KHÔNG phải bằng chứng DoD)** | cấp stack từ Frame arena rồi dispatch U-mode | **ĐỎ thật trên QEMU**: `PANIC: trap StorePageFault … stval=0xffffffc082007ff8` — cơ sở để chọn NOLOAD per-hart. **Log thô của lần thử bị ghi đè** khi chuyển sang NOLOAD (không còn tệp) — ghi ở đây để minh bạch, **không** tính là log DoD; bằng chứng kiến trúc: `# mmu layout: … none_high=1 none_physmap=1` (`JOURNAL/kt66/ktest-smp1-1.log:82`) | — (không có tệp) |
| **Không retry** | — | mọi log D1..D8 là lần chạy **đầu** sau khi xanh; không `#[ignore]`, không `should_panic`, không sửa `expected.txt` cho vừa log, không retry (D2-07) | — |

## 11. D10 — Clippy kernel + `v2 check`

- `cargo clippy --workspace --no-deps --target riscv64gc-unknown-none-elf -- -D warnings` ⇒
  **PASS**; thêm **4** tổ hợp: `-p vifast-kbin-riscv64 --features ktest`, `--features ktest,user`;
  `-p vifast-plat-riscv64` (mặc định) và `--features ktest,user` ⇒ **PASS** (log
  [`kt66-core-build.log`](../JOURNAL/kt66-core-build.log), có `CLIPPY_EXIT=0` từng tổ hợp).
- Nợ quy trình **vẫn còn** (đã ghi từ KT-61): `gate --fast` **không** phủ kernel (workspace host
  `v2/Cargo.toml` loại `kernel`), nên clippy kernel là bước **riêng** của task — đề xuất task
  riêng thêm vào cổng.
- `./v2/bin/v2 check` ⇒ **PASS** (fmt + clippy host + luật cấu trúc + gen-check); `scn-check:
  PASS (27 tệp, 0 blocked, 16 nhóm)`; `git diff --stat -- spec/` **rỗng**.

## 12. D11 — Phạm vi: K3.5 **xong cái gì**, **còn gì** (không tô hồng)

### 12.1 Đã xong (đo được ở các mục trên)

1. `procsrv` + `shell` + con là **chương trình U-mode Rust thật**, nạp ELF riêng, chạy trên QEMU
   (TAP 1..10 ảnh `ktest-proc`); `wait/kill` là giao thức, `ps` đọc `Machine`.
2. **Tách ảnh demo** khỏi ảnh syscall: headroom `.bss` **+36.936 B** cho `ktest-user`; matrix
   **4 ảnh × smp 1/2/4 = 12/12**.
3. **Stack syscall per-hart** trong NOLOAD per-hart (32 KiB/hart), **canary + low** thường trực;
   smp 2/4 **không** dùng chung.
4. Cổng K3.5: TAP 100 %, soak 0/1000, L1 400k, conformance 5/5, budget 2 ngưỡng, clippy sạch.

### 12.2 Còn lại — có chủ, có tiêu chí (không mục nào được gọi là "đã xong")

| # | Nợ | Vì sao chưa xong | Tiêu chí đóng |
|---|---|---|---|
| 1 | **Fault bridge ABI** (K3.6) | `Recv` chưa có trường fault; kernel đọc `Tcb.last_fault` rồi gửi **message chuyển tiếp** `FAULT_EXIT` tới `fault_ep` (arch bridge, `k35-procsrv.md` §2/§3) | ABI `Recv` trả fault + parity sim/core; bỏ message chuyển tiếp |
| 2 | `kill` chưa `Revoke cap TCB` | chỉ `Tcb.Suspend(Abort)` + cờ `suspended`; không thu hồi vspace/frame/cap của tiến trình (QD Q-A) | teardown trả tài nguyên + TAP `kill` kiểm `check_objmem`/cap |
| 3 | `kill <pid sai>` chưa bị từ chối | procsrv trả `REPLY_KILL code=0` **trước** khi kernel xác nhận (reply nằm ở procsrv) | ACK kernel→procsrv (hoặc kernel phục vụ kill) + TAP pid sai ⇒ lỗi |
| 4 | `vifast-plat-user` **không** là workspace member | `build.rs` biên dịch `rlib` bằng `$RUSTC` rồi `--extern` (D2-93 cho phép unsafe ở đây) | thêm crate vào workspace (hoặc quyết định giữ + `check` riêng) |
| 5 | Shell **chưa đọc UART IRQ** | dòng vào do kernel bơm từ kịch bản (message `LINE`); chưa có PLIC UART IRQ → user | PLIC UART IRQ + hàng đợi byte → message, giữ giao thức `LINE` |
| 6 | **Core multi-hart chưa nối IPI** | scheduler demo chạy hart 0; core chưa dùng IPI cho preemption/shootdown từ user | core gọi IPI khi schedule/`TlbFlush`; TAP smp 2/4 |
| 7 | `TlbFlush` `targets/acked` **chưa quan sát được** từ user | KT-64 đã có `TlbFlush` thật trong core, nhưng user không thấy `targets/acked` (TAP chỉ kiểm `seq/local/pending`) | bề mặt evidence cho `targets/acked` + ca parity |
| 8 | **Slot 192 word/Region** | slot arena (`CNode` retype) vẫn là word trong Region 192 word; kiến trúc slot đầy đủ (cap trong arena) chưa làm | đo/đóng theo thiết kế bộ nhớ cap; `check_bytes` không đổi |
| 9 | **Stack demo sát trần + hart 1..3 chưa dispatch** | hart 0 ảnh demo dùng **22.088 / 32.768 B** (headroom **10,4 KiB**); canary **đã fail-closed** (M1: hỏng ⇒ PANIC/halt, không chạy tiếp); nhưng **hart 1..3 chưa từng dispatch** (scheduler demo chỉ hart 0 — nợ #6) ⇒ `distinct=1` là đã **cấp** riêng, chưa chứng minh **chạy** song song | tăng block **có đo** khi demo mọc thêm (không nới ngầm); nối IPI multi-hart (nợ #6) rồi TAP dispatch ≥2 hart |

Nợ khác **giữ nguyên từ K3b/K3.5** (không mới, không giấu): scheduler U-mode **tuần tự trong
kernel** (không preemptive, chưa `Yield` timer-driven); `wait` **không chặn**; **đường trả về
`rust_main` sau nhiều `user_enter`** chưa chứng minh (demo dùng continuation); chưa có
`ReplyRecv`/`Call` trong giao thức; một bộ ba `procsrv/shell/con` cố định (chưa nhiều phiên);
soak chỉ 1 hart sản phẩm; perf chỉ máy này; AIA/MSI/Sstc; pager/COW; `FaultKind.vspace` là nhãn;
Frame 1 GiB `NotSupported`; sau `Exit` satp root user. **K3.5 KHÔNG phải kernel đầy đủ** (không
memsrv/IDN/virtio — K6).

## 13. D12 — Phạm vi file + commit + push

- Chỉ `v2/**`; **không** `spec/`, `viscript/`, `docs/` gốc, QUEUE/DECISIONS/AGENTS. `unsafe`
  **chỉ** trong `kernel/vifast-plat-*` (D2-17); `vifast-core` vẫn `#![forbid(unsafe_code)]`.
- Commit `[KT-66]` (kernel + harness + docs + log) và `[KT-66b]` (STATE) — **lead commit bằng
  pathspec** (D2-91), Agent 2 **không** `git add`; hash ghi ở §13.1 sau khi commit.
- `git rev-list --count HEAD..origin/main` = **0** trước push; push `origin main`.

### 13.1 Hash

- **Chưa commit tại thời điểm viết** (theo quy trình: lead commit `[KT-66]`/`[KT-66b]`
  pathspec `v2/**` **sau** verdict reviewer; D2-91). Hash commit sẽ ghi ở đây ngay sau push.
## 14. Review độc lập (`reviewer-kt66`, lead spawn, read-only)

- **Verdict vòng 1: DRIFT (không UNSAFE)** — reviewer tự tái lập: matrix 12/12, gate 649/84,
  soak 0/1000, L1 400.000 (có `Blocking waiting for file lock` chứng minh 4 tiến trình song song),
  conformance 61/61, budget khớp từng byte, **không nới ngưỡng**, `SYSCALL_STACK` đã xoá thật
  (`nm` = 0), NOLOAD không đếm trùng `.bss`, tách ảnh **không mất TAP**.
- **Đã vá 5 mục reviewer + 3 đề nghị:** M3 (harness `strtonum` false-green → portable + fail-closed
  + `--selftest-stack` 5/5 + `selftest.sh` 18/18), M1 (canary fail-closed → PANIC/halt), M2 (nợ #9
  §12.2: headroom 10,4 KiB + hart 1..3 chưa dispatch), M4 (file 335.520, L1 **UTC**, NOLOAD chiếm
  RAM), M5 (bỏ dẫn chứng log arena không tồn tại; DOD đóng băng + sha ở §13.1); D4 (distinct so
  **mọi cặp**), D3 (ghi rõ "đã cấp riêng 4 base, **chưa** dispatch song song hart 1..3"), D9
  (`expected.txt` "bốn nhãn", `ktest.sh:6` "bốn ảnh").
- Sau vá: matrix **12/12** (log mới, 0 lỗi awk), conformance **61/61**, `selftest.sh` **18/18**,
  `gate --fast` **PASS 649/84** (L1 **911 ms**), `v2 check` **PASS**, clippy 5 tổ hợp `EXIT=0`.
  Ảnh product/ktest/guard **không đổi** (`ce9a8c7c…`/`9cc6c8f2…`/`f5afbe3c…`); chỉ user/proc đổi
  (`fbc03a81…`/`333c35bb…`) ⇒ soak product vẫn đúng ảnh (sha `eb365a13…` khớp).
- **Confirm vòng 2: chờ** reviewer ngắn sau khi lead spawn lại.

## 15. Bảng DoD D1..D12 (đo được, không nới)

| # | Điều kiện | Ngưỡng | Kết quả | Nguồn |
|---|---|---|---|---|
| D1 | `gate --fast` | xanh, 0 failed, L1 1.000 seed | **PASS 649/84 suite**, L1 911 ms | `kt66-gate.log:1226-1227` |
| D2 | TAP trên QEMU + matrix | **100 %** smp 1/2/4, số TAP khớp `expected.txt`; stack `low/block` kiểm **thật** (fail-closed) | **12/12** boot (60/3/14/10 TAP); `--selftest-stack` **5/5** | `kt66-ktest-matrix.log:27-38,67`; `kt66-selftest.log` |
| D3 | Soak sản phẩm | **0 lỗi / 1.000**, 3 luồng, không icount, 20 s | **1000/1000, 0 lỗi**, 225 s | `kt66-soak.log:1016-1020` |
| D4 | L1 **400.000 lượt** | 100k × smp 1/2/4/8, 0 vi phạm | **400.000**, 0 vi phạm, 4 tiến trình song song, 33m30s | `kt66-l1-smp{1,2,4,8}.log` |
| D5 | Conformance | **5/5** ca, 0 khác biệt ngoài trường cho phép | **61/61 dòng, 5/5** | `kt66-trace-compare.log:5` |
| D6 | Hai ngưỡng | `size_of` ≤ **196.608**; ảnh ≤ **262.144** (mọi ảnh) | **188.872**; **200.120/209.856/2.680/202.280/206.472** | `kt66-budget.log:1,4,7,10,13` |
| D7 | Ảnh tách | ảnh demo riêng, cùng 10 ca, matrix tăng boot | **proc 206.472**, user **−36.936** | `kt66-budget.log:10,13` |
| D8 | Stack per-hart + canary | `distinct=1`, `canary=0`, smp 2/4 không chung | **ĐẠT** (bảng §9) | `JOURNAL/kt66/ktest-{user,proc}-smp{1,2,4}-1.log` |
| D9 | Đỏ trước / không retry | có log đỏ **trước**; không retry | **3 log đỏ** + harness selftest 6 ca (5 stack + full 18/18) | §10; `kt66-selftest.log` |
| D10 | Clippy kernel + `v2 check` | sạch `-D warnings` | **PASS** (5 tổ hợp) | `kt66-core-build.log` |
| D11 | Tuyên bố phạm vi | K3.5 = procsrv+shell thật, **không** kernel đầy đủ | §12 | §12 |
| D12 | Phạm vi + commit/push | chỉ `v2/**`; push sau xanh | §13 | §13.1 |

## 16. Tệp chạm (KT-66)

| Tệp | Loại |
|---|---|
| `v2/kernel/vifast-kbin-riscv64/src/ktest_proc.rs` | **mới** — ảnh demo thứ tư (TAP 1..10) |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_user.rs` | còn 14 TAP, bỏ `proc_demo`/`fault_phase`, kết thúc sạch |
| `v2/kernel/vifast-kbin-riscv64/src/proc_demo.rs` | TAP 15..21 → 1..7 |
| `v2/kernel/vifast-kbin-riscv64/{Cargo.toml,linker.ld}` | bin thứ tư; record NOLOAD + syscall stack 32 KiB |
| `v2/kernel/vifast-plat-common/src/smp.rs` | hằng số/offset record mới (`HART_SYSCALL_STACK_*`, stride 0x1D000) |
| `v2/kernel/vifast-plat-riscv64/src/user_stack.rs` | **mới** — stack per-hart + canary/low + evidence (tách khỏi `user.rs` cho R1) |
| `v2/kernel/vifast-plat-riscv64/src/{user.rs,mmu.rs,lib.rs}` | dispatch đổi `sp` sang block hart; layout/asm stride mới; `mod user_stack` |
| `v2/test/boot/{ktest.sh,expected.txt,budget.sh}` | 4 ảnh, manifest/evidence split, `check_stack_lines`, NOLOAD riêng |
| `v2/docs/k35-procsrv.md` | §9/§10 (KT-66) + §2/§8 cập nhật |
| `v2/agent/reports/K35-DOD.md` | **mới** (tệp này) |
| `v2/agent/JOURNAL/kt66-*`, `JOURNAL/kt66/` | log cổng + red-before + 12 log boot |
| `v2/agent/STATE.md` | HEAD/BUILD_ID/task/bước/cổng |
