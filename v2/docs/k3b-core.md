<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# K3b tầng 1 — port core `no_std`: Untyped + CNode + MDB + capability (KT-58)

- **Trạng thái:** DONE (KT-58). Task kế: **KT-59** (TCB/transition/sched + hộp thư),
  **KT-60** (IPC/Reply/Notification/WaitAddr/fault), **KT-61** (nối core vào kernel + cổng K3b).
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
| TCB/transition/scheduler/MCS | không | KT-59 |
| Endpoint/Notification/Reply/IPC/WaitAddr/fault | không | KT-60; object vẫn có `ObjType` + cap/MDB/byte |
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
| `tests/kt58_core_parity.rs` (trong sim) | 2680 | parity host: 14 kịch bản + differential 32×48 + 2 test core-only |

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

## 7. Chạy lại

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

## 8. Đề xuất cho KT-59/60/61

- **KT-59:** thêm body `TcbData`/`SchedData` vào `Meta` (tĩnh, có biên), port `transition_tcb`
  + bảng `ALLOWED` + ready queue; giữ nguyên API cap/MDB đã có.
- **KT-60:** `EndpointState`/`NotificationState`/`ReplyData` + `Queue` cố định; port
  `ipc::plan_cap_transfer` (4 cap, mất GRANT) — tái dùng `SlotPool`.
- **KT-61:** nối `vifast-core` vào `vifast-plat-common`/kbin; thay `SlotPool` bằng slot trong
  arena `PhysMem`; VM (`Frame`/`PageTable`/`VSpace`) đi qua object layer KT-52 (D2-83 — core
  vẫn `NotSupported`, test pin giữ); đo lại `.data+.bss` và cập nhật trần 262.144 B **trước**
  khi thêm bảng mới; khi đó TAP/QEMU mới có CNode/MDB thật (K3b).
