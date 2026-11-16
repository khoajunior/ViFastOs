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
