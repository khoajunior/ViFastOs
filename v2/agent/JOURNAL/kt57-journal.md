# KT-57 — cổng K3: soak 1.000 × 3 luồng, matrix, 100k L1, perf `[ĐO]`, K3-DOD (K3)

Người làm: **Agent 2 dòng OS** (phiên KT-57). HEAD vào: `d543352d` (`[KT-00]` lật KT-56 DONE +
mở KT-57; parent code KT-56 `25251719`). Working tree có `viscript/` + `review/` của tab khác —
**không chạm**; chỉ stage `v2/`. `git rev-list --count HEAD..origin/main` = **0** trước khi làm.

Nguồn: `review/agent0/K3-PLAN.md` KT-57 (§4.1) + **§5.2** (9 điều kiện cổng K3, không nới) +
§5.3; **D2-24** (máy `ps`, N = 1.000, 3 luồng, `n ≥ 3/p`, p = 0,3 %), **D2-72 Q10** (`soak.sh
1000 3`, timeout 20 s/boot, 0/1000, không retry), **D2-74** (soak **không** icount), **D2-80**
(log neo hash ảnh do kernel tự in), **D2-72 Q5** (budget), **D2-07** (không retry), **D2-72 Q13**
(K3b); `v2/agent/reports/K2-DOD.md` (mẫu D1..D11).

## 1. Baseline

- `./v2/bin/v2 gate --fast` → **PASS 574 test / 75 suite**, L1 1.000 seed **823 ms**, wall
  **280,61 s** ([`kt57-baseline-gate.log`](kt57-baseline-gate.log)).
- Ảnh vào task: product 89.848 B / `.data+.bss` 11.248; ktest 332.176 / 17.344; guard 53.800 /
  2.680; user 152.048 / 11.752 (KT-56). Máy QEMU 11.1.3 + OpenSBI v1.8.1.

## 2. Đỏ trước (D2-05) — `soak.sh` chưa có + ngưỡng sai + thiếu marker

1. `v2/test/boot/soak.sh` **chưa tồn tại** → viết mới (luật D2-24/D2-80 không thể "đã xanh").
2. `soak.sh 999 3` → **exit 2**: `N=999 < n ≥ 3/p = 1000` (p = 3/1000) — không có đường hạ N
   ([`kt57-soak-red-threshold.log`](kt57-soak-red-threshold.log)).
3. **Trước** khi thêm dòng evidence §3.1: `soak.sh --smoke 3 3` → **đỏ 3/3** boot:
   `image-line=0/1 image-build=none/072cf7807b5d4a94` — ảnh sản phẩm **không** tự in hash
   (D2-80); 1.000 boot sẽ không neo được ([`kt57-soak-red.log`](kt57-soak-red.log)).
4. Sau vá: `--smoke 3 3` xanh 3/3 ([`kt57-soak-smoke-fixed.log`](kt57-soak-smoke-fixed.log)).

## 3. Thay đổi

### 3.1 Kernel — **1 dòng evidence-only** (D2-80), lead đã đồng ý

`v2/kernel/vifast-kbin-riscv64/src/main.rs`: gọi `vifast_plat_riscv64::report_image_digest()` (hàm
**có sẵn** KT-55) ngay đầu `rust_main` ⇒ mỗi boot sản phẩm in
`# conf image: build=<16 hex> text=<len>` (FNV-1a 64 `.text` đã nạp). Không đổi ngữ nghĩa/thứ tự
bring-up, không thêm `unsafe`/dependency. Budget: product `.data+.bss` **11.248 B trước = sau**;
file 89.848 → 90.696; `.text` 19.302 → 20.210; BUILD_ID `072cf7807b5d4a94` → `e09ec255c17715ae`.
`soak.sh` băm lại đúng ảnh sản phẩm bằng `test/conformance/image_digest.py`; log thiếu/lệch ⇒ đỏ.

### 3.2 `v2/test/boot/soak.sh` (mới)

- `soak.sh [NBOOT] [LANES]` — mặc định **1000 3**; `--smoke N` chỉ để đỏ trước/kiểm marker (luôn
  exit 1); `--help`. `LANES` ngoài `[1,3]` chặn (AGENTS §9).
- Công thức in log: p = 3/1000 (0,3 %) ⇒ `n ≥ 3/p = ceil(3·1000/3) = 1000` (tính số nguyên).
  `N < 1000` ⇒ exit 2.
- Mỗi boot: build ảnh sản phẩm một lần (budget + D2-08 `ktest-strings=0` + sha256 + BUILD_ID),
  rồi boot `-machine virt,aia=none,aclint=on -cpu rv64 -m 128M -smp 1 -no-reboot`, **KHÔNG**
  icount, timeout **20 s**, kill **đúng** PID; kiểm 6 bất biến (TAP/plan/ok/halt; `# k3
  image=product`; `# conf image:` khớp ELF; `# smp: … harts=1 … timeout=0` + `Platform HART Count
  : 1`; không PANIC); 3 luồng; tổng slot phải = N; ảnh đổi giữa soak ⇒ FAIL; không retry.
- Kết quả: **1000/1000**, 0 lỗi, wall **219 s** (real 220,59 s), 3 luồng,
  BUILD_ID `e09ec255c17715ae`, sha256 `509d5adc…` — [`kt57-soak.log`](kt57-soak.log),
  [`kt57-soak-results.txt`](kt57-soak-results.txt) (1.000 dòng `ok i`), mẫu boot
  [`kt57-soak-boot1.log`](kt57-soak-boot1.log).

### 3.3 Docs

`v2/docs/k3-riscv64.md`: **§20** (soak.sh/công thức/neo ảnh/perf/giới hạn) + sửa số TAP cũ
38/5 → **43/7** ở §4/§5/§17; `v2/agent/reports/K3-DOD.md` (mới, D1..D11); STATE.

## 4. Cổng K3 §5.2 — kết quả (log trong `JOURNAL/`)

| # | Điều kiện | Kết quả | Log |
|---|---|---|---|
| 1 | `gate --fast` | **574/75**, 0 failed, L1 824 ms (lần cuối; lần trước 743 ms) | [`kt57-gate.log`](kt57-gate.log) |
| 2 | TAP matrix | **9/9** boot: 43/43 + 3/3 + 7/7; selftest **17/17** | [`kt57-ktest-matrix.log`](kt57-ktest-matrix.log), [`kt57-selftest.log`](kt57-selftest.log) |
| 3 | Soak | **0/1000**, 3 luồng, 20 s/boot, không icount, wall 219 s | [`kt57-soak.log`](kt57-soak.log) |
| 4 | Conformance | **61/61 dòng, 5/5 ca**, `build=3e6ae37a278300df` | [`kt57-trace-compare.log`](kt57-trace-compare.log) |
| 5 | Budget | product **11.248** / ktest **17.344** / guard **2.680** / user **11.752** B | [`kt57-budget.log`](kt57-budget.log) |
| 6 | L1 100k | **400.000 lượt, 0 vi phạm**, wall **1.299 s** | [`kt57-l1-100k.log`](kt57-l1-100k.log) |
| 7 | Mutation | **4/4** — KT-51 M1/M2 + KT-57 X (bỏ flush cục bộ), Y (bỏ drain hộp thư) | [`kt57-mutation-X-run.log`](kt57-mutation-X-run.log), [`kt57-mutation-Y-run.log`](kt57-mutation-Y-run.log) |
| 8 | Perf `[ĐO]` | build 3,35+5,51+2,75 s; boot→halt median **389.084.167 ns**; drift 9.999.900–10.020.100 ns; late **20,0–20,1 µs** | [`kt57-perf.log`](kt57-perf.log) |
| 9 | Docs/Q13 | K3-DOD + §20 + STATE; K3 = platform kernel thật, **chưa core** — K3b | `K3-DOD.md` |

## 5. Mutation 4/4 — 2 ca chạy lại ở mức kernel

- **X — bỏ flush cục bộ hart gọi** (`tlb::shootdown`, một điểm): TAP 25 đỏ
  (`local_verify_wrong=50`), TAP 28 đỏ (`fresh_ok=0`) — [`kt57-mutation-X-run.log`](kt57-mutation-X-run.log),
  [`…qemu.log`](kt57-mutation-X-bo-flush-cuc-bo-qemu.log).
- **Y — bỏ drain hộp thư** (`ipi::drain_hart_current`, một điểm): TAP 20/21/23 `not ok`
  (`sent=1 delivered=0`, `timeout_drains=0`), rồi treo chờ ack ⇒ runner `TIMEOUT` 20 s, kill đúng
  PID (đúng lỗ D2-68) — [`kt57-mutation-Y-run.log`](kt57-mutation-Y-run.log),
  [`…qemu.log`](kt57-mutation-Y-bo-drain-hop-thu-qemu.log).
- Khôi phục nguyên văn: sha nguồn khớp (`tlb.rs e2df0c55…`, `ipi.rs 161f8bfd…`);
  `git diff --stat -- v2/kernel` chỉ còn `main.rs +4` (evidence-only); sha 4 ảnh sau khôi phục
  **khớp** trước mutation ([`kt57-image-sha-before-mutation.log`](kt57-image-sha-before-mutation.log),
  [`…after-mutation.log`](kt57-image-sha-after-mutation.log)); matrix lại **9/9**.
- Không retry xanh; mutation X/Y là **sửa tạm + khôi phục**, không commit.

## 6. Perf `[ĐO]` (không ngưỡng — plan K3 §5.2 #8)

- **Build**: `cargo clean` workspace kernel rồi build release 4 ảnh — 3,35 s (product) + 5,51 s
  (ktest+guard) + 2,75 s (user); ảnh **tất định** (sha sau clean-build khớp trước) —
  [`kt57-perf-build.log`](kt57-perf-build.log).
- **boot→halt**: 100 boot ảnh sản phẩm, wall host `perf_counter_ns` — median **389.084.167 ns**,
  p95 **484.941.500**, min **175.066.000**, max **556.697.500**; thêm mốc kernel `entry_ns`
  (CSR `time`) và `kernel_tail_ns` — [`kt57-perf-boot.log`](kt57-perf-boot.log).
- **Tick drift** (icount, D2-74): `min_ns` 9.999.900–10.000.000, `max_ns` 10.019.900–10.020.100,
  `late=0 missed=0 non_monotonic=0`, `in_range=1` (ngưỡng [9,11] ms **giữ nguyên**).
- **IRQ/trap latency thô**: `max_late_ticks` **200–201 @10 MHz = 20,0–20,1 µs** (deadline→handler,
  icount). Không icount `delta_ns` 14,7–210,8 ms là **main-loop QEMU** (D2-74) — không ngưỡng.
- **IPI/shootdown**: 100/100 delivered, 100/100 vòng `Ok`; `wait_ticks` là **thời gian ảo** dưới
  icount — không phải wall (đo latency thật: task riêng, D2-74(d)).
- Tổng hợp: [`kt57-perf.log`](kt57-perf.log).

## 7. Review độc lập (`reviewer-kt57`)

- Verdict: **DRIFT mức THẤP — cổng đạt thật, không nới ngưỡng**. Reviewer tự kiểm **cả 1.000 log**
  (`bad=0 not_ok=0 panic=0`; mỗi file đúng `build=e09ec255c17715ae text=20210`; 1.000 id duy nhất
  `seq 1 1000`; mtime trải đúng 219 s; `user 362 s > real 220 s` ⇒ 3 luồng thật; patch sha
  `71318083` khớp `git diff v2/kernel`); `soak.sh 999` exit 2; matrix 9/9; L1 400.000/0; mutation
  4/4 đỏ đúng lý do (M-Y `not ok` + timeout 20 s, **không** overclaim); +4 dòng evidence-only đúng
  phạm vi; product 0 `ktest`; budget trước = sau.
- Vá **tài liệu** trước commit (không chạy lại soak/L1):
  - **F1** (bắt buộc): `k3-riscv64.md` còn số cũ 38/5 ở §4/§5/§17 (bảng ảnh, §17.2, §17.5 heading
    user, §17.6 `plan-1..38`, §17.4 `ktes 38 38`, `TAP 38` của trap lồng) — sửa hết **43/7**;
    thêm dòng `# conf image:` vào bảng ảnh sản phẩm; user table thêm TAP 6/7 (TAP 5 recover).
  - **F2**: “335 dòng” → **322** (`wc -l`).
  - **F3**: “tick drift 3 ảnh × smp” → **ảnh ktest** × smp 1/2/4 (3 log).
  - **F4** (bắt buộc): §12 thêm 5 nợ — `FaultKind.vspace` nhãn ASID; Frame 1 GiB `NotSupported`;
    `USER_RANGES` thay `walk`; sau `Exit` `satp` root user + `SIE=0`; TLB per-hart model-only
    trong conformance (kèm dòng docs hiện hành 2145/1361/1576/1574/1878-2012).
  - **F5**: “sha 4 ảnh after” → 3 ảnh test ở `kt57-image-sha-after-mutation.log`, product ở dòng 1
    `kt57-image-sha-after-clean-build.log`.
  - **F6**: median/p95 là **order statistic** (`sorted[n/2]`, `sorted[int(n·0,95)]`), không nội
    suy, n = 100; perf **không ngưỡng**.
  - **F7**: `k3:2237` trỏ nợ đúng **§12**.
  - **T-INFRA**: wall `gate --fast` **280,36 s** (baseline **280,61 s**) > câu “< 60 s” của K3-PLAN
    §5.2 ⇒ ghi rõ **60 s chỉ áp cho L1** (824 ms lần cuối), wall là **T-INFRA môi trường** (K2-DOD §8.6) —
    **không nới**.
- Giữ nguyên các mục “chưa chứng minh” reviewer nêu (soak chỉ 1 hart; perf chỉ máy này; “L1 chạy
  một mình” suy từ timeline; mutation 4/4 **cộng dồn** M1/M2 KT-51 + X/Y KT-57).
- Tự kiểm: `spec/` rỗng; `v2/` chỉ soak.sh + `main.rs` +4 + docs/reports/log/STATE; không retry;
  mọi số trong K3-DOD trích từ log trong `JOURNAL/`.

## 8. Nợ K3b (tóm tắt — chi tiết `K3-DOD.md` §12)

1. Core CNode/MDB/TCB/Endpoint/Notification/Reply/scheduler/MCS/IPC **chưa port `no_std`** —
   model-only trong sim, **K3b** KT-58..KT-61 (D2-72 Q13).
2. ABI chưa đầy đủ trên QEMU (`Busy`/`FaultKind::Vm` model-only; `UnknownSyscall` chưa có nguồn
   phần cứng — K3.5).
3. Pager/COW/shared memory thật; U-mode đa hart; procsrv/shell (K3.5).
4. Perf chưa ngưỡng; đo jitter/latency thật không icount là task riêng (D2-74(d)).
5. Miri không áp kernel `no_std` (D2-72 Q12) — miễn trừ có lý do; vẫn áp `lib/`/sim.
6. AIA/MSI, Sstc, hotplug, migration, preemption thật; soak SMP sản phẩm đầy đủ (K3b/K3.5).
7. Mutation 4 (bỏ drain) đỏ có `not ok` **và** treo tới timeout 20 s — đúng lỗ D2-68 nhưng không
   phải assert-only (ghi rõ, không overclaim).

## 9. Kết quả cuối

- Cổng K3 §5.2: **9/9 ĐẠT**, không nới ngưỡng; KT-57 and K3 **DONE (đề xuất)** — chờ reviewer +
  Agent 0 lật QUEUE (D2-37).
- Commit `[KT-57]` + `[KT-57b]` (hash ghi ở STATE sau commit), push `origin/main`; worktree
  `viscript/` của tab khác **không chạm**.
