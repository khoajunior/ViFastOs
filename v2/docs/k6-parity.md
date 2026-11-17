<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K6 — parity v1 ở mức người dùng (KT-77a): `spec/v1-parity.toml` + runner serial

- **Task:** KT-77a (Agent 2 dòng OS), thi hành **OD-K6-2** của Agent 0 (`OD-K6-1`…`OD-K6-8`,
  `reports/QUYET-DINH-A0-K6-2026-11-17.md`; uỷ quyền theo **D2-121**).
- **Trạng thái:** **DONE phần KT-77a** — spec + runner + ca âm + log trong repo. **KT-77b** (shell
  trong ảnh **sản phẩm** + chạy kịch bản trên ảnh sản phẩm) **chưa làm**: ảnh sản phẩm hiện
  **không có U-mode/shell** (S1). Vì vậy "tương đương v1" **chưa** được tuyên bố.
- **Không chạm:** ABI (`spec/abi.toml`, `spec/kernel.toml`), `v2/kernel/**/src/**`, `v2/test/boot/**`
  (KT-78), `docs/v1/**`. `spec/v1-parity.toml` là tệp **mới** duy nhất trong `spec/` (D2-119).

## §1. Nguồn sự thật: `spec/v1-parity.toml`

Mỗi mục là **một lệnh shell của v1** (tag `v1-archive`) và bắt buộc có `source = file:dòng` trong
`docs/v1/`; lệnh không tìm được bằng chứng thì **không** đưa vào. Số lệnh khớp bảng lệnh shell
sinh tự động của v1: 25 lệnh ở vòng 175 (`docs/v1/overnight-log.md:2982`), 26 lệnh sau khi thêm
`loop` (`docs/v1/overnight-log.md:3397`).

| # | lệnh | đối số | `source` (docs/v1) | trạng thái |
|---|---|---|---|---|
| 1 | `help` | — | `agent2-report-21.md:81` + `agent2-report-62.md:13` | **active** |
| 2 | `echo` | `<text>` | `agent2-report-21.md:81` + `agent2-report-62.md:23` | **active** |
| 3 | `ps` | — | `overnight-log.md:2633` + `:3470` | **active** |
| 4 | `kill` | `<pid>` | `overnight-log.md:2887` | **active** |
| 5 | `time` | — | `agent2-report-21.md:81` | deferred-K7 |
| 6 | `ls` | `[path]` | `agent2-report-21.md:81` + `overnight-log.md:3470` | deferred-K7 |
| 7 | `cat` | `<path>` | `agent2-report-21.md:81` | deferred-K7 |
| 8 | `mkdir` | `<path>` | `agent2-report-21.md:81` | deferred-K7 |
| 9 | `rm` | `<path>` | `agent2-report-21.md:81` | deferred-K7 |
| 10 | `mv` | `<src> <dst>` | `agent2-report-21.md:81` | deferred-K7 |
| 11 | `ln` | `<src> <dst>` | `agent2-report-21.md:82` | deferred-K7 |
| 12 | `stat` | `<path>` | `agent2-report-21.md:82` + `overnight-log.md:2576` | deferred-K7 |
| 13 | `sync` | — | `agent2-report-21.md:82` | deferred-K7 |
| 14 | `truncate` | `<path> <size>` | `agent2-report-21.md:82` | deferred-K7 |
| 15 | `fsck` | — | `agent2-report-21.md:82` | deferred-K7 |
| 16 | `free` | — | `overnight-log.md:2640` | deferred-K7 |
| 17 | `df` | — | `overnight-log.md:2387` | deferred-K7 |
| 18 | `uptime` | — | `overnight-log.md:2576` | deferred-K7 |
| 19 | `uname` | — | `overnight-log.md:2424` | deferred-K7 |
| 20 | `run` | `<path>` | `overnight-log.md:1582` | deferred-K7 |
| 21 | `runfs` | `<path> [args...]` | `overnight-log.md:1598` | deferred-K7 |
| 22 | `exec` | `<path> [args...]` | `overnight-log.md:1843` | deferred-K7 |
| 23 | `jobs` | — | `overnight-log.md:2903` | deferred-K7 |
| 24 | `loop` | `<n>` | `overnight-log.md:3388` | deferred-K7 |
| 25 | `history` | — | `overnight-log.md:2776` | deferred-K7 |
| 26 | `!!` | — | `overnight-log.md:2776` | deferred-K7 |

- **4 active** = v2 shell **đã có** lệnh (`shell_user.rs:247-269`): `help`, `echo`, `ps`, `kill`.
- **22 deferred-K7** = lệnh v1 **chưa có** trên shell v2; lý do từng mục ghi trong
  `spec/v1-parity.toml` (`reason`). Phần lớn cần syscall/ABI mới (`MemInfo`, `TcbInfo`, `STATFS`,
  exec-from-FS) hoặc VFS client trong shell — **không** mở ABI trong KT-77a.
- Mỗi mục có **kịch bản serial**: `send` (byte, `\n` = LF), `expect` (regex khi `active`), `timeout_ms`
  và (cho `active`) `serial_min_count` / `transcript_expect`.
- **Khác biệt newline:** v1 nhận `\r` trên PL011 (`docs/v1/agent2-report-62.md:23`), shell v2 nhận
  `\n` (`v2/kernel/vifast-kbin-riscv64/init/shell_user.rs:299`). Kịch bản dùng `\n` (v2) và ghi rõ
  khác biệt trong `[meta]`.

## §2. Runner: `v2/test/parity/run.sh`

```
bash v2/test/parity/run.sh [--mode transcript|serial] [--image ktest-proc|product|<path>]
                           [--negative wrong-expect|no-send] [--smp N] [--send-delay-ms N] [--logdir DIR]
```

- **`--mode transcript`** (mặc định): boot ảnh **một** lần rồi đối chiếu đầu ra shell với `expect`
  của từng lệnh `active`. Nguồn byte vào shell là **script nội bộ của kernel**
  (`proc_demo::build_script`, `v2/kernel/vifast-kbin-riscv64/src/proc_demo.rs:270-280`), nên chế
  độ này đo "shell có lệnh + in đúng dạng", **không** đo đường serial ngoài.
- **`--mode serial`**: mỗi lệnh `active` một lần boot; byte của kịch bản được bơm vào **stdin QEMU**
  qua `stdin_wrap.sh` (FIFO + trễ 50 ms, `KT77_SEND_DELAY`). Attribution theo `serial_min_count`:
  `help`/`ps` phải xuất hiện ≥ 2 lần (kernel **cũng** tự chạy một lần), `echo` (chuỗi `K77A`) và
  `kill 999` (pid khác kernel) dùng chuỗi đặc trưng.
- **Không sửa** `v2/test/boot/qemu_guard.sh` (vùng KT-78): runner `source` tệp đó, mọi boot đi qua
  `qg_boot` (process group riêng, timeout cứng 20 s, dọn khi cha chết), `qg_infra` trước/sau.
- **Exit code**: `0` mọi mục active OK · `2` lỗi cấu hình · `3` có `not ok` · `4` timeout · `5` không
  thấy shell.
- **TAP**: `ok N - <lệnh>` / `not ok N - <lệnh>: <lý do>`; dòng tổng hợp
  `# parity: total=26 ok=M deferred=22 active=4` + `# parity: not_ok= timeout= khong_co_shell=`.

## §3. Kết quả đo (log trong repo)

**Transcript trên ảnh test** (`ktest-proc`, smp=1): 4/4 mục active OK.

```text scope=v2/agent/JOURNAL/kt77a-transcript-ktest-proc-smp1.log
147: ok 1 - help
148: ok 2 - echo
149: ok 3 - ps
150: ok 4 - kill
151: # parity: total=26 ok=4 deferred=22 active=4 (active_da_chay=4)
```

**Serial trên ảnh test** (`ktest-proc`, smp=1): 4/4 OK; marker đếm đúng (help 3 lần = 2 của ta + 1
của kernel; ps 2 lần; echo/kill 1 lần đặc trưng).

```text scope=v2/agent/JOURNAL/kt77a-serial-ktest-proc-smp1.log
150: ok 1 - help (khop=3 >= 2, waited_ms=0)
291: ok 2 - echo (khop=1 >= 1, waited_ms=0)
436: ok 3 - ps (khop=2 >= 2, waited_ms=0)
578: ok 4 - kill (khop=1 >= 1, waited_ms=0)
579: # parity: total=26 ok=4 deferred=22 active=4 (active_da_chay=4)
583: # parity: KET_LUAN=ok (exit 0)
```

Độ ổn định: 3 lần chạy lại `--mode serial` liên tiếp, mỗi lần `ok=4`
(`v2/agent/JOURNAL/kt77a-stab1.log:15`, `:kt77a-stab2.log:15`, `:kt77a-stab3.log:15`). Lý do
`help` gửi **hai** lần trong kịch bản: dòng đầu của payload trùng cửa sổ RX với dòng đầu script nội
bộ của kernel nên hay bị gộp thành một dòng; gửi hai lần cho bộ đếm ổn định 3/3 trong 5 lần đo
(xem §5.1 về `inject_ok=0`).

## §4. Ca âm (D2-108B) — ba ca, ba exit code khác nhau

1. **Gửi lệnh sai** (`zzz`) với `expect` không thể khớp ⇒ shell in `# shell: ?`, TAP `not ok`, exit
   **3**, có hạn 2000 ms (không treo):

```text scope=v2/agent/JOURNAL/kt77a-serial-ktest-proc-smp1-neg-wrong-expect.log
148: not ok 1 - help: byte da vao shell (co 2 dong 'shell: ?') nhung expect khong khop sau 2000 ms (^# shell: echo KHONG-BAO-GIO$)
579: # parity: KET_LUAN=not_ok (exit 3)
```

2. **Không gửi gì** ⇒ `expect` không xuất hiện đủ số lần ⇒ `timeout`, exit **4**:

```text scope=v2/agent/JOURNAL/kt77a-serial-ktest-proc-smp1-neg-no-send.log
146: not ok 1 - help: timeout sau 2000 ms — expect khong xuat hien du 2 lan (^# shell: help cmds=6$)
571: # parity: KET_LUAN=timeout (exit 4)
```

3. **Shell không in gì** (ảnh **sản phẩm** hiện tại — chưa có U-mode/shell) ⇒ mọi mục `not ok`,
   exit **5**; log ảnh sản phẩm **không** có dòng `# shell: up`:

```text scope=v2/agent/JOURNAL/kt77a-serial-product-smp1.log
89: not ok 1 - help: khong thay shell trong log (qemu_rc=0)
343: # parity: KET_LUAN=khong_co_shell (exit 5)
```

## §5. Giới hạn đã đo (phải đọc trước khi tuyên bố "tương đương v1")

1. **Ảnh test không có shell "tự do"**: shell trong `ktest-proc` đọc **script 6 dòng do kernel bơm**
   qua loopback 16550 (`proc_demo.rs:370-386`), rồi `exit` ngay sau dòng cuối. Byte từ serial ngoài **vào
   được** shell (bằng chứng: `# shell: ?` và các dòng `help/ps/kill` của kịch bản), nhưng **gộp
   chung cửa sổ RX** với dòng script của kernel ⇒ `inject_ok=0` và TAP nội bộ của ảnh test đỏ
   (`not ok 3`, `not ok 4`) — runner parity **không** chấm TAP nội bộ đó, chỉ chấm `expect` của
   kịch bản. Vì vậy chế độ serial trên ảnh test là **đo chuyển động**, không phải cổng chặt.

```text scope=v2/agent/JOURNAL/kt77a-serial-1-help-serial-ktest-proc-smp1.log
91: # shell: help cmds=6
93: # shell: help cmds=6
95: # shell: help cmds=6
118: # con uart: irqs=41 bytes=55 const:script_bytes=45 lines=6 inject_ok=0 drops=0 timeouts=0 head=42 tail=42 used=0 empty_polls=0 const:va=0x43000000 ring_ok=1
```

2. **Ảnh sản phẩm chưa có shell** (S1): `v2/kernel/vifast-kbin-riscv64` in `ok 1 - k3 boot` rồi
   `# k3 halt`, **0** dòng `# shell: up`. Muốn chạy kịch bản parity trên ảnh sản phẩm phải làm
   **KT-77b** (đưa U-mode/shell vào ảnh sản phẩm, OD-K6-2 tiêu chí 1–6). **Không** được nhét harness
   vào ảnh sản phẩm (OD-K6-1).
3. **Chỉ 4/26 lệnh chạy được**: 22 lệnh `deferred-K7` (bảng §1). Danh sách này là đầu vào để lead
   và Agent 0 duyệt `deferred` khi đóng K6.
4. **Đo trên máy đang bận**: `loadavg_1m=2.86..3.31` trên `ncpu=12` (nguồn:
   `v2/agent/JOURNAL/kt77a-infra.log:1-8`) — `qg_infra` kết luận `OK`,
   `qemu_ppid1=0`, `qemu_foreign=0` trước/sau; số **thời gian** không dùng để kết luận hiệu năng.
5. **Ảnh test có sẵn, không build lại trong KT-77a** (ghi kèm sha để truy vết):
   `ktest-proc` sha256 `f7c6b4a360d54e0edb04fd302e3d38a1e56680ae231cca73d2759388c281cfc1`
   (437.672 B), ảnh sản phẩm `fd20227658832a3718b390b0cd7aab563396e23e01ecbd04632f7a9c4393a6da`
   (188.552 B, trùng sha KT-76/79 trong `STATE.md`).

## §6. D2-116 / OD-K6-6 — QEMU

- Mọi boot đi qua `qemu_guard.sh` (không sửa): `qg_init` + `qg_boot` + `qg_infra`.
- Sau mỗi lần đo: `# parity: qemu_con_sot_total=0 qemu_cua_script=0` (ví dụ
  `kt77a-transcript-ktest-proc-smp1.log:154`, `kt77a-serial-ktest-proc-smp1.log:582`).
- Không có job nền ghi vào repo; log do chính runner sinh trong `v2/agent/JOURNAL/`.
- **D2-108B**: bộ đếm `qemu_con_sot_total=0` **không** có ca âm riêng trong KT-77a (nó chỉ là
  phép đếm lại của `qg_infra`, vốn có ca âm `QG_FORCE_NO_SETPGRP` ⇒ `isolation_bad>0` ở
  `v2/docs/k6-harness.md` §7 / `guard_selftest`); ghi rõ ở đây để không đọc "0" thành "đã chứng
  minh". Các bộ đếm `not_ok`, `timeout`, `khong_co_shell` **đều có** lần khác 0 trong ba ca âm §4.

## §7. Việc chưa làm

- **KT-77b**: shell + U-mode trong **ảnh sản phẩm**; chạy cùng runner với `--image product`;
  lúc đó chế độ serial trở thành cổng thật (ảnh sản phẩm không có script nội bộ cạnh tranh).
- **22 lệnh deferred-K7**: mỗi lệnh cần syscall/ABI mới hoặc VFS client trong shell — thuộc K7.
- Chưa chạy **smp 2/4** cho parity (OD-K6-2 tiêu chí 1 nói 1/2/4 hart cho ảnh sản phẩm — KT-77b).
- Chưa có ca âm "ảnh sản phẩm build với shell bị tắt" (OD-K6-2 tiêu chí 3) vì ảnh sản phẩm **chưa**
  có shell để tắt; ca âm hiện có là "ảnh sản phẩm không shell ⇒ exit 5" (§4.3).
