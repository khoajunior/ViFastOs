<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3-DOD — bằng chứng cổng cuối phase K3 (KT-57)

Người ghi: **Agent 2 dòng OS** · Ngày: 2026-11-15 · Task: **KT-57** (cổng K3) · HEAD khi mở
task: `d543352d` (`[KT-00]` lật KT-56 DONE, mở KT-57) · Nhánh `main` · Máy: `ps` (12 nhân,
QEMU `11.1.3`, OpenSBI `v1.8.1` — version do script đọc và in, xem log).

> **K3 = kernel thật trên QEMU `riscv64` — nhưng CHƯA có core.** K3 đã có platform kernel thật
> (trap/`stvec`, Sv39 + `satp` thật, timer SBI 100 Hz, PLIC/UART IRQ, SMP SBI HSM, IPI
> `sip.SSIP`, ASID + `sfence.vma` + shootdown ack, object layer `no_std`, U-mode 3 syscall,
> `FaultKind` từ trap thật, conformance 5 `.scn` × sim/QEMU). K3 **chưa** port core
> (CNode/MDB/TCB/Endpoint/Notification/Reply/scheduler/MCS/IPC) sang `no_std` — phần đó vẫn
> **model-only trong sim** (D2-72 **Q13**). Vì vậy K3 **không** phải ABI đầy đủ trên QEMU; mở
> **K3b** (KT-58..KT-61) trước K3.5 (procsrv/shell). Câu này không được nới ở bất kỳ mục nào
> dưới đây.

**Phạm vi KT-57**: chạy đủ **K3-PLAN §5.2** (9 điều kiện, không nới ngưỡng), viết
`v2/test/boot/soak.sh`, thu log QEMU/đo perf, viết **K3-DOD.md**, cập nhật `v2/docs/k3-riscv64.md`
+ `STATE.md`. **Không** thêm tính năng, **không** sửa semantics kernel; thay đổi kernel duy
nhất là **1 dòng evidence-only** (D2-80 — §4.3). Không đụng `spec/`, sim semantics, `.scn`,
QUEUE/DECISIONS/AGENTS, `viscript/`.

Nguồn đối chiếu: `review/agent0/K3-PLAN.md` §4.1 (KT-57) + **§5.2** (cổng K3) + §5.3 (cấm nới);
**D2-24**, **D2-72 Q10** (soak), **D2-74** (không icount), **D2-80** (neo hash ảnh), **D2-72 Q5**
(budget), **D2-07** (không retry), **D2-72 Q13** (K3b); `v2/docs/k3-riscv64.md` (§1..§20);
tiền lệ báo cáo [`K2-DOD.md`](K2-DOD.md).

## 0. TL;DR — 9 điều kiện cổng K3 (§5.2), đối chiếu ở §11

| # | Điều kiện | Kết quả | Log |
|---|---|---|---|
| 1 | `gate --fast` xanh | **PASS 574 test/75 suite**, 0 failed; L1 1.000 seed **824 ms**; wall **280,36 s** | [`kt57-gate.log`](../JOURNAL/kt57-gate.log) |
| 2 | TAP ktest **100 %** ở `smp 1/2/4` | **9/9 boot sạch**: `ktest` **43/43**, `ktest-guard` **3/3**, `ktest-user` **7/7**; selftest **17/17** | [`kt57-ktest-matrix.log`](../JOURNAL/kt57-ktest-matrix.log), [`kt57-selftest.log`](../JOURNAL/kt57-selftest.log) |
| 3 | Soak **0 lỗi/N = 1.000**, 3 luồng, máy `ps` | **1000/1000 boot sạch, 0 lỗi**, 3 luồng, timeout 20 s/boot, **không icount**, wall **219 s**; BUILD_ID `e09ec255c17715ae`, sha256 `509d5adc…` | [`kt57-soak.log`](../JOURNAL/kt57-soak.log), [`kt57-soak-results.txt`](../JOURNAL/kt57-soak-results.txt) |
| 4 | Conformance **5/5** `.scn` sim ↔ QEMU | **61/61 dòng, 5/5 ca**, `build=3e6ae37a278300df` khớp ảnh ktest (D2-80); `check_scn` **26 tệp/16 nhóm** | [`kt57-trace-compare.log`](../JOURNAL/kt57-trace-compare.log) |
| 5 | `.data+.bss` ≤ **262.144 B** cả ảnh | **product 11.248 / ktest 17.344 / guard 2.680 / user 11.752** B (trần 262.144) | [`kt57-budget.log`](../JOURNAL/kt57-budget.log) |
| 6 | Sim L1 **100k × smp 1/2/4/8**, 0 vi phạm | **400.000 lượt**, **0 vi phạm**, wall **1.299 s** | [`kt57-l1-100k.log`](../JOURNAL/kt57-l1-100k.log) |
| 7 | **4 mutation** bị giết | **4/4**: KT-51 M1 (bỏ `sfence.vma`) + M2 (bỏ chờ ack); KT-57 chạy lại **2 ca còn thiếu ở mức kernel** — bỏ flush cục bộ (TAP 25/28 đỏ, `local_verify_wrong=50`), bỏ drain hộp thư (TAP 20/21/23 đỏ + treo tới timeout 20 s) | [`kt57-mutation-X-run.log`](../JOURNAL/kt57-mutation-X-run.log), [`kt57-mutation-Y-run.log`](../JOURNAL/kt57-mutation-Y-run.log), [`kt51-mutation-1-*`](../JOURNAL/kt51-mutation-1-bay-sfence-qemu.log), [`kt51-mutation-2-*`](../JOURNAL/kt51-mutation-2-bo-cho-ack-qemu.log) |
| 8 | Perf đầu tiên `[ĐO]` | build 4 ảnh sau `cargo clean`: **3,35 + 5,51 + 2,75 s**; boot→halt wall host (100 boot) median **389.084.167 ns**, p95 **484.941.500 ns**; tick drift icount `min_ns` 9.999.900–10.000.000 / `max_ns` 10.019.900–10.020.100; IRQ/trap lateness **20,0–20,1 µs** | [`kt57-perf.log`](../JOURNAL/kt57-perf.log), [`kt57-perf-boot.log`](../JOURNAL/kt57-perf-boot.log), [`kt57-perf-build.log`](../JOURNAL/kt57-perf-build.log) |
| 9 | Tài liệu: K3 = platform kernel thật, **không** ABI đầy đủ (Q13); nợ K3b | tệp này + `v2/docs/k3-riscv64.md` **§20** + STATE; nợ K3b ở §12 | tệp này |

**Đề xuất trạng thái**: **KT-57 DONE (đề xuất)** và **K3 DONE (đề xuất)** — 9/9 điều kiện §5.2
ĐẠT, không nới ngưỡng nào; chờ **reviewer độc lập** (verdict ở §13) + **Agent 0** lật QUEUE
(D2-37: Agent 2 không tự lật).

## 1. Baseline (trước khi chạy cổng)

- HEAD `d543352d`; `git rev-list --count HEAD..origin/main` = **0**; `v2/` sạch (worktree có
  `viscript/` + `review/` của tab khác — **không chạm**, chỉ stage `v2/`).
- Baseline `gate --fast`: **PASS 574 test/75 suite**, L1 1.000 seed **823 ms**, wall **280,61 s**
  ([`kt57-baseline-gate.log`](../JOURNAL/kt57-baseline-gate.log)).
- Máy: 12 nhân; QEMU `11.1.3` (sàn 9.2, D2-72 Q2); OpenSBI `fw_dynamic v1.8.1`. Mọi log ghi
  **version + BUILD_ID + sha256** ngay trong lệnh chạy; **không retry** để lấy xanh (D2-07).

## 2. D1 — `gate --fast` (điều kiện 1)

Lệnh `./v2/bin/v2 gate --fast` (không retry) — log [`kt57-gate.log`](../JOURNAL/kt57-gate.log):

```
v2 check: PASS
--- L0 test: PASS (574 test, 0 failed)        # 75 suite
--- L1: OK (824 ms, đo bởi cổng)              # ngưỡng QD §5.2 = 60 s
v2 gate --fast: PASS (gen-check ON từ KT-05; L1 sim 1.000 seed)
real 280.36
```

- **574 test / 75 suite / 0 failed** — bằng baseline (KT-57 không thêm/bớt test host).
- L1 1.000 seed **824 ms** ⇒ ngưỡng **≤ 60 s chỉ áp cho L1** (K3-PLAN §5.2, K2-DOD §8.6) giữ
  nguyên. `real 280,36 s` là **wall host** của cả cổng (L0 + clippy + fmt trên 12 nhân, có tải
  nền); baseline trước KT-57 cũng **280,61 s** ⇒ **T-INFRA môi trường**, **không** phải ngưỡng và
  **không** nới gì (cùng cách ghi ở K2-DOD §8.6).
- `scn-check: PASS (26 tệp, 0 blocked, 16 nhóm)`, `gen-check: PASS`, `git diff --stat -- spec/`
  **rỗng** (KT-57 không chạm `spec/`).

## 3. D2 — Matrix TAP `smp 1/2/4` × 3 ảnh test (điều kiện 2)

`bash v2/test/boot/ktest.sh 1 matrix` (1 boot mỗi cấu hình = **9 boot**, `-icount shift=6` theo
D2-74) — log [`kt57-ktest-matrix.log`](../JOURNAL/kt57-ktest-matrix.log):

| Ảnh | `smp 1` | `smp 2` | `smp 4` |
|---|---|---|---|
| `ktest` | **43/43 PASS** | **43/43 PASS** | **43/43 PASS** |
| `ktest-guard` | **3/3 PASS** | **3/3 PASS** | **3/3 PASS** |
| `ktest-user` | **7/7 PASS** | **7/7 PASS** | **7/7 PASS** |

- `# KTEST: 9/9 boot sạch (smp=matrix, NBOOT=1 …)`; `expected.txt` là **nguồn sự thật duy nhất**
  (43/43, 3/3, 7/7); runner kiểm dãy `ok` 1..N, tên từng TAP, 65 evidence (ktest)/5 (guard)/13
  (user), `Platform HART Count` của OpenSBI = `-smp N`, đúng 1 PANIC theo mẫu ảnh.
- Không thiếu/thừa TAP: ảnh chính 43 (KT-45..52 + conformance 37 + KT-56 38..42 + trap lồng 43),
  guard 3, user 7.
- **Runner tự kiểm** `selftest.sh`: **17/17 ca** đúng kỳ vọng (1 ca xanh + 15 ca đỏ đúng token +
  1 ca `expected.txt` nguyên vẹn) — [`kt57-selftest.log`](../JOURNAL/kt57-selftest.log).
- 50 boot/cấu hình đã có từ KT-54/56 (`kt56-boot50.log` **450/450**); cổng KT-57 chạy **1 boot
  mỗi cấu hình** theo đúng câu chữ §4.1 (KT-57), không thay thế log 450 boot.

## 4. D3 — Soak **0 lỗi/1.000** ảnh sản phẩm (điều kiện 3)

### 4.1 Lệnh và bất biến

`bash v2/test/boot/soak.sh 1000 3` — ảnh **sản phẩm** `vifast-kbin-riscv64`, 3 luồng QEMU, timeout
**20 s**/boot, **không** `-icount` (D2-74 điều kiện (c)), **không retry**, mỗi boot một tệp log.
Log: [`kt57-soak.log`](../JOURNAL/kt57-soak.log), [`kt57-soak-results.txt`](../JOURNAL/kt57-soak-results.txt)
(1.000 dòng `ok i`), mẫu boot: [`kt57-soak-boot1.log`](../JOURNAL/kt57-soak-boot1.log).

Công thức **n ≥ 3/p** (D2-24) in trong log, tính **số nguyên** để tránh sai số dấu phẩy động:

```
p = 3/1000 (0,3 %) ⇒ n ≥ 3/p = ceil(3·1000/3) = 1000
```

`soak.sh 999 3` **bị từ chối exit 2** (`N=999 < n ≥ 3/p = 1000`) — không có đường nới N
([`kt57-soak-red-threshold.log`](../JOURNAL/kt57-soak-red-threshold.log)). `LANES` ngoài `[1,3]`
bị chặn (AGENTS §9).

Mỗi boot **fail-closed** (đủ 6 bất biến): đúng 1 `TAP version 13`, đúng 1 `1..1`, đúng 1
`ok 1 - k3 boot`, đúng 1 `# k3 halt`; `# k3 image=product` (đúng ảnh sản phẩm, D2-08); **đúng 1**
`# conf image: build=<16 hex> text=<len>` **khớp ELF** (D2-80); `# smp: boot=0 harts=1 …
timeout=0` + `Platform HART Count : 1` (OpenSBI — bằng chứng ngoài kernel); **không** dòng `PANIC`.
Quá 20 s ⇒ `kill` **đúng PID** (AGENTS §9), boot đó đỏ; ảnh đổi giữa soak (sha256 đầu ≠ cuối) ⇒
soak **FAIL** (bằng chứng mất neo, D2-80).

### 4.2 Kết quả

- `# soak: xong wall=219 s (4 boot/s); ok=1000 fail=0 tổng=1000/1000`; `/usr/bin/time`: **real
  220,59 s**; `# soak: 0/1000 lỗi (PASS)`; `# soak.sh: PASS`.
- **BUILD_ID** `e09ec255c17715ae` (FNV-1a 64 trên `.text` đã nạp, kernel **tự in** ở **mọi** boot)
  = hash băm lại từ ELF bằng `test/conformance/image_digest.py`; **sha256 toàn ảnh**
  `509d5adc6695432431c51b9ca59cdcb9105200b5f570e599cf0a5c00645948d7` (90.696 B). 1.000 boot đều
  khớp (script so từng boot; sai ⇒ đỏ).
- 3 luồng thật: log ghi `N=1000 lanes=3`; mỗi boot có tệp riêng `boot-<i>.log`; tổng slot kiểm
  `ok+fail == N` (nếu thiếu slot ⇒ FAIL, không "xanh vì không chạy gì").
- **0/N chỉ là trần** (QD §5.3): 0 lỗi trong 1.000 không bảo đảm p < 0,3 %; báo cáo đúng như vậy.
- Không retry: 1.000 boot chạy một lần; boot đỏ (nếu có) được đếm và fail cả soak (D2-07).

### 4.3 Thay đổi kernel duy nhất — evidence-only, có lý do (D2-80) và số budget trước/sau

- **Đỏ trước** (D2-05): trước khi sửa, `soak.sh --smoke 3 3` đỏ **3/3** boot:
  `image-line=0/1 image-build=none/072cf7807b5d4a94` — ảnh sản phẩm **không** in hash ảnh (chỉ ảnh
  ktest in ở pha conformance KT-55), nên **không thể** neo log soak theo D2-80
  ([`kt57-soak-red.log`](../JOURNAL/kt57-soak-red.log)).
- **Vá 1 dòng**: `v2/kernel/vifast-kbin-riscv64/src/main.rs` — gọi hàm **có sẵn**
  `vifast_plat_riscv64::report_image_digest()` (KT-55) ngay đầu `rust_main`; không đổi ngữ nghĩa,
  thứ tự bring-up, không thêm `unsafe`/dependency, không đổi `stvec`/MMU/timer/SMP.
- **Budget trước → sau** (`JOURNAL/kt57-budget.log` + baseline `kt56-budget.log`): `.data+.bss`
  product **11.248 B → 11.248 B** (không đổi; trần 262.144), file **89.848 → 90.696 B**, `.text`
  **19.302 → 20.210 B**, BUILD_ID product `072cf7807b5d4a94 → e09ec255c17715ae`. Ảnh sản phẩm vẫn
  **0 chuỗi `ktest`** (D2-08; `soak.sh`/`run.sh` kiểm).
- Sau vá: `soak.sh --smoke 3 3` xanh 3/3 (`kt57-soak-smoke-fixed.log`), rồi soak **1000/1000**.
  Thay đổi này nằm trong commit `[KT-57]`; lead đã đồng ý cách làm (2026-11-15).

## 5. D4 — Conformance **5/5** `.scn` × sim/QEMU (điều kiện 4)

`bash v2/test/conformance/run.sh --compare --smp 2` — log
[`kt57-trace-compare.log`](../JOURNAL/kt57-trace-compare.log):

```
# sim trace: 61 dòng → /tmp/k3-kt55/sim-trace.log
scn-check: PASS (26 tệp, 0 blocked, 16 nhóm)
ok - trace-compare: 61/61 dòng khớp, 5/5 ca (22:16 23:19 24:7 25:7 26:12), build=3e6ae37a278300df
```

- **5/5** ca (22 vm-map 16 bước, 23 vm-wx 19, 24 vm-tlb 7, 25 vm-fault 7, 26 vm-shared 12);
  **61/61** dòng trace khớp sau chuẩn hoá; khác biệt ngoài trường cho phép = **0**.
- Log QEMU dùng để so là boot **thật** ảnh `ktest` `smp=2` vừa chạy ở §3; dòng `# conf image:
  build=3e6ae37a278300df` được `trace_compare.py --image` bắt buộc khớp ELF (D2-80) ⇒ không thể
  ghép trace từ nguồn khác.
- Không nới: không sửa `.scn`/kỳ vọng/`expected.txt`; ca 24 trên `-smp 1` vẫn là `steps=0
  skipped=smp1` (so ở `-smp 2`).

## 6. D5 — Budget **≤ 262.144 B** (điều kiện 5); D6 — L1 100k (điều kiện 6); D7 — Mutation (điều kiện 7)

### 6.1 Budget (4 ảnh)

`bash v2/test/boot/budget.sh <ảnh> <nhãn>` — log [`kt57-budget.log`](../JOURNAL/kt57-budget.log):

| Ảnh | `.data+.sdata+.bss+.sbss` (B) | File (B) | `.stack` (báo riêng, NOLOAD) | Trần |
|---|---|---|---|---|
| product | **11.248** | 90.696 | 65.536 | 262.144 |
| ktest | **17.344** | 332.176 | 65.536 | 262.144 |
| ktest-guard | **2.680** | 53.800 | 65.536 | 262.144 |
| ktest-user | **11.752** | 152.048 | 65.536 | 262.144 |

- Cả 4 ảnh ≤ trần; `.stack_guard` 4.096 / `.trap_stack` 16.384 / `.hart_stacks` 602.112 là
  **NOLOAD** → báo riêng, **không** cộng vào trần (D2-72 Q5). Không nới, không tách ngân sách ktest.

### 6.2 Sim L1 100.000 seed × smp 1/2/4/8 (giữ nguyên K2 — D2-65)

Lệnh (chạy **một mình**, không song song gate/build/soak):

```bash
CARGO_PROFILE_TEST_OPT_LEVEL=2 VIFAST_DOI=100000 \
  ./v2/bin/v2 nightly --l1 --seeds 100000 --smp 1,2,4,8
```

- Log [`kt57-l1-100k.log`](../JOURNAL/kt57-l1-100k.log): **400.000 lượt** (100.000 seed × 4 smp,
  2 lần/seed = 800.000 lượt chạy; **51,2 triệu op**), **0 vi phạm** TLB-safety/checker, **0 panic**;
  wall **1.298,81 s** (`real`), `user 1.306,83 s`; `v2 nightly --l1: PASS (1299 s …)`.
- D2-70 min/median in đủ; counters khớp K2 (cùng seed ⇒ cùng trace, byte-for-byte):

| smp | `# L1 OK` | `ipc_sent` | `ipc_delivered` | `ipc_dropped` | VM map | walk | unmap | fault |
|---|---|---|---|---|---|---|---|---|
| 1 | 100.000 × 64 op | 1.212.128 | 226.334 | 782.515 | 257.149 | 71.791 | 135.089 | 237.472 |
| 2 | 100.000 × 64 op | 1.165.183 | 217.375 | 734.940 | 257.080 | 71.601 | 135.157 | 232.545 |
| 4 | 100.000 × 64 op | 1.137.251 | 211.851 | 707.418 | 257.644 | 71.499 | 135.394 | 228.758 |
| 8 | 100.000 × 64 op | 1.125.024 | 209.918 | 693.268 | 257.069 | 71.693 | 135.075 | 226.058 |

- `# L1 OK: 400000 lượt (smp [1, 2, 4, 8], seeds 100000)`. Wall **1.299 s** > K2 (690 s) do máy
  tải/khác thời điểm — **không** có ngưỡng wall cho `nightly --l1`; không giảm seed/op.

### 6.3 Mutation — **4/4 bị giết** (điều kiện 7)

| # | Mutation (một điểm) | Bằng chứng đỏ | Log |
|---|---|---|---|
| 1 | **Bỏ `sfence.vma`** (no-op cả 3 dạng) | TAP 25/26/27/28 đỏ: `local_verify_wrong=50`, peer `prime_wrong=50 verify_wrong=50`, `stale_seen=0` | [`kt51-mutation-1-bay-sfence-qemu.log`](../JOURNAL/kt51-mutation-1-bay-sfence-qemu.log) |
| 2 | **Bỏ chờ ack** (trả ngay) | TAP 25 `ok=0 acked_ok=0`; TAP 27/28/29 `flush=Fail`, `timeout: waited=587 < probe` | [`kt51-mutation-2-bo-cho-ack-qemu.log`](../JOURNAL/kt51-mutation-2-bo-cho-ack-qemu.log) |
| 3 | **Bỏ flush cục bộ** hart gọi (KT-57 chạy lại ở mức kernel) | TAP 25 đỏ + `# shootdown rounds … local_verify_wrong=50`; TAP 28 `fresh_ok=0` | [`kt57-mutation-X-run.log`](../JOURNAL/kt57-mutation-X-run.log), [`…qemu.log`](../JOURNAL/kt57-mutation-X-bo-flush-cuc-bo-qemu.log) |
| 4 | **Bỏ drain hộp thư** khi chờ ack (KT-57 chạy lại ở mức kernel) | TAP 20 `not ok` (`sent=1 delivered=0 traps=1`), TAP 21 `not ok` (`timeout_drains=0`), TAP 23 `not ok`; sau đó treo chờ ack ⇒ runner `TIMEOUT` 20 s (đúng lỗ D2-68), `kill` đúng PID | [`kt57-mutation-Y-run.log`](../JOURNAL/kt57-mutation-Y-run.log), [`…qemu.log`](../JOURNAL/kt57-mutation-Y-bo-drain-hop-thu-qemu.log) |

- KT-57 **không** sửa logic kernel để làm mutation; 2 ca 3/4 chỉnh **tạm** `tlb.rs`/`ipi.rs`, chạy
  `ktest.sh 1 2`, thu log, rồi **khôi phục nguyên văn**: sha256 nguồn khớp trước mutation
  (`tlb.rs e2df0c55…`, `ipi.rs 161f8bfd…`), `git diff --stat -- v2/kernel` chỉ còn **1 tệp
  `main.rs` +4 dòng** (bằng chứng evidence-only §4.3), và **sha256 ảnh sau khôi phục khớp trước**:
  3 ảnh test ở [`…after-mutation.log`](../JOURNAL/kt57-image-sha-after-mutation.log); ảnh **sản
  phẩm** ở dòng 1 [`kt57-image-sha-after-clean-build.log`](../JOURNAL/kt57-image-sha-after-clean-build.log)
  (`509d5adc…`, khớp `kt57-image-sha-before-mutation.log`). Không retry xanh.
- Sau khôi phục: `ktest.sh 1 matrix` lại **9/9** (§3), `selftest 17/17`, trace compare **61/61**,
  soak giữ nguyên ảnh (sha `509d5adc…`).

## 7. D8 — Perf `[ĐO]` (điều kiện 8; **chưa đặt ngưỡng perf**)

Log: [`kt57-perf.log`](../JOURNAL/kt57-perf.log) (tổng hợp), [`kt57-perf-boot.log`](../JOURNAL/kt57-perf-boot.log)
(100 boot, ns), [`kt57-perf-build.log`](../JOURNAL/kt57-perf-build.log).

| Hạng mục | Cách đo | Số đo được |
|---|---|---|
| **Thời gian build** | `cargo clean` toàn workspace kernel rồi build release 4 ảnh; `/usr/bin/time -p` | ảnh sản phẩm **3,35 s**; ktest + ktest-guard **5,51 s**; ktest-user **2,75 s** (tổng 3 lệnh **11,61 s**); ảnh tất định — sha 4 ảnh sau clean-build khớp trước |
| **boot→halt** (wall host) | `perf_counter_ns` quanh tiến trình QEMU, **100 boot** ảnh sản phẩm, không icount | median **389.084.167 ns**, p95 **484.941.500 ns**, min **175.066.000**, max **556.697.500**, mean **366.572.645** |
| **Mốc kernel trong boot** | CSR `time` 10 MHz in ở `# hart=` (entry) và tick cuối trước halt | `entry_ns` median **109.080.000**, p95 **153.396.000**; `kernel_tail_ns` median **182.972.000**, p95 **214.310.000** (cận dưới kernel entry→halt) |
| **Tick drift** | `# timer drift:` dưới `-icount shift=6` (D2-74), **ảnh ktest** × smp 1/2/4 (3 log) | smp1 `min_ns=9.999.900 max_ns=10.020.000`; smp2 `10.000.000/10.019.900`; smp4 `9.999.900/10.020.100`; `late=0 missed=0 non_monotonic=0`, `in_range=1` (ngưỡng [9,11] ms **giữ nguyên**) |
| **IRQ/trap latency thô** | `max_late_ticks` (deadline→handler) dưới icount | **200–201 tick @10 MHz = 20,0–20,1 µs** |
| **IPI/shootdown baseline** | log ảnh ktest smp2/4 | 100/100 IPI `sent=delivered=traps=100 timeout=0`; 100/100 vòng shootdown `ok=100 acked_ok=100 sent_expected=sent`; `wait_ticks` là **thời gian ảo** dưới icount (5,0000138e7–9,9987953e7 tick) — **không** phải wall |
| **Không icount — độ trễ timer thô** | `# timer: delta_ns` ảnh sản phẩm 100 boot | min **14.668.000 ns**, median **177.272.000**, p95 **208.098.000**, max **210.830.000** — đo **main-loop QEMU** (D2-74), không phải toán deadline kernel; không dùng làm ngưỡng |

- Quy ước thống kê (ghi rõ, perf **không ngưỡng**): `median = sorted[n/2]`, `p95 =
  sorted[int(n·0,95)]`, `mean = tổng/n` — là **order statistic**, **không nội suy**; n = 100.
- Mọi số do script/tiến trình đo, trích từ log bằng script (không chép tay — QD §9 Q12).
- Plan K3 §5.2 #8 **không đặt ngưỡng perf**; đây là baseline đầu tiên. Đo jitter/latency thật
  không icount là **task riêng** (D2-74 điều kiện (d)).

## 8. D9 — Tài liệu, tuyên bố phạm vi (điều kiện 9)

- `v2/docs/k3-riscv64.md` **§20** (mới) mô tả `soak.sh`, công thức `n ≥ 3/p`, neo D2-80, cách đo
  perf + giới hạn; đồng thời sửa các chỗ còn **số cũ** (38/5 TAP → **43/7**) ở §4/§5/§17 để tài
  liệu khớp `expected.txt` hiện hành.
- Tuyên bố phạm vi (lặp ở đầu tệp): **K3 = kernel thật** (platform: trap/MMU/SMP/IPI/timer/PLIC/
  U-mode/object layer/conformance) nhưng **chưa có core** (CNode/MDB/TCB/Endpoint/Notification/
  Reply/scheduler/MCS/IPC) — core vẫn **model-only trong sim**, **K3b** (D2-72 Q13) port trước
  K3.5. Vì vậy **không** được đọc K3 là "ABI đầy đủ trên QEMU"; các `.scn` IPC đầy đủ vẫn chạy
  trên sim, không phải QEMU.
- Nợ K3b đầy đủ ở §12; không tô hồng.

## 9. D10 — Đỏ trước + harness + không retry

- **`soak.sh` đỏ trước có log**:
  - `soak.sh 999 3` ⇒ **exit 2**: `N=999 < n ≥ 3/p = 1000` — chứng minh không thể hạ N
    ([`kt57-soak-red-threshold.log`](../JOURNAL/kt57-soak-red-threshold.log));
  - trước dòng evidence §4.3, `soak.sh --smoke 3 3` ⇒ **3/3 boot đỏ** `image-line=0/1` vì thiếu
    `# conf image:` ([`kt57-soak-red.log`](../JOURNAL/kt57-soak-red.log));
  - sau vá: smoke 3/3 xanh (`kt57-soak-smoke-fixed.log`), rồi 1.000 boot.
- **Runner tự kiểm**: `selftest.sh` **17/17** — 1 xanh (không báo động giả) + 15 đỏ đúng token +
  1 `expected.txt` nguyên vẹn (`kt57-selftest.log`).
- **Không retry**: soak/matrix/L1/gate chạy một lần; không `#[ignore]`/`#[should_panic]`; không
  sửa kỳ vọng/`.scn`/`expected.txt`; mutation có log đỏ **trước** khi khôi phục.

## 10. D11 — Phạm vi, commit, push

- `git diff --stat -- spec/` **rỗng**; không đụng `v2/agent/QUEUE.md`, `DECISIONS.md`, `AGENTS.md`,
  `viscript/`, `docs/v1/`; không thêm dependency/`unsafe` ngoài danh sách đóng (D2-17); không thêm
  `cfg(target_arch)`.
- Tệp thay đổi (chỉ `v2/`):
  - `v2/test/boot/soak.sh` (**mới**, 322 dòng — KT-57);
  - `v2/kernel/vifast-kbin-riscv64/src/main.rs` (**+4 dòng evidence-only**, D2-80 — §4.3);
  - `v2/docs/k3-riscv64.md` (§20 + sửa số TAP cũ 38/5 → 43/7);
  - `v2/agent/reports/K3-DOD.md` (tệp này), `v2/agent/JOURNAL/kt57-*` (log + journal),
    `v2/agent/STATE.md`.
- Commit `[KT-57]` (sản phẩm + log + docs) và `[KT-57b]` (STATE/kết quả cuối sau review) — hash
  ghi ở `STATE.md`/`JOURNAL/kt57-journal.md` sau khi commit; push `origin/main` sau mỗi commit
  xanh. `git rev-list --count HEAD..origin/main` kiểm **trước** push.

## 11. Bảng DoD D1..D11 (đo được, không nới)

| # | Tiêu chí | Cách đo | Ngưỡng | Kết quả |
|---|---|---|---|---|
| D1 | `gate --fast` | `./v2/bin/v2 gate --fast` | xanh, 0 failed, L1 1.000 seed | **ĐẠT** — 574/75, L1 824 ms (§2) |
| D2 | TAP matrix | `ktest.sh 1 matrix` + so `expected.txt` | 100 % × 3 ảnh × smp 1/2/4 | **ĐẠT** — 9/9, 43/3/7 (§3) |
| D3 | Soak | `soak.sh 1000 3`, không icount | **0/1000**, 3 luồng, 20 s/boot, không retry | **ĐẠT** — 1000/1000, wall 219 s (§4) |
| D4 | Conformance | `run.sh --compare --smp 2` | 5/5 `.scn`, khác biệt = 0 | **ĐẠT** — 61/61 dòng, 5/5 ca (§5) |
| D5 | Budget | `budget.sh` 4 ảnh | `.data+.bss ≤ 262.144 B` | **ĐẠT** — 11.248/17.344/2.680/11.752 (§6.1) |
| D6 | L1 100k | `nightly --l1 --seeds 100000 --smp 1,2,4,8` | 0 vi phạm | **ĐẠT** — 400.000 lượt, 0 vi phạm (§6.2) |
| D7 | Mutation | 4 mutation §5.2 #7 | 4/4 bị giết, khôi phục | **ĐẠT** — 4/4 (§6.3) |
| D8 | Perf `[ĐO]` | script đo + log QEMU | có số, **không ngưỡng** | **ĐẠT** — §7 |
| D9 | Tài liệu/Q13 | K3-DOD + §20 + STATE | K3 = platform kernel thật, chưa core; nợ K3b | **ĐẠT** — §8/§12 |
| D10 | Đỏ trước/harness | log đỏ + `selftest.sh` | đỏ đúng lý do; 17/17 | **ĐẠT** — §9 |
| D11 | Phạm vi/commit | `git diff`/`git status` + push | chỉ `v2/`, spec rỗng | **ĐẠT** — §10 (hash ở STATE) |

## 12. Nợ **K3b** — có chủ, không tô hồng

1. **Core chưa port `no_std`** (D2-72 Q13): CNode/MDB/Untyped, TCB/transition/scheduler/MCS,
   Endpoint/Notification/Reply/IPC/WaitAddr/fault — vẫn **model-only trong sim**. K3b = KT-58
   (CNode/MDB/Untyped), KT-59 (TCB/sched), KT-60 (IPC/Reply/Notification/WaitAddr/fault), KT-61
   (cổng K3b: TAP + soak + conformance trên nhân thật). K3 **không** chứa core.
2. **ABI chưa đầy đủ trên QEMU**: `Busy`/`FaultKind::Vm` vẫn model-only ở sim (D2-64/D2-65 Q6);
   `UnknownSyscall` chưa có nguồn phần cứng (D2-79) — procsrv K3.5 mới tạo.
3. **Pager/COW/shared memory thật**: K3 chưa có memory server, không cấp trang khi fault; shared
   read-only mới ở mức mô hình (K2) + ca conformance 26 (test-only).
4. **U-mode đa hart / procsrv / shell**: ảnh user K3 chạy 1 tác vụ 1 hart; K3.5 mới có procsrv/shell.
5. **Perf chưa có ngưỡng**: K3 chỉ baseline; đo jitter/latency thật **không icount** (IPI/shootdown/
   timer) là task riêng (D2-74 điều kiện (d)); `boot→halt` hiện là wall host gồm QEMU/OpenSBI.
6. **Miri không áp kernel `no_std`** (D2-72 Q12) — miễn trừ **có lý do** (asm/MMIO/S-mode); Miri
   vẫn chạy cho `lib/`/sim; kernel kiểm bằng build + TAP QEMU.
7. **Một số mutation chỉ là chứng cứ cộng dồn**: KT-51 M1/M2 chạy ở KT-51; KT-57 chạy lại 2 ca
   còn thiếu ở mức kernel (§6.3). Mutation 4 (bỏ drain) đỏ bằng `not ok 20/21/23` rồi **treo tới
   timeout 20 s** — đúng lỗ D2-68, nhưng không phải assert-only; ghi rõ thay vì overclaim.
8. **AIA/MSI, Sstc, hotplug hart, migration, preemption thật** hoãn (D2-72 Q6/Q8; K4'/K5'/K3b).
9. **`mmu::map_scratch_4k`** (KT-56, test-only) vẫn là đường ghi bảng trang thứ hai **bỏ qua** kế
   toán object layer — chỉ trong ảnh test, gỡ ngay sau probe (nợ đã ghi ở KT-56 §19.7).
10. **Soak chỉ 1 hart ảnh sản phẩm**: matrix TAP phủ smp 1/2/4 với ảnh test; soak 4-hart sản phẩm
    mới có 50 boot/cấu hình (KT-49/56) — soak SMP đầy đủ thuộc K3b/K3.5 khi core chạy thật.
11. **`FaultKind.vspace` là nhãn ASID, không phải cap** (`k3-riscv64.md` §19.7(5), dòng 2145):
    procsrv K3.5 cần cap thật thì mở rộng bằng D2 mới, **không** tự đổi `spec/`.
12. **Frame 1 GiB chưa map được** (`NotSupported`; `k3-riscv64.md` §15.10 mục L7, dòng 1361): cửa
    sổ PA là **nhãn** trùm RAM thật `-m 128M`; TAP 32 chỉ kiểm nhãn/căn/cửa sổ. Mở ở K4/K5 khi có
    backing thật (nhiều RAM hơn) hoặc vùng PA riêng ngoài RAM.
13. **`USER_RANGES` thay `walk`** (`k3-riscv64.md` §16.8(16), dòng 1576): sau khi đổi `satp`,
    handler U-mode không có physmap nên kiểm con trỏ bằng **bảng dải tĩnh**; loader map thêm trang
    ngoài segment/stack (tương lai) phải cập nhật bảng; `DebugGetInfo` (15) **chưa cài**.
14. **Sau `Exit`, `satp` vẫn root user + `SIE=0`** (`k3-riscv64.md` §16.8(15), dòng 1574): ảnh test
    kết thúc bằng fault nên chấp nhận; K3.5 phải chuyển về root kernel/TCB idle trước khi chạy
    tiếp — đường này fragile nếu tái dùng.
15. **TLB per-hart model-only trong conformance** (`k3-riscv64.md` §18.2/§18.9(3), dòng 1878/2012):
    các bước `vm_tlb_*` bị **loại** khỏi tập conformance vì là mô hình K2; TLB **thật** có TAP
    25..29 (KT-51) nhưng **không** được phủ bởi 5 `.scn` conformance. Ghi rõ, không overclaim.

## 13. Review độc lập (`reviewer-kt57`, lead spawn, read-only)

**Verdict: DRIFT mức THẤP — cổng đạt thật, không nới ngưỡng** (reviewer tự kiểm **cả 1.000 log**:
`bad=0 not_ok=0 panic=0`, mỗi file đúng `build=e09ec255c17715ae text=20210`, 1.000 id duy nhất
khớp `seq 1 1000`, mtime trải đúng **219 s**, `user 362 s > real 220 s` ⇒ 3 luồng thật, patch sha
`71318083` khớp `git diff v2/kernel`; `soak.sh 999` exit 2; matrix 9/9; L1 400.000/0; mutation 4/4
đỏ đúng lý do, M-Y `not ok` + timeout 20 s **không overclaim**; +4 dòng evidence-only đúng phạm vi;
product 0 `ktest`; budget trước = sau). Các mục vá **tài liệu** đã áp trước commit (không chạy lại
soak/L1):

| Mục | Vấn đề | Vá |
|---|---|---|
| **F1** (LOW–MED, bắt buộc) | `K3-DOD` khai đã sửa 38/5 → 43/7 nhưng `k3-riscv64.md` còn 38/5 ở nhiều chỗ (§4 bảng ảnh, §5, §17.2/§17.5/§17.6/§17.7) — tài liệu **tự mâu thuẫn** | sửa hết: bảng ảnh **43 / 3 / 7**, user table **7 TAP** (thêm TAP 6/7, TAP 5 recover), `plan-1..43=0`, `ktes 43 43`, `TAP 38 → 43`, §17.5 heading 43; thêm dòng evidence `# conf image:` vào bảng ảnh sản phẩm |
| **F2** | “soak.sh 335 dòng” | **322** dòng (`wc -l`) |
| **F3** | “tick drift 3 ảnh × smp” | thực tế **ảnh ktest** × smp 1/2/4 (3 log) |
| **F4** (bắt buộc) | §12 thiếu 5 nợ đã có trong docs | thêm **11..15**: `FaultKind.vspace` nhãn ASID (dòng 2145); Frame 1 GiB `NotSupported`; `USER_RANGES` thay `walk`; sau `Exit` `satp` root user + `SIE=0`; TLB per-hart model-only trong conformance |
| **F5** | “sha 4 ảnh before/after” — log after-mutation chỉ có **3 ảnh test** | sửa trích dẫn: 3 ảnh test ở `…after-mutation.log`; ảnh sản phẩm ở dòng 1 `kt57-image-sha-after-clean-build.log` (`509d5adc…`) |
| **F6** | median/p95 là order statistic | ghi quy ước `median=sorted[n/2]`, `p95=sorted[int(n·0,95)]`, **không nội suy**, n = 100 |
| **F7** | `k3:2237` trỏ “nợ ở K3-DOD §8” | đúng **§12** |
| **T-INFRA** | K3-PLAN §5.2 ghi `gate --fast < 60 s` nhưng wall **280,36 s** (lần chạy cuối; baseline trước KT-57 **280,61 s**) | ghi rõ **T-INFRA môi trường**; **60 s chỉ áp cho L1** (824 ms) — **không nới ngưỡng** |

- Các mục reviewer nêu là “chưa chứng minh” được **giữ nguyên** ở §12 (soak chỉ 1 hart; perf chỉ máy
  này; “L1 chạy một mình” suy từ timeline; mutation 4/4 là **cộng dồn** M1/M2 KT-51 + X/Y KT-57) —
  không viết lại thành mạnh hơn.
- Tự kiểm trước commit: `git diff --stat -- spec/` rỗng; `v2/` chỉ các tệp ở §14; không retry; mọi
  số trong tệp này trích từ log trong `JOURNAL/`.

## 14. Tệp chạm (KT-57)

- `v2/test/boot/soak.sh` — **mới**: soak 1.000 × 3 luồng, timeout 20 s, neo D2-80, từ chối N <
  `n ≥ 3/p`, không retry.
- `v2/kernel/vifast-kbin-riscv64/src/main.rs` — **+4 dòng**: gọi `report_image_digest()` (hàm có
  sẵn KT-55) đầu `rust_main` ảnh sản phẩm (D2-80; evidence-only — §4.3).
- `v2/docs/k3-riscv64.md` — §20 + sửa số TAP cũ 38/5 → **43/7**.
- `v2/agent/reports/K3-DOD.md` (tệp này), `v2/agent/JOURNAL/kt57-*`, `v2/agent/STATE.md`.
- **Không** sửa: `spec/`, sim semantics, `.scn`, `expected.txt`, `.engine`, `viscript/`, QUEUE/
  DECISIONS/AGENTS, `docs/v1/`.
