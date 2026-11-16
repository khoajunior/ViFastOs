# K6 — memsrv + allocator Frame (`vifast-alloc`), cầu nối D2-83 (KT-67)

Trạng thái task: **KT-67** (K6, D2-95). Phần **allocator + cầu nối + TAP** xong và xanh trên
QEMU; phần **memsrv U-mode** = **BLOCKED** (lý do đo được ở §6, có `file:line` + 2 lựa chọn mở).

Số đo trong tài liệu này **không** viết tay: mọi con số lấy từ log boot do chính kernel in
(`# mem: …`, D2-80) — xem bảng log ở `v2/agent/JOURNAL/kt67-journal.md` §3.

## 1. Phạm vi (đúng D2-83, không nới biên giới)

| Tầng | Sở hữu | Trong KT-67 |
|---|---|---|
| `vifast-core` | `Untyped`/`CNode`/`MDB`/cap/TCB/IPC/sched | **giữ cap** cho Frame + object **đại diện** (`ObjType::Frame`, không byte arena) |
| `vifast-plat-common::obj` | `Frame`/`PageTable`/`VSpace` (KT-52) | **sở hữu** Frame: PA nhãn, căn cỡ, W^X, mapping, byte |
| `vifast-alloc` (mới) | số học **cửa sổ PA** của Untyped | allocator Frame **duy nhất** (4 KiB/2 MiB/1 GiB, free-list + watermark) |
| `vifast-plat-riscv64::mem_bridge` (mới) | cầu nối một chiều | `grant`/`release`/`resolve`/`budget` — **kernel-mediated** |
| `memsrv` U-mode (KT-67a) | **policy façade** | KT-67 **chưa cài**; chọn (A) theo **D2-96** — xem §6 |

Không có syscall mới, không sửa `spec/`, không procsrv/memsrv trong kernel, không nới ngưỡng.

## 2. Một nguồn sự thật byte (vì sao `vifast-alloc` là bản duy nhất)

KT-52 cài cấp/thu hồi PA **bên trong** `vifast-plat-common::obj::arena` (trường `pa_used`,
`pa_holes`, `n_pa_holes`). KT-67 **rút** phần đó ra crate `vifast-alloc` (`no_std`,
`forbid(unsafe_code)`, không dependency) rồi object layer **giữ một** `FrameArena`:

- `Untyped` **không còn** `pa_used`/`pa_holes`; `Untyped::pa_alloc`/`pa_free` uỷ quyền
  `FrameArena::alloc`/`free` (ánh xạ lỗi **một bảng**: `InvalidSize→InvalidArgument`,
  `NotEnoughMemory`, `Alignment`, còn lại `Corrupt`).
- `check_objmem` **không** tự soát hole PA nữa: nó **liệt kê** `(off, len)` của mọi Frame đang
  sống từ **bảng object** rồi gọi `FrameArena::check_accounting(&spans)`; allocator kiểm span
  trong `[0, used)`, rời nhau, không chồng hole, và `Σ span + Σ hole == used`. **Không
  tautology**: danh sách Frame do caller liệt kê (allocator **không** giữ bản sao), còn `used` là
  watermark do `alloc` đẩy; ba điều kiện (rời nhau + trong `[0,used)` + tổng bằng `used`) **chứng
  minh phủ kín** watermark — khai thiếu/khai chồng bị bắt (`Corrupt`), có test host
  `kiem_ke_toan_bat_duoc_khai_gian` + ca âm trong TAP.
- **Ba nguồn độc lập trong bằng chứng** (không "kiểm chéo hai lần cùng một hàm"):
  `live` = tổng cỡ Frame mà **TAP tự biết** nó vừa cấp; `stats.pa_used` = **Σ span Frame liệt kê
  từ bảng object** (`untyped_stats`: `pa_live`, không đọc `FrameArena`); `used` = **watermark**
  của `FrameArena`. TAP 60 đòi **cả ba** bằng nhau, và `check_objmem` kiểm
  `Σ span + Σ hole == used` **từ danh sách** (khai thiếu/khai chồng ⇒ `Corrupt`).
- `UNTYPED_PA_WINDOW`/`MAX_HOLES` **không** khai lại: lấy `vifast_alloc::WINDOW_BYTES`/`MAX_HOLES`.
- Hằng số cỡ Frame (12/21/30) vẫn là bộ của `vifast-pte`; `vifast-alloc` khai `FRAME_SIZE_BITS`
  cùng bộ và test `ba_co_khop_hang_so_vao_so` khoá nó.

Ngữ nghĩa **giữ nguyên** KT-52/K2: first-fit hole (có kiểm căn **tuyệt đối**), rồi bump; **không**
đệm căn lề ngầm (K2 KT-36 `Alignment`), cấp **giảm dần** khi trộn cỡ; free = hole + gộp + rewind
watermark; lỗi **nguyên tử**, không panic. Vì vậy `docs/k3-riscv64.md` §15 (mô hình byte Frame)
vẫn đúng — chỉ chỗ **cài** đổi.

## 3. Cầu nối D2-83 (một chiều, **kernel-mediated**)

> **Tên cho đúng (D2-96)**: trong KT-67 đường cấp Frame là **kernel-mediated** — kernel làm bước
> quyết định + thi hành. `memsrv` U-mode ở task này chỉ là **policy façade** (chưa cài, xem §6);
> **không** được mô tả là "memsrv cấp Frame". KT-67a (D2 mới) sẽ thêm invocation hẹp
> `Untyped.Retype` để memsrv giữ cap `Untyped` trực tiếp và tự retype (đường seL4), rồi bỏ
> kernel-mediated.

1. **Cấp**: `mem_bridge::grant(client_cap, source_untyped, size_bits)`
   a. kiểm **cap client gửi** trong core: sống, `ObjType::Untyped`, có `RIGHT_WRITE` ⇒ sai loại
      `WrongObjectType{2}`, thiếu quyền `InsufficientRights{3}`, cap chết `InvalidCapability{1}`
      (mã ABI từ `spec/abi.toml`; **nguyên tử**: chưa cấp byte nào);
   b. object layer `retype(source, Frame, size_bits)` — tầng duy nhất tạo Frame;
   c. core `add_foreign_object(Frame, key)` + `root_cap` — object **đại diện**: `untyped: None`,
      `region: None`, `len: 0` ⇒ **không** tiêu byte arena và **không** vào `check_bytes`
      (D2-83: core không sở hữu byte object layer). Lỗi core ⇒ object layer **rollback**
      (`revoke`) ngay, không Frame mồ côi;
   d. `key = (object_id << 16) | cap_index` — danh tính bên object layer. `object_id` do
      `next_id: u32` cấp tuần tự và **có thể quay vòng sau 2^32** cấp; vì vậy `resolve` so **cả**
      `object_id` lẫn chỉ số cap, và khi lệch thì fail-closed (`None`) chứ không trả PA sai. Rollover
      sau 2^32 cap là giới hạn mô hình (chưa có thế hệ/GIL), ghi nợ.
2. **Tra ngược** (một chiều): `mem_bridge::resolve(core_obj) → (obj_cap, pa, size_bits)`;
   `Machine::foreign_key` chỉ đọc `key`, **không** đọc byte/PTE của Frame.
3. **Trả**: `mem_bridge::release(grant)` — kiểm `frame_counts` (còn mapping ⇒ `HasChildren`),
   `kill_foreign_object` (từ chối khi còn cap sống, giữ bất biến `check_caps`), rồi
   `obj::revoke` trả PA/byte. Cap phải được `delete` trước (caller/TAP làm).

Chuyển cap Frame cho client đi **đúng cơ chế KT-63**: `Send` mang cap → `plan_cap_transfer` theo
receive window của TCB nhận → cap cài **bỏ `RIGHT_GRANT`** (`MessageInfo.caps=1`). TAP
`mem bridge cap` chạy đường này trên **2 TCB + Endpoint + receive window thật** của core, rồi
`resolve` ra đúng `pa/bits` và `release` trả byte về 0.

## 4. TAP trên QEMU (`# mem:` evidence)

Ảnh `vifast-kbin-ktest` (feature `ktest`), TAP **60..64** (trap lồng fatal dời sang **65**):

| TAP | Đo được |
|---|---|
| `ok 60 - mem alloc frame` | cấp 2 MiB + 4 KiB từ U0 (cấp giảm dần), PA nhãn căn đúng cửa sổ, **ghi/đọc thật** 8 B qua physmap tại `pa`; `FrameBudget`(allocator) `used` == `UntypedStats.pa_used`(object layer) == `2101248`, `live4=1 live2=1 live1g=0`, `check_objmem` sạch |
| `ok 61 - mem revoke frame` | `Revoke` Frame ⇒ `used=0`, `holes=0`, `frames=0`, `free=capacity`; cấp lại 4 KiB rơi **đúng PA cũ** (`same=1`); `stats=0`, `check_objmem` sạch |
| `ok 62 - mem out of memory` | cửa sổ 1 GiB (U1): Frame 1 GiB thứ hai và 4 KiB kế ⇒ `NotEnoughMemory` **mã 9**, watermark **không đổi** (`atomic=1`), boot tiếp tục (`panic=0`); trả Frame ⇒ `used=0` |
| `ok 63 - mem bridge cap` | cầu nối D2-83 + KT-63: cap Frame của object layer được đại diện trong core, chuyển vào receive window của TCB nhận với `rights=0xb` (bỏ GRANT), `resolve` ra đúng `(obj cap, pa, bits)`, `check_caps`/`check_ipc` xanh; `release` ⇒ `used_after=0` |
| `ok 64 - mem deny rights` | từ chối đúng mã: cap client sai loại `2`, thiếu `RIGHT_WRITE` `3`, cap không tồn tại `1`; `atomic=1` (0 byte, số lần cấp không đổi) |

Định dạng từng dòng `# mem:` nằm trong `v2/test/boot/ktest.sh` (`evidence_for ktest`, mục
`# mem:`); runner **đòi** đúng các dòng đó (fail-closed). Tên TAP ở `manifest_main`, số TAP ở
`v2/test/boot/expected.txt` (`ktest 65 65`).

Test host của crate: `cargo test -p vifast-alloc` (10 ca) — cấp/trả 3 cỡ, căn lề không tiêu ngầm,
hết chỗ, hole first-fit, trả sai, cửa sổ 1 GiB, rewind, reset, bất biến `live + hole == used`.

## 5. Giới hạn của mô hình (nợ, không giấu)

- Frame 1 GiB chỉ **nhãn** (D2-66: `-m 128M`); 4 KiB/2 MiB mới ghi/đọc thật.
- `FrameArena` **không** giữ danh sách Frame sống (không bản sao thứ hai): `free` phát hiện
  "chưa từng cấp" qua watermark + đếm theo cỡ; double-free cùng cỡ vẫn cần object layer
  (`maps==0`, bảng object) làm chốt — như KT-52.
- `check_accounting` nhận `live_bytes` từ chủ sở hữu; đó là **một chiều** (không lưu bản sao),
  đúng tinh thần "một nguồn sự thật byte".

## 6. BLOCKED — memsrv U-mode (chưa có trong boot này)

Đã làm: bước **quyết định + thi hành** (`mem_bridge::grant`) + **cơ chế chuyển cap** (KT-63) +
TAP trên 2 TCB. **Chưa** làm: chương trình `memsrv` chạy U-mode có vòng `NBRecv` riêng trên
Endpoint của nó, vì **ABI hiện tại không đủ** (không thêm syscall/spec theo ràng buộc task).

> **Đừng đọc quá xa**: "Frame cấp qua IPC" ở KT-67 = **cap Frame của object layer được kernel
> dựng và chuyển vào receive window** của TCB nhận đúng cơ chế KT-63, rồi tra ngược ra PA —
> **chưa** phải "client U-mode đã map/đọc Frame": ABI chưa có invocation `VSpace.Map` (bảng
> dispatch chỉ `0..15`) nên chương trình U-mode **chưa** dùng được Frame nhận trong cspace.
> Đó là nợ của K6 (pager/map) và KT-67a; TAP chỉ chứng minh **cap + PA + kế toán byte**, không
> chứng minh map. Cùng lý do, `memsrv` U-mode hiện là **policy façade** (chưa cài) — không phải
> "memsrv cấp Frame".

1. **U-mode không gửi được cap trong message**: shim ABI chỉ có `ecall` 7 thanh ghi
   (`v2/kernel/vifast-plat-user/lib.rs:28` `syscall7`) — `NBSend` nhận `label + 4 MR`, **không**
   có khe cap. `Message.caps` chỉ được kernel dựng
   (`v2/kernel/vifast-core/src/core_capxfer.rs`, `resolve_caps`/`prepare_caps`). Muốn memsrv
   **tự** trả cap Frame cho client thì phải mở rộng ABI `NBSend`/`NBRecv` (sửa `spec/` ⇒ D2 mới).
2. **U-mode không có invocation tạo `Frame`**: bảng dispatch chỉ syscall `0..15`
   (`v2/kernel/vifast-core/src/syscall.rs:279` `dispatch`), và `Retype` trong core **trả
   `NotSupported`** cho loại VM (`v2/kernel/vifast-core/src/machine.rs:828`) — biên D2-83.
   Frame chỉ tạo được ở object layer (`v2/kernel/vifast-plat-common/src/obj/mod.rs:714`
   `Machine::retype`), tức trong kernel.

**ĐÃ CHỌN (A) — D2-96** (lead, 2026-11-16): KT-67 = allocator thật + **memory bridge
kernel-mediated**; `memsrv` U-mode = **policy façade**; **KT-67a** mở invocation hẹp
(`Untyped.Retype`, cần D2 mới vì mở `spec/[method_signature]`) để memsrv giữ cap `Untyped` trực
tiếp và tự retype, sau đó bỏ kernel-mediated. Lý do không mở ABI ngay trong KT-67: mở ABI là đổi
nguồn sự thật, cần D2 + rủi ro + ngân sách riêng. Hai lựa chọn đã cân nhắc:

- **(A) Giữ ABI, memsrv = policy-only (chọn)**: memsrv `NBRecv` request, quyết định theo
  `label/MR` (cỡ, số lượng, hạn mức), gửi `NBSend` "grant request" cho kernel; kernel
  (`mem_bridge::grant` + `prepare_caps`) cấp Frame và cài cap vào receive window của client.
  Không sửa `spec/`; nhược điểm: memsrv **không** tự đọc được quyền của cap client (kernel kiểm
  hộ) ⇒ quyền vẫn là nguồn sự thật của kernel.
- **(B) Mở ABI một lần (D2 mới)**: thêm khe cap cho `NBSend`/`NBRecv` (hoặc invocation
  `Untyped.Retype` cho Frame trong core, phá D2-83) ⇒ memsrv tự cấp/trả cap. Cần sửa `spec/` +
  sim + parity; đắt và đụng biên giới đã chốt.

Chốt: **(A)** trong KT-67 (D2-96); phần invocation để memsrv tự retype là **KT-67a** (D2 mới),
không nhét vào task này.

## 7. KT-67a — invocation hẹp: memsrv **thật** (tự retype) + client **map/đọc/ghi thật**

**D2-97 sửa D2-96(b)/(c)**: invocation **đã** nằm trong ABI (`spec/abi.toml` `[syscall].0`:
*“Endpoint: IPC; object khác: invocation (method = label), trả ngay”*), nên KT-67a **không** mở
`spec/`, không thêm syscall, không đổi 16 số hiệu, không đổi `MessageInfo` 35 bit. Việc của task
là **cài đặt** đường invocation.

### 7.1 Đã cài (danh sách trắng hẹp, tăng dần)

| Object (id `[object]`) | method | Nghĩa | Trạng thái |
|---|---|---|---|
| `Untyped` (1) | 0 (`Retype`) | memsrv tự cấp `Frame`/`PageTable`/`VSpace` từ cap `Untyped` của mình | **cài** (đợt 1) |
| `Frame` (11) | 0 (`Map`) | map **chính Frame** vào `VSpace` truyền ở `MR0` | **cài** (đợt 2) |
| `Frame` (11) | 1 (`Unmap`) | gỡ mapping tại `MR1` | **cài** (đợt 2) |
| mọi cặp khác | — | `NotSupported` **18** (D2-53 Q11) — **không** `Ok` giả; có đếm (`# inv: deny`) | giữ nguyên |

> **Tên method**: “`VSpace.Map`” của D2-97 **không có số method** trong ABI hiện tại —
> `VSpace` (id 9) chỉ có `GetAsid = 0`, method id 1 **để trống theo D2-32**; `PageTable`
> (id 10) có `Map = 0` nhưng root PageTable **không có cap** trong object layer. Vì vậy phép
> map đi qua đúng tên/số method mà spec cấp cho thao tác này: **`Frame.Map` = 11.0** (map Frame
> vào VSpace ở `MR0`). Không thêm method id mới.

Kênh tham số (core `v2/kernel/vifast-core/src/invoke.rs`) — **quy ước D2-99** (lead, 2026-11-16):
`MR0..MR3` + trả `a1`; **KT-67b đã cập nhật `spec/` đúng một lần** theo D2-99 (`Untyped.Retype`
thêm `dst` + `returns=["slot"]`, thêm `Frame.Map`/`Frame.Unmap`) — chi tiết + diff ở **§8.1**:

| Method | `MR0` | `MR1` | `MR2` | `MR3` |
|---|---|---|---|---|
| `Untyped.Retype` | cptr đích (slot trong CSpace caller) | `type` id (`[object]` 9/10/11) | `size_bits` | `flags` (0; `GUEST` reserved ⇒ `NotSupported` với VSpace) |
| `Frame.Map` | cptr cap `VSpace` | `va` | cờ `R=1,W=2,X=4,U=8,Device=16` | — |
| `Frame.Unmap` | cptr cap `VSpace` | `va` | — | — |

Kiểm quyền/bất biến trên đường invocation (reviewer-kt67a M1/M2):

- `Frame.Map` **và** `Frame.Unmap` đòi cap `VSpace` ở `MR0` có `RIGHT_WRITE` (cap chỉ READ ⇒
  `InsufficientRights` **3**);
- `Frame.Unmap` **ràng buộc Frame ↔ mapping**: leaf tại `va` phải thuộc **đúng** cap Frame được
  gọi (so PA — PA Frame là duy nhất); VA đang map Frame khác/không map ⇒ `InvalidCapability` **1**
  hoặc `NotMapped` **11**, mapping **không** bị gỡ (test âm trong TAP 17);
- `MessageInfo.caps > 0` trên **invocation** ⇒ `NotSupported` **18** + đếm (D2-99(g)) — **giữ
  nguyên** sau KT-67b: chuyển cap đi đường **Endpoint** bằng header IPC buffer (§8), không đi
  đường invocation; method/loại ngoài danh sách cũng **18** + đếm.

Biên giới D2-83 giữ nguyên: core **không** đọc/ghi PTE/byte; nó kiểm cap/quyền/loại, giải mã
method rồi gọi hook `SyscallEnv::invoke` — arch cài bằng object layer (`mem_bridge::invoke` →
`ObjectLayer::retype/map/unmap` + `sfence.vma`). Retype xong, object layer trả `key`; core dựng
**object đại diện** + cap gốc rồi `put_cap` vào slot đích của caller (nguyên tử; lỗi bảng ⇒
`release_object` trả object về object layer).

### 7.2 memsrv: bỏ đường kernel-mediated

Chương trình U-mode `init/inv_user.S` (ELF riêng `inv.elf`, nạp vào ô tác vụ 1) giữ **cap
`Untyped` được cấp lúc spawn** (slot 13 — bản `copy` của cap Untyped boot, có `RIGHT_WRITE`) và
tự `Untyped.Retype` ⇒ `Frame` 4 KiB vào slot 15. **Không** gọi `mem_bridge::grant` (đường
kernel-mediated của KT-67); `grant`/`release`/`resolve` vẫn giữ trong crate như **đường phụ**
(TAP 63/64 cũ dùng), không phải đường cấp Frame của pha này.

Bridge chỉ còn **một bảng một chiều** `core ObjId → cap Untyped của object layer`
(`mem_bridge::register_untyped`, tối đa 4 entry) để hook biết cap nào sở hữu byte — byte/PA vẫn
do object layer quyết (D2-83), core không kế toán PA.

### 7.3 Client U-mode: map/ghi/đọc/unmap/fault thật

Cùng chương trình (một cspace) làm **client**: `Frame.Map` map Frame vừa retype vào VSpace của
chính nó tại `0x4100_0000` (cap VSpace ở slot 14, do kernel cài lúc spawn), rồi:

1. `sd`/`ld` 8 B qua PTE vừa cài — giá trị `0x1122334455667788`;
2. `Frame.Unmap` gỡ mapping;
3. đọc lại đúng VA ⇒ **load page fault** `scause=0xd`, `stval=0x4100_0000` — kernel phân loại
   `FaultKind::Vm{access=Read, reason=NotMapped}` (lượt chạy dùng `enter_expect_fault`, test-only);
4. hai invocation ngoài danh sách trắng ⇒ `NotSupported` 18.

Bằng chứng do **kernel đo**, không tin lời chương trình: `mem_bridge::resolve(core obj)` → PA
thật của Frame, kernel đọc 8 B **vật lý** tại PA (đúng giá trị chương trình ghi),
`ObjectLayer::walk` sau `Unmap` trả invalid, `FaultKind` lấy từ trap phần cứng.

### 7.4 TAP + bằng chứng (`# inv:`, KT-67a)

TAP 15..18 của ảnh `ktest-user` (**ở thời điểm KT-67a**: 18 TAP, `expected.txt` khi đó
`ktest-user 18 18`; từ KT-67b ảnh có **19 TAP** — xem §8.5); dòng log lấy từ
boot thật (`v2/agent/JOURNAL/kt67a-journal.md` §3, D2-80):

```text
# inv: prep untyped_slot=13 vspace_slot=14 frame_slot=15 frame_b_slot=18 vspace_ro_slot=19 tcb=16 va=0x41000000 registered=1 vspace_key=0x13000e frame_b_pa=0x82015000 root=0x804fb000
# inv: retype method=0 ty=Frame size=12 err=0 slot=15 stats=1/3
# inv: map method=0 va=0x41000000 pa=0x82015000 err=0 stats=1 phys=0x1122334455667788
# inv: unmap method=1 va=0x41000000 err=0 stats=1 walk_after=0 objmem=1 caps=1
# inv: deny err=18 stats=3 allowed=3 last_err=18 syscalls=22 entries=2
# inv: deny2 ran=1 frame_a_maps=0 frame_b_maps=0 ro_vspace=1 caps_msg=1 walk_after=0
# inv: fault va=0x41000000 kind=Vm{NotMapped,Read} ok=1 snap_state=Faulted
ok 15 - invocation untyped retype
ok 16 - invocation frame map
ok 17 - invocation frame unmap va fault
ok 18 - invocation ngoai danh sach
```

`stats=…/…` là **delta** của riêng pha (mốc trước pha đã có 1 lần `NotSupported` từ TAP 8
kernel-level: `Call` trên cap TCB) — TAP không phụ thuộc thứ tự pha.

Red-before (log trong `v2/agent/JOURNAL/`):
`kt67a-red-no-invocation.log` (bỏ nhánh invocation ⇒ cả 4 TAP đỏ, trả 18),
`kt67a-red-no-map.log` (chỉ có Retype ⇒ TAP 15 xanh, 16/17/18 đỏ),
`kt67a-red-retype-unregistered.log` + `kt67a-red-satp-kernel-fault.log` (đường memsrv thật hỏng:
bridge chưa đăng ký ⇒ `err=1`; và kernel fault khi chạm object layer lúc `satp` = root user).

### 7.5 Còn BLOCKED (file:line) — mục 1 **đã đóng** ở §8 (KT-67b)

1. ~~**Chuyển cap Frame từ memsrv U-mode sang một client TCB khác**~~ — **ĐÓNG Ở §8** (KT-67b,
   TAP 19). Giữ lại đoạn dưới làm mốc lịch sử: **không phải thiếu ABI** (D2-98): receive window **đã** nằm trong ABI — `spec/abi.toml`
   `[calling.riscv64].ipc_buffer = "Frame bind qua Tcb.Configure; header chứa receive window
   (CPTR, ≤4 cap)"` (dòng 58) — và core đã có `tcb_set_recv_window`/`plan_cap_transfer`/
   `deliver_caps` (`v2/kernel/vifast-core/src/core_ipc.rs`, `core_capxfer.rs`). Phần **thiếu là
   shim U-mode**: `syscall7` (`v2/kernel/vifast-plat-user/lib.rs:28`) không ghi header IPC buffer
   (khai `caps` + CPTR nguồn); chỗ **từ chối** `MessageInfo.caps > 0` hiện là nhánh invocation
   (`v2/kernel/vifast-core/src/invoke.rs`, trả 18) — `build_message`
   (`v2/kernel/vifast-core/src/syscall.rs`) **không** kiểm `caps`. Vì vậy pha này **memsrv + client là
   một chương trình/cspace**; **KT-67b (D2-98)** cài header IPC buffer + receive window trong
   shim để tách 2 tiến trình — **không** cần sửa `spec/`.
2. **`CNode.Insert`** (D2-96/97 nhắc) **không có method id** trong bảng `CNode` của
   `spec/abi.toml` (`Copy=0, Mint=1, Move=2, Mutate=3, Delete=4, Revoke=5, Rotate=6`; `spec/`
   đóng băng) ⇒ chưa cài, `NotSupported`. Core **đã có** API `Machine::copy/mint/delete/revoke`
   nên task sau chỉ cần gắn method id có sẵn (0/1/2/4/5) — không thêm id mới.

## 8. KT-67b — header IPC buffer U-mode + chuyển cap Frame giữa **hai tiến trình** (đóng §7.5)

**Trạng thái: DONE** — ảnh `ktest-user` lên **19 TAP** (`ok 19 - ipc capx hai tien trinh`), hai
**ELF riêng** `memsrv.elf`/`client.elf`, hai VSpace, hai TCB, **hai CNode/cspace riêng**. Không
thêm syscall, không đổi 16 số hiệu, `MessageInfo` vẫn **35 bit**, `unsafe` vẫn chỉ
`kernel/vifast-plat-*` (shim + arch), không nới ngưỡng nào.

### 8.1 `spec/` — mở **đúng một lần** theo D2-99 (và chỉ 3 mục)

`git diff spec/abi.toml` = **một tệp**, **`+15 −3`**, đúng **3 entry** trong `[method_signature]`
(kèm **3 khối comment** của chính 3 entry đó — `Retype` thay 1 dòng cũ bằng 2, `Map`/`Unmap` mỗi
entry 1 dòng — vì comment cũ nói “`returns=[]`, chưa có nguồn” đã **mâu thuẫn** với D2-99):

| Entry | Trước | Sau |
|---|---|---|
| `Untyped.Retype` | `params=["cap","type","size_bits","flags"]`, `returns=[]`, comment “Giá trị trả `cap` chưa có nguồn thiết kế … (D2-35)” | `params=["cap","dst","type","size_bits","flags"]`, `returns=["slot"]`, comment D2-99: `dst`=MR0, `type`=MR1, `size_bits`=MR2, `flags`=MR3, `a1`=slot |
| `Frame.Map` (**mới**) | — | `params=["frame","vspace","va","flags"]`, `returns=[]`, comment `# D2-99(d): 11.0; MR0=cap VSpace, MR1=va, MR2=flags` + 1 dòng `note` (metadata như mọi entry khác — **trong** phạm vi lần mở D2-99 này) |
| `Frame.Unmap` (**mới**) | — | `params=["frame","vspace","va"]`, `returns=[]`, comment `# D2-99(d): 11.1; leaf tại va phải thuộc đúng cap Frame (f)` |

**Không** đổi syscall/error/object/`[message]`/`[calling]`/`[debug_info]` — `Frame`/`PageTable`/
`VSpace` trong `[object]` vẫn `stability="reserved"`, `unsupported=true` (D2-98 giữ nguyên phạm
vi). Artifact sinh lại bằng `./v2/bin/vifast-gen gen`: `v2/lib/vifast-abi/src/generated.rs`
(`METHOD_SIGNATURES` 13 → **15**), `docs/abi.md`; `./v2/bin/vifast-gen --check` **PASS**.

Máy kiểm ABI (`v2/test/abi/check_abi.py`) trước đây **pin cứng** `len(methods) == 13` (đặt từ
KT-05). KT-67b thay pin bằng **suy dẫn**: `expected` = các khoá `[method_signature."X.Y"]` đọc từ
spec lúc parse, so **tập** với entry có `params`, và cross-check **số phần tử**
`METHOD_SIGNATURES` trong `generated.rs` (nguồn thứ hai cùng pipeline). Thêm chữ ký sau này
không phải sửa pin; `gen-check` vẫn là chốt **byte**.

### 8.2 Header IPC buffer (shim U-mode — `v2/kernel/vifast-plat-user/lib.rs`)

`spec/abi.toml` dòng 58 đã có câu *“header chứa receive window (CPTR, ≤4 cap)”*; KT-67b cài
**nơi đặt** header + API đọc/ghi (model `[TK]`, không phải ABI mới):

| Word | Nội dung |
|---|---|
| 0 | magic `b"KT67b"` little-endian (`IPC_HEADER_MAGIC`) — 0 = chưa dùng header ⇒ kernel bỏ qua |
| 1 | `MessageInfo` 35 bit (bản ghi của shim; **nguồn ABI vẫn là `a2`**) |
| 2..5 | **receive window** — 4 CPTR đích khi nhận cap (spec dòng 58) |
| 6..9 | **send caps** — 4 CPTR nguồn cap của message đang gửi |

Trang 4 KiB tại VA **0x4002_0000** (`USER_IPC_BUFFER_VA`) do **loader kernel** map `R|W|U|NX` +
zero cho **mọi** ELF U-mode (`user::load_init_slot`, cùng bảng cấp 1 với ảnh/stack nên **không**
thêm bảng trang); dải này được đăng ký vào `active_ranges` để đường đọc của kernel không fault
S-mode. Shim cấp: `init_ipc_buffer`, `set_recv_window([CPTR;4])`, `recv_window`,
`set_send_caps(&[CPTR])`, `send_caps`, `info(label,caps,no_reply,length)` (+ `info_label/caps/
length`), `note_received(info)`, `received_info`, `read_received_caps() -> ReceivedCaps{count,
slots}`, `call/nb_send/recv`, `peek_u64/poke_u64`, `yield_now`. Chương trình Rust vì vậy **không**
cần `unsafe`/`asm!` (R5) mà vẫn ghi/đọc thật vùng đã map.

### 8.2b `unsafe` của shim — đúng D2-101 (không lệnh đặc quyền)

Shim U-mode **được** dùng `unsafe` cho đúng hai việc: `ecall` (chuyển syscall) và
`read_volatile`/`write_volatile` trên **địa chỉ user** (IPC buffer của chính nó, VA do chương
trình truyền) — shim chạy **U-mode** với page table user nên **không** với tới kernel. **Cấm**
`csrr/csrw/sfence/sret/mret/satp/wfi`, CSR, truy cập vật lý/physmap (D2-101 thay D2-93).
Bằng chứng máy: `llvm-objdump -d` cả `memsrv.elf`/`client.elf`/`inv.elf` — hợp mnemonic chỉ
`addi auipc beqz bgeu bltu bne bnez ecall j jalr ld li lui mv pause ret sd slli srli`, **0** dòng
lệnh đặc quyền/CSR/satp (`JOURNAL/kt67b-udisasm-no-priv.log`). Chương trình Rust vì vậy không có
`unsafe`/`asm!` trong mã nguồn (R5) — chỉ shim (`kernel/vifast-plat-*`, danh sách đóng D2-17).

### 8.3 Kernel: window + cap **lấy từ header**, không đặt ngoài băng

1. `SyscallEnv::ipc_header(tcb)` (mặc định `None`) — arch `UserMemAccess` đọc 10 word ở VA cố
   định bằng `sstatus.SUM` **chỉ khi** trang đã đăng ký + magic khớp; **không** đọc byte nào từ
   core (D2-83). Header zero (mọi ảnh cũ) ⇒ `None` ⇒ hành vi KT-67a giữ nguyên 100 %.
2. `syscall::apply_ipc_header` (chỗ **duy nhất** window vào core): 4 CPTR → `cap_slot` → 
   `Machine::tcb_set_recv_window_obj` (API mới, cùng thân kiểm với bản `SlotRef` của KT-63:
   CNode sống, 4 ô **khác nhau**; sai ⇒ lỗi).
3. `syscall::build_message(m, tcb, info, MR0..3, header)`:
   - `info.caps() == 0` ⇒ đường cũ (`Message::new`) không đổi;
   - `caps > 0` ⇒ lấy CPTR nguồn ở **word 6..9 của header**, `cap_slot` → `cap_at` (rỗng/chết ⇒
     `InvalidCapability` **1**), `Message::with_caps`; **không có header** ⇒ `InvalidArgument` **5**.
4. Giao message vẫn **nguyên** KT-63: `resolve_caps` → `prepare_caps` theo **window của bên nhận**
   → `deliver_caps` cài cap con **bỏ `RIGHT_GRANT`** vào window và cập nhật `MessageInfo.caps` =
   số cap **thật sự** cài. `MessageInfo.caps > 0` trên **invocation** vẫn `NotSupported` 18
   (D2-99(g)) — không đổi.

### 8.4 Hai tiến trình thật (rig `src/ktest_capx.rs`)

| | memsrv (TCB A, `memsrv.elf`) | client (TCB B, `client.elf`) |
|---|---|---|
| CNode/cspace | riêng (16 slot) | riêng (16 slot) |
| slot 0 | cap CNode tự thân (cspace của A) | cap CNode tự thân (cspace của B) |
| slot 1 | `Endpoint` E | `Endpoint` E (**copy** cùng obj, không phải obj khác) |
| slot 2 | cap `VSpace` **của A** | cap `VSpace` **của B** |
| slot 3 | cap `Untyped` (tự Retype) | — |
| slot 5 / 6 | `Frame` (retype) / `Frame` **thiếu `GRANT`** (rig cấp) | — |
| window | [8,9,10,11] (header) | **[4,5,6,7]** (header) |

Luồng: (1) A `Untyped.Retype` Frame vào slot 5 (invocation 1.0); (2) ca âm `caps=1` + CPTR nguồn
rỗng ⇒ **1**; (3) A `NBSend` message `caps=1` mang Frame (cap có `GRANT`); (4) A `NBSend` message
mang Frame **thiếu `GRANT`** ⇒ chuyển **mềm** 0 cap; (5) A `NBSend` message `caps=0`; (6) A
**`Yield`** — *không* `Exit`, vì `Exit` **huỷ** message đang chờ trong hàng Endpoint (D2-25) và
TCB chết không còn là “hai tiến trình đang nói chuyện”; (7) B `Recv` → window header của **B**
được áp → cap cài vào **cspace B** slot 4; B `Frame.Map` vào VSpace **của B** tại `0x4100_0000`,
`poke/peek` 8 B, `Frame.Unmap` bằng chính cap nhận, map lại + ghi giá trị 2, `Recv` 2 message đối
chứng (0 cap), `Exit(0)`.

**Policy `copy`**: cap nhận là **cap con** bỏ `RIGHT_GRANT`; memsrv **vẫn** giữ cap nguồn
(`src_alive=1`). “Chuyển hết” (move) không thuộc ABI/KT-63 — muốn move phải có `CNode.Delete`
(method 4, **ngoài** danh sách trắng invocation) hoặc `CNode.Revoke`; ghi nợ §8.8, **không** giả
vờ đã move.

### 8.5 Bằng chứng đo (`# capx:` — log boot thật, D2-80)

```text
U: memsrv chuyen cap Frame
U: client nhan cap va map/ghi/doc
# capx: pre cspace_b4=empty cspace_b5=empty walk=0 a5=Frame src_alive=1 policy=copy
# capx: recv slot=4 ty=Frame rights=0xb obj=29 from_obj=29 from_rights=0xf installed=1 failed=1
# capx: map va=0x41000000 pa=0x82030000 bits=12 walk=1 walk_pa=0x82030000 phys=0x8877665544332211 caps=1 ipc=1 obj=1 src_alive=1
ok 19 - ipc capx hai tien trinh
```

Đọc từng nhóm: `pre` = **trước** `Recv` của B, cspace B **rỗng** ở slot 4/5 và VSpace B **chưa**
map VA; `recv` = sau `Recv`, cap ở **cspace B** slot 4, loại `Frame`, `rights=0xb` = `R|W|G`
(`RIGHTS_ALL & !RIGHT_GRANT`), `obj/from_obj` **cùng một** object Frame, `installed=1` (đúng **1**
cap cài — `cap_transfer_installed` delta) và `failed=1` (message thiếu `GRANT` bị bỏ **mềm**);
`map` = kernel walk VSpace **B** thấy leaf hợp lệ tại VA, `pa` thật `0x82030000`, kernel đọc
**physmap** tại PA = `0x8877665544332211` (giá trị **B** ghi qua mapping), `caps/ipc/obj` = ba bất
biến core (`check_caps`/`check_ipc`/`check_objmem`) xanh, `src_alive=1` = policy copy.

### 8.6 Red-before (D2-05)

`JOURNAL/kt67b-red-before-caps-rejected.log` + `kt67b-red-before-ktest-user.log`: tạm tắt
`UserMemAccess::ipc_header` (đúng trạng thái trước KT-67b) ⇒ memsrv `NBSend caps=1` nhận
`InvalidArgument` **5**, thoát **71**, `not ok 19 - … memsrv khong Yield`; TAP 1..18 vẫn xanh ⇒
đỏ **đúng chỗ** (`caps>0` bị từ chối), không phải đỏ vì lý do khác.

### 8.7 Ngưỡng + hồi quy (số chép từ log, không viết tay)

- **Hai ngưỡng (D2-85a)**: `size_of::<Machine>()` **không đổi 188.872 B** ≤ pin **196.608 B**
  (TAP `# core: link` vẫn `bytes=188872 pin=196608`); ảnh `ktest-user` `.data+.bss` =
  **202.488 B** ≤ **262.144 B** (trần D2-72 Q5; `ktest` 209.920, `ktest-proc` 206.624 — như
  KT-67a). Hai ELF mới 8.560 B/tệp chỉ vào `.rodata` (+17.120 B file ảnh), **không** vào
  `.data/.bss`.
- **Matrix 12/12** boot (4 ảnh × smp 1/2/4): `ktest 65/65`, `ktest-guard 3/3`,
  **`ktest-user 19/19`**, `ktest-proc 10/10` — `JOURNAL/kt67b-ktest-matrix.log`.
- **Gate `--fast`**: **PASS** — 661 test / 87 suite (bằng mốc KT-67a) + L1 1.000 seed
  (`JOURNAL/kt67b-gate.log`); core test riêng `cargo test -p vifast-core` **32 passed**
  (`kt67b-core-tests.log`); clippy kernel **4 tổ hợp `EXIT=0`** (`kt67b-clippy-kernel.log`).
- **Soak**: `soak.sh 1000 3` → **0/1000**, wall **229 s**, BUILD_ID `02ed5a1810ba59ce`,
  sha256 `435e0f9d…` (ảnh sản phẩm hiện tại **đúng** sha256 đó) — `JOURNAL/kt67b-soak.log`.
- **L1 của cổng — đọc đúng D2-100**: gate sau vá in **L1 wall = 1.183 ms** (cây ấm). Lần gate
  **trước** (cùng mã, máy bận) in **171.115 ms** — đã kiểm là **artifact biên dịch/khoá build
  trong cửa sổ đo**, không phải số chạy: chạy thuần cùng lệnh = **0.80 / 0.82 / 0.85 s** (3 lần;
  tái lập đúng thứ tự cổng L0→L1 cũng 0.85 s; 2 lần sau vá = 1.00/1.01 s). Ghi **cả hai** số;
  **không** dùng 171 s làm số hiệu năng; chạy thuần ≪ 60 s nên AGENTS §6 **không** phải giảm
  seed.

### 8.8 Nợ mở (có chủ, không tô hồng)

1. **Policy move**: chuyển cap hiện là **copy** (KT-63); “memsrv không giữ cap” chỉ đúng nếu có
   policy move ⇒ cần `CNode.Delete`/`Revoke` có method id trong invocation (hiện ngoài danh sách
   trắng). Không tự thêm id (spec đóng băng).
2. **MR4..MR63 qua IPC buffer** vẫn chưa đọc/ghi (chỉ 4 MR thanh ghi) — header mới chỉ dùng cho
   window + cap, không phải vùng MR.
3. **`CNode.Insert`** vẫn `NotSupported` (không có method id, D2-98).
4. **Multi-hart U-mode**: rig chạy tuần tự một hart (như KT-65/66); chưa có preemption U-mode
   thật, nên “hai tiến trình” là hai **TCB/cspace/VSpace** chạy lần lượt dưới điều khiển kernel
   test — không phải SMP scheduling.
