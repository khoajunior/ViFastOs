# KT-59 — K3b tầng 2: TCB + transition + scheduler 256 mức + MCS + hộp thư (`no_std`)

- **Ngày:** 2026-11-15 (Asia/Tokyo). **Agent 2 dòng OS**, phiên mới (`agent2-kt59`).
- **HEAD lúc mở:** `ce5519f9` ([KT-00] KT-59 READY). Trong lúc làm, lead commit `f95891bf`
  (**D2-84**: pin core ≤ 192 KiB, cấm nâng lên trần 262.144) — parent thật của commit KT-59.
- **Phạm vi:** mở rộng `v2/kernel/vifast-core` (`no_std`, `forbid(unsafe_code)`, không
  `alloc`): TCB 11 trạng thái + bảng `ALLOWED`, một cửa `transition_tcb`, scheduler 256 mức +
  ready queue tĩnh FIFO/round-robin, ngân sách MCS budget/period/refill, hộp thư FIFO 8 +
  backpressure; parity host với sim (oracle). **Không** IPC/Reply/Notification (KT-60),
  **không** nối kernel/syscall (KT-61).
- **Không đụng:** `spec/`, `v2/sim/**/src/**` (diff rỗng), `.scn`/`expected.txt`/manifest/
  `ktest.sh`, QUEUE/DECISIONS/AGENTS, `viscript/`. Chỉ stage `v2/`.

## 1. Baseline + đỏ trước (D2-05)

- `JOURNAL/kt59-...` baseline `./v2/bin/v2 gate --fast` **PASS 579 test/76 suite** (KT-58).
- **Red-before**: trước KT-59 core tầng 1 **chưa có** module `tcb/sched/mailbox/mcs` ⇒ test
  parity không biên dịch: `JOURNAL/kt59-red-before.log` — `E0433: cannot find TcbState in kc`
  (exit 101). Đây là "tính năng chưa có" (cùng kiểu red-compile KT-22), không phải test đỏ
  giả.

## 2. Core — tệp và phạm vi

| Tệp | Dòng | Nội dung |
|---|---|---|
| `src/tcb.rs` | 575 | `TcbState` 11 + payload, `ALLOWED` 11×11 (48/2/73), `TcbSnapshot`/`TcbInfo`, `SuspendMode`, `FaultKind` |
| `src/mcs.rs` | 268 | `ScBudget` (budget/period/consumed/refill/charged), `configure`/`charge`/`refill_due`, `ScInfo`, `donated_priority`, `next_boundary` bão hoà |
| `src/sched.rs` | 398 | `HartSched` 256 mức + bitmap 4×`u64` + ready queue **liên kết tĩnh** `[u16; MAX_OBJECTS+1]`, `Published` |
| `src/mailbox.rs` | 433 | `Mailbox<T,8>` FIFO + backpressure, `MailboxMsg`, `HartMailbox` (log biên 64 + `stopped`) |
| `src/core_tcb.rs` | 726 | `impl Machine`: TCB/transition/suspend/resume/finalise, MCS/tick/refill + 2 unit test F1 |
| `src/core_sched.rs` | 866 | `impl Machine`: scheduler/`dispatch`/`block_current`/`wake`/`yield`/preempt, push migration, hộp thư, `check_sched`/`check_sc_refs`/`check_budget`/`check_mailboxes` |
| `src/machine.rs` | 1483 | tầng 1 (KT-58) + trường TCB/SC/hart/mailbox + body table/free-list + `footprint()` (D2-84) |

`machine.rs` 2.893 dòng ban đầu **vi phạm R1** (>1.500) ⇒ tách thành 3 khối `impl Machine`
của **cùng một kiểu** (`machine.rs` + `core_tcb.rs` + `core_sched.rs`), không có trạng thái
thứ hai; `v2 check` PASS.

Hằng tĩnh: `MAX_TCBS=64`, `MAX_SCHED_CONTEXTS=32`, `MAX_HARTS=8`, `MAILBOX_CAP=8`,
`MAX_MAIL_LOG=64`, `MAX_FINALISE=MAX_OBJECTS=256`. Đầy body TCB/SC ⇒ `NotEnoughMemory`
**atomic** (kiểm trước khi cấp byte); hộp thư đầy ⇒ `Busy` trả lại message; log finalise đầy
⇒ `NotEnoughMemory` + `finalise_overflow`, **không** panic/cắt im lặng.

## 3. Parity sim↔core

`v2/sim/vifast-sim/tests/kt59_core_parity.rs` (2.214 dòng), **8 test**:
5 kịch bản tường minh + differential + 2 test core-only. Sau **mỗi** bước so:

- kết quả `Ok`/`Err` + tên + số mã lỗi;
- digest: `TcbState`+payload, `ReadState` (4 giá trị), `suspended`/`exit_code`/`fault_ep`/
  `cspace`/priority/MCP/affinity/home/`sched_context`/`donated_sc`/`bound_notification`,
  `ipc_buffer[60]`/`regs[4]`, `ScInfo` từng SC + `sched_remaining`/`has_budget`/
  `effective_priority`, ready queue **cả 256 mức** (`len_at`)/`pick`/`current`/`idle`/
  `published`, mailbox `len`+**toàn bộ log**+`stopped`, `finalise_log`, `pair_hits` 121 ô,
  `live_cap_count`;
- 4 bất biến `check_sched`/`check_mailboxes`/`check_caps`/`check_all_bytes` hai bên.

Kịch bản: (1) KT-18 defaults/configure/ALLOWED/suspend/resume/finalise; (2) KT-19 FIFO/
round-robin/priority/preempt/affinity; (3) KT-20 budget/refill/pick có điều kiện/enforce;
(4) KT-22 hộp thư 8 + `Busy` + `NotSupported` + drain FIFO + `Stop`; (5) KT-29 2 hart,
`published` guard, push migration đúng một TCB. Differential **16 seed × 32 op** = 512 bước,
**16 trace khác nhau**, cùng seed cùng trace. Log `JOURNAL/kt59-parity.log`:
`8 passed; 0 failed`, differential `16 seed × 32 op, 16 trace`.

## 4. D2-84 — dấu chân + bảng phân bố

`Machine::footprint()` trả 14 thành phần + scalar + padding; test pin assert tổng khớp
`size_of::<Machine>()`. Số host (`JOURNAL/kt59-parity.log`):

```text
size_of::<Machine>() = 167.896 B (64,0 % trần ảnh 262.144 B; headroom tới pin 192 KiB: 28.712 B)
tcbs[64] 39.424 · objects[256] 22.528 · caps[512] 28.672 · slot_pool 16.656 · mail[8] 28.032
sched[8] 16.832 · finalise_log[256] 12.288 · scs[32] 1.792 · còn lại 6.xxx · scalars 111 · padding 1
```

Pin KT-58 `≤ 96 KiB` (tầng 1) → **D2-84 ≤ 196.608 B** trong `kt58_core_parity.rs`; KT-59 test
pin cùng trần. `262_144` chỉ còn là **mẫu số %**, không nằm trong assert pin. Core chưa link
nên `.data+.bss` ảnh **không đổi**; KT-61 đo lại ảnh link thật.

## 5. Review độc lập `reviewer-kt59` + vá

Verdict **DRIFT mức M — chặn commit tới khi vá F1**, không UNSAFE. Đạt: pin D2-84 đúng, bảng
phân bố khớp 100 % (`-Zprint-type-sizes` độc lập), mã lỗi/transition không lệch trong phạm vi
KT-59 (ALLOWED byte-identical 48/2/73), gate 587, matrix 9/9, sim `src/` rỗng diff.

- **F1 (bug thật, bắt buộc):** `MAX_FINALISE = MAX_TCBS` sai vì body tái dùng, một `Machine`
  finalise nhiều lượt. Reviewer tái hiện **2 vòng × 64 TCB ⇒ panic `finalise_log đầy`**. Đã vá:
  `MAX_FINALISE = MAX_OBJECTS = 256`; đầy ⇒ `NotEnoughMemory` **atomic trước mọi thay đổi**,
  `finalise_overflow: u32`; bỏ `debug_assert`/cắt im lặng. Test core:
  `finalise_log_day_tra_not_enough_memory_257_lan` (257 lần gọi khi log đầy ⇒ lỗi, state không
  đổi, overflow 257, không panic) + `finalise_hai_vong_64_tcb_du_128_dong` (đúng ca reviewer).
- **F2:** sửa tên test cũ trong docs; `TcbData` 616 B ≠ object 512 B — ghi rõ "object 512 B
  theo spec, body tĩnh 616 B".
- **(3):** xác nhận `git diff` chỉ còn `262_144` làm mẫu số %, assert pin là **196.608**.
- **(4):** thêm `ipc_buffer[60]`/`regs[4]` vào digest. **Nội dung hàng chờ hộp thư** không so
  trực tiếp được vì sim **không có accessor công khai** (`Machine.mail` private) — đã ghi nợ
  §7.5 (KT-60 phải thêm accessor/kiểm qua endpoint); digest hiện so `len`+log+stopped+thứ tự.
- **Nợ KT-60 ghi đúng mã:** huỷ donation khi rời `BlockedOnReply` (`release_donation_of`) và
  xung đột `Reply.caller` ⇒ **`IllegalOperation`** (KT-34), nối tại `transition_tcb`.

## 6. Cổng + kernel

- `./v2/bin/v2 gate --fast`: **PASS 587 test/76 suite**, 0 failed; L1 1.000 seed **839 ms** (`kt59-gate.log`).
- `bash v2/test/boot/ktest.sh 1 matrix`: **9/9 boot sạch**, TAP **43/43 + 3/3 + 7/7** smp
  1/2/4 (`kt59-ktest-matrix.log`); kernel **không đổi** (core chưa link).
- Core bare-metal: `cargo build --target riscv64gc-unknown-none-elf` + `clippy --all-targets
  -- -D warnings` xanh; core unit **14 test** (12 cũ + 2 F1).
- Mutation (độc lập, log riêng): **M1** bỏ kiểm `Suspend(Defer)` ⇒ bước 24 sai mã
  (`IllegalOperation` thay `InvalidArgument`); **M2** `pick` bỏ `pick_where(budget)` ⇒ digest
  bước 8 lệch (chọn TCB không budget); **M3** không chuyển `Sleeping{Budget}` khi cạn ⇒
  `check_sched` bắt `Running` hết budget. Khôi phục + xác nhận sha256 toàn bộ `src/*.rs`.

## 7. Biên giới + nợ (chi tiết `docs/k3b-core.md` §6/§7.5)

- Không có body/hàng chờ Endpoint/Notification/Reply/IPC (KT-60); `unlink_tcb_queues` là mốc;
  không có hook `Reply.caller`/waiter Notification.
- Donation KT-21 mới có trường + công thức, chưa có đường tạo (KT-60).
- `TlbFlush` trong hộp thư là placeholder (KT-61 nối TLB thật).
- `MAX_MAIL_LOG=64`: quá biên message vẫn xử lý, chỉ nhật ký dừng + `mail_log_overflow`.
- `MAX_FINALISE=256`: một `Machine` biểu diễn ≤ 256 finalise (thực tế ≤ số object id còn
  cấp được); vượt ⇒ `NotEnoughMemory`, caller tái tạo `Machine`.
- `ipc_buffer`/`regs` chỉ giữ chỗ + accessor (KT-60 mới ghi qua `transfer`).

## 8. Commit

`[KT-59]` mã + parity + docs = **`5374920a`**; `[KT-59b]` journal/STATE. Chỉ `v2/`; push sau
khi xanh; kiểm `git rev-list --count HEAD..origin/main = 0` trước push.
