<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3b — port core `no_std`: tầng 1 (KT-58) + TCB/scheduler (KT-59) + IPC/Reply/Notification/WaitAddr/fault (KT-60)

- **Trạng thái:** KT-58 **DONE**; **KT-59 DONE**; **KT-60 DONE**; **KT-61 DONE**; **KT-62 DONE**;
  **KT-63 DONE (đề xuất — chờ review độc lập + Agent 0 lật QUEUE)**: core `no_std` **chạy trên
  nhân thật** (TAP 43..55 trên QEMU, soak 0/1000 ảnh sản phẩm, matrix smp 1/2/4, conformance
  5/5) + **chuyển cap qua IPC** + **finalise IPC khi Revoke/Delete** (§12). **K3b xong phần core
  chạy được**; procsrv/shell là **K3.5** — xem §9 + `agent/reports/K3B-DOD.md`.
- **Nguồn sự thật:** sim `v2/sim/vifast-sim/src/kobj.rs` là **oracle**; task này **không**
  thiết kế lại ngữ nghĩa, không sửa `spec/`, không đụng `viscript/`.
- **Crate:** `v2/kernel/vifast-core` — `#![no_std]`, `#![forbid(unsafe_code)]`, **không**
  `alloc`/`Vec`/`String`; thêm vào workspace `v2/kernel/Cargo.toml` nhưng **chưa** crate nào
  của kernel phụ thuộc (chưa nối syscall — đúng phạm vi KT-58).
- **Nhãn:** `[TK]` hằng/quyết định kỹ thuật; số đo chỉ lấy từ log trong `v2/agent/JOURNAL/`.

## 1. Phạm vi

| Hạng mục | KT-58 | Ghi chú |
|---|---|---|
| `Untyped` (watermark, cỡ/căn lề, Retype, Revoke trả byte) | **CÓ** | port nguyên `Machine::retype`/`revoke_children` của sim |
| `CNode` + MDB (Copy/Mint/Delete/Revoke, cha–con, reparent, finalise lồng) | **CÓ** | slot 48 B theo spec, nhưng **id cap** lưu trong pool `.bss` (nợ §6) |
| Capability (4 quyền, badge, subset, kế thừa quyền khi Retype) | **CÓ** | `RIGHT_*` = thiết kế §4.3/QD §3.12, không nằm trong `spec/abi.toml` |
| Bất biến `check_caps` / `check_bytes` / `check_all_bytes` | **CÓ** | cùng bốn nhóm của sim; `Vec`/`sort` → mảng tĩnh/O(n²) |
| TCB/transition/scheduler/MCS/hộp thư | **CÓ** (KT-59) | 11 trạng thái + bảng ALLOWED, một cửa `transition_tcb`, 256 mức ưu tiên, budget/period/refill, mailbox FIFO 8 + backpressure |
| Endpoint/Notification/Reply/IPC/WaitAddr/fault | **CÓ** (KT-60) | body + hàng chờ FIFO 8, `Call/Reply/ReplyRecv/NBSendRecv/NBSendWait`, badge OR/wait/wake/bound, WaitAddr futex **không timeout**, donation KT-21, fault `Exit` không Reply; **chưa** chuyển cap (nợ §8.5) |
| `Frame`/`PageTable`/`VSpace` | không | đã có ở K3 (`vifast-plat-common::obj`, KT-52); `Retype` các loại này trong core trả `NotSupported` |
| Nối syscall / `init` / ảnh kernel | **KT-61**: core **link + chạy** trên ảnh sản phẩm và ảnh test (`ktest_core`, TAP 43..52); syscall/`init` U-mode vẫn là K3.5 | §9 |

## 2. Crate và API

| Tệp | Dòng | Nội dung |
|---|---|---|
| `src/lib.rs` | 57 | `no_std`, `forbid(unsafe_code)`, tài liệu phạm vi, re-export |
| `src/spec.rs` | 35 | hằng sinh từ `spec/abi.toml` + 4 bit quyền (thiết kế §4.3) |
| `src/error.rs` | 281 | `ObjType` (16), `KError` (19, tên/số từ `vifast_abi::ERRORS`), `SlotRef`, `CapInfo`, `CheckFailure` |
| `src/arena.rs` | 89 | arena **số học** của một Untyped (bump + align, không con trỏ) |
| `src/slotpool.rs` | 162 | pool word cap của CNode trong `.bss` (bump + free-list hole) |
| `src/machine.rs` | 1060 | `Machine`: cấp phát, truy vấn, Retype, Copy/Mint/Delete/Revoke, MDB |
| `src/check.rs` | 235 | `check_caps`, `check_bytes`, `check_all_bytes` |
| `src/tcb.rs` | 575 | `TcbState` 11 trạng thái + payload, `ALLOWED` 11×11 (48/2/73), `TcbSnapshot`/`TcbInfo`, `SuspendMode`, `FaultKind` (kiểu dữ liệu) |
| `src/mcs.rs` | 268 | `ScBudget` (budget/period/consumed/refill/charged), `ScInfo`, `donated_priority`, `next_boundary` bão hoà |
| `src/sched.rs` | 398 | `HartSched` 256 mức + bitmap 4×`u64` + ready queue liên kết tĩnh, `Published` |
| `src/mailbox.rs` | 433 | `Mailbox<T, CAP=8>` FIFO + backpressure, `MailboxMsg`, `HartMailbox` (log biên `MAX_MAIL_LOG=32` — D2-85b + `stopped`) |
| `src/core_tcb.rs` | 726 | khối `impl Machine`: TCB/transition/MCS (tách khỏi `machine.rs` để giữ R1) |
| `src/core_sched.rs` | 879 | khối `impl Machine`: scheduler/hộp thư/`check_sched`/`check_sc_refs`/`check_budget`/`check_mailboxes` |
| `src/ipc.rs` | 520 | **KT-60**: `MessageInfo` 35 bit, `Message` 64 MR, `IpcArea` (4+60), `transfer`; **KT-63**: `CapSnapshot`/`CapPlan`/`plan_cap_transfer` + `Message` mang `CapId` nguồn (≤ 4) |
| `src/objstate.rs` | 265 | **KT-60**: body Endpoint/Reply/Notification, `IpcQueue` FIFO 8, pool `QueuedMsg`, nút WaitAddr/`AddrWord` |
| `src/core_ipc.rs` | 1421 | **KT-60**: `Call/Send/Recv/NBRecv/Reply/ReplyMsg/ReplyRecv/NBSendRecv/NBSendWait`, `Exit`/`inject_fault`, `unlink_tcb_queues`, donation KT-21; **KT-63**: wire cap transfer (`resolve_caps`/`prepare_caps`/`deliver_message`) vào mọi đường IPC + receive window API (§12) |
| `src/core_capxfer.rs` | 136 | **KT-63**: `deliver_caps`/`deliver_message`/`resolve_caps`/`prepare_caps` — cài cap con **bỏ `RIGHT_GRANT`** vào receive window, cập nhật `MessageInfo.caps`/`badge`, thất bại mềm |
| `src/core_finalise.rs` | 111 | **KT-63**: finalise IPC khi `Revoke`/`Delete` — `cancel_reply_object`, `clear_dead_reply_in_waiters`, `finalise_endpoint_body`, `finalise_notification_body` |
| `src/mdb.rs` | 105 | **KT-63**: tách helper cây MDB (`collect_subtree`/`collect_untyped_tree`/`push_id`/`sort_dedup`) khỏi `machine.rs` để giữ R1 ≤ 1.500 dòng |
| `src/core_sync.rs` | 519 | **KT-60**: Notification (signal/wait/poll/bind) + WaitAddr (wait/wake/giá trị) |
| `src/check_ipc.rs` | 395 | **KT-60**: `check_ipc`/`check_reply_links`/`check_notifications`/`check_waitaddr`/`check_fault` |
| `src/footprint.rs` | 152 | **KT-60**: bảng phân bố 27 dòng + pin D2-84 (tách khỏi `machine.rs` để giữ R1); **KT-63**: scalars 12 × `u64` (3 bộ đếm cap) |
| `src/{captable,tcbdata}.rs` | 60+102 | tách khỏi `machine.rs` (R1 ≤ 1.500 dòng) |
| `tests/kt58_core_parity.rs` (trong sim) | 2680 | parity host KT-58: 14 kịch bản + differential 32×48 + 2 test core-only |
| `tests/kt59_core_parity.rs` (trong sim) | 2214 | parity host KT-59: 5 kịch bản KT-18/19/20/22/29 + differential 16×32 + 2 test core-only |
| `tests/kt60_core_parity.rs` (trong sim) | 2730 | parity host KT-60: 6 kịch bản KT-24/25/27/28/26/21+34 + 3 test core-only + pin footprint |

API chính (giữ tên/ngữ nghĩa sim):

```text
add_untyped(base,size) -> ObjId            add_cnode(slots) -> ObjId
add_kernel_object(ty) -> ObjId             root_cap(obj) -> CapId
put_cap(cnode,slot,cap) -> Result          retype(src: SlotRef, ty, size_bits, flags) -> Result<CapId>
copy / copy_rights / mint                  delete(slot) / revoke(cnode, slot?) -> Result<usize>
cap_at / cap_obj / cap_obj_at / cap_info   cap_table() -> Iterator<CapInfo>
cap_children(cap) -> Iterator<CapId>       cnode_slot_count / object_{alive,type,bytes,count}
untyped_{size,used,free,allocated,gap,arena} / total_used
check_caps / check_bytes / check_all_bytes -> Result<(), CheckFailure>

// KT-60 (giữ tên/ngữ nghĩa sim)
call(tcb, ep, mr) / call_msg(tcb, ep, msg)     send(tcb, cap, msg) / nb_send(...)
recv(tcb, ep, reply?) / nb_recv(tcb, ep)       reply(tcb, reply) / reply_msg(...) / reply_recv(...)
nb_send_recv / nb_send_wait                    notification_signal / notification_wait / notification_poll
tcb_bind_notification / tcb_unbind_notification
wait_addr(tcb_slot, frame, offset, expected)   wake_addr(frame, offset, n) / set_addr_word / addr_word
exit(tcb, code) / inject_fault(tcb, kind)      unlink_tcb_queues (nội bộ) / release_donation_of (hook)
endpoint_info / endpoint_queued / endpoint_msg_ids / endpoint_last_mr / reply_caller
notification_{word,badge,waiter,bound_sc} / tcb_notification / tcb_last_fault / tcb_last_ipc_error
ipc_{sent,delivered,dropped,pending} / transfer_calls / mail_items
check_ipc / check_reply_links / check_notifications / check_waitaddr / check_fault
```

**D2-83:** `untyped_arena` của core trả **base nhãn PA** (`Region.base`: Untyped con =
`base_cha + offset`), **không** phải địa chỉ block heap như `sim::untyped_arena`; parity và
digest **không** so `base` (chỉ `capacity` + thống kê byte). Mọi accessor khác giữ nguyên
ngữ nghĩa sim.

## 3. Bản đồ sim → core (giữ nguyên **ngữ nghĩa cap/MDB/byte**; `Region.base` khác — D2-83)

| Sim (`kobj.rs`) | Core (`vifast-core`) | Ghi chú |
|---|---|---|
| `Vec<Meta>` / `Vec<Cap>` | `[Meta; MAX_OBJECTS]` / `[Cap; MAX_CAPS]` | id tuần tự 1-based, **không tái dùng** như `Vec` của sim |
| `Untyped` region (`OwnedMem`, `Arena::used`) | `arena::Region { base, capacity, cursor }` | watermark = `cursor`; `Retype` cấp `offset = align_up(cursor, align)`, `cursor += len`; **`base` là nhãn PA (D2-83)**, sim trả địa chỉ block thật — parity không so `base` |
| `Kind::CNode { slots: Vec<Option<CapId>> }` | `SlotPool` + `Meta{slot_base, slot_len}` | slot 48 B vẫn được **kế toán byte** trong Untyped (`cnode_base + slots*48`) |
| `Machine::retype` | `Machine::retype` | cùng thứ tự kiểm: cap→WRITE→GUEST→`is_core`→cỡ/căn→cấp byte→cap con kế thừa quyền |
| `retype` Untyped con: `len > ARENA_ALIGN ⇒ Alignment` | y hệt (`ARENA_ALIGN = page_bytes = 4096`) | KT-17 |
| `Machine::copy`/`copy_rights`/`mint`/`derive` | y hệt | `Some(0)` badge = xoá; badge chỉ Endpoint/Notification |
| `Machine::delete` | y hệt | con của cap bị xoá ⇒ **ông gần nhất còn sống** |
| `Machine::revoke`/`revoke_children`/`collect_subtree`/`collect_untyped_tree` | y hệt | Untyped: finalise cả cây (kể cả Untyped con), cap trong CNode chết + cap trỏ object chết; reset watermark |
| `Machine::check_caps`/`check_bytes` | `check.rs` | bốn nhóm bất biến; overlap kiểm O(n²) thay sort (không cấp phát) |
| `ObjType::{name,from_name,is_core,fixed_size}` | y hệt, cỡ đọc từ `vifast_abi::OBJ_*` |
| `KError::{id,name}` | y hệt, đọc `vifast_abi::ERRORS` |
| `TcbData` + `transition_tcb`/`tcb_configure`/`tcb_suspend`/`tcb_resume`/`finalise_tcb` | `machine::TcbData` + `core_tcb.rs` | cùng bảng `ALLOWED`, cùng mã lỗi; `Reply.caller`/notification chưa có (KT-60) |
| `Tcb.SetPriority/SetAffinity/SetHome/SetSchedParams/ReadState/Info` | y hệt | cùng kiểm MCP/affinity/home, cùng snapshot |
| `SchedControl.Configure` + `ScBudget` + `tick`/refill/`tcb_has_budget` | `mcs` + `core_tcb.rs` | chu kỳ thẳng hàng, charge trước — refill sau, `pick` chỉ TCB có budget |
| `HartSched` 256 mức + bitmap | `sched::HartSched` | `VecDeque` → danh sách liên kết tĩnh `[u16; MAX_OBJECTS+1]`, cùng FIFO/`pick` O(1)/`last_at` |
| `Machine::{dispatch,block_current,wake,yield_current,preempt}` | y hệt | cùng bất biến `Ready` ⇔ hàng, `Running` ⇔ `current`, ≤1 `Running`/hart |
| `Machine::{post_mail,process_mail,send_mail}` + `Mailbox`/`HartMailbox` | `mailbox` + `core_sched.rs` | FIFO 8, đầy `Busy`; `Migrate` 1 hart `NotSupported`; 2+ hart push migration |
| `Machine::check_sched`/`check_mailboxes` | `core_sched.rs` | trả `CheckFailure{what,a,b}` thay `String` (cùng điều kiện) |
| `EndpointState`/`ReplyData`/`NotificationState` + `QueuedMessage` | `objstate.rs` + `core_ipc.rs`/`core_sync.rs` | KT-60: hàng FIFO 8, message trong pool chung; cùng mã lỗi/thứ tự sim |
| `Kind::Endpoint/Reply/Notification` body | `Machine::{endpoints,replies,notifications}` + free-list | KT-60: biên tĩnh `MAX_ENDPOINTS/REPLIES/NOTIFICATIONS`; revoke trả body (finalise IPC đầy đủ: nợ §8.6) |
| `ipc::MessageInfo`/`Message`/`IpcArea`/`transfer` | `ipc.rs` + `TcbData.ipc` | KT-60: 35 bit/64 MR/4+60; `transfer` là đường copy duy nhất |
| `wait_addr`/`wake_addr` + `address_waiters`/`addr_words` | `core_sync.rs` + bảng bucket 64 | KT-60: khoá đầy đủ `(frame,offset)`, FIFO, không timeout (D2-53 Q4) |
| `donate_sc`/`return_donation`/`release_donation_of` | `core_ipc.rs` + hook `transition_tcb` | KT-60: donation tự động `Call`/`Reply`, huỷ khi rời `BlockedOnReply`/exit/finalise |
| `check_ipc`/`check_reply_links`/`check_notifications`/`check_waitaddr`/`check_fault` | `check_ipc.rs` | KT-60: cùng điều kiện sim, `CheckFailure` thay `String` |
| `endpoint_queued`/`mail_items` (KT-60 accessor **đọc**) | `machine`/`kobj` (sim) + `core_ipc.rs`/`core_sched.rs` | đóng nợ §7.5: parity so nội dung hàng chờ |

## 4. Parity — cùng bộ test logic trên sim và core

`v2/sim/vifast-sim/tests/kt58_core_parity.rs` chạy **một** `Op` trên hai `Machine` qua trait
`View`/`Mut`, sau **mỗi** bước so:

1. kết quả `Ok`/`Err` + **tên và số** mã lỗi (19 mã sinh từ spec);
2. digest toàn trạng thái: object (kiểu/alive/`object_bytes`), cap (obj/kiểu/cha/slot/quyền/
   badge/alive), từng slot của từng CNode sống, thống kê byte của từng Untyped;
3. `check_all_bytes` của **cả hai**; `check_caps` khi mọi cap con đã vào slot (sim cho phép
   trạng thái trung gian "cap Retype chưa đặt", nên chỉ kiểm ở trạng thái đóng — hai bên cùng
   luật).

Kịch bản tường minh (kỳ vọng **cứng**, không chỉ so hai bên):

- `bytes` — KT-09 bốn object lõi xếp liền, `used/allocated/gap/free` từng bước;
- `exhaust` — vừa khít 1024 B ⇒ `NotEnoughMemory` **không đổi watermark**;
- `retype_errors` — `SlotEmpty/RangeError/WrongObjectType/Alignment/InvalidArgument/
  NotSupported`, GUEST, `size_bits` 64 (không panic tràn shift), mọi lỗi không đốt byte;
- `nested_align` — Untyped con 4096 B ⇒ `offset=4096`, `used=8192`, `gap=3584` (KT-17);
- `revoke_nested` — Revoke Untyped cha finalise Untyped con + object trong con, watermark cả
  hai về 0, cap chết không còn slot;
- `double_free` — revoke lần hai `SlotEmpty`, không trả byte lần hai;
- `mdb_tree` — reparent cho ông, revoke chỉ nhánh con/cháu, `revoke(None)` xoá mọi cây con;
- `finalise_cnode` / `finalise_cnode_nested` — cap nằm trong CNode bị finalise chết theo (kể cả
  CNode lồng nhau, cap trỏ object **ngoài** Untyped);
- `rights` — `Mint(Some(0))` xoá badge, Copy/Mint không nới quyền, Retype thiếu WRITE ⇒
  `InsufficientRights`, cap con Retype **kế thừa** quyền;
- `retype_revoke_loop` — 20 vòng Retype→Revoke, byte về 0 mỗi vòng (không rò);
- `dead_cnode_errors` — `Revoke`/`put_cap` trên CNode **đã finalise** ⇒ `InvalidCapability`
  (D2-83; sim đọc body → `Dead`), kể cả `slot` ngoài dải: **body trước, metadata sau**;
- `delete_root_cap` — xoá cap gốc ⇒ con thành gốc (`parent = None`), slot giữ nguyên;
- `badge_and_align` — `put_cap` cap đã có slot ⇒ `IllegalOperation`; badge `Some(7)` trên
  Tcb ⇒ `InvalidArgument` còn `Some(0)` hợp lệ; Retype Untyped `len` 8192 > `ARENA_ALIGN`
  ⇒ `Alignment` (qua cổng `len <= u_size` trước).

Tổng: **14 kịch bản** + **1 differential** + **2 test core-only** (`core_d2_83_vm_reserved_not_supported`
pin D2-83; `core_hang_so_spec_va_dau_chan_tinh` pin hằng ABI/dấu chân tĩnh).

Differential: 32 seed × 48 op (LCG tất định), phủ 2 Untyped + 3 CNode + Endpoint/Notification,
cả thao tác hợp lệ và lỗi. Log `JOURNAL/kt58-parity.log` (đo 2026-11-15):

```text
# parity differential: Stats { steps: 2016, ok: 877, err: 1139, retype_ok: 57,
#   copy_ok: 71, mint_ok: 13, delete_ok: 37, revoke_ok: 219, revoke_deleted: 79,
#   caps_closed: 1138 }
test result: ok. 4 passed; 0 failed
```

**L1/T-INFRA:** baseline `86.297 ms` (`kt58-baseline-gate.log`) đo khi worktree đang được
sửa/recompile lại (máy bận/cold), gate cuối **833 ms** — cùng 1.000 seed, **không** nới
ngưỡng; số chuẩn để so là gate cuối.

**Đỏ trước** (D2-05) và mutation (log trong `JOURNAL/`):

| Mã | Đột biến | Kết quả (log) |
|---|---|---|
| **Red-before** | lần chạy parity **đầu tiên**: core trả `object_type` cho ô `Meta` rỗng (256 ô thay vì `n_objects`) | đỏ digest (`bytes#0`, `diff-0#0`) — `kt58-red-before.log` |
| **M1** (độc lập) | `Mint(Some(0))` giữ badge nguồn thay vì xoá | đỏ digest `rights#8` — `kt58-mutation-m1.log` |
| **M2** | `delete` không reparent con | đỏ `check_caps` — `kt58-mutation-m2.log` |
| **M3** | `Revoke` Untyped không reset arena | đỏ `CheckUntyped used` — `kt58-mutation-m3.log` |

M1..M3 độc lập nhau và độc lập red-before; mỗi lần sửa tạm `machine.rs`, chạy test, **khôi
phục** sha256 `acde4970…` (xác nhận trước/sau).

### 4.1 KT-59 — TCB/transition/scheduler/MCS/hộp thư

`v2/sim/vifast-sim/tests/kt59_core_parity.rs` chạy **cùng** `Op` trên hai `Machine` qua trait
`View`/`Mut`, sau **mỗi** bước so:

1. kết quả `Ok`/`Err` + **tên + số** mã lỗi;
2. digest TCB: `TcbState` (kèm payload: hart/ep/call/reply/ntfn/frame+offset/reason),
   `Tcb.ReadState` 4 giá trị, `suspended`/`exit_code`/`fault_ep`/`cspace`/priority/MCP/
   affinity/home/`sched_context`/`donated_sc`/`bound_notification`;
3. digest ngân sách: mọi `SchedContext` (`budget/period/consumed/refill/charged/configured`
   + donation hai chiều), `sched_remaining`, `tcb_has_budget`, `effective_priority`;
4. digest scheduler mỗi hart: `published` (`idle`,`cur_prio`), `current`, `idle`, `cur_prio`,
   `ready_len`, `len_at` **cả 256 mức**, `pick`, `ready_contains`, FIFO (so qua trace dispatch/
   yield), `migrate_count`;
5. digest hộp thư mỗi hart: `len`, **toàn bộ nhật ký** đã xử lý (tag + trường `Wake`/`Migrate`/
   `TlbFlush`), `stopped`; `finalise_log`; `pair_hits` 121 ô + coverage; `live_cap_count`;
6. bốn bất biến `check_sched` + `check_mailboxes` + `check_caps` + `check_all_bytes` của **cả
   hai** phải `Ok` (digest ghi 4 cờ).

Kịch bản tường minh (kỳ vọng **cứng**, không chỉ so hai bên):

- `kt59_parity_tcb_va_bang_allowed` — defaults TCB (Inactive/priority 0/MCP 0/affinity 1/
  home 0/không SC); `Tcb.Configure` gắn CSpace/xoá `exit_code`; `SetFaultEp` đúng Endpoint;
  `Inactive → Running` cấm; chuỗi ALLOWED `Ready → BlockedOnSend → BlockedOnReply → … →
  Sleeping` (qua `Ready`/`Running` đúng bảng); `Ready → Sleeping` cấm; `Suspend(Defer)` =
  `InvalidArgument`; `Suspend`/`Resume`/`Transition` trên `Dead` = `IllegalOperation`;
  `Suspend(Abort)` trên `Sleeping` **giữ** `Sleeping` + bật cờ; `Ready+suspended` **không**
  vào hàng; finalise `Inactive` được; log finalise.
- `kt59_parity_scheduler_fifo_round_robin` — FIFO cùng mức, `yield` xoay vòng 3 TCB; MCP <
  priority = `InvalidArgument`; rebucket priority; `block_current` chỉ nhận `BlockedOn*`/
  `Sleeping`; `wake`; preempt mỗi bước hai TCB cùng mức xen kẽ; affinity mask 0/bit lạ/home
  ngoài affinity = `InvalidArgument`.
- `kt59_parity_budget_va_refill` — `Configure` budget 0 / period 0 / budget > period =
  `InvalidArgument`; 4/8; TCB không SC **không được** `pick` (D2-50); charge đủ 4 tick ⇒
  `Sleeping{Budget}`; dispatch khi hết budget trả `None`; biên refill tại `now=8` đánh thức
  `Ready`; gỡ SC khi `Running` ⇒ ngủ ngay; Configure lại giữa chu kỳ đặt `consumed = 0`.
- `kt59_parity_hop_thu` — 8 message đầy hàng FIFO; thứ 9 ⇒ `Busy` (không ghi đè); hart không
  có ⇒ `InvalidArgument`; `Migrate` trên 1 hart ⇒ `NotSupported`; drain đúng 8 message một
  lần mỗi message; `send_mail(Stop)` đặt cờ `stopped`; nhật ký đúng thứ tự.
- `kt59_parity_nhieu_hart_va_push_migration` — `with_harts(2)`, affinity/home, `publish`
  guard luật 3 (hart khác `current_hart` ⇒ `IllegalOperation`, "nói dối" ⇒ `IllegalOperation`),
  `dispatch` hart 1, `Running → Ready` qua cổng, push **đúng một** TCB cuối hàng mức cao nhất
  sang hart rỗng, `migrate_count = 1`.

Differential: **16 seed × 32 op** (LCG tất định) trên rig 2 hart/4 TCB/3 SC — 16 trace khác
nhau, cùng seed cùng trace; phủ `SetPriority`/`Transition`/`Suspend`/`Resume`/`Tick`/
`Dispatch`/`Yield`/`Wake`/`BlockCurrent`/mailbox/`Migrate`/`PushMigration`/`Publish`/
`SetAffinity`/`ScConfigure`.

Test core-only: `core_bien_tinh_tcb_va_sched_context` (đầy `MAX_TCBS`/`MAX_SCHED_CONTEXTS` ⇒
`NotEnoughMemory` **nguyên tử**, revoke trả body về free-list rồi retype lại được) và
`core_dau_chan_tinh_va_bang_phan_bo_d2_84` (bảng phân bố 14 dòng + pin D2-84 + hằng bảng 48/2/73).

Log: `JOURNAL/kt59-parity.log` (đo trên máy này).

**Biên KT-59:** không có Endpoint/Reply/Notification nên differential chỉ dùng trạng thái
scheduler thuần; các trạng thái chặn IPC được kiểm qua `transition_tcb` trực tiếp (không đi
qua `check_ipc` của sim). Donation KT-21 (gắn `Call`/`Reply`) chưa có đường tạo — chỉ pin
công thức `donated_priority` bằng unit test core, parity phủ trạng thái **không donation**.
Đây là biên task, không phải nới kỳ vọng.

## 5. Hằng số ABI — sinh từ spec, không hardcode

- `v2/lib/vifast-abi/src/lib.rs`: thêm `#![cfg_attr(not(test), no_std)]` để kernel dùng chung
  crate sinh (D2-11).
- `v2/tools/vifast-gen/gen.py`: sinh thêm `SLOT_BYTES`, `SLOT_ALIGN_BYTES`,
  `CNODE_BASE_BYTES`, `DEFAULT_ALIGN_BYTES`, `PAGE_BYTES`, `UNTYPED_MIN_BYTES`,
  `OBJ_{TCB,ENDPOINT,NOTIFICATION,REPLY,SCHED_CONTEXT,SCHED_CONTROL}_BYTES` từ
  `spec/abi.toml` mục `[object_size]`. `generated.rs` là **đầu ra**, không sửa tay;
  `gen-check` xanh. `docs/abi.md` **không đổi** (gen chỉ bổ sung code Rust).
- `spec/` **không sửa**; 4 bit quyền là hằng thiết kế §4.3/QD §3.12 (spec không có bảng
  quyền) — ghi rõ trong `spec.rs`.

## 6. Khác biệt có chủ đích so với sim + nợ

1. **Bảng tĩnh thay `Vec`.** `MAX_OBJECTS = 256`, `MAX_CAPS = 512`, `MAX_UNTYPEDS = 16`,
   `MAX_SLOT_WORDS = 4096` (`[TK]`). Retype/copy khi bảng đầy trả `NotEnoughMemory` **nguyên
   tử** (sim không có giới hạn này vì heap không biên). Đây là biên triển khai, không phải ABI.
2. ~~**Slot CNode trong `.bss`, chưa nằm trong arena vật lý.**~~ — **ĐÓNG KT-64** (§13.2): byte
   slot CNode **Retype từ Untyped** nằm trong byte arena tại `offset + cnode_base`
   (`kernel/vifast-core/src/slotmem.rs`); `SlotPool` bị xoá; tiêu chí §9.6 đã đạt. (Trước KT-64:
   `CapId` lưu ở `SlotPool` ⇒ hai nguồn cho cùng cap; KT-61 giữ nguyên có chủ đích.)
3. **Body đã có cho TCB/SchedContext/Endpoint/Reply/Notification** (KT-59/60) trong bảng
   `.bss` có free-list; `Frame`/`PageTable`/`VSpace` vẫn model-only (D2-83). Vì vậy
   `finalise_tcb` đã port (kèm hook donation/KT-34), nhưng **finalise IPC object khi Revoke**
   (huỷ `Reply.caller`, đánh thức Recv waiter, huỷ hàng) **chưa** đầy đủ — nợ §8.6; nó không
   đổi cap/MDB/byte của tầng 1.
4. **`Retype` reserved — biên giới thiết kế (D2-83):** core tầng 1 **không** Retype
   `Frame`/`PageTable`/`VSpace` (và mọi reserved) — trả `NotSupported`, **không phải thiếu
   sót**: object layer `vifast-plat-common::obj` (KT-52) sở hữu VM. Test pin
   `core_d2_83_vm_reserved_not_supported` giữ biên giới này; parity sim↔core **loại trừ**
   Retype VM theo D2-83. KT-61 nối invocation VM qua obj layer (cầu nối), **không** nhân đôi
   chủ sở hữu; đổi biên giới cần D2 mới.
5. **`purge_dead_refs` vẫn rút gọn:** core dọn slot ↔ cap khi xoá/revoke; với body IPC chỉ
   trả body/message về free-list, **chưa** quét tham chiếu chết trong Endpoint/
   Reply/Notification/WaitAddr như `purge_dead_refs` của sim (nợ §8.6, parity không phủ).
6. **Dấu chân tĩnh:** `size_of::<Machine>() = 180.168 B` `[ĐO]` (KT-60 + D2-85b, log
   `JOURNAL/kt60-parity.log`). **KT-61 đã link vào ảnh kernel** và đo lại: `.data+.bss` tăng
   đúng `180.168 B` mỗi ảnh, vẫn ≤ trần 262.144 B (`K3B-DOD.md` §6) — **không** phải cắt.
7. **Vùng Untyped là số học + nhãn PA (D2-83):** `Region.base` là **nhãn PA** (Untyped con =
   `base_cha + offset`), chưa cấp/đọc byte thật (chưa có Frame/PageTable trong core).
   `untyped_arena` vì vậy **khác** sim (sim trả địa chỉ block heap thật); không được mô tả
   "y hệt sim", và digest parity **không** so `base` — chỉ so `capacity`/`used`/`free`/
   `allocated`/`gap`.

8. **Hàng chờ IPC đã có (KT-60)** — `unlink_tcb_queues` thật, hook `Reply.caller` (KT-34) và
   waiter Notification (KT-27) nối tại `transition_tcb`; còn nợ **finalise IPC object khi
   Revoke** (huỷ caller/waiter/hàng) và **chuyển cap** — §8.6.
9. **Donation KT-21 đã hoạt động (KT-60).** `Call`/`Reply` tạo/trả donation; `transition_tcb`
   và `Suspend`/`Exit`/`finalise` gọi `release_donation_of`. `check_sc_refs` cho phép **donor**
   (chủ SC) giữ `sched_context` khi SC đang donate (KT-60 sửa điều kiện KT-59 vốn cấm mọi
   `sched_context` trên SC đang donate).
10. ~~**`TlbFlush` trong hộp thư là placeholder.**~~ — **ĐÓNG KT-64** (§13.1): `TlbFlush` **thật**
    qua trait `TlbEnv` (arch `sfence.vma`), post tới `active_on`, chờ đủ ack, `dispatch` **chặn**
    trên hart còn flush chưa phục vụ; TAP 59 chứng minh bằng page fault thật sau `unmap`.
11. **Nhật ký hộp thư có biên** `MAX_MAIL_LOG = 32`/hart (sim `Vec` không biên; D2-85b giảm 64→32). Quá biên:
    message vẫn xử lý đúng một lần, bất biến hàng đợi giữ nguyên, chỉ **nhật ký** dừng ghi và
    `mail_log_overflow` tăng. Không phải mất message.
12. **`IpcArea` + `transfer` đã có (KT-60)** (4 MR thanh ghi + 60 word buffer + info/badge/
    msg_id); **KT-61 vẫn chưa ghi từ syscall** (đường syscall U-mode là K3.5). **Object** TCB 512 B theo spec, **body tĩnh** trong
    core 704 B (`IpcArea` 544 B + trường scheduler/state/IPC).
13. **Dấu chân tĩnh (KT-61 — đã link):** `size_of::<Machine>() = 180.168 B` `[ĐO]` (pin
    D2-84 **196.608 B**, còn 16.440 B). Core **đã link** vào ảnh: `.data+.bss` tăng đúng
    **180.168 B** trên cả hai ảnh (sản phẩm và test) — số đo từng ảnh + trần 262.144 B ở
    `K3B-DOD.md` §6; static nằm trong **vùng ảnh** (TAP `core link`) và ở **`.bss`** (NOBITS —
    `filesz` không chứa 180 KB) với `init_zeroed` tại chỗ (§9.5). Không nới pin, không nới trần.
14. **Không có `alloc`/`Vec`/`String`/`unsafe` mới:** ready queue là danh sách liên kết tĩnh
    `[u16; MAX_OBJECTS+1]`, body TCB/SC/IPC là bảng `.bss` có free-list (revoke trả body), hộp
    thư và hàng Endpoint là mảng FIFO cố định, message đang chờ nằm trong pool chung
    `[QueuedMsg; MAX_QUEUED_MESSAGES]` (hàng Endpoint giữ **chỉ số** `u16`).
15. **Chưa chuyển cap qua IPC (KT-60):** `Message` giữ hình dạng `caps` của sim nhưng
    `MessageInfo.caps > 0` bị từ chối `InvalidArgument`; receive window 4 ô +
    `plan_cap_transfer` (mất `RIGHT_GRANT`, unwrap badge, thất bại mềm) là nợ **K3.5** — §8.6.
    Parity KT-60 **không** dựng message có cap; đây là biên task, không phải quên.

## 7. KT-59 — TCB + transition + scheduler 256 mức + MCS + hộp thư (tầng 2)

**Trạng thái:** DONE (KT-59), chờ review độc lập + Agent 0 lật QUEUE. Sim vẫn là oracle;
core port nguyên ngữ nghĩa, **không** đổi `spec/`, `v2/sim/**/src/**`, `.scn`, QUEUE.

### 7.1 Tệp và phạm vi

| Tệp | Dòng | Nội dung |
|---|---|---|
| `src/tcb.rs` | 575 | `TcbState` 11 trạng thái + payload, bảng `ALLOWED` 11×11 (48 ALLOWED / 2 reserved / 73 cấm), `TcbSnapshot`/`TcbInfo`, `SuspendMode`, `FaultKind` (kiểu dữ liệu cho `BlockedOnSend`) |
| `src/mcs.rs` | 268 | `ScBudget` budget/period/consumed/refill/charged, `configure`/`charge`/`refill_due`/`check`, `ScInfo`, `donated_priority`, `next_boundary` bão hoà |
| `src/sched.rs` | 398 | `HartSched` 256 mức + bitmap 4×`u64`, ready queue **liên kết tĩnh** `[u16; MAX_OBJECTS+1]`, `pick`/`pick_where` O(1) khi đầu hàng hợp lệ, `Published` |
| `src/mailbox.rs` | 433 | `Mailbox<T, 8>` FIFO + backpressure trả lại message, `MailboxMsg` (Wake/Reschedule/Migrate/Stop/TlbFlush), `HartMailbox` (nhật ký biên 64 + `stopped`) |
| `src/core_tcb.rs` | 726 | khối `impl Machine`: TCB, một cửa `transition_tcb`, suspend/resume/finalise, MCS/tick/enforce/refill |
| `src/core_sched.rs` | 866 | khối `impl Machine`: scheduler/`dispatch`/`block_current`/`wake`/`yield`/preempt, push migration, hộp thư, `check_sched`/`check_sc_refs`/`check_budget`/`check_mailboxes` |
| `src/machine.rs` | 1483 | tầng 1 (KT-58) + thêm trường TCB/SC/hart/mailbox vào `Machine`, body table + free-list; **không** có logic KT-59 (giữ R1 ≤ 1.500 dòng) |
| `tests/kt59_core_parity.rs` | 2214 | parity host: 5 kịch bản + differential 16×32 + 2 test core-only |

**Có:** `Tcb.Configure`/`SetFaultEp`/`SetPriority`/`SetAffinity`/`SetHome`/`SetSchedParams`/
`ReadState`/`Info`, `Suspend(Abort)`/`Suspend(Defer)`/`Resume`, `finalise_tcb` + log; một cửa
`transition_tcb` tra `ALLOWED`, cấm cặp `.` trả `IllegalOperation` (đếm `forbidden_hits`), ≤1
`Running`/hart, `Running{hart} ⇒ home == hart`, `Running → Sleeping{Budget}` tăng bộ đếm; scheduler
256 mức FIFO/round-robin, `dispatch`/`block_current`/`wake`/`yield_current`, preempt mỗi bước,
`published`/`publish` guard luật 3, push migration (≥2 hart); MCS
`SchedControl.Configure`/`tick`/refill/`tcb_has_budget`/`pick_where`; hộp thư FIFO 8 + `Busy` +
`post_mail`/`process_mail`/`send_mail`/`mail_len`/`mail_log`/`hart_stopped`, `Migrate` (1 hart ⇒
`NotSupported`).

**Không (biên task):** IPC/Endpoint/Reply/Notification/WaitAddr/fault (KT-60 — object mới có
cap/MDB/byte, **chưa có body/hàng chờ**); donation KT-21 (chỉ trường + công thức); TLB handler
(`TlbFlush` placeholder — **KT-61 chưa nối**, nợ K3.5); nối syscall/kernel (KT-61 **đã link
core vào ảnh** — §9; syscall U-mode vẫn K3.5); Frame/PageTable/VSpace (D2-83).

### 7.2 Giới hạn tĩnh — đầy ⇒ lỗi, không panic im lặng

| Hằng | Giá trị | Đầy/giới hạn thì sao |
|---|---|---|
| `MAX_TCBS` | **64** | `Retype` TCB thứ 65 ⇒ `NotEnoughMemory` **nguyên tử** (không đốt byte/cap/object); `add_kernel_object(Tcb)` của harness panic (lỗi init, như sim khi arena đầy) |
| `MAX_SCHED_CONTEXTS` | **32** | `Retype` SC thứ 33 ⇒ `NotEnoughMemory`; harness panic như trên |
| `MAX_HARTS` | **8** | `with_harts(0/9+)` panic lúc init; `publish`/`dispatch`/`mail` hart ngoài dải ⇒ `InvalidArgument` |
| `MAILBOX_CAP` | **8** | `post_mail`/`send_mail` khi đầy ⇒ `Busy`, message **trả lại** (không ghi đè, không panic) |
| `MAX_MAIL_LOG` | **32**/hart | message vẫn xử lý đúng một lần; nhật ký dừng ghi + `mail_log_overflow` tăng (biên triển khai, nợ §6.11; D2-85b) |
| `MAX_FINALISE` | **256** (`= MAX_OBJECTS`) | **F1** (reviewer-kt59): body tái dùng nên một `Machine` finalise nhiều lượt; đầy ⇒ `finalise_tcb` trả `NotEnoughMemory` **trước mọi thay đổi** (atomic), `finalise_overflow` tăng, **không** panic/cắt im lặng. Một `Machine` biểu diễn **≤ 256** finalise; vượt ⇒ caller tái tạo `Machine`. Test core: 257 lần gọi khi log đầy ⇒ lỗi, state không đổi; 2 vòng × 64 TCB ⇒ 128 dòng |
| `MAX_OBJECTS`/`MAX_CAPS`/`MAX_SLOT_WORDS`/`MAX_UNTYPEDS` | 256/512/4096/16 | như KT-58 (Retype/copy ⇒ `NotEnoughMemory`; harness `add_*` panic) |

Body TCB/SC có **free-list**: `Revoke` Untyped finalise object ⇒ trả body về free-list để
`Retype` lại dùng (test pin), không rò chỉ số sau 64/32 lần.

### 7.3 Dấu chân tĩnh — pin D2-84 + bảng phân bố

**D2-84:** pin core = **≤ 196.608 B (192 KiB)** = trần ảnh 262.144 B (QD §3.5) trừ **64 KiB**
headroom cho static kernel + cấu trúc tương lai. Vượt ⇒ **tối ưu** (giảm `MAX_MAIL_LOG`,
chuyển body TCB sang arena, tách `SchedContext`), **không** nâng pin. KT-61 đo lại **ảnh link
thật** trước khi thêm bảng; từ **KT-61 core đã link** nên `.data+.bss` ảnh tăng đúng 180.168 B
(số đo từng ảnh: `K3B-DOD.md` §6.2).

`Machine::footprint()` trả bảng phân bố byte tĩnh (D2-84); test pin in bảng + assert
`sum(bytes) + scalars + padding == size_of::<Machine>()`. Bảng **cập nhật KT-63** (log
`JOURNAL/kt63-parity.log`; `[ĐO]`): KT-63 thêm `recv_window` trong `TcbData` (+4.608 B) và
cap nguồn/ảnh chụp trong `Message` (+3.648 B ở pool 24 message) — **pin không đổi**, còn
8.160 B headroom:

| Thành phần | Byte | Ghi chú |
|---|---:|---|
| `regions[16]` | 384 | `Region` 3×`usize`/`u64` |
| `objects[256]` | 22.528 | `Meta` 88 B |
| `caps[512]` | 28.672 | `Cap` 56 B |
| `slot_pool` | 16.656 | `MAX_SLOT_WORDS=4096` + hole |
| `tcbs[64]` | 49.664 | `TcbData` 776 B (IpcArea 544 B + `recv_window` 72 B + scheduler/state — KT-63) |
| `tcb_free[64]` | 128 | free-list body |
| `scs[32]` | 1.792 | `ScData` 56 B |
| `sc_free[32]` | 64 | free-list body |
| `pair_hits[121]` + `forbidden_hits[121]` | 484 + 484 | bộ đếm bảng ALLOWED |
| `finalise_log[256]` | 12.288 | `FinaliseEntry` 48 B (biên F1) |
| `sched[8]` | 16.832 | `HartSched` 2.104 B |
| `mail[8]` | 15.744 | `HartMailbox` 1.968 B (`MAX_MAIL_LOG=32`, D2-85b) |
| `published[8]` | 16 | `Published` 2 B |
| `endpoints[16]` + `endpoint_free[16]` | 1.280 + 32 | body Endpoint (KT-60) |
| `replies[32]` + `reply_free[32]` | 256 + 64 | body Reply (KT-60) |
| `notifications[16]` + `notification_free[16]` | 512 + 32 | body Notification (KT-60) |
| `ipc_msgs[24]` + `ipc_msg_free[24]` | 17.280 + 48 | pool `QueuedMsg` 720 B (KT-60; **KT-63** +152 B: `Message` mang cap nguồn/ảnh chụp) |
| `addr_words[64]` | 1.536 | `(frame, offset) -> u64` (KT-60) |
| `wait_nodes[64]` + `wait_free[64]` | 1.024 + 128 | nút waiter (KT-60) |
| `wait_buckets[64]` + `wait_buckets_tail[64]` | 128 + 128 | 64 bucket (KT-60) |
| scalars | 263 | 20 `usize` + 12 `u64` (3 bộ đếm cap — KT-63) + `u16` + `u32` + `bool` |
| padding | 1 | `repr(Rust)` |
| **Tổng** | **188.448** | **95,9 % pin 196.608 B; còn 8.160 B** (D2-84; KT-63 **không** nâng pin — chỉ thêm trường theo đúng phạm vi) |

Pin KT-58 `≤ 96 KiB` (dấu chân **tầng 1**) được thay bằng pin D2-84 trong
`tests/kt58_core_parity.rs` vì KT-59 thêm đúng phạm vi; **không** nới trần sản phẩm.

### 7.4 Đỏ trước + mutation (D2-05)

| Mã | Đột biến | Kết quả (log `JOURNAL/`) |
|---|---|---|
| **Red-before** | trước KT-59: core tầng 1 **chưa có** module `tcb/sched/mailbox/mcs`; test parity không biên dịch được | `kt59-red-before.log`: `E0433: cannot find TcbState in kc` (exit 101) — đúng "tính năng chưa có" |
| **M1** | `Tcb.Suspend(Defer)` bỏ kiểm `Defer` (rơi xuống nhánh trạng thái) | `kt59-mutation-m1.log`: bước 24 `Suspend(Defer)` trên `Inactive` core trả `IllegalOperation` thay `InvalidArgument` — parity đỏ (độc lập với M2/M3 và red-before) |
| **M2** | `sched_pick_on` dùng `pick()` thay `pick_where(tcb_has_budget)` | `kt59-mutation-m2.log`: bước 8 `Dispatch` digest lệch (core chọn TCB 10 **không** budget) — parity đỏ |
| **M3** | `charge_tick` không chuyển `Sleeping{Budget}` khi cạn budget | `kt59-mutation-m3.log`: `FAILED` — bất biến `check_sched` bắt `Running` hết budget (debug assert) |

M1..M3 độc lập, mỗi lần sửa tạm một tệp, chạy test, **khôi phục** và xác nhận sha256 toàn bộ
`src/*.rs` trước/sau (`/tmp` backup + `sha256sum -c`).

### 7.5 Nối KT-60/61

- **KT-60 đã nối (ghi lại để đối chiếu):** hai hook tại `transition_tcb` (KT-21
  `release_donation_of` khi rời `BlockedOnReply`; KT-34 xung đột `Reply.caller` ⇒
  `IllegalOperation`), `unlink_tcb_queues` thật (Endpoint/Reply/Notification/WaitAddr),
  `ipc_buffer`+`regs` ghi qua `ipc::transfer`, donation tự động qua `Call`/`Reply`, fault
  `Exit` không Reply. Chi tiết §8.
- **Accessor sim đã thêm (đóng giới hạn §7.5 cũ):** `Machine::endpoint_queued` (nội dung
  message đang treo) + `Machine::mail_items` (hàng hộp thư hart) — **chỉ đọc**, không đổi
  semantics; parity KT-60 so head/tail + toàn bộ nội dung.
- **Còn cho KT-61:** chuyển cap (receive window/`plan_cap_transfer`), finalise IPC object khi
  Revoke (huỷ caller/waiter/hàng), nối syscall/kernel + đo lại ảnh link — nợ §8.6.

## 8. KT-60 — IPC/Reply/Notification/WaitAddr/fault (tầng 3)

**Trạng thái:** DONE (KT-60), chờ review độc lập + Agent 0 lật QUEUE. Sim vẫn là **oracle**;
task này **không** đổi `spec/`, **không** đổi semantics `v2/sim/**/src/**` (chỉ **thêm
accessor đọc**: `Machine::endpoint_queued`/`Machine::mail_items`), không `.scn`/QUEUE/DECISIONS/
`viscript/`.

### 8.1 Tệp và phạm vi

Bảng tệp ở §2 (dòng KT-60: `ipc.rs`, `objstate.rs`, `core_ipc.rs`, `core_sync.rs`,
`check_ipc.rs`, `footprint.rs`, `captitable/tcbdata` tách để R1; `tests/kt60_core_parity.rs`).

**Có:** `Endpoint` (hàng gửi FIFO 8 + **một** Recv waiter + `last_mr` + `pushed/popped/
removed`), `Reply` (`caller` + `ReplyRecv`/`NBSendRecv`/`NBSendWait`), `Notification` (word/
badge OR 64 bit, **một** waiter, bound TCB 1:1, `Poll`/`NBRecv`), `WaitAddr` (bảng băm 64
bucket, khoá đầy đủ `(frame, offset)`, FIFO, **không timeout** — D2-53 Q4), **donation KT-21**
tự động qua `Call`/`Reply`, **fault** (`Exit` ⇒ `BlockedOnSend{call:false, fault}` **không
Reply**; fault khác ⇒ Call + Reply như sim), `ipc::transfer` (một đường copy), `unlink_tcb_queues`
thật, sáu bất biến IPC.

**Không (biên):** chuyển cap (receive window/`plan_cap_transfer` — §8.6), finalise IPC object
khi Revoke (huỷ caller/waiter/hàng — §8.6), nối syscall/kernel (KT-61), TLB/VM (KT-61/D2-83).

### 8.2 Hai hook còn nợ từ KT-59 — đúng một cửa

Tại `Machine::transition_tcb` (`core_tcb.rs`), **cùng chỗ** hook của sim:

1. **Rời `BlockedOnReply`** bằng đường khác `Reply` (wake/transition/suspend/exit/finalise) ⇒
   nếu `is_donation_donor(tcb)` gọi `release_donation_of(tcb)?` — trả SC về client, không để
   donor treo (KT-21). `finalise_tcb` cũng gọi `release_donation_of` trước `unlink` như sim.
2. **Vào `BlockedOnReply{reply}`** ⇒ Reply sống: `caller == None` ⇒ gán `Some(tcb)`;
   `caller == Some(tcb)` ⇒ no-op; `Some(khác)` ⇒ **`IllegalOperation`** (không ghi đè — KT-34).
   **Rời** ⇒ gỡ `caller` nếu đang là `tcb`.

### 8.3 Parity — 6 kịch bản, so mã lỗi + digest **sau từng bước**

`v2/sim/vifast-sim/tests/kt60_core_parity.rs` chạy cùng `Op` trên sim/core; so kết quả (tên +
số mã lỗi) và digest: TCB (state+payload, `TcbInfo`, `ReadState`, `IpcArea` info/regs/buffer/
badge/msg_id/`last_ipc_error`/`last_fault`/`notif`), **hàng Endpoint đầy đủ nội dung message**
(sender/call/blocking/fault/64 MR/info/badge/msg_id) + counters, Reply caller, Notification
word/badge/waiter/bound, WaitAddr waiter+giá trị theo 4 khoá, hộp thư hart (**hàng + nhật ký**
qua accessor mới), counters IPC, log finalise; 8 cờ bất biến hai phía. Kèm assertion cứng cho
các mốc then chốt.

| # | Kịch bản | Nội dung (mốc kiểm) | Log |
|---|---|---|---|
| 1 | **KT-24** | Send/Send/NBSend FIFO; Recv lấy đúng thứ tự; TCB đang `BlockedOnSend` ⇒ `IllegalOperation`; `wake` gỡ message của sender khỏi hàng (drop đếm); lấp **8** ⇒ thứ 9 `Busy`; `nbrecv` trả `msg_id` 4..11 đúng FIFO; hết hàng `Busy`; Send trên TCB/Reply sai kiểu `WrongObjectType`/Call `NotSupported` | `kt60-parity.log` |
| 2 | **KT-25 + KT-34** | Call xếp hàng → Recv(R0) giao, caller BlockedOnReply; Reply trả; Reply lần 2 vô hại; **tái dùng R0** cho Recv; `ReplyRecv` đường nóng (reply trước, recv sau); `recv_reply == reply` được phép; Recv bằng cap Reply **đang active** ⇒ `Busy`; transition chéo Reply ⇒ `IllegalOperation` | nt |
| 3 | **KT-27** | Wait chặn, Signal đánh thức + giao `(0x10,0x10)`; waiter thứ hai `Busy`; Signal badge 0 không mất tín hiệu; **OR** 0x10|0x02 = **0x12**; Poll `NBRecv` trả word/badge; bound TCB đang `BlockedOnRecv` được đánh thức qua Notification; bind 1:1 hai chiều `Busy`/unbind | nt |
| 4 | **KT-28** | `expected` sai `InvalidArgument` không xếp hàng; 3 waiter FIFO; `tick 100` **không** đánh thức/không sinh `Sleeping{Timeout}`; Wake 1 rồi tất cả; **va chạm bucket** `(3,0)`/`(3,64)` không đụng nhau; Suspend(Abort) gỡ nút; wait khi suspended `IllegalOperation` | nt |
| 5 | **KT-26 + D2-25** | `Exit` có fault_ep ⇒ `BlockedOnSend{call:false, fault:Exit}` **không Reply**; Recv giao fault ⇒ sender `Inactive`, receiver `last_fault`; fast path khi procsrv đang Recv; hàng đầy ⇒ `Busy` **trước** mọi thay đổi; Exit lần hai `IllegalOperation`; inject fault ≠ Exit ⇒ Call+Reply, Reply resume | nt |
| 6 | **KT-21** | `Call`+`Recv` tạo donation hai chiều (`T1.donated_sc`, SC `donor/donated_to`, `effective_priority`); `Reply` trả; **hook 1** khi rời `BlockedOnReply` (transition) cũng trả; exit khi donate trả SC + fault Exit; call sau Inactive đi `ensure_started` | nt |

Reviewer-kt60 vá F1/F2/F3 đã đưa vào kịch bản: kt21 thêm **donor không đổi
prio/SC** (`IllegalOperation`), **Suspend(Abort) donee** trả SC + đánh thức client; kt27 thêm
**`block_current(BlockedOnNotification)`** ghi waiter + TCB thứ hai `Busy` + Signal đánh thức
đúng (không chỉ đường `notification_wait`).

Core-only: `kt60_core_donation_tao_va_tra_kt21` (donation unit), `kt60_core_waitaddr_khong_timeout_va_wake_fifo`,
`kt60_core_endpoint_busy_va_fifo` (mã `Busy` cứng), `kt60_core_bien_tinh_body_ipc` (đầy
`MAX_ENDPOINTS/MAX_REPLIES/MAX_NOTIFICATIONS` + pool 24 ⇒ `NotEnoughMemory`/`Busy` **nguyên
tử**), `kt60_pin_dau_chan_d2_84`.

**Bằng chứng:** `JOURNAL/kt60-parity.log` — `11 passed`; gate tổng **598 test/78 suite**
(baseline 587 → +11); `JOURNAL/kt60-gate.log`; `JOURNAL/kt60-ktest-matrix.log` — **9/9** boot,
TAP **43/43 + 3/3 + 7/7**.

### 8.4 Đỏ trước + mutation (D2-05)

| Mã | Đột biến | Kết quả (log `JOURNAL/`) |
|---|---|---|
| **Red-before** | core = KT-59 (chưa có API IPC) | `kt60-red-before.log`: `E0433: cannot find MessageInfo/Message in kc` (exit 101) — đúng "tính năng chưa có" |
| **M1** | bỏ hook `release_donation_of` trong `transition_tcb` | `kt60-mutation-m1.log`: kt21 bước 5 digest lệch (donation còn treo) |
| **M2** | bỏ kiểm "Reply đang active" trong `recv` | `kt60-mutation-m2.log`: kt25 bước 10 kết quả lệch (`Busy` vs `Ok`) |
| **M3** | `notification_signal` dùng `word = badge` thay `word |= badge` | `kt60-mutation-m3.log`: kt27 bước 7 digest lệch (0x02 vs 0x12). Bước OR 0x10|0x02 là **tăng phủ** kịch bản (M3 trước đó không bị bắt vì chỉ một badge — ghi nhận, không hợp thức hoá) |

M1..M3 độc lập nhau; mỗi lần sửa tạm, chạy test, khôi phục và xác nhận sha256 `core_tcb/
core_ipc/core_sync` trước/sau (`sha256sum -c` OK).

### 8.5 Dấu chân + **điều kiện tiên quyết KT-61** (D2-84)

- `size_of::<Machine>() = 180.168 B` `[ĐO]` lúc KT-60 — **91,6 % pin 196.608 B**, còn
  **16.440 B** (bảng 27 dòng §7.3). **KT-63** thêm receive window + cap trong message ⇒
  **188.448 B** (95,9 %, còn 8.160 B) — vẫn **không** nâng pin (§12.4). Đã áp **D2-85b**: `MAX_MAIL_LOG` 64 → 32 (−12.288 B) **trước KT-61**;
  pin test KT-59 cập nhật 32.
- **D2-85a — hai ngưỡng khác nhau, cấm đọc lẫn:** pin **196.608 B** áp cho
  `size_of::<Machine>()`; trần **262.144 B** áp cho **ảnh link** (`.data+.bss` thật). KT-61
  phải báo **cả hai** số; một số xanh không suy ra số kia.
- Ảnh kernel trước link (KT-57..KT-60) static **11.248 B** sản phẩm / **17.344 B** ktest; sau
  link core (KT-61) tăng **+180.168 B** mỗi ảnh — số `[ĐO]` + đối chiếu trần **262.144 B** ở
  `K3B-DOD.md` §6. Không phải cắt gì; nếu vượt pin/trần thì **cắt**, không nâng ngưỡng.
- **D2-85c — test pin không tautology:** `padding` được assert `≤ 8 B` (hiện 1 B) và scalars
  kiểm theo công thức; nếu bảng bỏ sót trường, padding phình và test đỏ (bắt được lỗi log
  `kt60-footprint.log` từng thiếu 1 dòng nhưng vẫn xanh).
- **Phương án cắt còn lại cho KT-61 (A đã áp):**
  - **B. `MAX_QUEUED_MESSAGES` 24 → 16** (−8 × 568 = −4.544 B) **và `MAX_ENDPOINTS` 16 → 8**
    (−8 × 80 = −640 B) ⇒ **−5.184 B** (174.984 B). Giảm biên mô hình, ghi rõ trong docs;
    kịch bản KT-60 chỉ dùng 2 Endpoint + pool 24 nên parity vẫn đủ.
  - **C. Chuyển body `TcbData` sang arena `PhysMem`** (KT-61 thay `SlotPool` bằng slot arena):
    `704 B × 64 = 45.056 B` rời `.bss` của `Machine` (không còn trong `size_of`), giữ accessor;
    cần đo lại `.data+.bss` + arena thật, không được mô tả là "miễn phí".
  - **D (dự phòng, không khuyến nghị):** `MAX_FINALISE` 256 → 64 (−9.216 B) nhưng đổi hợp đồng
    log finalise KT-59 (test 257 lần + docs §7.2) — chỉ dùng nếu B/C không đủ.
- **Cổng KT-61:** in `size_of::<Machine>()` + bảng phân bố + `.data+.bss` ảnh link, báo **cả
  hai** ngưỡng D2-85a; áp B/C nếu cần **trước** khi thêm bảng.

### 8.6 Nợ KT-61 (từ KT-60)

0. **F4 — `SchedContext.Bind(notification)` chưa port:** `NotificationData.bound_sc` là trường
   **chết** (luôn `None` hai phía); digest KT-60 so nó nhưng **vô nghĩa** cho tới khi port
   `SchedContext.Bind`/`Tcb.BindNotification` phía SC. Đã ghi nợ, không giả vờ đã kiểm.
1. **Chuyển cap** (D2-53 Q6): receive window 4 ô + `plan_cap_transfer` — cap trỏ chính Endpoint
   + badge ⇒ unwrap, thiếu `RIGHT_GRANT`/ô bận ⇒ thất bại mềm; `MessageInfo.caps` = số cài
   được. Hiện `MessageInfo.caps > 0` ⇒ `InvalidArgument`; sim dùng `InvalidCapability`/giao
   mềm cho một số đường — **lệch đã biết**, chưa parity.
2. **Finalise IPC object khi Revoke/Delete**: body đã trả free-list nhưng **chưa** huỷ
   `Reply.caller`/đánh thức Recv waiter/huỷ hàng như finalise đầy đủ; parity KT-60 không phủ
   (đường Revoke Untyped có object IPC đang hoạt động). Phải làm trước khi nối Revoke vào kernel.
3. `TlbFlush` vẫn placeholder (KT-59); VM vẫn `NotSupported` (D2-83) — KT-61 đi qua object layer.
4. Nối syscall/`init`/ảnh kernel + đo lại ảnh (chưa làm — KT-61).
5. **Xoá/revoke cap Reply** khi caller `BlockedOnReply`: sim `Cancelled + Ready`; core hiện giữ
   caller treo tới khi có hook finalise (nợ §8.6.2) — **lệch đã biết**.
6. ~~**`tcb_resume_with_regs`** chưa port~~ — **ĐÓNG KT-64** (§13.4); **Reply cap trong `Endpoint.recv`**
   (cap Reply chết ⇒ coi như `Recv` không Reply) và **WaitAddr bucket/FIFO toàn phần** (mọi
   bucket, mọi thứ tự) chưa có ca parity riêng — nợ phủ, không phải nợ hành vi đã biết.
7. **F1/F2/F3 reviewer-kt60 đã vá + có ca:** suspend donee trả SC + đánh thức client; donor
   không đổi priority/SC; `block_current(BlockedOnNotification)` ghi/kiểm waiter + Busy.

**Trạng thái nợ sau KT-61 (không tô hồng):** các mục 1 (chuyển cap), 2 (finalise IPC object khi
Revoke), 3 (`TlbFlush` placeholder) **vẫn mở** — KT-61 **không** làm vì chúng không cần cho cổng
K3b và làm nửa đường sẽ tạo hai nguồn sự thật (cùng lý do với `SlotPool` §9.6). Mục 4 (nối
kernel/đo ảnh) **đã xong**; mục 6 (`tcb_resume_with_regs`, Reply cap trong `Endpoint.recv`,
WaitAddr toàn phần) **vẫn là nợ phủ**. Chủ: **K3.5**. Tiêu chí đóng: parity sim↔core có ca +
TAP kernel mới, không đổi `spec/`.

## 9. KT-61 — nối core vào kernel thật (cổng K3b)

**Trạng thái:** KT-61 **DONE (đề xuất; chờ review độc lập + Agent 0 lật QUEUE)**. Phạm vi: link
`vifast-core` vào **ảnh kernel thật** (sản phẩm + test), TAP core trên QEMU, đo **cả hai** ngưỡng
(D2-85a), soak 1.000, matrix smp 1/2/4, conformance 5/5, `gate --fast`. **Không** đổi `spec/`, sim
semantics, `.scn`, QUEUE/DECISIONS/AGENTS, `viscript/`; `unsafe` **chỉ** thêm ở
`kernel/vifast-plat-riscv64` (D2-17).

### 9.1 Phạm vi thay đổi

| Tệp | Thay đổi |
|---|---|
| `kernel/vifast-plat-riscv64/src/core_bridge.rs` (mới) | Cầu nối **một chiều**: static `Machine`, `init` từ **nhãn PA** của Untyped boot, `with_machine` (spin lock `AtomicBool`), `boot_smoke` (vòng đời TCB cho ảnh sản phẩm) |
| `kernel/vifast-plat-riscv64/{Cargo.toml,src/lib.rs}` | thêm dependency `vifast-core` + `pub mod core_bridge` |
| `kernel/vifast-kbin-riscv64/src/ktest_core.rs` (mới) | TAP **43..52** + evidence `# core:` (§9.3) |
| `kernel/vifast-kbin-riscv64/{src/main.rs,src/ktest.rs}` | ảnh sản phẩm: `core_bridge::init` + `boot_smoke` + `# core: product`; ảnh test: `mod ktest_core` + pha 43..52, trap lồng dời **53** |
| `test/boot/{ktest.sh,expected.txt}` | manifest TAP 43..52 + 11 regex evidence `# core:`; `ktest 53 53` (nguồn sự thật duy nhất) |
| `docs/{k3b-core,k3-riscv64}.md`, `agent/{reports/K3B-DOD.md,JOURNAL/kt61-*,STATE.md}` | tài liệu + log |

**Không** thay đổi `Machine`/ngữ nghĩa core: `footprint.rs` (27 dòng), cap/MDB/TCB/IPC giữ nguyên;
`vifast-core` vẫn `#![forbid(unsafe_code)]` (D2-17) — mọi `unsafe` của cầu nối nằm ở
`kernel/vifast-plat-*`. Kernel clippy `-D warnings` **đã sạch** sau KT-61 (18+27+2 lint **nợ tích tụ**
được sửa; `.text`/BUILD_ID không đổi; `JOURNAL/kt61-core-build.log` sạch,
`kt61-core-build-before-fix.log` là log đỏ trước khi sửa).

### 9.2 Đo **cả hai** ngưỡng (D2-85a) — không đọc lẫn

| Ngưỡng | Áp cho | Số đo | Kết quả |
|---|---|---|---|
| **196.608 B** (pin D2-84) | `size_of::<Machine>()` | **180.168 B** `[ĐO]` (91,6 % pin; còn 16.440 B) | ĐẠT |
| **262.144 B** (trần ảnh, QD §3.5) | `.data+.sdata+.bss+.sbss` **của ELF thật** | **191.416 (product) / 197.568 (ktest) / 2.680 (guard) / 11.752 (user) B** — `kt61-budget.log:2,5,8,11` | ĐẠT |

Số `[ĐO]` chỉ do script in (`budget.sh`, `# core: link`/`# core: product` trong log QEMU); tài liệu
không chép tay (QD §9 Q12). TAP `core link` kiểm thêm **bảng phân bố**: `sum(bytes 27 dòng) +
scalars + padding == total` và `padding ≤ 8` (D2-85c — chống bảng thiếu dòng, không tautology).

### 9.3 TAP core trên **nhân thật** (`ktest` 43..52)

Mỗi TAP in `# core:` với số **object/cap/TCB thật** của lần boot và trạng thái bất biến. Không có
test nào chạy trên host:

| TAP | Bằng chứng `# core:` (neo) |
|---|---|
| 43 `core link` | `link addr=0x… bytes=180168 pin=196608 image_cap=262144 parts=27 sum=180168 scalars=239 padding=1 sum_ok=1 in_image=1` — static nằm trong vùng ảnh `[KERNEL_PHYS_BASE, guard)`, không phải arena/heap |
| 44 `core untyped boot` | `untyped id=1 base=0x… bytes=65536 used=0 free=65536 allocated=0 gap=0 objects=1 caps=0 check_bytes=1 check_all_bytes=1` — Untyped dựng từ **nhãn PA** ngay sau arena object layer (D2-83: không dereference) |
| 45 `core tcb retype` | `tcb retype tcb=3 slot=1 objects=11 caps=11 used=2304 state=Inactive check_caps=1 check_bytes=1` |
| 46 `core configure resume suspend` | `configure cspace=1 sc=1 priority=3 mcp=10 budget=4 period=8 suspended_ok=1 resume=1 state=Ready check_sched=1 check_sc_refs=1 check_budget=1` |
| 47 `core sched 2 tcb` | `sched harts=1 pick0=3 pick1=4 dispatch0=3 dispatch1=4 t0=Ready t1=Running suspended=0 check_sched=1` — 2 TCB **có SchedContext riêng**; `Suspend` TCB 1 ⇒ ra khỏi hàng đợi ⇒ `dispatch` sang TCB 2 |
| 48 `core ipc round` | `ipc call=1 recv=1 reply=1 mr=0xfeed blocked_send=1 blocked_reply=1 t0=Ready ep_len=0 reply_caller=0 sent=1 delivered=1 check_ipc=1 check_reply_links=1` |
| 49 `core notification signal` | `ntfn signal=1 wake=1 word=0x0 badge=0x0 waiter=0 consumed=0x42 poll=0x8 state=Ready check_notifications=1` |
| 50 `core waitaddr` | `waitaddr frame=0x… label=… offset=64 value=0x1234 woke=1 waiters=0 buckets=0 state=Ready check_waitaddr=1` — khoá futex là **PPN của Frame thật ở object layer** (một chiều, §9.4) |
| 51 `core d2-83 boundary` | `d2-83 frame=NotSupported page_table=NotSupported vspace=NotSupported atomic=1 untyped_used=2304 obj_frame=1 obj_vspace=1 obj_health=1 owner=obj` |
| 52 `core invariants` | `invariants caps=1 objects=11 used=0 free=65536 deleted=9 check_caps=1 check_bytes=1 check_all_bytes=1 check_sched=1 check_sc_refs=1 check_budget=1 check_mailboxes=1 check_ipc=1 check_reply_links=1 check_notifications=1 check_waitaddr=1 check_fault=1 leak=0` |

Ảnh **sản phẩm** cũng chạy core: `# core: product addr=… bytes=180168 … retype=1 configure=1
suspend=1 resume=1 state_ready=1 objects=3 caps=1 used=0 check_caps=1 … footprint_ok=1` — mỗi boot
soak là một vòng đời TCB thật trên kernel sản phẩm; sai ⇒ `PANIC: core init product` (soak đỏ).

### 9.4 Biên D2-83 — **một chiều**, không nhân đôi chủ sở hữu

| Hạng mục | Chủ sở hữu | Bằng chứng KT-61 |
|---|---|---|
| Untyped/CNode/MDB/cap/TCB/SchedContext/Endpoint/Reply/Notification/IPC/WaitAddr | **core** (`Machine`) | TAP 44..50 |
| **Frame/PageTable/VSpace** | **object layer** `vifast_plat_common::obj` (KT-52) | TAP 51: core `Retype` ⇒ `NotSupported` **3/3**, `untyped_used` không đổi; **cùng lúc** obj layer tạo+revoke VSpace/Frame thật (`obj_frame=1 obj_vspace=1 obj_health=1`) |
| Cầu nối | **một chiều**: kernel truyền **nhãn** (PA/PPN) vào core | Untyped boot = `[obj.arena_end, +64 KiB)` (nhãn PA, D2-83) — core chỉ kế toán byte; khoá `WaitAddr` = **PPN** của Frame obj-layer, core lưu trong bảng `addr_words` và **không** đọc byte/PTE |

Ghi chú: core **từ chối** `add_kernel_object(ObjType::Frame)` (`is_core()` = false ⇒ panic
"chỉ object lõi mới dựng trực tiếp") — đúng biên giới, nên TAP 50 dùng PPN chứ không tạo Frame
trong core. Đổi biên giới này cần **D2 mới**.

### 9.5 Static trong **`.bss`** + `init_zeroed` — không 180 KB trong file ảnh

`Machine` có trường khởi tạo **khác 0** (`wait_buckets = NODE_NONE`, `n_harts`,
`published[*].idle`, `HartSched.idle`, 8 ô `Option<MailboxMsg>` với `None ≠ 0`), nên **không**
thể là zeroed `.bss` thuần. KT-61 thử ba đường:

1. `static Machine = Machine::with_harts(1)` → **`.data`**: 180.168 B nằm trong **FILE** (`filesz`)
   — loader copy mỗi boot; ảnh phình 180 KB. Đã đo và **bỏ**.
2. `MaybeUninit` + `MACHINE.write(Machine::with_harts(1))` → Rust trả struct lớn qua `sret` nên
   bản tạm 180.168 B dựng trên **stack 64 KiB** ⇒ tràn guard page
   (`PANIC: guard page fault stval=0x80400000`, `JOURNAL/kt61-red-before-core-link.log`).
3. **Chốt:** static `MaybeUninit<Machine>` trong **`.bss`** (`NOBITS`: `filesz = 0`, chỉ `memsz`)
   + `Machine::init_zeroed` khởi tạo **tại chỗ** sau `write_bytes(0)` — không bản tạm, không
   copy 180 KB. `.bss` tăng đúng `size_of::<Machine>()` và **được `budget.sh` cộng đủ**.

`init_zeroed` chỉ đặt các trường **khác 0** so với `with_harts`; phần còn lại có `0` trùng
`EMPTY`/constructor (body TCB/SC/Endpoint/… được ghi `*::EMPTY` **lúc cấp phát**; `HartSched`,
`SlotPool`, `Region`, `ScBudget`, `Mailbox`, `Meta`/`Cap` toàn 0). Bất biến được chứng minh trên
**kernel thật**: TAP 43..52 + `check_*` sau mỗi pha + soak 1.000 boot; nếu `init_zeroed` thiếu
sentinel, `check_sched`/`check_mailboxes` bắt ngay (đã bắt thật trong quá trình làm: `idle lệch
current`, `hop thu mat/them message` — sửa bằng `HartSched::reset`/`HartMailbox::reset`).

**Số đo chứng minh `.bss` không vào file** (`ktest`, `llvm-readobj --program-headers`): segment
RW có `filesz = 7.912` / `memsz = 2.588.672` — 189.656 B `.bss` (gồm Machine) **không** nằm
trong file; file ảnh `ktest` 484.600 B. Số 4 ảnh + `filesz/memsz` đầy đủ ở `K3B-DOD.md` §6.2.

### 9.6 `SlotPool` — **giữ nguyên**, nợ có chủ (không làm nửa vời)

KT-61 **không** chuyển slot CNode sang arena `PhysMem` (phương án §8.5-C): làm nửa đường sẽ tạo
**hai nguồn sự thật** cho cùng một cap (slot trong `SlotPool` + byte slot trong arena). Quyết định:
giữ `SlotPool` là **nguồn duy nhất** của `CapId`; byte slot vẫn được Untyped kế toán đúng spec
(`cnode_base + slots*48`) và `check_bytes`/`check_all_bytes` xanh trên kernel thật (TAP 44..52).
Việc chuyển sang arena là **K3.5**, có tiêu chí: (i) slot nằm trong vùng Untyped thật (PA), (ii)
`Revoke` trả byte slot, (iii) `check_bytes` không đổi kết quả, (iv) TAP 45/52 vẫn xanh. Đây là
**nợ**, không phải "đã xong".

### 9.7 Cổng K3b — kết quả

Số đo đầy đủ + log: [`../agent/reports/K3B-DOD.md`](../agent/reports/K3B-DOD.md) (D1..D11). Tóm
tắt điều kiện: `gate --fast` xanh; TAP **53/53 + 3/3 + 7/7** ở smp **1/2/4**; soak **0/1000** ảnh
sản phẩm (3 luồng, không icount, timeout 20 s, BUILD_ID neo D2-80); conformance **61/61 dòng / 5/5
ca** (core không đổi đường VM); budget **cả hai** ngưỡng. **Không nới ngưỡng nào.**
L1 100k **mới xong 1/4 cấu hình** (`smp 1`: 100.000 seed × 64 op, `kt61-l1-100k.log:253`);
`smp 2/4/8` dừng khi hết ngân sách 3 h — khai rõ, **không** claim 400k (`400k × smp 1/2/4/8` là
**DoD cổng K6**, không phải KT-61; nợ thời gian có task riêng — DOD §11.10).

## 10. Chạy lại

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# parity host (nằm trong gate --fast vì là test của workspace v2)
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt58_core_parity -- --nocapture
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt59_core_parity -- --nocapture
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt60_core_parity -- --nocapture
# core tự build cho bare-metal + clippy riêng (không nằm trong gate host)
cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-core --target riscv64gc-unknown-none-elf
cargo clippy --manifest-path v2/kernel/Cargo.toml -p vifast-core --all-targets -- -D warnings
# toàn kernel (product + ktest + user) — phải 0 warning (KT-61 đã sửa hết nợ):
cargo clippy --manifest-path v2/kernel/Cargo.toml --workspace --no-deps \
  --target riscv64gc-unknown-none-elf -- -D warnings
cargo clippy --manifest-path v2/kernel/Cargo.toml -p vifast-kbin-riscv64 \
  --no-deps --features ktest,user --target riscv64gc-unknown-none-elf -- -D warnings
# KT-61: core trên **nhân thật** — TAP 43..52 + matrix 1/2/4
bash v2/test/boot/ktest.sh 1 matrix
# ảnh sản phẩm: core init + một vòng đời TCB mỗi boot
bash v2/test/boot/run.sh 1 1
# soak + cổng
bash v2/test/boot/soak.sh 1000 3
./v2/bin/v2 gate --fast
```

## 11. KT-62 — 16 syscall `0..15` đi vào `Machine` thật + `init` U-mode dùng core (K3.5 tầng 1)

**Trạng thái:** KT-62 **DONE (đề xuất; chờ review độc lập + Agent 0 lật QUEUE)**. Phạm vi: xoá shim
3-syscall của KT-53 (`vifast-plat-common::user::dispatch` cũ), đưa **mọi** số hiệu `0..15` qua
`vifast_core::syscall::dispatch` rồi vào API công khai của `Machine`; `init` U-mode gọi
`DebugGetInfo`/`Yield`/`Exit` **qua core**; TAP mới trên QEMU. **Không** đổi `spec/`, sim semantics,
`.scn`, QUEUE/DECISIONS/AGENTS, `viscript/`; `unsafe` **chỉ** ở `kernel/vifast-plat-riscv64`
(D2-17); `vifast-core` vẫn `#![forbid(unsafe_code)]`.

### 11.1 Bảng 16 syscall → core (một nguồn: `kernel/vifast-core/src/syscall.rs`)

| nr | tên | đường vào core | kênh trả |
|---|---|---|---|
| 0 | `Call` | `Machine::call_msg` (Endpoint; cap khác ⇒ `NotSupported`, D2-53 Q11) | `a0/a1`; nhận ⇒ `a2` = `MessageInfo`, `a3..a6` = MR0..3 |
| 1 | `Send` | `Machine::send` (Endpoint/Reply/Notification) | `a0/a1` |
| 2 | `NBSend` | `Machine::nb_send` | `a0/a1` |
| 3 | `Recv` | `Machine::recv` (`a1` = reply cptr hoặc [`REPLY_NONE`]) | `a0/a1` + `a2`/MR |
| 4 | `NBRecv` | `Machine::nb_recv` (`Busy` 14 khi rỗng) | `a0/a1` + `a2`/MR |
| 5 | `ReplyRecv` | `Machine::reply_recv` | `a0/a1` + `a2`/MR |
| 6 | `NBSendRecv` | `Machine::nb_send_recv` (D2-53 Q15a) | `a0` = lỗi NBSend (nếu có) + `a2`/MR |
| 7 | `NBSendWait` | `Machine::nb_send_wait` (D2-53 Q15a) | như 6 |
| 8 | `Yield` | `Machine::yield_current` + `tcb_remaining` | `a1` = budget còn lại |
| 9 | `WaitAddr` | `Machine::wait_addr_obj` qua hook `resolve_addr` | chặn / 5 khi `expected` lệch |
| 10 | `WakeAddr` | `Machine::wake_addr` qua cùng hook | `a1` = số đánh thức |
| 11 | `ClockRead` | hook `SyscallEnv::now_ns` (**`rdtime`** qua `timer::now_ticks`, ns — D2-88(b)) | `a1` = ns đơn điệu |
| 12 | `DebugPutStr` | hook `SyscallEnv::put_str` (arch đọc user bằng `SUM`) | `a1` = số byte; quá 4 KiB ⇒ 5 |
| 13 | `DebugHalt` | trả `SyscallOutcome::Halt` — arch quyết định dừng | không trả |
| 14 | `Exit` | `Machine::exit` | fault_ep ⇒ `BlockedOnSend{call:false, fault=Exit{code}}`, **không Reply** |
| 15 | `DebugGetInfo` | hằng sinh từ spec (`ABI_VERSION`/`MIN_COMPAT`), `env.features()`, `env.num_harts()` | `a1` = abi_version, `a3` = min_compat, `a4` = features, `a5` = **num_harts = số hart nền tảng** (D2-88(a)) |

**Ngoài dải `0..15`** (kể cả EID SBI) ⇒ **19 `RangeError`** — D2-79, giữ nguyên hành vi KT-53; chỗ
quyết định **duy nhất** là `vifast-core::syscall::dispatch` (façade plat-common không kiểm lại).

**Quy ước thanh ghi** (không đổi `spec/`): `a7` = nr; `a2` = `MessageInfo` cho syscall **1/2/3**
cap; MR0..MR3 = `a3..a6`. Hai chỗ **không đủ 6 thanh ghi** được ghi rõ:
`ReplyRecv` (2 cap) dùng `a0` = reply cap, `a1` = cap, `a2` = info, `a3..a6` = MR;
`NBSendRecv` (3 cap) dùng `a0` = send cap, `a1` = recv cap, `a2` = reply cap (**hoặc**
`REPLY_NONE`), `a3` = info, `a4..a6` = MR0..2 (3 MR). Đây là quy ước **KT-62** cho tầng 1, không
sửa `[calling.riscv64]`; KT-63 (chuyển cap) sẽ chốt lại nếu cần.

**Chỉ số cap ngoài bảng CSpace** ⇒ **1 `InvalidCapability`** (không phải 19): 19 chỉ dành cho **số
hiệu syscall**; `cap_at` của core trả `RangeError` cho chỉ số ngoài bảng nên dispatcher chặn trước
(`syscall.rs`, có test host).

**Notification payload** (F1 reviewer-kt62): Notification **không có** `MessageInfo`; `Recv` (3),
`NBRecv` (4), `NBSendWait` (7) khi đích là cap Notification trả **`a1 = value = word`**,
**`mr[0] = word`**, **`mr[1] = badge`** (lấy từ `Machine::nb_recv` → `notification_message`, hoặc
`take_tcb_notification` — **không** đọc `ipc_area` của Endpoint cũ, tránh stale). Endpoint/Reply
giữ nguyên `a2 = MessageInfo`, `a3..a6` = MR0..3.

**D2-88(c)**: 16 hằng `SYS_*` **sinh từ `spec/abi.toml` `[syscall]`** bằng `vifast-gen`
(`SYS_CALL`/`SYS_SEND`/…/`SYS_DEBUG_GET_INFO`), core tái xuất — không chép tay số hiệu.

### 11.2 Biên core/arch (D2-83) — hook `SyscallEnv`

Core **không** đọc bộ nhớ user, không sở hữu VSpace/PTE. Ba việc arch làm thay (trait `SyscallEnv`,
`no_std`, không `unsafe` trong core):

| Hook | Cài ở | Việc |
|---|---|---|
| `put_str(ptr, len)` | `vifast-plat-riscv64::user::UserMemAccess` | kiểm dải user đã map + đọc bằng `sstatus.SUM` (KT-53) |
| `resolve_addr(va)` | nt | `walk` PTE thật của VSpace: **tạm** khôi phục root kernel (root user không có physmap), lấy **PPN + offset** rồi trả root user; lỗi ⇒ 11 |
| `now_ns()` | nt | **`rdtime`** (`timer::now_ticks`) → ns — D2-88(b); SBI TIME chỉ dùng cho **ngắt** timer |
| `num_harts()` | nt | `smp::hart_count()` — probe SBI HSM trước bring-up, sau đó dùng bitmask present ⇒ **1/2/4** theo `-smp` (D2-88(a)) |
| `features()` | nt | bitmap runtime bit 0 `DebugPutStr`, 1 `DebugHalt`, 2 `WaitAddr`, 3 `Yield` (spec **không** định nghĩa bitmap — KT-62 đặt bit, ghi ở đây) |

**Phủ QEMU (F4 reviewer-kt62):** các đường U-mode **thật sự chạy trên QEMU** là `DebugPutStr`
(12), 5 số ngoài dải (⇒19), `DebugGetInfo` (15), `Yield` (8), `Exit` (14). `resolve_addr` (9/10),
`now_ns` (11), `put_str` dùng lại KT-53 và nhánh `Switched` **chưa có phép thử QEMU** — chúng mới
được **biên dịch** + kiểm qua host rig (`kt62_syscall.rs`)/TAP kernel-level dùng `TestEnv`; ghi
đây là **nợ phủ**, không gọi là "đã chạy thật". Nếu `switch_root` trả về U-mode thất bại (F7) ⇒
`PANIC: u-mode resolve_addr khong tra duoc satp user` + halt (không trả lỗi mềm khi `satp` còn root
kernel).

`# sys:` evidence (KT-62): **mỗi** syscall từ U-mode in
`# sys: nr=<n> err=<mã> value=<a1> tcb=<id> state=<CoreTcbState> src=core` ngay trong trap handler;
các TAP kernel-level in `# sys: tap=<7..13> …`. Đây là dòng **văn bản** (không phải bản ghi 32 B
`vifast-traceformat`): trace record `KIND_SYSCALL` là binary ring, không xuất ra UART trong KT-62 —
**không** mở rộng format conformance (`# trace:` chỉ dành cho `.scn`, so sim↔QEMU), do đó
`trace_compare.py` không bị ảnh hưởng. Mở rộng sang bản ghi 32 B là việc của procsrv/K6.

### 11.3 TAP mới trên QEMU (ảnh `ktest-user`: 7 → **17**)

| TAP | Nhóm | Bằng chứng `# sys:` (neo) |
|---|---|---|
| 1 | `init u-mode` | `# init run: … syscalls=10 exits=1 exit=42` — 10 `ecall` U-mode |
| 2 | `debug_put_str` | `# user put_str: calls=2 bytes=51 sum_off=1` |
| 3 | `exit` | `# user exit: code=42 state=Exited exits=1` + `# sys: nr=14 err=0 value=42 …` |
| 4 | ngoài dải | `# user badsys: nr=31 err=19 calls=5 panic=0` + `# sys: nr=31 err=19 …` |
| 5 | `debug_get_info` | `# user getinfo: nr=15 calls=1 err=0 abi_version=1 exit=42` |
| 6 | `yield` | `# user yield: nr=8 calls=1 err=0 budget_left=…` |
| 7 | 16 số hiệu | 15 dòng `# sys: tap=7 nr=… out=… err=… state=… src=core` (nr 0..13 + 15; 14 là TAP 14); dòng nr=15 có thêm `harts=1/2/4` (**pin D2-88(a)**) |
| 8 | sai cap | `# sys: tap=8 empty=1 wrong_type=2 call_nonep=18 recv_wrong=2` |
| 9 | vòng IPC | `# sys: tap=9 call=1 recv=1 reply=1 mr=0x11/0x22 blocked=1 reply_caller=0 …` |
| 10 | nhóm NB + Notification | `# sys: tap=10 nb_send=0 nb_recv_mr=0x55 busy=14 nbsr_mr=0x66 nbwait=0 notif=0x99/0xab/0xcd ok=1` (payload `word/badge` cho 4/3/7) |
| 11 | WaitAddr | `# sys: tap=11 wait=1 wake=1 mismatch=5 notmapped=11 state=Ready ok=1` |
| 12 | ClockRead | `# sys: tap=12 nr=11 clock1=… clock2=… mono=1` |
| 13 | Yield đổi TCB | `# sys: tap=13 nr=8 tcb_before=… tcb_after=… state_after=Ready ok=1` |
| 14 | Exit không Reply | `# sys: tap=14 nr=14 err=0 code=42 … state=BlockedOnSend call=0 fault=Exit{42} ep_len=1 sender=… reply=0` |
| 15..17 | fault/wx (KT-56) | dời số, evidence giữ nguyên |

**Đỏ trước:** log ảnh `ktest-user` **HEAD cũ** (`JOURNAL/kt62-red-before-old-image.log`) chạy bằng
manifest/expected **mới** ⇒ **ĐỎ** (thiếu TAP 5..17 + evidence) — `JOURNAL/kt62-red-before-new-tap.log`.
Không retry lấy xanh; không nới ngưỡng.

### 11.4 Hai ngưỡng (D2-85a) + file ảnh

| Ngưỡng | Áp cho | Số đo KT-62 | Kết quả |
|---|---|---|---|
| **196.608 B** (pin D2-84) | `size_of::<Machine>()` | **180.168 B** (không đổi: KT-62 chỉ thêm **hàm**, không thêm trường) | ĐẠT |
| **262.144 B** (trần ảnh) | `.data+.sdata+.bss+.sbss` ELF thật | **191.416** (product) / **197.568** (ktest) / **2.680** (guard) / **192.304** (user) B | ĐẠT |

File ảnh (byte): product **185.736**, ktest **484.704**, guard **53.808**, user **323.296** — log
`JOURNAL/kt62-budget.log` (script `budget.sh` in, tài liệu không chép tay số đo ngoài log).
`ktest-user` tăng từ 11.752 → 192.304 B `.data+.bss` vì **link `Machine` vào ảnh test U-mode**
(+180.168 B `.bss`) — **bắt buộc** cho "syscall qua core", không phải "tiện cho test"; vẫn dưới
trần 262.144. **Lưu ý số file**: `.bss` là `NOBITS` (0 byte trong file); phần "file" lớn hơn
`.data+.bss` là **`.symtab`/`.strtab`** (bảng ký hiệu, không `PT_LOAD`, không nạp khi boot) — ngưỡng
dự án đo `.data+.sdata+.bss+.sbss` theo D2-72 Q5/D2-85a, **không** đo file thô.

### 11.5 Nợ mở (có chủ, không tô hồng)

| # | Nợ | Chủ |
|---|---|---|
| 1 | ~~chuyển cap trong message~~ — **ĐÓNG KT-63**: receive window + `plan_cap_transfer` + `RIGHT_GRANT` + badge (§12) | KT-63 |
| 2 | MR4..MR63 qua IPC buffer user (`length > 4` ⇒ 5 hiện tại) | KT-64 |
| 3 | invocation (Call trên cap không phải Endpoint) vẫn `NotSupported` (D2-53 Q11) | K3.5/K4 |
| 4 | scheduler U-mode: TCB chặn/ngủ ⇒ `PANIC: u-mode syscall khong the quay lai U-mode` có chẩn đoán (chưa có context switch user) | KT-64/65 |
| 5 | bản ghi trace 32 B cho syscall (hiện chỉ dòng văn bản `# sys:`) | K6 |
| 6 | **phủ QEMU** cho `resolve_addr` (9/10), `now_ns` (11) và nhánh `Switched` — hiện chỉ biên dịch + host rig/TAP `TestEnv` (F4) | KT-64/65 |
| 7 | ~~finalise IPC khi Revoke (caller treo)~~ — **ĐÓNG KT-63**: caller `Cancelled + Ready`, waiter thức, hàng huỷ (§12) | KT-63 |

### 11.6 Chạy lại

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt62_syscall -- --test-threads=1
cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-plat-common
bash v2/test/boot/ktest.sh 1 matrix
./v2/bin/v2 gate --fast
```

## 12. KT-63 — chuyển cap qua IPC + finalise IPC khi Revoke (K3.5 tầng 2)

**Trạng thái:** KT-63 **DONE (đề xuất; chờ review độc lập + Agent 0 lật QUEUE)**. Phạm vi: port
**nguyên ngữ nghĩa sim** đường **chuyển cap qua IPC** (receive window 4 ô, `plan_cap_transfer`,
`RIGHT_GRANT`, badge/unwrap, thất bại **mềm**, lỗi **cứng trước khi giao**) và **finalise IPC khi
`Revoke`/`Delete`** (caller `BlockedOnReply` ⇒ `Cancelled + Ready`; Recv/Notification waiter của
object bị finalise ⇒ thức, chống treo; hàng Endpoint huỷ + `ipc_dropped`). **Không** đổi `spec/`,
không sửa sim semantics, không đụng `viscript/`; `vifast-core` vẫn `#![forbid(unsafe_code)]`.

### 12.1 API + điểm nối (một nguồn: `kernel/vifast-core/src/{ipc,core_capxfer,core_finalise}.rs`)

| Thành phần | Ngữ nghĩa (giữ nguyên sim) |
|---|---|
| `Message.caps: [CapId; 4]` + `n_caps` | Cap **nguồn** đi cùng message — **không** copy object; `Message::with_caps`/`with_cap_ids` kiểm `caps.len() == info.caps()` ≤ 4 (**5 cap ⇒ `InvalidArgument`** ngay khi dựng, `.scn` 15) |
| `Message.snaps: [CapSnapshot; 4]` | Ảnh chụp `(cap, obj, ty, rights, badge)` lúc `admit` — bên gửi xoá cap ngay sau khi gửi vẫn cài đúng; cap chết giữa hàng và giao ⇒ thất bại **mềm** |
| `TcbData.recv_window: Option<[SlotRef; 4]>` | `None` = **chưa bind** (message có cap **cần cài** ⇒ `InvalidArgument` **trước** khi giao); `Some` = 4 ô CSpace (bind rỗng hợp lệ); ô đã có cap ⇒ cap tương ứng thất bại mềm (theo **vị trí**, không dồn) |
| `Machine::tcb_set_recv_window` / `tcb_recv_window` | Model API `[TK]` như sim: slot **có thật** của CNode còn sống, các ô **khác nhau**; **không** thêm chữ ký ABI (`Tcb.Configure` chỉ có CSpace) |
| `ipc::plan_cap_transfer` / `CapPlan` | (1) cap trỏ **chính Endpoint đang gọi** + có badge ⇒ **unwrap** (badge OR vào `Message.badge`, `extra_caps_unwrapped` tăng, **không** chiếm window); (2) thiếu `RIGHT_GRANT` ⇒ thất bại **mềm**; (3) ô đích đã có cap ⇒ thất bại mềm; (4) còn lại ⇒ cài con **bỏ `RIGHT_GRANT`**, giữ badge |
| `core_capxfer.rs` | `resolve_caps` (lỗi cứng `InvalidCapability` **trước** `msg_id`), `prepare_caps` (thêm **F1/F2** sau review: bảng cap đầy ⇒ `NotEnoughMemory`; ô đích thuộc CNode đã chết/slot sai ⇒ `InvalidCapability`/`RangeError` — **tất cả trước** `admit`/pop/take-waiter), `deliver_caps`, `deliver_message` — gọi ở `call_msg`/`endpoint_send`/`recv`/`nb_recv`/`reply_with`/`deliver_queued` |
| `core_finalise.rs` | `cancel_reply_object`, `clear_dead_reply_in_waiters`, `finalise_endpoint_body`, `finalise_notification_body` — gọi từ `Machine::{delete,revoke_children}` |
| Bộ đếm `cap_transfer_{installed,failed}`/`cap_unwrapped` | Cùng tên/nghĩa sim; accessor `cap_transfer_stats() -> (installed, unwrapped, failed)` |

**Thứ tự nguyên tử** (như sim): kiểm cap nguồn + receive window **trước** khi cấp `msg_id`/pop ⇒
lỗi cứng **không** giao nửa chừng, message **ở lại** hàng, không đổi trạng thái. Thất bại mềm
(thiếu GRANT/ô bận/cap chết) **không** làm mất message — `MessageInfo.caps` = số **cài được**.

### 12.2 Parity sim↔core — 5 kịch bản mới, so mã lỗi + digest **sau từng bước**

Mở rộng `v2/sim/vifast-sim/tests/kt60_core_parity.rs` (thêm `windows`/`cap_stats` vào `Digest`,
`Rig63` + `Op63` + runner `run63`) — **5 kịch bản mới**: 1 kịch bản **cap transfer** nhiều bước
(4 cap bỏ GRANT; thiếu GRANT/ô bận thất bại mềm; window chưa bind/cap nguồn chết lỗi cứng;
unwrap badge; Call/Reply mang cap) và **4 đường finalise** (`Delete` cap Reply, `Revoke` cap
Reply con, Reply cap chết trong waiter Q7b, `Revoke` Untyped khi có caller/waiter/hàng). Digests so **sau mỗi bước**: 3 TCB (state +
`TcbInfo` + `ReadState` + `IpcArea`), **receive window** (4 CPTR + `CapFp{ty,rights,badge}`),
hàng Endpoint đầy đủ nội dung, `Reply.caller`, Notification, counters IPC + **3 bộ đếm cap**,
8 cờ bất biến. Log: `JOURNAL/kt63-parity.log` — **16/16** test kt60 parity (11 cũ + 5 mới),
`JOURNAL/kt63-core-tests.log` — **13/13** test core-only (3 ca vá **F1/F2/F3** của reviewer-kt63: bảng cap đầy, window trỏ CNode chết, cap nguồn chết giữa hàng).

### 12.3 Đỏ trước (D2-05)

`JOURNAL/kt63-red-before.log`: viết test trước khi port (`kt63_core_cap_ipc.rs`) ⇒ core HEAD cũ
**không biên dịch** (`E0599` cho `Message::with_cap_ids`, `tcb_set_recv_window`,
`cap_transfer_stats`, `cap_transfer_installed/failed/unwrapped`) — đúng "tính năng chưa có",
không phải test sai. Sau port: 10/10 xanh. Không retry lấy xanh; không nới ngưỡng.

### 12.4 Hai ngưỡng (D2-85a) — vẫn **không** nâng pin

| Ngưỡng | Áp cho | KT-62 | KT-63 | Kết quả |
|---|---|---|---|---|
| **196.608 B** (pin D2-84) | `size_of::<Machine>()` | 180.168 | **188.448** (95,9 %; còn **8.160 B**) | ĐẠT — thêm `recv_window` (+4.608 B) + cap trong `Message` (+3.648 B) |
| **262.144 B** (trần ảnh) | `.data+.sdata+.bss+.sbss` ELF thật | 191.416 / 197.568 / 2.680 / 192.304 | **199.696** (product) / **209.856** (ktest) / **2.680** (guard) / **200.584** (user) | ĐẠT |

File ảnh (byte): product **191.824**, ktest **533.536**, guard **53.808**, user **330.704** — log
`JOURNAL/kt63-budget.log` (script `budget.sh` in, tài liệu không chép tay số đo ngoài log). Bảng
phân bố §7.3 cập nhật đúng 27 dòng + `scalars = 263`; test pin vẫn assert `padding ≤ 8` (D2-85c,
hiện 1 B) nên bảng thiếu dòng sẽ đỏ.

### 12.5 Anti-hang finalise IPC — **đã đồng bộ ở KT-64a** (D2-89 + D2-90), **không còn loại trừ**

**D2-89/D2-90 chốt chuẩn**: finalise IPC là **anti-hang** — caller `BlockedOnReply`,
waiter `Recv`/`Notification` **và** sender `BlockedOnSend` trên object bị `Revoke/Delete` đều
`Cancelled + Ready`; hàng gửi bị huỷ và message bị huỷ **đếm** vào `ipc_dropped` (D2-90).

**KT-64a (2026-11-16) đóng lệch** — hai đầu đã về **một nguồn sự thật**:

1. **Sim sửa theo chuẩn** (`sim/vifast-sim/src/kobj.rs::finalise_ipc_waiters`, gọi trong
   `revoke_children` **trước** khi body về free-list): Endpoint bị finalise ⇒ Recv waiter **và
   mọi** sender `BlockedOnSend` trong hàng nhận `Cancelled + Ready`; Notification ⇒ waiter
   `Cancelled + Ready`. `BlockedOnReply` (caller) vốn đúng từ KT-25 nên **không** đổi.
2. **Core**: KT-63 đã đúng cho caller/Recv/Notification; KT-64a đóng nốt **nợ D2-90 cho sender**
   trong `core_finalise.rs::finalise_endpoint_body` (cùng một chuẩn — điều khoản "trước KT-64a
   core được phép giữ hành vi sim (treo sender)" của D2-90 **hết** hiệu lực).
3. **Parity phủ trọn ba đường, không `skip`**: `kt60_core_parity.rs` có
   `kt63_parity_revoke_finalise_caller_va_sender` (caller `BlockedOnReply` **và** sender
   `BlockedOnSend`; chốt thêm pha **trước** `Revoke` sender còn `BlockedOnSend` — không phải
   kịch bản né), `kt64a_parity_finalise_recv_waiter`, `kt64a_parity_finalise_notification_waiter`.
   Digest KT-63 thêm `Tcb.last_ipc_error` ⇒ `Cancelled` được so **giữa hai bên** sau từng bước.
4. **`.scn` đỏ-trước**: `v2/test/scn/27-finalise-ba-loai-cho.scn` (26 ⇒ **27 tệp**, **16 nhóm**
   không đổi) + runner `sim/vifast-sim/tests/kt64a_scn.rs`; trên sim **cũ** runner đỏ đúng 3
   assert trạng thái (`JOURNAL/kt64a-red-scn.log`), sau khi sửa xanh. QEMU: TAP 55 (KT-63) vẫn
   là bằng chứng `# ipc:` cho đường revoke-có-caller/waiter/hàng.

Không đổi `spec/`; không nới ngưỡng.

### 12.6 TAP QEMU (ảnh `ktest`: 53 → **56** TAP) + `# ipc:` evidence

| TAP | Nhóm | Bằng chứng `# ipc:` (neo) |
|---|---|---|
| 53 | **chuyển cap** Endpoint→Recv qua 2 TCB | `# ipc: cap-transfer installed=8 failed=1 unwrap=1 caps4=4 len=2 rights=0xb caps3=3 caps1=1 extra=1 badge=0xa5a5 check_ipc=1 check_caps=1` — 4 cap cài đủ **bỏ** `RIGHT_GRANT`, 1 cap thiếu GRANT thất bại mềm (3 cài), unwrap badge 0xA5A5 không chiếm ô window |
| 54 | **message mang cap** qua Call/Reply | `# ipc: cap-message direction=call-reply caller_caps=2 reply_caps=2 caller_bound=1 caller_ready=1 rights=0xb reply_caller=0 check_ipc=1 check_reply_links=1` |
| 55 | **`Revoke` khi có caller/waiter/hàng** (+ **sender** từ KT-64a) | `# ipc: revoke deleted=5 caller=Ready caller_err=Cancelled recv_waiter=Ready notif_waiter=Ready sender=Ready sender_err=Cancelled queue=0 queued=1 pending=0 sent=2 dropped=1 check_ipc=1 check_reply_links=1 check_notifications=1` |

**KT-64a** — TAP 55 mở rộng **cùng số TAP**: thêm `sender=Ready sender_err=Cancelled` (D2-90:
sender `BlockedOnSend` trên Endpoint bị finalise được đánh thức — trước KT-64a core **treo**
nhánh này) + regex evidence trong `ktest.sh` sửa cùng commit (luật KT-54); `expected.txt`
**không** đổi (số TAP 60 và số mục evidence giữ nguyên — chỉ mở rộng nội dung **một** dòng).

Matrix `ktest.sh 1 matrix` ⇒ **9/9** boot sạch (smp 1/2/4 × 3 ảnh; ktest **56/56**, guard **3/3**,
user **17/17**); log `JOURNAL/kt63-ktest-matrix.log`. Trap lồng dời **53 → 56** (cùng commit sửa
kernel + manifest + `expected.txt` — luật KT-54). Soak ảnh sản phẩm: `JOURNAL/kt63-soak.log`.

### 12.7 Chạy lại

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt63_core_cap_ipc -- --test-threads=1
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt60_core_parity -- --test-threads=1
bash v2/test/boot/ktest.sh 1 matrix
bash v2/test/boot/soak.sh 1000 3
./v2/bin/v2 gate --fast
```

## 13. KT-64 — `TlbFlush` thật + `SlotPool`→arena + `SchedContext.Bind` + `tcb_resume_with_regs` (K3.5 tầng 3)

**Trạng thái:** KT-64 **DONE (đề xuất; chờ review độc lập + Agent 0 lật QUEUE)**. Phạm vi: bốn
mảng của nợ D2-86/D2-87 — (1) `TlbFlush` **thật** qua hook arch, chặn TCB chạy lại trên TLB cũ;
(2) slot CNode **từ arena** (bỏ pool `.bss`); (3) `SchedContext.Bind`/`Unbind` (hết “trường chết”
`bound_sc`); (4) `tcb_resume_with_regs` (resume sau fault một cửa). **Không** đổi `spec/`,
không sửa sim semantics, không đụng `viscript/`; `vifast-core` vẫn `#![forbid(unsafe_code)]` —
`TlbEnv` là **trait**, arch cài `sfence.vma` (D2-17).

### 13.1 `TlbFlush` thật (một nguồn: `kernel/vifast-core/src/tlb.rs`)

Core **không** sở hữu `Frame`/`PageTable`/`VSpace` (D2-83) và **không** chạm `sfence.vma`; object
layer (KT-52) sinh `FlushRequest` khi `map`/`unmap`/`protect`/teardown. Cầu nối **một chiều**:

| Thành phần | Ngữ nghĩa |
|---|---|
| `trait TlbEnv` | `active_on()` = bitmask hart **đang chạy** VSpace bị đổi; `generation()` = thế hệ ASID; `flush_local(hart, scope, asid, va)` = `sfence.vma` **thật** |
| `Machine::tlb_shootdown_begin(env, scope, asid, va)` | flush **cục bộ** trên hart gọi **trước**, rồi post `MailboxMsg::TlbFlush{seq, ack, generation, asid, va}` tới `active_on & !hart_bit(caller)`; **không** chờ |
| `Machine::tlb_shootdown_wait(env, hart)` | chờ **đủ** ack: mỗi vòng hart chờ **chỉ** rút hộp thư của mình (luật chống deadlock §4.10.3); ack của hart đích chỉ đến khi hart đó chạy `tlb_service` (**không** ack hộ — D2-92b); quá `MAILBOX_CAP+2` vòng ⇒ `Busy` (**không** treo, **không** `Ok` giả, `pending` giữ nguyên) |
| `Machine::tlb_flush(env, scope, asid, va)` | one-call = begin + wait; trả `TlbFlushReport{seq, active_on, targets, acked, local=true}`. Dùng được khi `targets == 0` (kernel hiện tại) hoặc khi caller đã cho hart đích chạy; ngược lại `wait` ⇒ `Busy` (không tự ack hộ) |
| `Machine::tlb_service(env, hart)` | hart đích flush thật + ack (+ rút `TlbFlush` khỏi hộp thư); trả 0/1 |
| `Machine::dispatch(hart)` | **từ chối** `Busy` khi `tlb_pending[hart]` còn active — TCB **không** chạy lại trên TLB cũ cho tới khi `tlb_service` |

Bất biến: (a) **luôn** flush cục bộ, kể cả 1 hart; (b) chỉ post/chờ hart trong `active_on` khác
hart gọi (cùng công thức `tlb::targets`, D2-78); (c) hart gọi **ngoài** `active_on` ⇒
`IllegalOperation`; (d) đang có shootdown chưa ack ⇒ `begin` thứ hai `Busy`; (e) post **sau** khi
kiểm `hộp thư đích còn chỗ` ⇒ không nửa chừng. (f) **không ack hộ**: `wait` không flush/ack thay
hart khác ⇒ ack không tới là `Busy` **thật** (có test `kt64_tlb_ack_khong_toi_tra_busy_khong_treo`,
D2-92b), không phải nhánh chết.

**Bằng chứng (TAP 59 + mutation):** map → ghi/đọc qua VA (phần cứng dịch) → `protect`/`unmap`/
**đổi root** mỗi bước đi qua `tlb_flush` ⇒ một **load thật** sau `unmap` fault `scause=13` tại
đúng VA, và sau khi về root kernel cũng `13` ⇒ **không còn trang cũ đọc được**. **Mutation M1**
(bỏ hẳn đường core `tlb_flush`) ⇒ TAP 59 **ĐỎ** (`requests=0`, 58/60) — chứng minh TAP **phụ
thuộc** đường flush, không tự chứng minh (`JOURNAL/kt64-mutation-m1-core.log`). **Mutation M2**
(`KernelTlb::flush_local` không `sfence.vma`) ⇒ TAP 59 **vẫn xanh**: QEMU TCG không giữ
translation cũ theo cách TAP quan sát được, nên **hiệu lực phần cứng của `sfence` không đo được
bằng TAP này** — nó được pin ở **KT-51** (TAP 25..29 + conformance 24, mô hình TLB + shootdown
thật); ghi nợ, không tô hồng (`JOURNAL/kt64-mutation-m2-sfence.log`).

**F3 — parity `TlbFlush` đã làm ở KT-64a (D2-92c), không còn miễn:** sim **có** giao thức
shootdown (`tlb_shootdown_begin/wait/poll`, `tlb_pending`, mask ack — KT-40) nên
`seq/targets/pending/acked/Busy` so được với core. Kịch bản **C** của
`v2/sim/vifast-sim/tests/kt64_core_parity.rs` (`kt64a_parity_tlb_flush_mot_hart`,
`kt64a_parity_tlb_flush_hai_hart_busy_va_pending`) so **cùng một định nghĩa**:

| Trường | Sim (KT-40) | Core (KT-64) | Phép so |
|---|---|---|---|
| `seq` | `TlbShootdownInfo.seq` / `tlb_shootdown_count` | `tlb_seq()` | **bằng** (1-based, không tăng khi `Busy`) |
| `targets` | `targets` = `active_on` (quy ước `TlbShootdownInfo` **và** `tlb::last_shootdown` của kernel) | **tập hart bị post**: `mail_len(h)` từng hart + `tlb_remote_posts()`; `TlbFlushReport.targets` chỉ đọc được ở ca **`targets == 0`** (C1) | C1: `report.active_on == info.targets` và `report.targets == tlb::targets(info.targets, caller)` (giá trị **0**). C2 (`targets = {1}`): so **gián tiếp** đúng tập hart bị post + `info.messages == tlb_remote_posts()` — **không** đọc `report.targets` |
| `pending` | hart gọi: `tlb_pending`; hart đích: **`tlb_pending_ack`** (KT-64a **thêm vào sim**) | `tlb_pending(hart)` (một ô/hart, hai vai) | **bằng** `Some(seq)`/`None` theo từng vai (C1 + C2) |
| `acked` | `TlbShootdownInfo.acked` (gồm ack ngầm hart gọi, D2-68) | `TlbFlushReport.acked` chỉ ở ca `targets == 0` (C1) | C1: `report.acked == tlb::targets(info.acked, caller)` (giá trị **0**). C2: so **sự kiện** — sim `acked == targets` ⇔ core mọi hart đích hết `tlb_pending` — + `tlb_ack_count()` **bằng** `tlb_serviced()`; **mask `acked ≠ 0` của core chưa quan sát được** ⇒ nợ §13.9 mục 5 |
| `Busy` | `begin` lần hai khi còn pending (**cùng hart gọi**) | cùng luật | **cùng** mã `Busy` (không ghi đè lệnh chưa ack). **Chỉ** ca cùng hart gọi |

Hai chi tiết **sim thiếu** đã thêm cho khớp chuẩn (không hạ core): (i) `tlb_pending_ack(hart)` —
vai “hart đích đang nợ ack” của `pending` (core giữ chung một ô/hart); (ii) shootdown **không
còn hart nào phải ack** (`targets ⊆ {hart gọi}`) **xong ngay ở `begin`** — không giữ ô `pending`
rỗng (core: `targets == 0` ⇒ không `pending`; kernel KT-51 cũng `completed` ở vòng chờ đầu).

**Phạm vi mô hình (không phải loại trừ) — không tuyên bố “đã kiểm”:** sim còn cho **hai hart
cùng mở** shootdown (`kt40_tlb::deadlock_hai_hart_cho_nhau_poll_tu_phuc_vu`, chứng minh luật chống
deadlock §4.10.3 / D2-68), còn core (và `IN_FLIGHT` của kernel KT-51) giữ **một** shootdown toàn
cục ⇒ `begin` từ hart đang nợ ack trả `Busy` bên core. Parity **chỉ** so `Busy` ở ca **cùng hart
gọi** (nơi hai bên cùng luật); shootdown **đồng thời** là **phạm vi mô hình** của sim K2 —
**không** được đọc là “đã kiểm parity” cho đường đó.

**Nợ quan sát (D2-92c, trung thực):** `TlbFlushReport.targets/.acked` với giá trị **≠ 0** của
core **chưa** đọc được qua `begin`+`wait` (`tlb_flush` nhiều hart luôn `Busy` khi chưa ai chen
`tlb_service` — D2-92b). Vì vậy ca 2 hart so **đúng tập hart bị post** + `pending` hai vai + bộ
đếm ack + sự kiện `acked == targets`, **không** so mask `acked` của core. Muốn so mask thật phải
thêm report cho `wait`/`flush` (đổi API core) — **không** làm trong KT-64a; ghi nợ §13.9 mục 5.

**Biên kernel có chủ đích:** `core_bridge` giữ **một** `Machine` sau spin lock trên BSP
(D2-88(a): `num_harts` là số hart nền tảng, không phải số hart core chạy). `KernelTlb::active_on`
= `tlb::active_on() & (1 << hart đang chạy)` ⇒ trong nhân **targets = 0**: flush cục bộ là bắt
buộc và đủ; đường multi-hart (post + chờ ack + `dispatch` chặn) pin bằng test host, còn
shootdown nhiều hart **thật** của kernel vẫn là `tlb::shootdown` KT-51 (TAP 25..29, conformance
24). Nối IPI cho đường core multi-hart là việc của KT-65+ (context switch U-mode); ghi nợ, không
gọi là “đã chạy đa hart trong core”. **Nếu** đường multi-hart lọt vào kernel hiện tại
(`active_on` có hart khác hart đang chạy core) thì `KernelTlb::flush_local` `debug_assert_eq!` sẽ
nổ/`sfence` sai hart ⇒ **lỗi lập trình lộ ngay**, không im lặng “đúng”.

### 13.2 Slot CNode → **arena** (một nguồn sự thật; `kernel/vifast-core/src/slotmem.rs`)

`SlotPool` (bump + free-list riêng, `.bss` 16.656 B) bị **xoá**. Byte slot của CNode **Retype từ
Untyped** nằm **trong byte arena** tại `offset + cnode_base`; `CapId` của slot được lưu ngay
trong cửa sổ đó. **Không** còn allocator thứ hai: chỉ số word là **hàm thuần của offset arena**

```text
word(offset, slot) = (offset + CNODE_BASE_BYTES) / SLOT_BYTES + slot
```

**F1 (D2-92a) — cửa sổ phải là hàm của `(Region, offset)`, không chỉ `offset`:** `Region` nào
cũng bắt đầu `cursor = 0`, nên nếu chỉ dùng `offset` thì hai Untyped khác nhau có **cùng** word ⇒
cap của CSpace này ghi đè CSpace kia, **im lặng** (đọc ra cap sai quyền). Bản sửa: vùng A
(3072 word) chia **đều** cho `MAX_UNTYPEDS = 16` ⇒ [`REGION_SLOT_WORDS`] = **192 word/Region**,
`word(region, offset, slot) = region·192 + (offset + 64)/48 + slot`. Test hồi quy
`kt64_slot_hai_untyped_cung_offset_khong_alias` (hai CNode từ **hai** Untyped, cùng offset 0 ⇒
cap **không** chồng, đọc đúng của mình) pin. Trong **cùng** Region, hai CNode liên tiếp không
chồng (block sau bắt đầu sau `64 + 48·s` byte, căn 64 ⇒ chỉ số đầu `≥ index + s + 1`); test
`kt64_slot_hai_cnode_khong_chong` pin. Cửa sổ word `MAX_SLOT_WORDS = 4096` chia hai vùng rời:
**A arena** (3072 word ⇒ offset arena < **147.456 B**, 192 word/Region ≈ **190 slot**) và
**B harness** (**1024** word cuối, ánh xạ **lùi**) cho CNode do kernel cấp
(`add_cnode`/`add_kernel_object` — như `add_cnode` của sim, **không** tiêu byte arena). Công thức
harness là `base = MAX_SLOT_WORDS − index − slots` (bỏ `−1` từng có); `index + slots ≤ 1024` ⇒
`base ≥ 3072` — **không** trùng word 3071 (word cuối vùng A của Region 15). Test biên: harness đầy
**đúng** 1024 word ⇒ `base = 3072`; vượt 1 word ⇒ `None` (fail-closed). Retype
CNode vượt dải của Region mình ⇒ `NotEnoughMemory` **trước** khi cấp (`Region::probe`,
fail-closed). Biên `192 word/Region` là `[TK]`, **không** ABI — ghi nợ §13.9.
`Revoke` Untyped đặt `Region::reset` ⇒ byte slot trả lại arena (không còn free-list), cửa sổ
`fill_zero` khi CNode chết/tái dùng. **Tiêu chí §9.6:** (i) slot nằm trong byte arena thật —
`cnode_slot_span` trả `(region.base + offset + cnode_base, slots*48)`, TAP 56 chứng minh span
**trong** `untyped_arena`; (ii) `Revoke` trả byte (`used` về mốc); (iii) `check_bytes`/
`check_all_bytes` không đổi kết quả; (iv) TAP 45/52 vẫn xanh. Hai nguồn sự thật cho byte arena
đã hết: **`Region.cursor` là nguồn duy nhất**.

### 13.3 `SchedContext.Bind`/`Unbind` (`core_sync.rs`, `tcbdata.rs`)

`ScData.bound: Option<CapId>` (trường **mới**; trước KT-64 `Notification.bound_sc` là trường
**chết** §8.6.0). `sched_context_bind(sched, target)` port nguyên sim KT-27: `target` phải là cap
**Reply/Notification** (khác ⇒ `WrongObjectType`; cap chết ⇒ `InvalidCapability`; slot rỗng ⇒
`SlotEmpty` — kiểm `cap_at` trước, đúng sim); notification đã bind **SC khác** ⇒
`IllegalOperation` **trước** mọi mutation; SC đổi target ⇒ gỡ binding cũ phía notification
(1:1 hai chiều). `sched_context_unbind(sched)` (spec method 1; **sim không mô hình hoá** ⇒
core-only + test) xoá cả hai chiều, idempotent. `Machine::delete` cap Notification và finalise
Notification khi `Revoke` gọi `drop_sched_binding_to_cap`/`_to_obj` ⇒ không `bound_sc` mồ côi.
**D2-92d — bất biến 1:1 hai chiều vào `check_notifications`** (chạy sau **mỗi** public op):
`ScData.bound` phải trỏ cap Reply/Notification **còn sống**; nếu là Notification thì
`Notification.bound_sc` phải trỏ **lại đúng** SC đó, và ngược lại. Test
`kt64_bind_revoke_go_binding_mo_coi` pin `Revoke` Untyped chứa Notification ⇒ hook gỡ `SC.bound`,
bất biến xanh trước/sau.
**`Signal` chỉ đánh thức TCB đúng bound**: nhánh `bound_tcb_blocked_on_recv` chỉ chọn TCB có
`bound_notification == ntfn`, TCB bound notification khác vẫn `BlockedOnRecv` (test + TAP 57);
parity so đường này từng bước.

### 13.4 `tcb_resume_with_regs` (`core_tcb.rs`)

Port sim KT-26: chỉ nhận **bốn** trạng thái — `BlockedOnReply{reply}` **đúng caller** (Reply của
người khác ⇒ `IllegalOperation`), `BlockedOnSend{call:true, fault:Some(k≠Exit)}`, `Ready`,
`Sleeping{..}` (giữ nguyên reason). Thứ tự: kiểm trạng thái đích → gỡ mọi hàng chờ/`Reply.caller`
→ ghi `regs` qua **một** cửa `ipc::transfer` (`resolve_caps` + `admit` + `deliver`) **trước** khi
đổi trạng thái → bỏ `suspended` → `transition_tcb(Ready)` (trừ `Sleeping`/`Ready`). Giữ
`exit_code`; không phát lại IPC cũ; message mang cap ⇒ `InvalidArgument`. **Không mở đường cho
user tự resume:** đây là model API — 16 syscall `0..15` (D2-88(c)) **không** có mục nào gọi nó;
`Running`/`Inactive`/`Dead` bị từ chối (test + TAP 58 in `user_nr_max=15 user_path=0`). Dùng cho
U-mode fault recovery **test-only** trong KT-56/Giai đoạn sau (procsrv KT-65 gọi model API này).

### 13.5 Parity sim↔core — 2 kịch bản mới, so mã lỗi + digest từng bước

`v2/sim/vifast-sim/tests/kt64_core_parity.rs` (mới, **2/2**): (A) `Bind` + `Signal` — bind 1:1,
vi phạm 1:1, rebind gỡ phía cũ, target sai kiểu, hai TCB bound hai notification chờ Recv, signal
chỉ đánh thức đúng bound; (B) `resume_with_regs` — fault `BlockedOnSend` → resume ghi regs,
`BlockedOnReply` → resume, `Running` cấm, message mang cap `InvalidArgument`. So **mã lỗi ABI**
(`KError::id()`) + digest (notification `bound/word/badge/waiter`, trạng thái TCB, thanh ghi,
`last_fault`, `reply_caller`, `ipc_sent/delivered/dropped`, 3–4 cờ bất biến) **sau từng bước**.
`Unbind` không có ở sim (core-only, ghi trên); slot-arena không có đối tượng sim tương ứng
(13.2) ⇒ core-only + TAP. `TlbFlush`: **KT-64a (D2-92c)** thêm **kịch bản C** (2 test, §13.1 F3) ⇒
tệp parity **4/4** (A `Bind`+`Signal`, B `resume_with_regs`, C1 `TlbFlush` 1 hart, C2 `TlbFlush`
2 hart `Busy`+`pending`). **Không còn mảng nào của KT-64 bị miễn parity.**

### 13.6 Đỏ trước (D2-05)

`JOURNAL/kt64-red-before.log`: test viết **trước** khi port (`kt64_core_tlb_bind_resume.rs`) ⇒
core HEAD-cũ **không biên dịch**: `E0432 unresolved imports TlbEnv, TlbScope`; `git grep` tại
HEAD `2e2631fa` cho `sched_context_bind`/`sched_context_unbind`/`tcb_resume_with_regs`/
`cnode_slot_span`/`TlbEnv` ⇒ **0 kết quả** (tính năng chưa có, không phải test sai). Sau port:
**15/15** test core + **2/2** parity xanh. Không retry lấy xanh; không nới ngưỡng.
**Mutation sau port** (`JOURNAL/kt64-mutation.log`): M1 bỏ core `tlb_flush` ⇒ TAP 59 **đỏ**
(`requests=0`); M2 bỏ `sfence.vma` ⇒ TAP 59 **không** đỏ (giới hạn TCG — nêu ở 13.1/13.9).

### 13.7 Hai ngưỡng (D2-85a) — vẫn **không** nâng pin

| Ngưỡng | Áp cho | KT-63 | KT-64 | Kết quả |
|---|---|---|---|---|
| **196.608 B** (pin D2-84) | `size_of::<Machine>()` | 188.448 | **188.872** (còn **7.736 B**) | ĐẠT |
| **262.144 B** (trần ảnh) | `.data+.sdata+.bss+.sbss` ELF thật | 199.696 / 209.856 / 2.680 / 200.584 | **200.120 / 209.856 / 2.680 / 201.008** (product / ktest / guard / user) | ĐẠT |

Bảng phân bố §7.3 cập nhật: `slot_pool` 16.656 → **`slot_mem` 16.384 + `harness_region` 24**;
thêm **`tlb_pending[8]`** (384 B); `ScData` 56 → **64 B** (`bound`, +256 B); 4 bộ đếm TLB ⇒
`scalars` 263 → **295**; tổng **29 dòng** (27 → 29). Dải slot **theo Region** là hằng compile-time
(192 word), **không** thêm trường ⇒ pin không đổi sau F1. Log `JOURNAL/kt64-budget.log`; test pin vẫn
assert `padding ≤ 8` (D2-85c, hiện 1 B) nên bảng thiếu dòng là đỏ.

### 13.8 TAP QEMU (ảnh `ktest`: 56 → **60** TAP) + evidence riêng

| TAP | Nhóm | Bằng chứng (neo) |
|---|---|---|
| 56 | **slot arena** | `# slot: arena=0x…+65536 span=0x… bytes=768 within=1 used0=8192 used1=9024 used2=0 grown=1 slot_ok=1 revoke_back=1 check=1` — span nằm trong arena, `Retype` +768 B, `Revoke` trả hết byte; dải word theo Region (F1) pin thêm ở test host |
| 57 | **bind** | `# bind: sc=… ntfn=… two_way=1 second=IllegalOperation rebind_clears=1 wrong_target=1 signal_wake=1 other_blocked=1 unbind=1 delete_clears=1 check=1` |
| 58 | **resume** | `# resume: fault=1 state=Ready regs=0xaa/0xbb running=IllegalOperation regs_kept=1 reply=1 reply_caller=0 cap=InvalidArgument inactive=IllegalOperation exit_kept=1 user_nr_max=15 user_path=0 check=1` |
| 59 | **`TlbFlush` thật** | `# tlb: core asid=1 requests=3 seq=3 local=3 targets=0 acked=0 pending=0 stale_load=13 stval_ok=1 protect_read=1 restore=1 root_fault=13 harts=1 check=1` |

Matrix `ktest.sh 1 matrix` ⇒ **9/9** boot sạch (smp 1/2/4 × 3 ảnh; ktest **60/60**, guard
**3/3**, user **17/17**; file ảnh 183.272 / 573.136 / 53.808 / 335.920 B); log
`JOURNAL/kt64-ktest-matrix.log`. Trap lồng dời **56 → 60** (cùng
commit sửa kernel + manifest + `expected.txt` — luật KT-54). TAP 59 chạy **ngoài** khoá
`core_bridge` (lock không re-entrant; nó tự `with_machine` cho từng `FlushRequest`). Soak ảnh sản
phẩm: `JOURNAL/kt64-soak.log`.

### 13.9 Nợ còn lại (không tô hồng)

1. **Core multi-hart chưa nối IPI:** `active_on` của kernel giao với hart đang chạy core ⇒
   targets = 0 trong nhân; đường post + chờ ack **chỉ** có test host (QEMU không chạy đường này).
   Nối vào IPI khi core chạy TCB trên hart khác (KT-65+).
2. **`TlbFlush` theo `Generation`** (cạn ASID) vẫn là việc của `tlb::shootdown` KT-51 (core chỉ
   có `Page`/`Asid`/`All` như `FlushRequest`); không mở thêm mức ở đây.
3. **`SchedContext.YieldTo`/`Consumed`** chưa port (ngoài phạm vi tầng 3; KT-65 dùng).
4. **`tcb_resume_with_regs` chưa có đường W^X/VM fault U-mode thật** trong ảnh sản phẩm — nó là
   model API cho procsrv (KT-65); TAP 58 chỉ chứng minh ngữ nghĩa trạng thái.
5. **KT-64a ĐÓNG** (D2-89/D2-90 + D2-92c): sim đã theo chuẩn anti-hang (§12.5) và parity
   `TlbFlush` đã làm (§13.1 F3) — **không** còn loại trừ ở hai đường đó. Còn lại **phạm vi mô
   hình**: shootdown **đồng thời** trên hai hart (sim cho, core/kernel `Busy` — **không** parity)
   và `active_on` nhiều hart **thật** trong nhân (mục 1 ở trên). **Nợ quan sát:** mask
   `TlbFlushReport.targets/.acked ≠ 0` của core chưa đọc được qua `begin`+`wait` ⇒ ca 2 hart so
   tập hart bị post + `pending` + bộ đếm ack + sự kiện `acked == targets`, **không** so mask
   (§13.1 F3). TAP 55 trên QEMU nay in **cả** `sender=Ready sender_err=Cancelled` (D2-90).
6. **Dải slot 192 word/Region (`[TK]`, ≈190 slot/Untyped)** — đủ cho mọi ca hiện tại
   (`Retype` CNode ≤ 96 slot) nhưng **giảm** so với pool 4096 word cũ; nếu procsrv (KT-65) cần
   CNode lớn hơn, phải tách dải theo sức chứa/`MAX_UNTYPEDS` thực dùng (quyết định riêng, không
   nới ngầm).
7. **Hiệu lực phần cứng của `sfence.vma` không đo được bằng TAP 59 trên QEMU TCG** (mutation M2
   vẫn xanh — 13.1/13.6); nó dựa vào KT-51 (TAP 25..29 + conformance 24). Ghi để người đọc không
   tưởng TAP 59 chứng minh `sfence` chạy đúng.

### 13.10 Chạy lại

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt64_core_tlb_bind_resume -- --test-threads=1
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt64_core_parity -- --test-threads=1
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt60_core_parity -- --test-threads=1
cargo clippy --manifest-path v2/kernel/Cargo.toml --workspace --no-deps --target riscv64gc-unknown-none-elf -- -D warnings
bash v2/test/boot/ktest.sh 1 matrix
bash v2/test/boot/soak.sh 1000 3
./v2/bin/v2 gate --fast
```

## 14. KT-67a — invocation hẹp trên syscall 0 (D2-97): core giữ cap/quyền, object layer thi hành

> **Số mục**: tệp này đang dừng ở §13; §14 là **mục kế tiếp** cho KT-67a (D2-97). Yêu cầu của
> task ghi “§16” theo dự kiến có thêm §14/§15 (KT-65/KT-66 nằm ở `k35-procsrv.md`) — nội dung
> invocation nằm **ở đây**, không có mục 15/16 nào khác.

### 14.1 Vì sao không sửa `spec/`, không thêm syscall

`spec/abi.toml` `[syscall].0` đã ghi *“Endpoint: IPC; object khác: invocation (method = label),
trả ngay”* và `[object]` đã cấp `Untyped.Retype = 0`, `Frame.Map = 0`/`Frame.Unmap = 1` (D2-97).
KT-67a chỉ **cài** đường đó trong `v2/kernel/vifast-core/src/invoke.rs`:

- `Call` (0) trên cap **không** phải Endpoint ⇒ giải mã `MessageInfo` (`label = method`,
  `length ≤ 4` MR, `caps` phải 0) → **danh sách trắng hẹp** → hook `SyscallEnv::invoke`.
- Mọi cặp `(loại object, method)` ngoài danh sách ⇒ `NotSupported` **18** (giữ D2-53 Q11 cho phần
  chưa cài, **không** `Ok` giả) + tăng bộ đếm `INVOCATION_STATS.denied` (TAP `# inv: deny`).
- Không block/không Reply/không đổi trạng thái TCB: `dispatch` trả `SyscallOutcome::Return` ngay.

### 14.2 Danh sách trắng + kênh tham số (D2-99; `MessageInfo` 35 bit không đổi)

| Object (id) | method | Thao tác | Điều kiện cap |
|---|---|---|---|
| `Untyped` (1) | 0 | `Retype` `Frame`/`PageTable`/`VSpace` từ object layer, cài cap vào slot đích (`MR0`) | `RIGHT_WRITE` |
| `Frame` (11) | 0 | `Map` chính Frame vào `VSpace` (`MR0`) tại `va` (`MR1`), cờ `MR2` | `RIGHT_WRITE` |
| `Frame` (11) | 1 | `Unmap` tại `va` (`MR1`) của `VSpace` (`MR0`) | `RIGHT_WRITE` |

**Quy ước tham số = D2-99** (lead, 2026-11-16): `MR0..MR3` + trả `a1`; **KT-67b** cập nhật
`spec/` **một lần** cho `Untyped.Retype`/`Frame.Map`/`Frame.Unmap` khớp quy ước này — KT-67a
**không** tự coi đó là “không có ABI mới”. Kiểm thêm trên đường invocation: `Frame.Map/Unmap` đòi
cap `VSpace` có `RIGHT_WRITE` (chỉ READ ⇒ 3); `Frame.Unmap` so PA leaf tại `va` với **đúng** cap
Frame (sai ⇒ 1/11, mapping giữ nguyên); `MessageInfo.caps > 0` ⇒ **18** + đếm (D2-99(g)).

**Tên “`VSpace.Map`” của D2-97 không có method id**: `VSpace` (id 9) chỉ có `GetAsid = 0`,
method 1 **để trống theo D2-32**; `PageTable.Map = 0` nhưng root PageTable không có cap. Phép map
vì vậy dùng đúng tên/số spec cấp: **`Frame.Map` = 11.0** (map Frame vào VSpace ở `MR0`) — không
thêm method id (cấm sửa `spec/`). Hệ quả/thứ tự kiểm ghi ở `v2/docs/k6-memsrv.md` §7.1.

### 14.3 Biên giới D2-83 giữ nguyên (core **không** sở hữu VM)

- Core **không** đọc/ghi PTE, không kế toán PA, không gọi object layer: nó chỉ kiểm cap/quyền/
  loại, lấy **khoá** `foreign_key` (`(object_id << 16) | cap_index`) và chuyển cho hook arch.
- `Untyped.Retype`: hook arch (object layer) cấp object và trả `key`; core kiểm **đúng loại**
  đã yêu cầu rồi dựng **object đại diện** (`add_foreign_object`, đã có từ KT-67) + cap gốc và
  `put_cap` vào slot đích — cap U-mode giữ là cap **của core**, byte/PA vẫn do object layer. Lỗi
  core sau khi object layer đã cấp ⇒ rollback **đủ ba bước**: `drop_root_cap` (cap gốc chưa vào
  slot) → `kill_foreign_object` → `SyscallEnv::release_object(key)`; không để cap mồ côi trỏ object
  chết (reviewer L2).
- Cap `Untyped` của memsrv là **cap core** (bản `copy` của cap boot, `RIGHT_WRITE`); bridge
  (`v2/kernel/vifast-plat-riscv64/src/mem_bridge.rs` `register_untyped`, ≤ 4 entry) giữ ánh xạ
  một chiều `core ObjId → cap Untyped object layer` để hook biết **cap nào sở hữu byte**. Không
  có bản sao thứ hai của allocator/kế toán PA.
- Trong syscall U-mode, `satp` vẫn trỏ **root user** (không có physmap — KT-53): hook invocation
  bọc `with_kernel_root` (khôi phục root kernel → chạy object layer + `sfence.vma` → trả lại root
  user trước `sret`), cùng khuôn `resolve_addr`. Thiếu bước này ⇒ kernel fault
  `stval=0xffffffc0…` (log đỏ-trước `JOURNAL/kt67a-red-satp-kernel-fault.log`).

### 14.4 Bộ đếm + test host

- `INVOCATION_STATS` (5 × `AtomicU32` = 20 B `.bss`): `allowed`, `denied`, `retype`, `map`,
  `unmap` — TAP đọc **delta** của riêng pha (`# inv: retype … stats=1/3` v.v.).
- 6 test host mới trong `invoke.rs` (tổng core **25** test): `map_flags_kiem_bien`,
  `type_id_sinh_tu_spec` (id đọc từ `vifast_abi`, không chép số), `danh_sach_trang_hep`,
  `retype_chi_ba_loai_vm`, `so_dem_chi_tang`.
- Pin tĩnh không đổi: `size_of::<Machine>() = 188.872 B` ≤ pin **196.608** (D2-84/85); phần thêm
  của KT-67a là static đếm 20 B, không nằm trong `Machine`.

### 14.5 TAP QEMU (ảnh `ktest-user`: 14 → **18** TAP)

| TAP | Nhóm | Bằng chứng (neo, log `JOURNAL/kt67a-ktest-user.log`) |
|---|---|---|
| 15 | `invocation untyped retype` (`src/ktest_inv.rs`) | `# inv: retype method=0 ty=Frame size=12 err=0 slot=15 stats=1/3` — cap Frame do **U-mode tự retype** (không kernel-mediated), `resolve` ra PA thật |
| 16 | `invocation frame map` | `# inv: map method=0 va=0x41000000 pa=0x82015000 err=0 stats=1 phys=0x1122334455667788` — kernel đọc **vật lý** tại PA đúng giá trị U-mode ghi qua PTE vừa map |
| 17 | `invocation frame unmap va fault` | `# inv: unmap … walk_after=0` + `# inv: fault va=0x41000000 kind=Vm{NotMapped,Read} ok=1` — PTE invalid + fault phần cứng thật |
| 18 | `invocation ngoai danh sach` | `# inv: deny err=18 stats=3 allowed=3 last_err=18` — hai method ngoài danh sách + `caps=1` (D2-99(g)) đều trả **18**, không `Ok` giả; `# inv: deny2 ran=1 frame_a_maps=0 frame_b_maps=0` (M1/M2 đã chạy, mapping Frame A còn sau ca unmap sai Frame) |

Matrix `ktest.sh 1 matrix` ⇒ **12/12** boot sạch (smp 1/2/4 × 4 ảnh; ktest **65/65**, guard
**3/3**, user **18/18**, proc **10/10**); log `JOURNAL/kt67a-ktest-matrix.log`. Soak ảnh sản phẩm
**0/1000**: `JOURNAL/kt67a-soak.log`. Gate `--fast`: **661/87** + L1 1.000 seed
(`JOURNAL/kt67a-gate.log`; cổng không chạy unit-test `vifast-core` — 5 test mới chạy bằng
`cargo test -p vifast-core`, tổng core 25 test).

Red-before (cùng commit, log trong `JOURNAL/`): `kt67a-red-no-invocation.log` (bỏ nhánh
invocation ⇒ TAP 15..18 đỏ, mọi `Call` non-Endpoint trả 18), `kt67a-red-no-map.log` (chỉ Retype ⇒
15 xanh, 16/17/18 đỏ), `kt67a-red-retype-unregistered.log` (bridge chưa đăng ký ⇒ `err=1`) và
`kt67a-red-satp-kernel-fault.log` (thiếu `with_kernel_root` ⇒ kernel fault).

### 14.6 Nợ còn lại (không tô hồng)

1. ~~**Chuyển cap giữa hai TCB U-mode**~~ — **ĐÓNG ở §15** (KT-67b): **ABI đã đủ** — receive
   window nằm trong IPC buffer header (`spec/abi.toml` `[calling.riscv64].ipc_buffer`, dòng 58) và
   core đã có `tcb_set_recv_window`/`plan_cap_transfer`/`deliver_caps`; phần thiếu là **shim
   U-mode** + `build_message` đọc header — KT-67b cài cả hai (TAP 19: memsrv.elf → client.elf,
   cap cài vào **cspace B**).
2. **`CNode.Insert`** không có method id trong `spec/abi.toml` (bảng CNode 0..6) ⇒ chưa cài,
   `NotSupported`; core đã có API `copy/mint/delete/revoke` để gắn method id có sẵn sau này.
3. **Revoke xuyên invocation** (`CNode.Revoke`/`Untyped` revoke qua invocation) chưa mở; chỉ
   TAP/test kernel gọi API core. `release_object` chỉ là rollback nội bộ của Retype.
4. `Frame.Map` chỉ nhận cap `Frame` **đại diện** (foreign) — map Frame core-owned (nếu có) trả
   `WrongObjectType`; object layer vẫn kiểm W^X/Device/căn lề và trả mã ABI đúng.

## 15. KT-67b — header IPC buffer trong core: `build_message` lấy window + cap từ header (D2-98)

Mục tiêu: **hai tiến trình U-mode** thật sự trao đổi **cap Frame** qua Endpoint, cap nhận nằm
trong **cspace của bên nhận**. ABI đã đủ (receive window trong header IPC buffer — `spec/abi.toml`
dòng 58); task này cài phần **core + shim** còn thiếu, không thêm syscall, không đổi
`MessageInfo` 35 bit, không nới ngưỡng.

### 15.1 Model: `IpcHeader` + hook arch (D2-83)

`v2/kernel/vifast-core/src/ipc.rs` thêm (model `[TK]`, không phải ABI):

| Hằng/kiểu | Nội dung |
|---|---|
| `IPC_HEADER_MAGIC` | `0x0000_0062_3736_544b` = `b"KT67b"` little-endian |
| `IPC_HEADER_WORDS = 10` | magic + info + window(4) + send caps(4) |
| `IPC_HEADER_W_INFO = 1`, `W_WINDOW = 2`, `W_SEND_CAPS = 6` | offset word |
| `IpcHeader { info, recv_window: [u64;4], send_caps: [u64;4] }` | bản đọc từ bộ nhớ user |

`SyscallEnv` thêm `fn ipc_header(&mut self, _tcb: ObjId) -> Option<IpcHeader> { None }` — **mặc
định `None`** nên mọi ảnh/nền tảng chưa dùng header giữ nguyên hành vi cũ (kể cả test kernel-level
với `TestEnv`). Arch `vifast-plat-riscv64::user::UserMemAccess` cài: chỉ đọc khi `LAST_VSPACE` đã
có (đang trong syscall U-mode), trang nằm trong `active_ranges` (đăng ký lúc nạp), đọc 10 word
bằng `sstatus.SUM` rồi tắt ngay; magic không khớp ⇒ `None`. Core **không** đọc/ghi byte user
(D2-83 giữ nguyên).

### 15.2 Đường syscall (`v2/kernel/vifast-core/src/syscall.rs`)

- `apply_ipc_header(m, tcb, env)` — **chỗ duy nhất** window vào core: 4 CPTR → `cap_slot` →
  `Machine::tcb_set_recv_window_obj` (API mới dùng `ObjId`; thân kiểm dùng chung với bản
  `SlotRef` của KT-63: CNode còn sống, 4 ô khác nhau; sai ⇒ `InvalidCapability` **1** /
  `InvalidArgument` **5**, không im lặng).
- `build_message(m, tcb, info, MR0..3, header)`:
  - `info.caps() == 0` ⇒ `Message::new` (đường cũ, không đổi);
  - `caps > 0` ⇒ CPTR **word 6..9** của header → `cap_slot` → `cap_at` (rỗng/chết ⇒
    `InvalidCapability`); `Message::with_caps(info, words, caps, 0)`; **không header** ⇒
    `InvalidArgument` **5**;
  - `length > 4` vẫn `InvalidArgument` (MR4..MR63 qua IPC buffer vẫn là nợ).
- Nhánh **gửi** (0 Endpoint/1/2/5/6/7) dùng `build_with_header`; nhánh **nhận** (3/4) áp window
  **trước khi giao** (message có cap cần window — KT-63 kiểm ở `prepare_caps` lúc `recv`, không
  phải lúc `send`); Notification không áp window.
- Nhánh **invocation** (Call trên cap không phải Endpoint) **không** đổi: `MessageInfo.caps > 0`
  vẫn `NotSupported` **18** + đếm (D2-99(g)) — chuyển cap đi đường Endpoint, không đi invocation.

### 15.3 Không nhân đôi chủ sở hữu (D2-83)

Core vẫn **không** sở hữu `Frame`/`PageTable`/`VSpace`: nó chỉ thấy `CapId` của object **đại
diện** (foreign) do `mem_bridge` dựng và chuyển nguyên qua KT-63. Cap cài vào window là **cap
con** của object đại diện (bỏ `RIGHT_GRANT`); PA/PTE vẫn chỉ object layer biết — TAP đọc PA qua
`mem_bridge::resolve` rồi `read_phys_u64`, không đi qua core.

### 15.4 Test host mới (`cargo test -p vifast-core`)

`syscall.rs` thêm 6 test (đếm ở log `JOURNAL/kt67b-core-tests.log`):

1. `apply_ipc_header` ghi đúng window 4 CPTR vào TCB (so `tcb_recv_window`);
2. window có CPTR ngoài bảng CSpace ⇒ `InvalidCapability`;
3. window trùng ô ⇒ `InvalidArgument`;
4. `build_message` `caps=1` + header → `Message` có đúng 1 cap nguồn (so `cap_ids`);
5. `caps>0` **không** header ⇒ `InvalidArgument`;
6. CPTR nguồn trỏ ô rỗng ⇒ `InvalidCapability`.

### 15.5 Không hồi quy

TAP KT-63 (`ktest` 53..55) và toàn bộ ảnh `ktest` vẫn **65/65**; `ktest-user` lên **19 TAP**
(KT-67a 15..18 + KT-67b 19); `ktest-guard` 3/3; `ktest-proc` 10/10; matrix **12/12** boot
(smp 1/2/4). Gate `--fast` **PASS 661/87**; soak **0/1000** (BUILD_ID `02ed5a1810ba59ce`);
`cargo test -p vifast-core` **32 passed** (7 test mới §15.4); clippy kernel **4 tổ hợp `EXIT=0`**;
hai ngưỡng giữ nguyên (`size_of::<Machine>() = 188.872 B` ≤ 196.608; ảnh user 202.488 ≤ 262.144).
Header chỉ được đọc khi magic khớp nên mọi chương trình cũ (`init`, `procsrv`, `shell`, `child`,
`inv`) **không** đổi hành vi. Số đo chi tiết + red-before + nợ: `v2/agent/JOURNAL/kt67b-journal.md`.

### 15.6 Chạy lại

```bash
./v2/bin/v2 gate --fast                    # L0 + L1 (chạy thuần — xem D2-100 về artifact biên dịch)
cargo test -p vifast-core --manifest-path v2/Cargo.toml     # 6 test mới của §15.4
bash v2/test/boot/ktest.sh 1 matrix        # 4 ảnh × smp 1/2/4 = 12 boot
bash v2/test/boot/soak.sh 1000 3           # 0/1000
```

Log tương ứng: `v2/agent/JOURNAL/kt67b-{gate,ktest-matrix,soak,core-tests,clippy-kernel,
red-before-ktest-user,red-before-matrix1,udisasm-no-priv}.log`; chi tiết phía memsrv/ABI ở
`v2/docs/k6-memsrv.md` §8. Ba ELF U-mode được disassemble (F2, D2-101): chỉ `ecall` + load/store
thường, **0** lệnh đặc quyền/CSR/satp.

## 16. KT-68 — `CNode.*` trên đường invocation + capalloc (cấp/thu hồi theo cây MDB)

### 16.1 Vì sao không cần syscall/kernel API mới

D2-97/D2-99 đã chốt: syscall `0 Call` trên cap **không phải Endpoint** là invocation
(`method = label`, MR0..MR3, trả `a1`). `spec/abi.toml` `[object].CNode` đã cấp đủ số cho
`Copy/Mint/Move/Delete/Revoke`; `Untyped.Retype`/`Frame.Map`/`Frame.Unmap` (KT-67a) đã là đường
invocation đầu tiên. KT-68 **chỉ cài thêm** 5 method đó trên core — không thêm syscall, không
sửa `spec/`, không thêm `unsafe` (shim U-mode chỉ thêm wrapper `nb_recv` quanh `ecall`).

Số method được **sinh** từ spec (`v2/tools/vifast-gen` sinh `CNODE_METHODS`), test
`so_method_cnode_khop_spec` chốt: `Copy=0, Mint=1, Move=2, Mutate=3, Delete=4, Revoke=5,
Rotate=6`. `Mutate`/`Rotate` **chưa cài** ⇒ giữ `NotSupported` (18) + có đếm; `CNode.Insert`
(D2-98) không có số ⇒ không tồn tại.

### 16.2 Ngữ nghĩa (một nguồn: `vifast-core/src/{invoke,machine}.rs`; oracle `vifast-model`)

- Cap được gọi là cap `CNode` trong CSpace **caller**, phải có `RIGHT_WRITE` (model F3):
  thiếu ⇒ `InsufficientRights` (3).
- `src` = CPTR trong CSpace **caller**; `dst` = slot trong **CNode của cap được gọi**
  (`check_dst_in_cnode` của model). Nhờ vậy một server giữ cap CNode của client có thể cấp cap
  **vào CSpace của client** — đúng mô hình capalloc.
- `Machine::move_cap` port D2-44 từ sim: `dst` có cap ⇒ `delete(dst)` trước (con reparent lên
  ông), `src == dst` ⇒ `IllegalOperation` (17), cap giữ `CapId`/cha/quyền/badge.
- Lỗi: 3/5/7/8/17/18/19 theo bảng chung; `caps > 0` trên invocation ⇒ 18 (D2-99(g), có đếm).
- **Arity CHÍNH XÁC** (D2-104): `Copy` 2|3, `Mint` 2|3|4, `Move` 2, `Delete` 1, `Revoke` 0|1;
  `length` thiếu **hoặc thừa** ⇒ `InvalidArgument` (5) + đếm, kiểm **trước** khi đổi CPTR.
- Bộ đếm `invoke::stats()` thêm `copy/mint/moved/delete/revoke` (TAP `# capa:` đọc).

### 16.3 capalloc U-mode (`capa_server`/`capa_client`, ảnh `ktest-user`)

Server nhận **cap CNode của client** qua chuyển cap (KT-67b), quét slot rảnh bằng
`Copy` (lỗi 7 = đã dùng), cấp Frame của mình vào slot đầu tiên trống; thu hồi bằng
`Revoke(slot)` + `Delete(slot)`; cấp lại **cùng slot**. Client dùng cap thật (`Frame.Map` →
ghi/đọc → `Frame.Unmap`), tự dựng cây 2 tầng trong CSpace của mình, và kiểm CPTR cũ sau
`Revoke` cap nguồn (xoá **xuyên CNode**). Vì hai chương trình chạy **tuần tự** trên một hart,
kernel làm scheduler round-robin qua `NBRecv` + `Yield` (không chặn cứng — `Exit` không
`fault_ep` làm rơi message đang xếp hàng, D2-25, nên server `Yield` trước khi `Exit`).

Chi tiết thuật toán/bất biến/TAP/nợ: **`v2/docs/k6-capalloc.md`**.

### 16.4 TAP + test host

| Nơi | Nội dung |
|---|---|
| `ktest-user` TAP 20..24 | cap vòng (2 tiến trình thật) · Mint/Copy/Move · Revoke 2 tầng + Delete reparent + xuyên CNode · ca âm mã lỗi · bất biến `slot_rfree`/`live_cap` |
| `# capa:` evidence | 9 dòng do **kernel** in (cap/PA/PTE/physmap/slot rảnh/`cap_transfer`), runner đòi đủ (fail-closed) |
| host | `cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-core --lib`: **9 test** capalloc (Copy/Mint/Move/Delete/Revoke/ca âm/cap chết/capalloc chu kỳ) — đã đỏ-trước |

### 16.5 Hai ngưỡng + hồi quy

`size_of::<Machine>()` **không đổi** (không thêm field: `move_cap` là hàm, method id là hằng)
⇒ vẫn 188.872 B ≤ 196.608 B (D2-84/85); ảnh test vẫn dưới trần 262.144 B (runner kiểm từng ảnh).
Matrix **12/12** (4 ảnh × smp 1/2/4), conformance 61/61 dòng / 5/5 ca, gate `--fast` PASS,
soak 0/1000, L1 1.000 seed chạy thuần — số đo + log ở `v2/agent/JOURNAL/kt68-journal.md`.

### 16.6 Chạy lại

```bash
cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-core --lib   # 42 test (9 mới)
bash v2/test/boot/ktest.sh 1 matrix                                    # 12 boot
bash v2/test/conformance/run.sh --boot --smp 2                         # simulation + boot + trace
./v2/bin/v2 gate --fast                                                # cổng
```

## 17. KT-69 — IDL tối thiểu + ring buffer chia sẻ (K6)

Nguồn chi tiết: [`k6-idl.md`](k6-idl.md). Tóm tắt phần **kernel/ABI**:

- **Không mở `spec/`**: `spec/abi.toml` giữ nguyên (D2-99); IDL của interface dịch vụ nằm ở
  `v2/lib/vifast-idl/idl/ring.vidl`, method id sinh tuần tự từ IDL (ghi rõ nguồn số).
- **Không thêm syscall/không đổi `MessageInfo` 35 bit**: `attach` mang cap Frame qua **send caps
  header** (KT-67b) + receive window; frame được map bằng invocation `Frame.Map` (11.0 — số lấy
  từ hằng sinh `vifast_abi::FRAME_MAP`, D2-102c).
- **Ring**: `vifast-ring` thuần `no_std`/không `unsafe`; header 4 KiB, 8 slot × 32 B; `send`/
  `recv` một slot, **Release/Acquire** qua `head`/`tail`; đầy ⇒ **9** (`NotEnoughMemory`), rỗng
  ⇒ **14** (`Busy`), không chặn/không ghi đè; wrap bằng bộ đếm một chiều.
- **Hai tiến trình U-mode thật** (`ring_server.elf`/`ring_client.elf`, 2 CNode/2 TCB/2 VSpace):
  server lấp đầy ring + probe mã 9; client thử rỗng 3 lần (mã 14), tiêu thụ 40 message/5 vòng,
  kiểm thứ tự/nội dung; 40 message, `prod_sum = cons_sum = 820`.
- **Dọn tài nguyên giữa pha (KT-69)**: `LoadedInit::release` (module `init_pages.rs`) gỡ mapping
  + thu hồi **Frame/PageTable/VSpace** sau khi đo xong TAP của từng pha (object layer chỉ 64 KiB
  và `MAX_OBJECTS = 96`); cap tạm trong cspace gốc (13..28) được `delete` sau mỗi pha. **Chỉ
  dọn sau khi đo**, không đổi điều kiện TAP nào của KT-67a/67b/68.
- **TAP 25..29** trên `ktest-user` (29 TAP): IDL handshake · đầy/rỗng đúng mã · thứ tự + wrap ·
  race có kiểm soát + mapping hai VSpace cùng PA + bất biến. Evidence `# idl:`/`# ring:` do
  kernel in từ biến đo (D2-102); runner đòi đủ (fail-closed).
- **Ngưỡng**: `size_of::<Machine>()` **188.872 B** không đổi ≤ 196.608 B; `ktest-user`
  `.data+.bss` **202.792 B** ≤ 262.144 B; matrix **12/12**; soak **0/1000**; clippy 4/4
  `EXIT=0` — log ở `v2/agent/JOURNAL/kt69-*.log`.
