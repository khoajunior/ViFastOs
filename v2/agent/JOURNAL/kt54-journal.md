# KT-54 — ktest TAP + runner QEMU smp 1/2/4 + `expected.txt` (K3)

- **Ngày:** 2026-11-15
- **Task:** KT-54 (QUEUE v2) — **hợp nhất + đóng lỗ hổng tin cậy** của harness ktest: `expected.txt`
  là nguồn sự thật duy nhất, runner kiểm đủ bất biến TAP (dãy `1..N`, PANIC, evidence, hart theo cấu
  hình), bảng tổng kết `image × smp × boot`, log riêng từng `(ảnh, smp, boot)`, `kill` đúng PID — và
  **chứng minh runner đỏ đúng lý do** bằng 15 ca mutation của chính harness. **Không** thêm TAP kernel,
  **không** sửa logic kernel, **không** `.scn` (KT-55), **không** `FaultKind` (KT-56), **không** soak
  1.000 (KT-57).
- **HEAD vào:** `540c0546` ([KT-00] lật KT-53 DONE + mở KT-54); khi chạy ma trận cuối, `main` đã tiến
  lên `d3027284` do **tab ViScript** commit `[ST-113]`/`[ST-00]` (`git diff 540c0546..d3027284 -- v2/`
  **rỗng** ⇒ không ảnh hưởng). Working tree có sửa đổi `viscript/` của tab khác — **không chạm**, chỉ
  stage `v2/`.
- **Trạng thái:** **DONE** — reviewer-kt54 **DRIFT** (2 đường xanh sai + 2 mức siết + lỗi tài liệu) →
  đã vá hết, **đóng băng script** rồi chạy lại ma trận; commit `[KT-54]` + `[KT-54b]` (§9).

## 1. Baseline + lỗ hổng tin cậy của runner cũ

- Baseline **runner cũ**: `bash v2/test/boot/ktest.sh 1 matrix` ⇒ **9/9 boot sạch**
  (`kt54-baseline-matrix.log`) ⇒ hạ tầng KT-44..53 đã có và xanh; KT-54 **không** làm lại hạ tầng mà
  **đóng lỗ hổng tin cậy** (đúng cảnh báo của lead: "phần lớn hạ tầng đã có từ trước").
- Rà `ktest.sh` cũ (528 dòng) + `expected.txt` ⇒ **10 lỗ hổng**, mỗi cái đều làm runner **xanh sai**:

| # | Lỗ hổng (bản cũ) | Hậu quả |
|---|---|---|
| 1 | `tap_numbers_ok` so **tập đã `sort -n`** với `seq 1..N` | không bắt thứ tự `ok` bị đảo; phụ thuộc lệnh ngoài `seq` |
| 2 | `grep -q "^1\.\.$plan$"`, `grep -q '^TAP version 13$'` | **hai** dòng `1..37`/`TAP version` vẫn xanh; không kiểm **thứ tự** version < kế hoạch < `ok` đầu |
| 3 | PANIC: chỉ `grep -Evc "$re"` | **hai** PANIC đúng mẫu vẫn xanh; mẫu guard/user **không neo cuối dòng** |
| 4 | 37 lệnh `grep -q '^ok N - …'` | chỉ đòi **có mặt**, không đòi **đúng một** dòng; va chạm `ok 1` vs `ok 10` không được kiểm |
| 5 | số TAP ở **hai nguồn** (`expected.txt` + 37 chuỗi hardcode) | nâng `37 → 38` chỉ ở tệp vẫn xanh |
| 6 | `expected_field` `exit` ở dòng khớp **đầu tiên**, không kiểm định dạng | dòng lặp/nhãn gõ nhầm bị bỏ qua âm thầm |
| 7 | không có chế độ kiểm **một log** | không thể chứng minh runner đỏ rẻ/tất định ⇒ **không có test nào cho runner** |
| 8 | timeout `kill` rồi `wait` | QEMU bỏ qua `TERM` ⇒ `wait` treo vô hạn (không có `SIGKILL` dự phòng cùng PID) |
| 9 | bảng tổng kết chỉ `# KTEST: x/y` + cộng bù `total_boots` khi timeout | không có bảng `image × smp × boot` + số TAP; không đọc được cái gì thật sự đã chạy |
| 10 | đường dẫn log rải trong script | quy ước tên log không một nguồn ⇒ khó trích log theo `(ảnh, smp)` |

## 2. Đỏ trước (test-harness trước, code sau)

- Viết **`v2/test/boot/selftest.sh` trước** rồi chạy với runner **cũ**: bản 10 ca ⇒ **1/10** ca đúng
  (chỉ ca `expected.txt` trả lại nguyên vẹn là xanh); 9 ca đỏ vì runner cũ hiểu `--check-log` là
  `NBOOT` ⇒ `ktest.sh: SMP 'ktest' không phải số` + `exit 2` (**không** có dòng `not ok` để máy đọc)
  — log `kt54-selftest-red.log`.
- Đó là "đỏ trước" đúng nghĩa: runner cũ **không có** khả năng tự kiểm và **không** in chẩn đoán TAP.

## 3. Thiết kế đã làm

| Tệp | Nội dung |
|---|---|
| `v2/test/boot/expected.txt` | **chỉ sửa header** (3 dòng dữ liệu **không đổi**, `diff` dòng dữ liệu rỗng): cú pháp + 5 luật runner kiểm khi nạp tệp |
| `v2/test/boot/ktest.sh` | viết lại (**528 → 883 dòng**, sha256 `f3063426…`): 5 chế độ (`matrix`, `--check-log`, `--log-path`, `--print-expected`, `--print-evidence`); `load_expected`/`check_manifests` fail-closed; manifest tên **37/3/5** TAP; evidence **51/5/13** mục **neo `^…$`**; `panic_allow_re` neo **cả đầu và cuối**; `validate_log` (chuẩn hoá `\r`, biến toàn cục `MISSING`, không subshell); `boot_image` (20 s, `TERM`→`KILL` đúng PID); `run_one` (`PASS/FAIL/TIMEOUT`), `print_summary` (bảng + gộp + `SKIP` chỉ khi `TIMEOUT`), chặn **0 boot** (`SMP_LIST` rỗng ⇒ `die`; `ROWS == NBOOT × smp × 3`) |
| `v2/test/boot/selftest.sh` | **mới** (sha256 `6ec0d723…`): **17 ca** mutation harness (1 xanh + **15 đỏ kèm token lý do** + 1 sha256/cmp), `trap` khôi phục `expected.txt`, `mut_diff` chặn "mutation không áp dụng", `pgrep` chặn chạy song song với `ktest.sh` |
| `v2/docs/k3-riscv64.md` | **§17** (cách chạy/đọc, ý nghĩa **từng dòng evidence** theo 3 ảnh, 17 ca mutation + token, 12 giới hạn) + header + §5 + danh sách log |

### 3.1 Lỗi tự phát hiện khi làm — subshell nuốt lỗi ⇒ xanh sai

Bản nháp đầu dùng `missing="$(check_tap_common …)"`; trong đó `local … missing` **không khởi tạo** ⇒
khi mọi kiểm đều qua, `printf '%s' "$missing"` gặp biến chưa gán, bash in `unbound variable` **trong
subshell** và subshell chết, command substitution thu về **chuỗi rỗng** ⇒ `validate_log` coi là "không
thiếu gì" ⇒ **PASS oan**. Đã sửa: mọi hàm kiểm ghi vào **biến toàn cục `MISSING`** (không subshell),
mọi `local` khởi tạo `=""`. Ghi lại vì đây đúng là lớp lỗi KT-54 phải chặn.

### 3.2 `expected.txt` — nguồn sự thật duy nhất

- Runner **đọc số** bằng `expected_field` (awk, bỏ dòng `#`/rỗng) — không hardcode `37/3/5` trong luồng kiểm.
- `load_expected` kiểm: đúng **3 trường**, trường 2/3 số `>= 1`, **`plan == ok`**, **không nhãn lặp**,
  **không nhãn lạ** (gõ nhầm ⇒ đỏ), **đủ** 3 nhãn.
- `check_manifests` kiểm **kế hoạch == số TAP runner biết tên** ⇒ sửa một bên là **đỏ**.
- `KT54_EXPECTED` chỉ cho `--check-log`; vòng boot thật **luôn** đọc tệp trong repo.

### 3.3 Tám nhóm bất biến cho mỗi boot (chi tiết + ý nghĩa evidence: docs §17.3/§17.5)

1. khung TAP: `TAP version 13` **đúng 1**, `1..N` **đúng 1**, **thứ tự** version < kế hoạch < `ok` đầu;
2. dãy `ok` **đúng thứ tự `1..N`**, không trùng/thiếu; số dòng `ok` = trường 3 của tệp;
3. **không** `not ok`;
4. **đúng 1 PANIC**, khớp **nguyên dòng** theo mẫu ảnh;
5. mỗi TAP `N` có **đúng một** dòng `ok N - <tên>` khớp manifest;
6. đủ `# evidence` bắt buộc (51/5/13) **neo cả dòng** + `OpenSBI vX.Y.Z` + `Platform HART Count : N`;
7. dòng per-hart: **tập** hart `0..N-1` (mỗi hart đúng một dòng `ok=1`) cho `# timer hart=`/`# plic hart=`,
   và `# hart h alive`/`# stack hart=h` cho từng h; guard: hart boot nằm trong dải hart của máy;
8. **không "xanh vì không chạy gì"**: `SMP_LIST` rỗng ⇒ `die`; cuối vòng `ROWS == NBOOT × smp × 3`
   và phải có ≥ 1 boot thật.

### 3.4 Bảng tổng kết + exit code + idempotent + `kill` đúng PID + chuẩn hoá `\r`

- Bảng in cuối: `image | smp | boot | TAP (thực/kế hoạch) | PASS/FAIL/TIMEOUT/SKIP | log` + khối gộp
  `(image, smp)` + `# KTEST: sạch/tổng` + `# ktest.sh: PASS|FAIL`.
- Exit: `0` xanh; `1` có boot/log đỏ; `2` lỗi cấu hình (thiếu QEMU/OpenSBI/cargo, `expected.txt` sai,
  `SMP_LIST` rỗng, **thiếu slot**).
- Idempotent: mỗi boot **truncate** đúng tệp log của nó; bảng chỉ ra stdout.
- Timeout: `200 × 0,1 s` ⇒ `kill <pid>` → chờ ≤ 5 s → `kill -9 <pid>` **đúng PID đó**; không
  `pkill -f qemu-system` (AGENTS §9). Log riêng từng `(ảnh, smp, boot)`:
  `${KT54_LOGDIR:-/tmp/k3-ktest}/<ảnh>-smp<N>-<boot>.log`.
- **Log QEMU có CRLF ở dòng firmware** (`OpenSBI v1.8.1\r`, `Platform HART Count … 2\r`), dòng kernel là
  LF ⇒ `validate_log` chuẩn hoá `\r` vào **bản tạm** trước khi kiểm (không sửa log gốc) để mọi mẫu
  neo `^…$` đúng cho cả hai loại dòng.

## 4. Mutation của harness (chứng minh runner đỏ **đúng lý do**)

`bash v2/test/boot/selftest.sh /tmp/k3-ktest/ktest-smp2-1.log 2` ⇒ **17/17** (`kt54-selftest.log`):

| Ca | Mutation (harness, **không** phải kernel) | Token lý do | Kết quả |
|---|---|---|---|
| 1 | log thật | — | **xanh** (không báo động giả) |
| 2 | `expected.txt` **thật**: `ktest 37 37 → 36 36` | `kế hoạch 36 nhưng runner kiểm 37 TAP` | đỏ |
| 3 | `expected.txt` mất dòng `ktest-user 5 5` | `thiếu dòng cho ảnh [ktest-user]` | đỏ |
| 4 | log: dòng `ok` đầu → `not ok` | `not-ok=1` | đỏ |
| 5 | log: xoá 1 dòng `ok` | `tap-day-1..` | đỏ |
| 6 | log: nhân đôi 1 dòng `ok` | `ok-count=38/` | đỏ |
| 7 | log: xoá evidence `# obj init:` | `ev1` | đỏ |
| 8 | log: xoá dòng kế hoạch `1..N` | `plan-1..37=0` | đỏ |
| 9 | log: thêm PANIC thứ hai (đúng mẫu) | `panic-tong=2/1` | đỏ |
| 10 | log: PANIC đúng mẫu **thêm đuôi lạ** | `panic-khac=1` | đỏ |
| 11 | log: PANIC **lạ** | `panic-khac=1` | đỏ |
| 12 | **0 boot**: `ktest.sh 1 " "` | `SMP_LIST rỗng` (`die`) | đỏ |
| 13 | log: `Platform HART Count 2 → 12` | `ev51` (tra bằng `--print-evidence`) | đỏ |
| 14 | log: `# mmu on: …` **thêm đuôi lạ** | `ev9` | đỏ |
| 15 | log: lặp `# timer hart=0` thay hart 1 | `timer-hart-lines` | đỏ |
| 16 | log: lặp `# hart 0 alive` thay hart 1 | `hart1-alive` | đỏ |
| 17 | `expected.txt` sau ca 2 | — | **xanh** (`sha256` trước = sau + `cmp`) |

- Mutation chỉ chạm **harness**: 2 ca chạm `expected.txt` (ca 2 phá tệp thật rồi trả lại ngay bằng
  `trap` + sha256), 12 ca chạm **bản sao log** trong `mktemp -d`. **Không** sửa kernel, không sửa ảnh.
  Mỗi mutation log được `mut_diff` xác nhận **đã áp dụng** (nếu không ⇒ `die`, test không thể "xanh
  vì mutation không xảy ra").
- Ca 12–16 là **các đường xanh sai reviewer-kt54 tìm ra** (xem §5.3) — đã vá và có ca test khoá lại.
- Đỏ trước (runner cũ): **1/10** trên bản 10 ca — `kt54-selftest-red.log`.

## 5. Kiểm chứng

### 5.1 9 log thật, kiểm lại **không boot** (`kt54-checklog-cheo.log`)

- `--check-log` trên 9 log đóng băng (smp 1/2/4 × 3 ảnh): **9/9 xanh**.
- Kiểm chéo **sai cấu hình/ảnh** ⇒ **đỏ** với token cụ thể: log `smp=1` kiểm bằng `smp=2` ⇒ 22 token
  (`tap15 tap20 hart1-alive … ev23 …`); log ảnh guard kiểm như ảnh chính ⇒ `plan-1..37=0 ok-count=3/37 …`.

### 5.2 Budget **không đổi** — 4 ảnh **byte-identical** với KT-53 (`kt54-image-sha.log`)

`diff` danh sách sha256 giữa `JOURNAL/kt53-image-sha.log` và `kt54-image-sha.log` ⇒ **rỗng** ⇒ kernel
không bị chạm bởi KT-54:

| Ảnh | File | data+bss | sha256 (16 ký tự đầu) |
|---|---|---|---|
| `vifast-kbin-riscv64` | 89.640 B | 11.248 B | `ed9a8a4c305ab8c4` |
| `vifast-kbin-ktest` | 230.912 B | 13.200 B | `50bbe48b0278d219` |
| `vifast-kbin-ktest-guard` | 53.728 B | 2.680 B | `79f2b37cd4584a58` |
| `vifast-kbin-ktest-user` | 141.992 B | 11.688 B | `e21f9ae699bafe7f` |

Trần **262.144 B** (D2-72 Q5) giữ nguyên, `budget.sh` **không sửa** (sha256 `7dd9d761…`).

### 5.3 Review độc lập `reviewer-kt54` — **DRIFT** (không UNSAFE) ⇒ đã vá **hết** trước commit

| Mức | Phát hiện | Vá |
|---|---|---|
| **A (1)** | `ktest.sh 1 " "` ⇒ **0 boot** mà `# KTEST: 0/0` + **PASS, exit 0** (`for s in $SMP_LIST` không kiểm rỗng; `total_boots=0` không assert) | `check_smp_arg` đếm `SMP_COUNT` + `die "SMP_LIST rỗng"`; cuối vòng `ROWS == NBOOT × smp × 3` + `total_boots ≥ 1` ⇒ `die`; `ipi_name` chặn tham số rỗng (hết `[: : integer expression expected`) |
| **B (2)** | `Platform HART Count : 1` → `: 10/12/19` vẫn **PASS** (mẫu không neo cuối) | thêm `$` cho cả 3 ảnh ⇒ ca 13 |
| **C (3)** | nhiều mẫu evidence thiếu `$` (mmu, mmu on/layout, timer arm/ticks/drift/irq/sie, walk khop, plic/plic map, guard image/mmu map/load stats, `OpenSBI v…`) | **neo `^…$` toàn bộ** (51/5/13 mục), đối chiếu log thật; phát hiện thêm log có **CRLF ở dòng firmware** ⇒ chuẩn hoá `\r` vào bản tạm (nếu chỉ thêm `$` mà không chuẩn hoá thì 9/9 log thật **đỏ oan**) ⇒ ca 14 |
| **D (3)** | `check_hart_lines` chỉ **đếm** dòng `# timer hart=`, lặp dòng hart 0 thay hart 1 vẫn PASS | kiểm **tập** hart `0..N-1` (mỗi hart đúng một dòng `ok=1`) ⇒ ca 15/16 |
| selftest | `case_red` chỉ đòi `exit≠0` + một dòng `not ok` | thêm **token lý do bắt buộc** (ca 4–16) + `mut_diff` (mutation phải áp dụng) + sha256 `expected.txt` trước/sau (ca 17) |
| tài liệu | header `ktest.sh` ghi 54/4 (thật 51/5/13); §17.5 mô tả `SKIP` sai (mã SKIP mọi lỗi, không chỉ `TIMEOUT`); journal ghi 795 dòng / lẫn 1/10 vs 1/12 | sửa header **51/5/13**; **sửa mã**: `SKIP` chỉ khi `TIMEOUT` (FAIL vẫn chạy tiếp ảnh khác); §17 viết lại theo mã cuối; journal ghi **883 dòng** và red-first **1/10** (bản 10 ca) |
| bắt buộc | ma trận đang chạy bị ghi đè giữa chừng ⇒ **không tính** | **đóng băng** `ktest.sh`/`selftest.sh`/`expected.txt` (`kt54-freeze.sha` + kiểm lại sau run `kt54-freeze-check.log`) rồi chạy lại **selftest + 450 boot + gate** |

## 6. Ma trận cuối `ktest.sh 50 matrix` — **450/450 boot sạch**

- **Đóng băng trước khi chạy**: `ktest.sh` sha256 `f3063426…`, `selftest.sh` `6ec0d723…`,
  `expected.txt` `652476d5…`, `budget.sh` `7dd9d761…` (`kt54-freeze.sha`); sau run kiểm lại
  **không đổi** (`kt54-freeze-check.log`) ⇒ 450 boot chạy **đúng** bản đã kiểm.
- `bash v2/test/boot/ktest.sh 50 matrix` ⇒ `# KTEST: 450/450 boot sạch (smp=matrix, NBOOT=50,
  ảnh/boot=ktest ktest-guard ktest-user)`, `# ktest.sh: PASS`, **EXIT=0**, wall **30 m 53 s**
  (`kt54-boot50-final.log`; `ok - …` = 450 dòng, `not ok` = **0**; bảng 450 dòng `PASS`).
- Gộp theo `(ảnh, smp)` — **9/9 dòng `50/50`**, `FAIL=0 TIMEOUT=0`:
  `ktest`/`ktest-guard`/`ktest-user` × `smp=1/2/4`.
- 450 = **50 vòng × 3 ảnh × 3 cấu hình** (D2-72 Q4 matrix 1/2/4). Ghi chú: con số "300 boot" trong
  đề bài là **2 ảnh**/vòng (thời điểm trước KT-53); từ KT-53 có **3 ảnh test** nên `NBOOT=50` cho
  **450** — vẫn thoả "50 boot mỗi N" và ≥ 300.
- Matrix `NBOOT=1` (9 boot) trên **cùng** bản đóng băng: `9/9` sạch, EXIT=0 (`kt54-matrix1.log`);
  log từng `(ảnh, smp)` trích nguyên văn: `kt54-{ktest,guard,user}-smp{1,2,4}.log` (9 tệp).
- Selftest sau đóng băng: **17/17** (`kt54-selftest.log`); phản biện thêm 7 ca log hỏng/rỗng/cắt/ghép
  (`kt54-adversarial.log`): log rỗng, log cắt, `ok 1`→`ok 10`, PANIC thụt dòng, 2 dòng kế hoạch,
  38 TAP, 2 boot ghép — **tất cả đỏ**, log thật xanh.

## 7. Cổng `./v2/bin/v2 gate --fast`

- Kết quả: `v2 gate --fast: PASS`, **569 test passed / 0 failed**, L1 1.000 seed (871 ms, đo bởi cổng),
  `GATE_EXIT=0` (`kt54-gate.log`) — **không đổi** so baseline KT-53
  (`JOURNAL/kt53-gate-baseline.log`), vì KT-54 **không** chạm kernel/lib/sim
  (`git diff --stat` chỉ `v2/test/boot/*` + `v2/docs/k3-riscv64.md` + `JOURNAL/`).
- Kernel **không** nằm trong `gate --fast` (`v2/Cargo.toml` `exclude = ["kernel"]`) — kiểm bằng
  build 4 ảnh + `ktest.sh` (trên) + sha ảnh **giống hệt** KT-53 (§5.2).

## 8. Phạm vi đã chạm (không chạm gì khác)

`git status --short v2/` = `M v2/docs/k3-riscv64.md`, `M v2/test/boot/expected.txt`,
`M v2/test/boot/ktest.sh`, `?? v2/test/boot/selftest.sh`, + `JOURNAL/kt54-*`. **Không** chạm
`v2/kernel/`, `v2/lib/`, `v2/sim/`, `spec/`, `.scn`, `QUEUE.md`, `DECISIONS.md`, `AGENTS.md`,
`viscript/`; không dependency mới; không `cfg(target_arch)`; không `unsafe` mới.

## 9. Commit

- `[KT-54]` — harness + docs + evidence (chỉ `v2/`); `[KT-54b]` — STATE. Hash + push ghi ở
  `STATE.md` (cập nhật ngay sau commit, như KT-53).
