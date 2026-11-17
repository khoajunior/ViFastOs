<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# KT-77a — `spec/v1-parity.toml` + runner serial (OD-K6-2, phần đầu)

**Ngày:** 2026-11-17 · **Agent:** Agent 2 dòng OS (phiên KT-77a) · **Uỷ quyền:** Agent 1 (D2-121),
thi hành **OD-K6-2** của Agent 0 (D2-119/D2-120).
**HEAD lúc bắt đầu:** `f7b17e51`. **Vùng ghi:** `spec/v1-parity.toml` (mới), `v2/test/parity/**`
(mới), `v2/docs/k6-parity.md` (mới), `v2/agent/JOURNAL/kt77a-*`, thêm dòng vào `STATE.md`/`BLOCKED.md`.

## 1. Đã làm

1. **Đọc bằng chứng v1** trong `docs/v1/` (đọc-only) và lập `spec/v1-parity.toml`:
   - **28 mục** = 22 builtin của shell v1 (bảng SHELL-TABLE ở tag `v1-archive`) + 6 applet ngoài
     (`ps`, `free`, `df`, `uname`, `uptime`, `jobs` — `v1-archive:crates/vifast-bin/src/main.rs:221-239`),
     mỗi mục có `source = file:dòng` (`docs/v1/…` hoặc `v1-archive:…`); `args` (applet debug) ghi rõ
     là **không** đưa vào.
   - **4 active** (`help`, `echo`, `ps`, `kill` — v2 shell đã có, `shell_user.rs:247-269`);
     **24 deferred-K7** kèm `reason`; `cd`/`pwd` có **hai chiều** bằng chứng + `decision = cho Agent 0`.
   - Mỗi mục có kịch bản serial: `send`, `expect`, `timeout_ms`, `serial_min_count`,
     `transcript_expect` (khi script nội bộ kernel dùng đối số khác).
2. **Runner** `v2/test/parity/run.sh` + helper `v2/test/parity/stdin_wrap.sh`:
   - `--mode transcript` (đo shell trên ảnh test, nguồn byte = script nội bộ kernel);
   - `--mode serial` (bơm byte qua stdin QEMU; attribution theo `serial_min_count`);
   - ca âm `--negative wrong-expect` / `--negative no-send`; `--image product` cho ca "không shell".
   - Dùng lại **nguyên** `v2/test/boot/qemu_guard.sh` (KT-78) — **không sửa**; mọi boot qua `qg_boot`
     (process group riêng, timeout 20 s) + `qg_infra` trước/sau.
3. **Đo + log trong repo** (`v2/agent/JOURNAL/`):
   - transcript `ktest-proc` smp1: **4/4** active OK (`kt77a-transcript-ktest-proc-smp1.log:147-151`);
   - serial `ktest-proc` smp1: **4/4** OK (`kt77a-serial-ktest-proc-smp1.log:150,291,436,578,579`);
     3 lần chạy lại: mỗi lần 4/4 (`kt77a-stab1.log:15`, `kt77a-stab2.log:15`, `kt77a-stab3.log:15`);
   - ca âm (a) sai lệnh ⇒ `not ok`, exit **3** (`...neg-wrong-expect.log:148,579`);
   - ca âm (b) không gửi gì ⇒ `timeout`, exit **4** (`...neg-no-send.log:146,571`);
   - ca âm (c) ảnh sản phẩm **không shell** ⇒ `not ok` hết, exit **5**
     (`kt77a-serial-product-smp1.log:89,343`), log không có dòng `# shell: up`.
4. **Tài liệu** `v2/docs/k6-parity.md`: bảng 28 mục + `source` + trạng thái + kịch bản + kết quả +
   giới hạn.

## 2. Phát hiện quan trọng (giới hạn, không phải lỗi)

- Shell trong `ktest-proc` **không** phải shell tự do: kernel bơm **script 6 dòng**
  (`proc_demo::build_script`, `proc_demo.rs:270-280`) qua loopback 16550 rồi shell `exit`. Byte từ
  serial ngoài **vào được** shell (có `# shell: ?` và các dòng lệnh của kịch bản), nhưng **gộp cửa
  sổ RX** với dòng script của kernel ⇒ `inject_ok=0`
  (`kt77a-serial-1-help-serial-ktest-proc-smp1.log:118`) và TAP nội bộ của ảnh test đỏ
  (`not ok 3`, `not ok 4`) — runner parity chỉ chấm `expect` của kịch bản, **không** chấm TAP đó.
- Vì vậy chế độ serial trên ảnh test là **đo chuyển động**, không phải cổng chặt; cổng thật phải là
  **ảnh sản phẩm có shell** (KT-77b). Ảnh sản phẩm hiện in `ok 1 - k3 boot` rồi `# k3 halt`, **0**
  dòng `# shell: up` ⇒ S1 **vẫn mở**.
- `help` gửi **hai lần** trong kịch bản: dòng đầu payload trùng cửa sổ RX với dòng đầu script nội bộ
  nên hay bị gộp; hai lần cho bộ đếm 3/3 ổn định (5 lần đo).

## 3. Ca âm / kỷ luật đo

- Không retry để xanh: mỗi phép đo chạy **một** lần và ghi log; 3 lần "stab" là phép **đo độ ổn
  định** (cả 3 xanh), không dùng để che lần đỏ.
- Không sửa ngưỡng, không `#[ignore]`, không đổi ABI, không đụng `v2/kernel/**`, `v2/test/boot/**`,
  `docs/v1/**`, `docs/v2/**`.
- QEMU: `qemu_ppid1=0 qemu_foreign=0` trước/sau mỗi lần đo (`kt77a-infra.log:1-8`);
  `# parity: qemu_con_sot_total=0 qemu_cua_script=0` cuối mỗi lần
  (`kt77a-transcript-ktest-proc-smp1.log:154`, `kt77a-serial-ktest-proc-smp1.log:582`).
- Ảnh test dùng **có sẵn** (không build lại trong KT-77a): `ktest-proc`
  sha256 `f7c6b4a3…c281cfc1` (437.672 B); ảnh sản phẩm `fd202276…4393a6da` (188.552 B).

## 4. Chưa làm / chuyển tiếp

- **KT-77b**: shell + U-mode trong ảnh **sản phẩm**, chạy `run.sh --mode serial --image product`
  (lúc đó mới là cổng parity thật); smp 1/2/4; ca âm "shell bị tắt".
- **22 lệnh `deferred-K7`**: cần syscall/ABI mới (`MemInfo`, `TcbInfo`, `STATFS`, exec-from-FS) hoặc
  VFS client trong shell. Danh sách nằm trong `spec/v1-parity.toml` để lead/Agent 0 duyệt lúc đóng K6.

## 5. Lệnh tái lập

```bash
bash v2/test/parity/run.sh --mode transcript --image ktest-proc --logdir v2/agent/JOURNAL
bash v2/test/parity/run.sh --mode serial     --image ktest-proc --logdir v2/agent/JOURNAL
bash v2/test/parity/run.sh --mode serial --image ktest-proc --negative wrong-expect --logdir v2/agent/JOURNAL
bash v2/test/parity/run.sh --mode serial --image ktest-proc --negative no-send  --logdir v2/agent/JOURNAL
bash v2/test/parity/run.sh --mode serial --image product                 --logdir v2/agent/JOURNAL
```

## 6. Vá sau review `reviewer-kt77a` (cùng ngày)

Reviewer chấm **DRIFT** nhưng xác nhận 26/26 `source` đúng lệnh, 0 bịa, và tự chạy lại 5 cảnh khớp số.
Sáu mục đã vá trong vùng được phép (`spec/v1-parity.toml`, `v2/docs/k6-parity.md`,
`v2/test/parity/**`, `v2/agent/**`):

1. **`reason` sai hiện trạng** — sửa: `time`/`uptime` chỉ thiếu **builtin** (`ClockRead` = syscall 11
   đã có: `spec/abi.toml:117`, `user.rs:1229`, TAP `ktest_user.rs:1105`); `jobs` chỉ thiếu builtin
   (state từng TCB đã có trong reply `PS_REQ`, `shell_user.rs:189-218`) — **không** cần ABI mới.
2. **`cd`/`pwd` trước đây bị bỏ im lặng** — thêm 2 mục `deferred-K7` với `source` (tag `v1-archive`
   có: `crates/vifast-client/src/main.rs:878,883`; `docs/11-hien-tai/16-van-hanh.md:109,121`) **và**
   `counter_evidence` (`docs/v1/overnight-log.md:3664-3669` ghi đã revert) + `decision = 'cho Agent 0'`.
3. **`TcbInfo`** đã bỏ khỏi danh sách "ABI còn thiếu" trong `k6-parity.md`.
4. **D2-102a** — `help` không còn khoá regex vào `cmds=6` (`cmds=[0-9]+`); việc in nhãn `const:cmds=`
   trong `shell_user.rs:45,248-249` thuộc KT-77b (không sửa `v2/kernel/**`).
5. **Nguồn gốc ảnh (D2-109/D2-122)** — runner ghi `image_sha256`/`image_bytes`/`artifacts_bin` vào
   **log runner**; `fd202276…` gắn nhãn **trước canonical, KHÔNG dùng làm bằng chứng v2.0**; đóng băng
   `artifacts/<sha>.bin` thuộc KT-84. Log mới (không ghi đè log cũ): `*-r2b-20261117T1305.log`
   (vòng `-r2-20261117T1245` trước đó có 1 lần serial 3/4 — giữ nguyên làm bằng chứng).
6. **Lệch dòng** — `free` `2640→2641`, `free`/`df` `source_more 3470→3469`, `\r` `:23→:24`.

**Cảnh báo đã thêm (OD-K6-2 tiêu chí 6):** `ps` là `active` nhưng bên phục vụ `PS_REQ` chỉ có trong
ảnh test (`proc_demo.rs:338`) — đường sản phẩm không có procsrv; muốn `ps` trên ảnh sản phẩm phải do
**KT-77b** đưa procsrv vào đường sản phẩm. Ghi ở `k6-parity.md` §5.2 và `BLOCKED.md`.

**Kết quả đo vòng cuối** (`-r2b-20261117T1305`): transcript 4/4 (`:150-154`), serial 4/4
(`:153,294,439,583`), ca âm exit **3**/**4**/**5** (`neg-wrong-expect:151,589`;
`neg-no-send:149,575`; `product:92,348`). Vòng này có QEMU của tab KT-77b ⇒ `qemu_foreign=1`,
`verdict=KHONG_HOP_LE` (không dùng số thời gian).
