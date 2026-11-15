<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3b — port core `no_std`: tầng 1 (KT-58) + TCB/scheduler (KT-59) + IPC/Reply/Notification/WaitAddr/fault (KT-60)

- **Trạng thái:** KT-58 **DONE**; **KT-59 DONE**; **KT-60 DONE** (chờ review độc lập + Agent 0
  lật QUEUE). Task kế: **KT-61** (nối core vào kernel/syscall + cổng K3b).
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
| Nối syscall / `init` / ảnh kernel | không | KT-61 |

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
| `src/ipc.rs` | 320 | **KT-60**: `MessageInfo` 35 bit, `Message` 64 MR, `IpcArea` (4+60), `transfer` |
| `src/objstate.rs` | 265 | **KT-60**: body Endpoint/Reply/Notification, `IpcQueue` FIFO 8, pool `QueuedMsg`, nút WaitAddr/`AddrWord` |
| `src/core_ipc.rs` | 1325 | **KT-60**: `Call/Send/Recv/NBRecv/Reply/ReplyMsg/ReplyRecv/NBSendRecv/NBSendWait`, `Exit`/`inject_fault`, `unlink_tcb_queues`, donation KT-21 |
| `src/core_sync.rs` | 519 | **KT-60**: Notification (signal/wait/poll/bind) + WaitAddr (wait/wake/giá trị) |
| `src/check_ipc.rs` | 395 | **KT-60**: `check_ipc`/`check_reply_links`/`check_notifications`/`check_waitaddr`/`check_fault` |
| `src/footprint.rs` | 124 | **KT-60**: bảng phân bố 27 dòng + pin D2-84 (tách khỏi `machine.rs` để giữ R1) |
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
2. **Slot CNode trong `.bss`, chưa nằm trong arena vật lý.** Byte slot vẫn được Untyped kế
   toán đúng spec (`cnode_base + slots*48`), nhưng `CapId` lưu ở `SlotPool`; nợ KT-61: đưa
   slot vào arena/`PhysMem` khi nối kernel (giống nợ metadata của KT-52). Pool có free-list để
   `Revoke`/finalise trả word.
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
   `JOURNAL/kt60-parity.log`) — chưa link vào ảnh kernel nên **ngân sách `.data+.bss`
   262.144 B của ảnh không đổi**; KT-61 phải đo lại ảnh link và cắt theo §8.5 nếu cần.
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
10. **`TlbFlush` trong hộp thư là placeholder.** Core chưa sở hữu TLB (object layer K2); message
    giữ đúng hình dạng (`seq/ack/generation/asid/va`), handler no-op — KT-61 nối TLB thật mà
    không đổi enum.
11. **Nhật ký hộp thư có biên** `MAX_MAIL_LOG = 32`/hart (sim `Vec` không biên; D2-85b giảm 64→32). Quá biên:
    message vẫn xử lý đúng một lần, bất biến hàng đợi giữ nguyên, chỉ **nhật ký** dừng ghi và
    `mail_log_overflow` tăng. Không phải mất message.
12. **`IpcArea` + `transfer` đã có (KT-60)** (4 MR thanh ghi + 60 word buffer + info/badge/
    msg_id); chưa ghi từ syscall (KT-61). **Object** TCB 512 B theo spec, **body tĩnh** trong
    core 704 B (`IpcArea` 544 B + trường scheduler/state/IPC).
13. **Dấu chân tĩnh (KT-60 + D2-85b):** `size_of::<Machine>() = 180.168 B` `[ĐO]` (bảng 27
    dòng, `JOURNAL/kt60-parity.log`; 91,6 % pin 196.608 B, còn **16.440 B**). **D2-84 pin core ≤
    196.608 B (192 KiB)** = trần ảnh 262.144 B trừ 64 KiB headroom; vượt phải **tối ưu**,
    không nâng pin. Core **chưa link** nên `.data+.bss` ảnh không đổi; KT-61 đo lại ảnh link và
    áp phương án cắt §8.5 nếu `size_of` vượt pin (hoặc ảnh vượt trần).
14. **Không có `alloc`/`Vec`/`String`/`unsafe` mới:** ready queue là danh sách liên kết tĩnh
    `[u16; MAX_OBJECTS+1]`, body TCB/SC/IPC là bảng `.bss` có free-list (revoke trả body), hộp
    thư và hàng Endpoint là mảng FIFO cố định, message đang chờ nằm trong pool chung
    `[QueuedMsg; MAX_QUEUED_MESSAGES]` (hàng Endpoint giữ **chỉ số** `u16`).
15. **Chưa chuyển cap qua IPC (KT-60):** `Message` giữ hình dạng `caps` của sim nhưng
    `MessageInfo.caps > 0` bị từ chối `InvalidArgument`; receive window 4 ô +
    `plan_cap_transfer` (mất `RIGHT_GRANT`, unwrap badge, thất bại mềm) là nợ KT-61 — §8.6.
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
(KT-61 — `TlbFlush` placeholder); nối syscall/kernel (KT-61); Frame/PageTable/VSpace (D2-83).

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
thật** trước khi thêm bảng; core hiện **chưa link** nên ngân sách `.data+.bss` của ảnh không
đổi.

`Machine::footprint()` trả bảng phân bố byte tĩnh (D2-84); test pin in bảng + assert
`sum(bytes) + scalars + padding == size_of::<Machine>()`. Bảng **cập nhật KT-60** (log
`JOURNAL/kt60-parity.log`, 2026-11-15; `[ĐO]`) — KT-59 chỉ có 14 dòng / 167.896 B; KT-60 thêm
body Endpoint/Reply/Notification, pool message, bảng WaitOnAddress và `IpcArea` trong TCB:

| Thành phần | Byte | Ghi chú |
|---|---:|---|
| `regions[16]` | 384 | `Region` 3×`usize`/`u64` |
| `objects[256]` | 22.528 | `Meta` 88 B |
| `caps[512]` | 28.672 | `Cap` 56 B |
| `slot_pool` | 16.656 | `MAX_SLOT_WORDS=4096` + hole |
| `tcbs[64]` | 45.056 | `TcbData` 704 B (IpcArea 544 B + scheduler/state) |
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
| `ipc_msgs[24]` + `ipc_msg_free[24]` | 13.632 + 48 | pool `QueuedMsg` 568 B (KT-60) |
| `addr_words[64]` | 1.536 | `(frame, offset) -> u64` (KT-60) |
| `wait_nodes[64]` + `wait_free[64]` | 1.024 + 128 | nút waiter (KT-60) |
| `wait_buckets[64]` + `wait_buckets_tail[64]` | 128 + 128 | 64 bucket (KT-60) |
| scalars | 239 | 20 `usize` + 9 `u64` + `u16` + `u32` + `bool` |
| padding | 1 | `repr(Rust)` |
| **Tổng** | **180.168** | **91,6 % pin 196.608 B; còn 16.440 B** (D2-85b; phân tích + điều kiện KT-61 ở §8.5) |

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

- `size_of::<Machine>() = 180.168 B` `[ĐO]` — **91,6 % pin 196.608 B**, còn **16.440 B**
  (bảng 27 dòng §7.3). Đã áp **D2-85b**: `MAX_MAIL_LOG` 64 → 32 (−12.288 B) **trước KT-61**;
  pin test KT-59 cập nhật 32.
- **D2-85a — hai ngưỡng khác nhau, cấm đọc lẫn:** pin **196.608 B** áp cho
  `size_of::<Machine>()`; trần **262.144 B** áp cho **ảnh link** (`.data+.bss` thật). KT-61
  phải báo **cả hai** số; một số xanh không suy ra số kia.
- Ảnh kernel hiện tại (KT-59) static **11.248 B** (đo KT-57); KT-61 đo lại sau link. Nếu
  `Machine` vượt **196.608 B** hoặc ảnh vượt **262.144 B** ⇒ **cắt**, không nâng ngưỡng.
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
6. **`tcb_resume_with_regs`** chưa port (như KT-59 ghi); **Reply cap trong `Endpoint.recv`**
   (cap Reply chết ⇒ coi như `Recv` không Reply) và **WaitAddr bucket/FIFO toàn phần** (mọi
   bucket, mọi thứ tự) chưa có ca parity riêng — nợ phủ, không phải nợ hành vi đã biết.
7. **F1/F2/F3 reviewer-kt60 đã vá + có ca:** suspend donee trả SC + đánh thức client; donor
   không đổi priority/SC; `block_current(BlockedOnNotification)` ghi/kiểm waiter + Busy.

## 9. Chạy lại

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# parity host (nằm trong gate --fast vì là test của workspace v2)
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt58_core_parity -- --nocapture
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt59_core_parity -- --nocapture
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt60_core_parity -- --nocapture
# core tự build cho bare-metal + clippy riêng (không nằm trong gate host)
cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-core --target riscv64gc-unknown-none-elf
cargo clippy --manifest-path v2/kernel/Cargo.toml -p vifast-core --all-targets -- -D warnings
# cổng đầy đủ
./v2/bin/v2 gate --fast
```

## 10. Đề xuất cho KT-61

- **Nối:** `vifast-core` vào `vifast-plat-common`/kbin; thay `SlotPool` bằng slot trong arena
  `PhysMem`; VM (`Frame`/`PageTable`/`VSpace`) đi qua object layer KT-52 (D2-83 — core vẫn
  `NotSupported`, test pin giữ); TAP/QEMU mới có CNode/MDB/IPC thật.
- **Trước khi link:** đo `size_of::<Machine>()` + bảng phân bố + `.data+.bss` ảnh; áp §8.5 nếu
  cần (ưu tiên A `MAX_MAIL_LOG 64→32`), **không** nâng pin. Nếu link core làm ảnh vượt
  262.144 B thì cắt tính năng tĩnh theo D2-84.
- **Hoàn tất nợ KT-60:** chuyển cap (receive window/`plan_cap_transfer` + 4 cap/mất GRANT/
  unwrap badge), finalise IPC object khi Revoke, `TlbFlush` thật; giữ parity sim↔core cho các
  phần mới (D2-05, thêm ca trước).
