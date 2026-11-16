<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# KT-76 — nhật ký (cổng K6: `K6-DOD` "tương đương v1" + soak + L1 `seeds_total=400000`)

Người ghi: **Agent 2 dòng OS** · Ngày: **2026-11-17** · HEAD khi mở: `3776689d` (`[KT-00]` D2-117)
· Nhánh `main` · Máy `ps` (mười hai nhân, arm64) · QEMU (phiên bản do script in — `kt76-build-sha.log:6`).

## 1. Vào task

- `KT-76` = **READY** trong QUEUE (dòng K6 cuối); phụ thuộc `KT-75` **DONE**. Đọc D2-117 (nợ cho
  KT-76), D2-116 (QEMU orphan), D2-115/D2-113 (ảnh sản phẩm **không** U-mode/shell), D2-112
  (verify số theo scope, **cấm** ngoại lệ chữ số), D2-109 (số đo phải nằm trong log **đã ở repo**),
  D2-108A (hash từ build **sạch**), D2-108B (bộ đếm ca âm), D2-105 (verify HEAD bằng worktree sạch),
  D2-84/85 (pin/trần ảnh — không đổi).
- Baseline `./v2/bin/v2 gate --fast` **PASS**; ảnh **sản phẩm** **không feature** (D2-108A) **trùng
  byte** KT-75 ⇒ soak/conformance chạy trên đúng ảnh đã neo. **NHƯNG** build ghi trong log là cargo
  **tái dùng** artifact (0.03s, không compile lại); clean-build `git archive` cho **sha khác** ⇒
  **tái lập byte chưa chứng minh** (nợ **P20**, có log `kt76-clean-build.log`).
- **QEMU orphan (D2-116)**: trước chuỗi `qemu_ppid1=0`; lúc build (giờ máy `07:08`) đếm được **hai**
  tiến trình QEMU **không** phải của tôi (không `ppid=1` — của tab khác) ⇒ ghi lại trong
  `kt76-orphan.log`; không dùng số **thời gian** của lần đo nào trong lúc đó để kết luận hiệu năng.

## 2. Thứ tự làm — DOD trước, bằng chứng sau (đúng yêu cầu)

| Bước | Giờ máy | Việc | Ghi chú |
|---|---|---|---|
| b1 | `07:10` | trích **nguyên văn** tiêu chí từ `docs/v2/ViFastOs-v2-thiet-ke.md` §10.1 + `docs/v1/KNOWN-ISSUES.md` ⇒ `kt76-criteria.txt` | **trước** khi có bất kỳ kết quả cổng nào; script `kt76-extract-criteria.sh` |
| b2 | `07:10` | viết khung `K6-DOD.md` (bảng C1..C8, N1..N7, dòng S1) với bằng chứng **để trống** | tiêu chí **không** suy từ kết quả |
| b3 | `07:08`–`07:21` | chạy cổng nặng: build sạch → gate → matrix → soak → persist → conformance | không retry (D2-07) |
| b4 | `07:21`–`07:56` | L1 **bốn cấu hình song song** | máy không chạy QEMU trong lúc L1 để wall sạch |
| b5 | sau L1 | điền bằng chứng `log:dòng`, chạy verify + tự phá trên **bản copy**, freeze | D2-112/D2-113F |

**Ghi chú đo L1 (D2-100/D2-116)**: trong lúc L1 chạy, **không** có QEMU nào của task (kiểm bằng
`ps`), chỉ có việc host nhẹ (kiểm `strings`/`budget`, tự kiểm harness, kiểm orphan); wall L1 in ở
khối dưới, **không** dùng để kết luận hiệu năng (L1 bất biến theo seed).

## 3. Cổng — số nguyên văn (log trong repo)

```text scope=kt76-build-sha.log
# KT-76 — baseline: HEAD + máy + ảnh SẢN PHẨM build SẠCH (KHÔNG feature, D2-108A)
# HEAD=3776689d
# head_subject=[KT-00] D2-117: KT-75 DONE (persist qua reboot that, flush ca am dung tang); mo KT-76 cong K6
# origin_main_delta=0
# date_utc=2026-11-16T22:08:21Z
# qemu=QEMU emulator version 11.1.3
# opensbi: OpenSBI tự in trong serial boot — `kt76-product-serial.log:4` (không chép tay)
# cpu=12 host=arm64 os=27.0.1
# qemu_orphan_before=2
# cargo=cargo 1.99.1 (5f94df478 2026-08-27)
# lệnh: cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-kbin-riscv64 --bin vifast-kbin-riscv64 --target riscv64gc-unknown-none-elf --release
    Finished `release` profile [optimized] target(s) in 0.03s
BUILD_EXIT=0
# bytes=188552
fd20227658832a3718b390b0cd7aab563396e23e01ecbd04632f7a9c4393a6da  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
build=ef566cfa583e34bb text=49874
```

```text scope=kt76-baseline-gate-summary.log
TESTS=729 / SUITES=100
```

```text scope=kt76-soak.log
# soak: xong wall=227s (4 boot/s); ok=1000 fail=0 tổng=1000/1000
# soak: BUILD_ID=ef566cfa583e34bb sha256=fd20227658832a3718b390b0cd7aab563396e23e01ecbd04632f7a9c4393a6da git=c9249b7989a5978a8010faf96121e3b608606bec
# soak: 0/1000 lỗi (PASS)
# soak.sh: PASS
SOAK_EXIT=0
```

```text scope=kt76-persist-run.log
# persist: phases=7 ok=7 boots=10 neg=1 host_checks=10
# reboot: boots=10 pids=10 phase_reboots=7 (7 pha; mỗi pha ghi ở một tiến trình, đọc lại ở tiến trình KHÁC)
# persist.sh: PASS
PERSIST_EXIT=0
```

```text scope=kt76-conformance.log
ok - trace-compare: 61/61 dòng khớp, 5/5 ca (22:16 23:19 24:7 25:7 26:12), build=7304a40491224327
# trace-compare PASS (/tmp/k3-kt55/sim-trace.log ↔ /tmp/k3-ktest/ktest-smp2-1.log)
CONF_EXIT=0
```

```text scope=kt76-ktest-matrix.log
# KTEST: 21/21 boot sạch (smp=matrix, NBOOT=1, ảnh/boot=ktest ktest-guard ktest-user ktest-proc ktest-vblk ktest-con ktest-exec)
# ktest.sh: PASS
MATRIX_EXIT=0
```

```text scope=kt76-selftest.log
# selftest: 18/18 ca đúng như kỳ vọng
# selftest.sh: PASS
SELFTEST_EXIT=0
```

```text scope=kt76-l1-timing.txt
# KT-76 — L1 400.000 lượt: bốn cấu hình smp 1/2/4/8 × 100.000 seed × 64 op, BỐN tiến trình SONG SONG
# lệnh mỗi cấu hình: ./v2/bin/v2 nightly --l1 --seeds 100000 --smp <N>  (log riêng kt76-l1-smp<N>.log, có EXIT=…)
# qemu_orphan_before=0
start_utc=2026-11-16T22:21:20Z
end_utc=2026-11-16T22:56:09Z
wall_s=2089
# qemu_orphan_after=0
# xong: mỗi log phải có '# L1 OK: 100000 lượt' + 'EXIT=0'
```

```text scope=kt76-l1-smp1.log
# L1 OK: smp 1, 100000 seed × 64 op (2000 seed đầu chạy 2 lần; ipc_min 2, ipc_median 12; ipc_sent 1212128, ipc_delivered 226334, ipc_dropped 782515; vm map 257149, walk 71791, unmap 135089, fault 237472, shootdown 135089)
# L1 OK: 100000 lượt (smp [1], seeds 100000)
EXIT=0
```

```text scope=kt76-l1-smp2.log
# L1 OK: smp 2, 100000 seed × 64 op (2000 seed đầu chạy 2 lần; ipc_min 1, ipc_median 12; ipc_sent 1165183, ipc_delivered 217375, ipc_dropped 734940; vm map 257080, walk 71601, unmap 135157, fault 232545, shootdown 135157)
# L1 OK: 100000 lượt (smp [2], seeds 100000)
EXIT=0
```

```text scope=kt76-l1-smp4.log
# L1 OK: smp 4, 100000 seed × 64 op (2000 seed đầu chạy 2 lần; ipc_min 1, ipc_median 11; ipc_sent 1137251, ipc_delivered 211851, ipc_dropped 707418; vm map 257644, walk 71499, unmap 135394, fault 228758, shootdown 135394)
# L1 OK: 100000 lượt (smp [4], seeds 100000)
EXIT=0
```

```text scope=kt76-l1-smp8.log
# L1 OK: smp 8, 100000 seed × 64 op (2000 seed đầu chạy 2 lần; ipc_min 1, ipc_median 11; ipc_sent 1125024, ipc_delivered 209918, ipc_dropped 693268; vm map 257069, walk 71693, unmap 135075, fault 226058, shootdown 135075)
# L1 OK: 100000 lượt (smp [8], seeds 100000)
EXIT=0
```

## 4. Kết luận (trung thực, không tô hồng)

- **ĐẠT**: **tám** hạng mục **đo được** của §10.1 dòng K6 (C1..C8) — mỗi hạng mục có `log:dòng`
  trong repo, trích nguyên văn ở `K6-DOD.md` §3.3.
- **KHÔNG ĐẠT**: **S1** — ảnh **sản phẩm** không có U-mode/shell. Đây **không** phải hạng mục của
  §10.1 nhưng là điều kiện để gọi "tương đương v1" ở mức người dùng (v1 có `shell>` trên ảnh boot).
  ⇒ `K6-DOD.md` **không** tuyên bố "tương đương v1"; `BLOCKED.md` mục `KT-76` nêu hai lựa chọn cho
  **Agent 0** (KT-77 hoặc hạ phạm vi **có văn bản**).
- **Bảy lỗi v1**: N4/N6 **ĐẠT**; N1/N2/N3/N7 **KHÔNG ĐẠT** (N1 **một phần**); N5 **KHÔNG ÁP DỤNG ở
  K6** (`x86` **phần cứng** sau K6, D2-10 — **không** miễn ca kiểm sim). Chi tiết ở `K6-DOD.md` §4.
- **Nợ**: `K6-DOD.md` §6 (mười chín dòng, có `file:line` + đề xuất). Không dòng nào bị "ẩn" khỏi
  bảng DOD; không nới ngưỡng nào (trần ảnh, pin `size_of`, N soak, seed L1, smp — số ở §5 DOD).

## 5. Kiểm số (D2-112) + tự phá trên bản copy (D2-113F)

- `kt76-verify-numbers.sh`: quét `K6-DOD.md`, `k3b-core.md` §24, `STATE.md` (khối đầu + mục KT-76),
  `BLOCKED.md` mục KT-76, `kt76-journal.md`; mỗi khối ```text scope=…``` kiểm **đúng tệp log đó**;
  chữ số ngoài khối bị **từ chối** trừ **định danh** (hex, mã task/D2/lỗi v1, `§N`, `file:line`,
  ngày/giờ, tên arch) và **trích tiêu chí nguyên văn** (đoạn trong `“…”` phải là **xâu con** của
  `kt76-criteria.txt`, mà tệp đó **sinh lại từ nguồn đọc-only** rồi so **từng byte**).
- `kt76-verify-selfbreak.sh`: **năm** ca phá trên **bản copy** (D2-113F), phải đỏ **đúng lý do** (thêm ca **số đo giấu trong heading**);
  md5 cây thật **trước = sau**. Kết quả cuối: `verify_fail=0`, `selfbreak_fail=0` (đối chứng +
  năm ca phá đỏ **đúng lý do** (gồm ca số đo giấu trong heading) + md5 cây thật không đổi); log `kt76-verify-numbers.log`,
  `kt76-verify-selfbreak.log` (chạy lại được, lệnh ở `K6-DOD.md` §9).

## 5b. Vá theo review KT-76 (delta, không đổi kết luận S1)

| Mã | Review chỉ | Đã vá |
|---|---|---|
| A | `kt76_extract_criteria.py` **cắt hai cột cuối** (mất cột “Ca kiểm tương ứng trong v2” — cột nói **test nào** phải có) | trích **đủ sáu cột**; `kt76-criteria.txt` sinh lại; khối trong `K6-DOD.md` thay bằng bản đủ; câu khai §1 sửa khớp (“**đủ sáu cột**”) |
| B | N1/N7 gán **N/A** là **né**: nguồn giao test là **sim/riscv64**, D2-10 chỉ hoãn **phần cứng** x86 | **N1 = KHÔNG ĐẠT (một phần)** (`kt76-baseline-gate.log:1096`: chỉ `ClockRead` **một hart**, `kt62_syscall.rs:862-871`); **N7 = KHÔNG ĐẠT** (`kt76-baseline-gate.log:1202`: mailbox/MCS có, **không** có test “một cơ chế wake chung/ba port”). TL;DR, `STATE`, `k3b-core` §24 đồng bộ |
| C | verifier **miễn mọi số ở heading** ⇒ heading chỉ có một số lớn vẫn PASS | bỏ miễn trừ; chỉ tha **số hiệu tiết** ngay đầu tiêu đề, mỗi thành phần **tối đa hai chữ số**; **thêm ca tự phá**: heading chỉ có một số **năm chữ số** — **đỏ đúng lý do** (cả **bảy** ca đúng) |
| D | “build sạch” **không** được chứng minh (log `0.03s` = tái dùng artifact; clean `git archive` cho sha **khác**) | thêm `kt76-clean-build.log`: clean build **target dir riêng** tại `1f522686` ⇒ `sha256=29d7e875…`, `build=317ffc48e0e71354`, `bytes=188552` — **khác** `fd202276…` ⇒ DOD **hạ tuyên bố** (“không feature” đúng; **tái lập byte CHƯA chứng minh”) + **nợ P20** + **§7** |
| E | P8 sai (`STATE.md:126` không nói IPI; “chưa có IPI” sai — IPI **đã có**) | P8 → “**scheduler** đa hart **chưa dùng** IPI”, dẫn `v2/kernel/vifast-plat-riscv64/src/ipi.rs:1-10`; `STATE`/`k3b` đồng bộ |

**Không** đụng `K6-DOD.md` §3.2 (S1) — reviewer xác nhận **đúng**, giữ nguyên.

Sau vá: `verify_fail=0`, `selfbreak_fail=0` (**bảy** ca, gồm ca **heading số lớn**), `extract --check` OK,
`report-check` PASS. Không chạm code ⇒ **không** chạy lại gate/matrix (reviewer yêu cầu vậy).

**Quyết định của Agent 0 sau review**: **KT-76 = DONE** (task cổng, DOD trung thực); **phase K6 =
KHÔNG ĐẠT** ở nghĩa "tương đương v1" (vì S1); Agent 0 **mở KT-77**. DOD/`BLOCKED.md`/`STATE.md` đã
ghi đúng trạng thái này.

## 6. Freeze + commit (lead commit theo pathspec, D2-105a)

- Trước freeze: **không** job nền nào của tôi còn chạy (L1 đã `wait` xong; QEMU của soak/matrix/
  persist/conformance đã thoát; `ps` lần cuối — dòng cuối `kt76-orphan.log`: `qemu_all=0`,
  `qemu_ppid1=0`). Trên cây cuối: `./v2/bin/v2 check` **PASS** (`kt76-check.log:75`), `report-check`
  **PASS** (khối dưới), `verify_fail=0`, `selfbreak_fail=0`.
- Tệp của task: `modified_v2=3` (`STATE.md`, `BLOCKED.md`, `docs/k3b-core.md`) + `untracked_v2=33`
  (`K6-DOD.md` + các tệp `JOURNAL/kt76-*`: log, script kiểm/trích, tiêu chí, `kt76-clean-build.log`). **Không** sửa code
  (`v2/kernel/**`, `v2/lib/**`), không đụng `spec/`, `docs/v1/`, `docs/v2/ViFastOs-v2-thiet-ke.md`,
  `viscript/`, `review/`.
- **Chưa stage** (lead stage theo pathspec — D2-105a). `HEAD` lúc freeze: `1f522686` (dòng ViScript
  commit thêm `c9249b79` + `1f522686` giữa task — chỉ chạm `viscript/agent/**`;
  `dirty-kernel-paths=0` ⇒ `v2/` không đổi).

```text scope=kt76-check.log
report-check: PASS (1 báo cáo, 85 hàng QUEUE phủ đủ)
v2 check: PASS (L1 chạy trong gate --fast)
```
