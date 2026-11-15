<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3b — port core `no_std`: tầng 1 (KT-58) + TCB/transition/scheduler/hộp thư (KT-59)

- **Trạng thái:** KT-58 **DONE**; **KT-59 DONE** (chờ review độc lập + Agent 0 lật QUEUE).
  Task kế: **KT-60** (IPC/Reply/Notification/WaitAddr/fault), **KT-61** (nối core vào kernel +
  cổng K3b).
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
| Endpoint/Notification/Reply/IPC/WaitAddr/fault | không | KT-60; object vẫn có `ObjType` + cap/MDB/byte; TCB chỉ giữ `ipc_buffer` 60 word + 4 MR (chưa có đường transfer) |
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
| `src/mailbox.rs` | 433 | `Mailbox<T, CAP=8>` FIFO + backpressure, `MailboxMsg`, `HartMailbox` (log biên `MAX_MAIL_LOG=64` + `stopped`) |
| `src/core_tcb.rs` | 726 | khối `impl Machine`: TCB/transition/MCS (tách khỏi `machine.rs` để giữ R1) |
| `src/core_sched.rs` | 866 | khối `impl Machine`: scheduler/hộp thư/`check_sched`/`check_sc_refs`/`check_budget`/`check_mailboxes` |
| `tests/kt58_core_parity.rs` (trong sim) | 2680 | parity host KT-58: 14 kịch bản + differential 32×48 + 2 test core-only |
| `tests/kt59_core_parity.rs` (trong sim) | 2164 | parity host KT-59: 5 kịch bản KT-18/19/20/22/29 + differential 16×32 + 2 test core-only |

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
3. **Chưa có body TCB/Endpoint/...** Object chỉ có sổ cái `Meta` (kiểu/alive/UnTyped/offset/
   len/CNode) — đủ cho CNode/MDB/Untyped; trạng thái TCB/scheduler/IPC là KT-59/60. Vì vậy
   hiệu ứng phụ của sim như `finalise_tcb`, `cancel_reply`, `purge_dead_refs` (TCB/Endpoint/
   Notification) **chưa** port; chúng không đổi cap/MDB/byte ở tầng 1.
4. **`Retype` reserved — biên giới thiết kế (D2-83):** core tầng 1 **không** Retype
   `Frame`/`PageTable`/`VSpace` (và mọi reserved) — trả `NotSupported`, **không phải thiếu
   sót**: object layer `vifast-plat-common::obj` (KT-52) sở hữu VM. Test pin
   `core_d2_83_vm_reserved_not_supported` giữ biên giới này; parity sim↔core **loại trừ**
   Retype VM theo D2-83. KT-61 nối invocation VM qua obj layer (cầu nối), **không** nhân đôi
   chủ sở hữu; đổi biên giới cần D2 mới.
5. **`purge_dead_refs` rút gọn:** chỉ dọn slot ↔ cap (đã có trong `clear_slot_of`); chưa có
   trường object-body nào trỏ cap/object.
6. **Dấu chân tĩnh:** `size_of::<Machine>() = 68.264 B` `[ĐO]` (host, log parity) — chưa link
   vào ảnh kernel nên **ngân sách `.data+.bss` 262.144 B của ảnh không đổi** (KT-58 không nối
   core); KT-61 phải đo lại và có thể giảm `MAX_*`/tách `SchedContext`.
7. **Vùng Untyped là số học + nhãn PA (D2-83):** `Region.base` là **nhãn PA** (Untyped con =
   `base_cha + offset`), chưa cấp/đọc byte thật (chưa có Frame/PageTable trong core).
   `untyped_arena` vì vậy **khác** sim (sim trả địa chỉ block heap thật); không được mô tả
   "y hệt sim", và digest parity **không** so `base` — chỉ so `capacity`/`used`/`free`/
   `allocated`/`gap`.

8. **Hàng chờ IPC chưa tồn tại (KT-60).** `unlink_tcb_queues` của core là **mốc** (không có
   hàng Endpoint/Reply/Notification/WaitAddr để gỡ); `transition_tcb` chưa có hook
   `Reply.caller` (KT-34) và chưa có waiter Notification (KT-27). KT-60 nối **tại đúng cửa**
   này.
9. **Donation KT-21 chỉ có khung (KT-60).** `TcbData::donated_sc`, `ScData::{donated_to,donor}`
   và `mcs::donated_priority` đã có; chưa có `Call`/`Reply` để tạo/thu hồi donation. Hệ quả:
   `check_sc_refs` kiểm đối xứng nhưng mọi trường donation luôn `None` trong KT-59.
10. **`TlbFlush` trong hộp thư là placeholder.** Core chưa sở hữu TLB (object layer K2); message
    giữ đúng hình dạng (`seq/ack/generation/asid/va`), handler no-op — KT-61 nối TLB thật mà
    không đổi enum.
11. **Nhật ký hộp thư có biên** `MAX_MAIL_LOG = 64`/hart (sim `Vec` không biên). Quá biên:
    message vẫn xử lý đúng một lần, bất biến hàng đợi giữ nguyên, chỉ **nhật ký** dừng ghi và
    `mail_log_overflow` tăng. Không phải mất message.
12. **`ipc_buffer`/`regs` chỉ là chỗ giữ + accessor** (60 + 4 word) — chưa có `transfer`
    (KT-60), chưa ghi từ syscall; **object** TCB 512 B theo spec, **body tĩnh** trong core 616 B
    (60 word IPC buffer + 4 MR + trường scheduler/state).
13. **Dấu chân tĩnh tăng:** `size_of::<Machine>() = 167.896 B` `[ĐO]` (host, bảng phân bố
    trong `JOURNAL/kt59-parity.log`; 64,0 % trần ảnh 262.144 B) do thêm 64 body TCB + 32
    SchedContext + 8 `HartSched` + 8 hộp thư + log finalise biên 256. **D2-84 pin core ≤ 196.608 B (192 KiB)** =
    trần ảnh trừ 64 KiB headroom; vượt phải **tối ưu**, không nâng pin. Core **chưa link** nên
    ngân sách `.data+.bss` của ảnh **không đổi**; KT-61 phải đo lại ảnh link trước khi thêm
    bảng và có thể giảm `MAX_*`/tách `SchedContext`.
14. **Không có `alloc`/`Vec`/`String`/`unsafe` mới:** ready queue là danh sách liên kết tĩnh
    `[u16; MAX_OBJECTS+1]`, body TCB/SC là bảng `.bss` có free-list (revoke trả body), mailbox
    là `[Option<T>; 8]`.

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
| `MAX_MAIL_LOG` | **64**/hart | message vẫn xử lý đúng một lần; nhật ký dừng ghi + `mail_log_overflow` tăng (biên triển khai, nợ §6.11) |
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
`sum(bytes) + scalars + padding == size_of::<Machine>()`. Số đo host hiện tại (log
`JOURNAL/kt59-parity.log`, 2026-11-15; `[ĐO]`):

| Thành phần | Byte | Ghi chú |
|---|---:|---|
| `regions[16]` | 384 | `Region` 3×`usize`/`u64` |
| `objects[256]` | 22.528 | `Meta` 88 B (body ref + CNode/arena metadata) |
| `caps[512]` | 28.672 | `Cap` 56 B |
| `slot_pool` | 16.656 | `MAX_SLOT_WORDS=4096` + hole |
| `tcbs[64]` | 39.424 | `TcbData` 616 B (60 word IPC buffer + 4 MR + trường scheduler) |
| `tcb_free[64]` | 128 | free-list body |
| `scs[32]` | 1.792 | `ScData` 56 B |
| `sc_free[32]` | 64 | free-list body |
| `pair_hits[121]` + `forbidden_hits[121]` | 484 + 484 | bộ đếm bảng ALLOWED |
| `finalise_log[256]` | 12.288 | `FinaliseEntry` 48 B (biên F1 = `MAX_OBJECTS`) |
| `sched[8]` | 16.832 | `HartSched` 2.104 B (256 head/tail/len + bitmap + next) |
| `mail[8]` | 28.032 | `HartMailbox` 3.504 B (hàng 8 + nhật ký 64 message) |
| `published[8]` | 16 | `Published` 2 B |
| scalars | 111 | 9 `usize` + 4 `u64` + `u16` + `u32` + `bool` |
| padding | 1 | `repr(Rust)`, đo khớp |
| **Tổng** | **167.896** | **64,0 %** trần 262.144 B; còn **28.712 B** headroom tới pin 192 KiB |

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

### 7.5 Nối KT-60/61 (không làm ở task này)

- **KT-60** thêm body Endpoint/Notification/Reply + IPC: nối `unlink_tcb_queues` (hiện no-op),
  hook `Reply.caller`/waiter Notification **tại `transition_tcb`** (một cửa), donation tự động
  qua `Call`/`Reply` dùng `donated_sc`/`donation_of`/`effective_priority` đã có, `ipc_buffer`
  + `regs` ghi qua `transfer` (digest KT-59 đã so **nội dung** hai mảng này).
- **Hai hook sim vắng mặt ở KT-59 (nợ KT-60, đúng mã):**
  1. rời `BlockedOnReply` bằng đường khác `Reply` ⇒ `release_donation_of` (KT-21) — hiện chưa có
     donation nên vắng; KT-60 phải gọi tại `transition_tcb`;
  2. xung đột `Reply.caller` (KT-34): vào `BlockedOnReply` khi Reply đã bind TCB khác ⇒
     **`IllegalOperation`** (không ghi đè); KT-60 nối cùng chỗ.
- **Nội dung hàng chờ hộp thư — giới hạn oracle (không phải đã so đủ):** sim **không có
  accessor công khai** cho message đang treo (`Machine.mail` private; chỉ `mail_len`/
  `mail_log`/`hart_stopped`). Vì vậy digest KT-59 **chưa so trực tiếp** head/tail/nội dung
  message đang treo; nó so `len` + **toàn bộ log** + `stopped` + thứ tự xử lý. Đây là **giới
  hạn của oracle sim**, **không phải** core bỏ trường (core đã có đủ trạng thái). KT-60 phải
  thêm accessor (hoặc kiểm qua endpoint/IPC) trong **task được sửa sim** rồi mới so head/tail;
  không được đọc §7.3/§4.1 như "đã so đủ nội dung hàng chờ".
- **Parity digest KT-59 đã gồm:** state+payload, `ReadState`, `suspended`/`exit_code`/`fault_ep`/
  `cspace`/priority/MCP/affinity/home/`sched_context`/`donated_sc`/`bound_notification`,
  `ipc_buffer[60]`/`regs[4]`, budget/refcount, ready queue 256 mức + `pick`/`current`/
  `published`, mailbox len/log/stopped, finalise log, pair hits. Không bỏ trường nào của mô
  hình KT-59.
- **KT-61** đo lại `size_of::<Machine>()` + `.data+.bss`, quyết định giảm `MAX_TCBS`/
  `MAX_MAIL_LOG`/tách `SchedContext` trước khi link; thay `SlotPool` bằng slot trong arena
  `PhysMem`; `TlbFlush` nối TLB thật; VM đi qua object layer (D2-83).

## 8. Chạy lại

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# parity host (nằm trong gate --fast vì là test của workspace v2)
cargo test --manifest-path v2/Cargo.toml -p vifast-sim --test kt58_core_parity -- --nocapture
# core tự build cho bare-metal + clippy riêng (không nằm trong gate host)
cargo build --manifest-path v2/kernel/Cargo.toml -p vifast-core --target riscv64gc-unknown-none-elf
cargo clippy --manifest-path v2/kernel/Cargo.toml -p vifast-core --all-targets -- -D warnings
# cổng đầy đủ
./v2/bin/v2 gate --fast
```

## 9. Đề xuất cho KT-60/61

- **KT-59:** thêm body `TcbData`/`SchedData` vào `Meta` (tĩnh, có biên), port `transition_tcb`
  + bảng `ALLOWED` + ready queue; giữ nguyên API cap/MDB đã có.
- **KT-60:** `EndpointState`/`NotificationState`/`ReplyData` + `Queue` cố định; port
  `ipc::plan_cap_transfer` (4 cap, mất GRANT) — tái dùng `SlotPool`.
- **KT-61:** nối `vifast-core` vào `vifast-plat-common`/kbin; thay `SlotPool` bằng slot trong
  arena `PhysMem`; VM (`Frame`/`PageTable`/`VSpace`) đi qua object layer KT-52 (D2-83 — core
  vẫn `NotSupported`, test pin giữ); đo lại `.data+.bss` và cập nhật trần 262.144 B **trước**
  khi thêm bảng mới; khi đó TAP/QEMU mới có CNode/MDB thật (K3b).
