<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# KT-78 — OD-K6-6: harness QEMU (process group, timeout cứng, dọn dẹp) — nhật ký

Người ghi: **Agent 2 dòng OS** · Ngày: **2026-11-17** · HEAD khi mở: `dbd97729` (`[KT-00]` D2-119);
trong lúc làm, nhánh `main` tiến thêm do tab khác (`0df923c2` khi chạy cổng) · Máy `ps`
(mười hai nhân, arm64) · QEMU `11.1.3` (do script in — `kt78-guard/selftest.log:2`).

## 1. Vào task

- `KT-78` = task **đầu tiên** theo thứ tự Agent 0 đặt (`KT-78` → `77a` → `77b` → `82`). Đọc
  **nguyên văn** `reports/QUYET-DINH-A0-K6-2026-11-17.md` (sha256 `0361fb7e…`, khớp) + **D2-119**,
  **OD-K6-6**; kèm D2-116 (QEMU orphan đốt hai core), D2-113 (selfbreak trên **bản copy**), D2-112
  (kiểm số theo scope, cấm ngoại lệ chữ số), D2-108A/B (hash từ build sạch; bộ đếm `0` phải có ca
  âm), D2-102/106 (evidence lấy từ biến; giá trị suy ra phải có công thức), D2-100 (L1 wall gồm
  biên dịch), D2-105 (một người ghi), D2-94, D2-84/85 (pin/trần — **không** đổi).
- **Phạm vi ghi**: `v2/test/boot/**`, `v2/agent/**`, `v2/docs/k3b-core.md` §25, `v2/docs/k6-harness.md`.
  **Không** chạm `spec/`, `v2/kernel/src/**`, `v2/lib/**`, `crates/`, `viscript/`, `review/`.
  `v2/bin/v2` **không** sửa: tệp đó **không** mở QEMU (`grep -c 'qemu-system' v2/bin/v2` = `0`).
- Cây chung có việc của tab khác: `rust-toolchain.toml`, `v2/bin/v2` (+`check-repro`), `v2/tools/`
  của KT-79 — **không** phải delta của tôi; lead commit theo pathspec (D2-105a).

## 2. Hiện trạng mở QEMU (pattern cũ — ghi lại trước khi sửa)

| Tệp | Dòng (trước KT-78) | Pattern |
|---|---|---|
| `v2/test/boot/ktest.sh` | `boot_image` | `"$QEMU" … >"$out" 2>&1 & pid=$!`; vòng đếm `kill -0`; hết hạn `kill $pid` rồi `kill -9 $pid`; `wait` |
| `v2/test/boot/soak.sh` | `boot_one` | y hệt (3 lane là 3 subshell) |
| `v2/test/boot/persist.sh` | `run_boot` | y hệt (10 boot, `pid` ghi vào bảng bằng chứng) |
| `v2/test/boot/run.sh` | vòng boot | y hệt |
| `v2/bin/v2` | — | **không** có `qemu-system` (grep = `0`) |

⇒ Thiếu **cả bốn** thứ OD-K6-6 đòi: process group riêng, timeout kill group, trap dọn khi cha chết,
ghi hạ tầng trước/sau phép đo.

## 3. Đỏ-trước (D2-05) — tái hiện D2-116 bằng **chính** pattern cũ

`guard_selftest.sh --mode old` dựng lại **đúng** pattern cũ (không group, không trap, không
watchdog), rồi `kill -9` / `kill -INT` tiến trình cha khi QEMU đang chạy. Kết quả: QEMU **vẫn sống**
với `ppid=1` (bị `init` nhận nuôi) — đúng sự cố D2-116. Log trong repo.

## 4. Sửa

- **`v2/test/boot/qemu_guard.sh` (mới)** — hợp đồng `qg_init`/`qg_boot`/`qg_tmp`/`qg_infra`/`qg_own`/
  `qg_cleanup`; chi tiết cơ chế ở [`../../docs/k6-harness.md`](../../docs/k6-harness.md). Tóm tắt:
  `perl setpgrp+exec` ⇒ **process group riêng** `PGID == PID`; perl ghi `pid`/`pgid` **đo bằng
  `getpgrp`** trước `exec` (boot thoát nhanh vẫn có bằng chứng); timeout = đếm bước `0,1 s` **cộng
  trần `SECONDS`** (2× danh nghĩa + 5 s) rồi `kill -TERM -- -PGID` → `-KILL`; trap `EXIT/INT/TERM/HUP`
  dọn **mọi** group + `rm` tệp tạm; **watchdog** `disown` bám PID cha ⇒ cha bị `SIGKILL` vẫn về
  `0` QEMU; mỗi lần kill kiểm group **có** QEMU (không `pkill -f`, không giết nhầm).
- **Nối vào**: `ktest.sh` (`boot_image` → `qg_boot`; FIFO console vào `qg_tmp`), `soak.sh`
  (`boot_one`), `persist.sh` (`run_boot`; `pid` vẫn lấy từ `$!` **trong** guard ⇒ bằng chứng “10 boot
  = 10 PID khác nhau” của KT-75 giữ nguyên), `run.sh` (thêm `LOGDIR`).
- **Hạ tầng**: `qg_infra` gọi **trước/sau** mỗi phép đo ở cả bốn script; `gate`/`L1` chụp bằng
  `v2/test/boot/infra.sh` (vì `v2/bin/v2` ngoài phạm vi ghi và không mở QEMU).
- **Không đổi ngữ nghĩa cổng**: `TIMEOUT_DS` vẫn `200`, `-icount` chỉ trong `ktest.sh`, `-smp` 1/2/4,
  số boot/seed/ngưỡng/`expected.txt` nguyên; **không** chèn dòng vào log QEMU (bằng chứng process
  group ở tệp riêng `qemu-guard.log`).

**Lỗi tự gặp khi làm (ghi để người sau không lặp)**:
1. Đọc `ps` **ngay** sau `&` ⇒ thấy PGID của cha (perl chưa kịp `setpgrp`) ⇒ cảnh báo giả; sửa bằng
   cách đọc tệp `binfo` do perl ghi bằng `getpgrp`.
2. `QG_BINFO` đặt tên `qg-$$-<seq>` ⇒ **ba lane soak dùng chung `$$` và chung seq** ⇒ lane đọc pgid
   của lane khác (đo được cảnh báo giả trong lần soak đầu); sửa bằng `mktemp`.
3. Ca `clean-boot` đầu tiên dùng `-S` (CPU đóng băng) nên đúng là **phải** timeout — sửa để boot
   sạch chạy không `-S`; ca `guard-timeout` mới là ca `-S`/khách treo.

## 5. Ca âm (a)(b) + đỏ-trước — log trong repo

Driver: `v2/test/boot/guard_selftest.sh` (tự sinh wrapper cho từng ca vào thư mục log, nên log chứa
**nguyên văn** kịch bản đã chạy). Mỗi ca ghi `alive_before=[pid ppid pgid stat]` **trước** khi gửi
signal ⇒ ca chỉ có giá trị khi QEMU **thật sự còn sống** lúc đó (chống ca âm rỗng, D2-108B).

- (a) cha chết: `old-sigkill`/`old-sigint` ⇒ QEMU `ppid=1` (đỏ-trước); `guard-sigkill`/`guard-sigint`/
  `guard-sigterm` ⇒ **0 QEMU** trong ≤ `100 ms` (watchdog/trap), `wrapper_rc` `137`/`130`/`143`.
- (b) boot treo: khách `j .` bốn byte nạp bằng `-bios` (raw) ⇒ `exit=124`, dừng sau vài giây, `0` QEMU.
- (c) hạ tầng: tạo QEMU `ppid=1` có kiểm soát ⇒ `infra.sh` phải `verdict=KHONG_HOP_LE` +
  `ly_do=qemu_ppid1=1` (ca âm cho bộ đếm `qemu_ppid1`).
- Sự cố thật gặp trong lúc làm: `job_kill` soak giữa lúc ba lane đang boot ⇒ watchdog dọn **hết**
  QEMU của cả ba lane (không còn tiến trình nào) — hành vi đúng của OD-K6-6, ghi ở đuôi log soak cũ.
- **Rerun lúc máy có QEMU của tab khác** (sau khi đã đóng băng log): ca `*-timeout` báo “còn `1`
  QEMU” và `infra-orphan` thấy `qemu_total=2 qemu_ppid1=1 qemu_foreign=2` ⇒ driver ĐỎ **đúng**
  (phép đo không hợp lệ vì QEMU lạ). Đã xoá artefact của lần rerun đó; log đóng băng là lần chạy
  sạch (`baseline: qemu_total=0`). Ghi lại để reviewer không chạy selftest song song với tab khác
  rồi tưởng harness hỏng.

## 6. Cổng chạy lại sau khi sửa (không retry — D2-07)

`v2 check` PASS (clippy `EXIT=0` đọc `$?` thật); `gate --fast` PASS, L1 đo **thuần** không kèm biên
dịch (D2-100); matrix `21/21`; soak `0/1000` trên **đúng** sha ảnh sản phẩm của KT-76; persist bảy
pha. Số nguyên văn + `log:dòng` ở khối `scope=` trong `STATE.md` (§KT-78) và `docs/k3b-core.md` §25;
script kiểm `JOURNAL/kt78-verify-numbers.sh`, tự phá `JOURNAL/kt78-verify-selfbreak.sh` (bản copy).

## 7. Freeze

- Không job nền, `0` QEMU (`ps -ax -o pid,ppid,command | grep qemu` rỗng), không sửa `viscript/`,
  `review/`, `spec/`, `v2/kernel/**`, `v2/lib/**` (ảnh sản phẩm **không đổi byte**).
- Delta của task (lead commit theo pathspec): `v2/test/boot/qemu_guard.sh`, `guard_selftest.sh`,
  `infra.sh`, bốn script boot đã sửa, `v2/docs/k6-harness.md`, `v2/docs/k3b-core.md` §25,
  `v2/agent/STATE.md` §KT-78, `v2/agent/JOURNAL/kt78-*`.
- **Bàn giao cho reviewer**: đọc `docs/k6-harness.md` trước, rồi `kt78-guard/selftest.log`, rồi tự
  chạy `bash v2/test/boot/guard_selftest.sh --mode both` (khoảng vài chục giây) và **cố tình phá**:
  bỏ `trap`, bỏ watchdog, hoặc đổi `setpgrp` thành không làm gì — cả ba phải làm ca `(a)` ĐỎ.

## 8. Vòng review KT-78 — vá ba phát hiện (reviewer DRIFT mức THẤP)

Reviewer xác nhận lõi `OD-K6-6` đúng (tự chạy `guard_selftest.sh --mode both` hai lần: `9/9`, tự
thấy `verdict=KHONG_HOP_LE ly_do=qemu_ppid1=1`, `old-sigkill` ⇒ `ppid=1`) và nêu **ba** mục phải vá.
Đã vá cả ba; sau khi vá chạy lại toàn bộ cổng trên **đúng** ảnh `fd202276…` (sha kiểm **trước và
sau** chuỗi chạy: không đổi).

1. **D2-108B — ba bộ đếm `isolated_0`/`isolation_bad`/`isolated_unknown` không có ca âm.** Thêm ca
   `iso-bad` (ca thứ mười của `guard_selftest.sh`) + hook **chỉ cho test** `QG_FORCE_NO_SETPGRP`:
   wrapper chạy QEMU **không** qua `setpgrp` ⇒ guard ghi `isolated=0`, `QG_ISO_BAD` tăng, `qg_infra`
   kết luận `verdict=KHONG_HOP_LE ly_do=isolation_bad=…` (đổi ở `qg_infra`: `isolation_bad>0` cũng
   là phép đo không hợp lệ). Log: `kt78-guard/selftest-20261117T015708Z.log` (ca 10).
2. **D2-112 — verifier bị phá bằng fence `text` không `scope=`.** Bản cũ coi **mọi** fence là
   “block, bỏ qua”, nên chèn fence chứa `boot_done=999 isolated_0=888` vẫn PASS. Sửa: **chỉ** khối
   `text scope=` được miễn; fence khác kiểm như prose (số phải nằm trong backtick). Đồng thời đưa
   mẫu dòng `# infra` trong `k6-harness.md` thành khối `scope=kt78-ktest-matrix.log` thật, và đổi
   danh sách lệnh ở §6 thành prose (mỗi lệnh trong backtick) — hết số đo ngoài `scope=`. Tự phá
   thêm **đường 5** đúng đường reviewer đã phá; `kt78-verify-selfbreak.sh` nay `6/6`.
3. **D2-106 — dòng `# tong_hop …` không do script nào sinh.** Thay bằng `qg_guard_summary` (awk
   trong `qemu_guard.sh`, **nguồn + công thức in kèm**), được `ktest.sh`/`soak.sh`/`persist.sh`/
   `run.sh` gọi sau mỗi lần chạy (ghi `guard-summary.log`), và `kt78-collect-guard.sh` (script
   **commit**) sinh lại tệp bằng chứng trong repo; tài liệu trích dòng `# guard_summary boot_end=…`
   do script sinh, không còn số tự đếm.

**Chạy lại sau vá** (đều trên ảnh `fd202276…`, sha kiểm trước/sau = không đổi; HEAD lúc chạy
`d2dbb0cc` — nhánh `main` tiến thêm do tab khác, **không** phải delta của task):
`gate --fast` PASS (`TESTS=729 / SUITES=100`, `L1` `865 ms`), `v2 check` PASS, clippy `EXIT=0`,
matrix `21/21` (guard log `42/42` dòng `isolated=1`), soak `0/1000` (wall `238 s`, guard log
`boot_end=1000 isolated_1=1000`), persist `phases=7 ok=7 boots=10` với `10` PID khác nhau; hạ tầng
`verdict=OK` trước/sau mỗi phép đo, `invalid=0 isolation_bad=0`.

**Sự cố D2-108A của tab khác (không phải delta này, ghi để tra cứu)**: reviewer bắt gặp artifact
`release/vifast-kbin-riscv64` bị build canonical (strip) của KT-79 ghi đè (`c1072d72…`/`82552`), nên
không tái lập được `fd202276…`; reviewer đã báo lead và artifact đã về đúng. Từ đó mọi lần chạy của
KT-78 đều `shasum` **trước và sau** và ghi vào log (`kt78-soak-sha-before.log`, `kt78-product-sha.log`).

## 9. Vòng review KT-78b — hai mục nhỏ

Reviewer KT-78b xác nhận **cả ba** fix của vòng trước là thật (tự chạy `cases=10 ok=10`, tự chèn
fence `text` ⇒ verifier ĐỎ, tự chạy `kt78-collect-guard.sh` ⇒ sinh lại `2008` dòng trùng) và nêu hai
mục nhỏ; đã vá:

1. **Hook `QG_FORCE_NO_SETPGRP` chưa override được từ ngoài** (blocking): dòng sinh wrapper gán cứng
   `=1` nên `QG_FORCE_NO_SETPGRP=0` bên ngoài không có tác dụng; reviewer phải sửa script trên bản
   copy. Vá thành `QG_FORCE_NO_SETPGRP=${QG_FORCE_NO_SETPGRP:-1}` (mặc định `1`, cho override), rồi
   tự chứng minh **từ ngoài**:
   - không đặt gì: `bash v2/test/boot/guard_selftest.sh --mode guard` ⇒ `cases=8 ok=8` PASS
     (`kt78-guard/selftest-guard-default.log`);
   - `QG_FORCE_NO_SETPGRP=0 …` ⇒ `not ok 8 - iso-bad`, `cases=8 ok=7`, FAIL
     (`kt78-guard/selftest-guard-override0.log`).
   Ảnh sản phẩm `shasum` trước/sau hai lần chạy: **không đổi**.
2. **Nguồn `/tmp` trong dòng tổng hợp** (nhỏ): `qg_guard_summary` nay in `nguon=` theo nhãn truyền
   vào (`QG_SUMMARY_SRC`) và thêm dòng `sinh_lai=` (lệnh tái lập); `kt78-collect-guard.sh` truyền
   `harness:<log gốc> (ban sao loc: <tệp tạm>)` để không ai đọc tệp tạm thành nguồn. Hai dòng
   `# guard_summary_log=/tmp/...` trong tài liệu được ghi rõ ngay cạnh khối là **con trỏ** tới thư
   mục làm việc, **không** phải nguồn số (nguồn là các dòng trong khối `scope=`).

**Bằng chứng untracked cần cho `D2-109`** (reviewer tự `git add`): `kt78-guard/selftest-*.log`,
`kt78-guard/run-*/` (kịch bản wrapper của từng ca), `kt78-soak-qemu-guard.log`,
`kt78-persist-qemu-guard.log`, `kt78-collect-guard.sh`, `kt78-verify.log`, `kt78-selfbreak.log`,
`kt78-soak-sha-before.log`, `kt78-product-sha.log`, `kt78-gate-summary.log`, và các log chính
(`kt78-{check,clippy,gate,ktest-matrix,soak,persist}.log`).

Ràng buộc vòng này: **không** chạm `v2/test/parity/**` và `spec/v1-parity.toml` (KT-77a đang chạy
song song), `v2/bin/v2`, `v2/tools/**`, `rust-toolchain*` — đã giữ; chạm `.sh` harness ⇒ chạy lại
`gate --fast` + `v2 check` (có `shasum` trước/sau).
