# K6 — capalloc: cấp/thu hồi capability theo cây MDB (KT-68)

Trạng thái: **KT-68 DONE** (K6, D2-95). Sản phẩm: invocation `CNode.Copy/Mint/Move/Delete/Revoke`
trên core thật + chương trình **capalloc U-mode** (`capa_server` / `capa_client`) + TAP 20..24
trên QEMU + test host. Không thêm syscall, **không sửa `spec/`**, không nới ngưỡng.

Số đo trong tài liệu này **không viết tay**: mọi con số lấy từ log boot do kernel in
(`# capa: …`, D2-80) hoặc từ log cổng — bảng log ở `v2/agent/JOURNAL/kt68-journal.md`.

## 1. Phạm vi

| Tầng | Sở hữu | Trong KT-68 |
|---|---|---|
| `vifast-core` | `Untyped`/`CNode`/`MDB`/cap/quyền | **cài** 5 method `CNode.*` đường invocation; thêm `Machine::move_cap` (D2-44) — không đổi ngữ nghĩa sim |
| `vifast-plat-user` (shim U-mode) | `ecall` + đọc-ghi bộ nhớ user | thêm `nb_recv` (syscall 4) — **không** lệnh đặc quyền (D2-101) |
| `capa_server` U-mode | **policy capalloc** | quét slot rảnh của CNode client, `Copy`/`Revoke`/`Delete`, trả slot; **không** kernel-mediated |
| `capa_client` U-mode | người dùng cap | nhận slot, `Frame.Map/Unmap` thật, tự dựng cây 2 tầng, yêu cầu thu hồi, cấp lại |
| object layer (`vifast-plat-common::obj`) | Frame/VSpace | không đổi — capalloc chỉ đi qua cap (D2-83) |

**Không** có “kernel API capalloc”: từ KT-67a/D2-97 đã có đường invocation trên syscall 0 nên
việc cấp/thu hồi cap làm được **hoàn toàn từ U-mode** (server giữ cap CNode của client, nhận qua
chuyển cap KT-67b). Đây là đường seL4, không phải façade.

## 2. Số method — lấy từ `spec/abi.toml` (không tự đặt)

`spec/abi.toml` `[object].CNode` (frozen):

| id | Tên | KT-68 |
|---|---|---|
| 0 | `Copy` | **cài** |
| 1 | `Mint` | **cài** |
| 2 | `Move` | **cài** |
| 3 | `Mutate` | **chưa cài** ⇒ `NotSupported` (18) |
| 4 | `Delete` | **cài** |
| 5 | `Revoke` | **cài** |
| 6 | `Rotate` | **chưa cài** ⇒ `NotSupported` (18) |

> **Đính chính so với mô tả task**: mô tả viết `Revoke (4), Delete (5)`; **spec** ghi
> `Delete = 4`, `Revoke = 5` (`Mutate = 3` xen giữa). KT-68 làm **đúng spec** — số 4/5 trong task
> là nhầm thứ tự; nếu làm theo mô tả thì `Delete` sẽ chạy `Revoke` (sai ngữ nghĩa). Bảng
> `CNODE_METHODS` được **sinh** từ spec (`v2/tools/vifast-gen`), test host
> `so_method_cnode_khop_spec` chốt lại — không chép tay.
>
> `CNode.Insert` (D2-96/98 nhắc) **không có số** trong spec ⇒ vẫn không tồn tại; capalloc dùng
> `Copy/Mint` thay `Insert` (đúng D2-98).

## 3. Kênh tham số invocation (D2-99: `MR0..MR3`, `length` = số MR dùng)

Cap **được gọi** là cap `CNode` trong CSpace **caller**; nó phải có `RIGHT_WRITE` (model F3,
oracle `vifast-model::invoke_*`). **Nguồn** (`src`) là CPTR trong CSpace **caller**; **đích**
(`dst`) là slot trong **CNode của cap được gọi** — nhờ vậy server cấp cap vào CNode **của client**
bằng cap CNode client mà nó được chuyển.

| Method | MR0 | MR1 | MR2 | MR3 | `a1` trả |
|---|---|---|---|---|---|
| `Copy` | `src` (caller) | `dst` (target) | `rights?` | — | 0 |
| `Mint` | `src` | `dst` | `rights?` | `badge?` | 0 |
| `Move` | `src` | `dst` | — | — | 0 |
| `Delete` | `slot` (target) | — | — | — | 0 |
| `Revoke` | `slot?` (bỏ trống ⇒ mọi slot) | — | — | — | số cap bị xoá |

- `rights?`/`badge?` vắng mặt được mã hoá bằng `length` (≤ 2 ⇒ giữ nguyên nguồn; `badge = 0` có
  mặt ⇒ **xoá** badge — ngữ nghĩa `Machine::mint`).
- `Mint` badge chỉ cho `Endpoint`/`Notification`; object khác ⇒ `InvalidArgument` (5).
- `Move`: đích có cap ⇒ **xoá đích trước** (D2-44), con của cap đích reparent lên ông; `src ==
  dst` ⇒ `IllegalOperation` (17); cap giữ nguyên `CapId`/cha MDB/quyền/badge (không tạo cap mới).
- `Revoke`: xoá **con/cháu** của cap trong slot (cap trong slot được **giữ**); `Revoke` trên cap
  `Untyped` vẫn finalise cả cây Untyped (ngữ nghĩa sim, không đổi).
- Lỗi: thiếu `RIGHT_WRITE` trên cap CNode ⇒ 3; đích bận ⇒ 7; quyền vượt nguồn ⇒ 3; slot ngoài
  bảng ⇒ 19; ô rỗng ⇒ 8; `length` thiếu/thừa ⇒ 5; method chưa cài/object khác ⇒ 18 (có đếm
  `invoke::stats().denied`).

## 4. capalloc — thuật toán cấp phát **duy nhất**

Chương trình `capa_server` U-mode (không `unsafe`, không `asm!`):

1. **Đăng ký**: client gửi `OP_ALLOC` mang **cap CNode của chính nó** (`MessageInfo.caps = 1` +
   CPTR nguồn trong header IPC buffer — KT-67b). Kernel cài cap vào **receive window** của server.
2. **Cấp**: duyệt slot `1..N` của CNode client; mỗi slot thử `CNode.Copy` cap nguồn (Frame của
   server) → `dst`. Lỗi **7 `SlotOccupied`** = đã dùng ⇒ thử slot kế; lỗi khác ⇒ dừng (không im
   lặng). Slot đầu tiên cấp được trả về client ở `MR0`.
3. **Dùng**: client `Frame.Map` cap vừa nhận vào VSpace của mình, ghi/đọc 8 B thật,
   `Frame.Unmap`; tự dựng cây 2 tầng bằng `CNode.Copy` trên **cap CNode của chính nó**
   (`grant → con → cháu`) + một nhánh khác.
4. **Thu hồi**: client yêu cầu `Revoke(slot)` (xoá đúng cây con — 2 cap) rồi `Delete(slot)` (trả
   slot). Cấp lại ⇒ **cùng slot** (không rò slot/cap).
5. **Thu hồi theo cây MDB xuyên CNode**: server `Revoke` **cap nguồn** của mình ⇒ mọi bản sao đã
   cấp (con trực tiếp trong CNode client) bị xoá; CPTR cũ của client ⇒ `SlotEmpty`.

## 5. Bất biến

Sau mỗi thao tác (kiểm trong TAP + test host):

- `check_caps` xanh: cap sống ↔ object sống, cha–con không chu trình, slot ↔ cap đối xứng; **cap
  trỏ object chết** là trạng thái **không hợp lệ** (bất biến 3/4 bắt được — test host
  `cap_chet_tra_invalid_capability_va_bat_bien_do` dựng trạng thái đó qua API nội bộ và chứng
  minh `cap_at` ⇒ 1 + `check_caps` đỏ). Trên QEMU, ca “cap chết” ở mức ABI là **CPTR cũ** sau
  `Delete`/`Revoke` ⇒ `SlotEmpty` (8) — không thể có cap chết **trong slot** nếu bất biến giữ.
- `check_bytes`/`check_ipc`/`check_objmem` xanh.
- **Không rò**: `live_cap_count` và **slot rảnh** (`slot_rfree`) của hai CNode trở về đúng mốc
  trước chu kỳ (TAP 24). `cap_count` (số cap **từng** cấp) tăng đơn điệu — **ID cap không tái
  dùng** (spec `[frozen].id_reuse = "never"`), đây là high-water, không phải rò sống; trần
  `MAX_CAPS` là giới hạn mô hình (ghi nợ §8).
- `Revoke` chỉ xoá **cây con** của cap trong slot: nhánh khác trong cùng CNode và cap trong slot
  không bị đụng (TAP 22 + test host `cnode_revoke_cay_hai_tang`).
- `Delete` **reparent** con của cap bị xoá lên ông gần nhất còn sống (MDB kiểu seL4), object
  **không** bị finalise (TAP 22 + test host `cnode_delete_reparent_mdb`).

## 6. TAP QEMU (ảnh `ktest-user`: 19 → **24** TAP) + `# capa:` evidence

| TAP | Đo được |
|---|---|
| `ok 20 - capalloc cap vong` | hai ELF U-mode riêng (`capa_server.elf` + `capa_client.elf`), hai cspace: client gửi cap CNode của mình → server cấp Frame vào slot rảnh (4 lần thử: 3 × 7 + 1 thành công) → client map/ghi/đọc/unmap → cây con 2 tầng (Revoke trả **2**) → `Delete` trả slot → cấp lại **cùng slot** → map lại, kernel đọc **physmap** đúng giá trị thứ hai → server `Revoke` cap nguồn xoá **xuyên CNode**; `cap_transfer_installed` +1 |
| `ok 21 - capalloc mint copy move` | `Mint` badge `0xabc` + quyền `R\|W`; badge trên Frame ⇒ 5; `Copy` giữ quyền/badge + cha MDB; `Move` xoá nguồn, giữ `CapId`, đích bận bị xoá trước + con reparent lên ông |
| `ok 22 - capalloc revoke cay delete reparent` | cây 2 tầng: `Revoke` trả **2** (con + cháu), cap trong slot và nhánh khác **sống**; `Delete` reparent MDB; `Revoke` cap nguồn xoá bản sao ở CNode client (xuyên CNode) |
| `ok 23 - capalloc ca am` | ô rỗng ⇒ 8; đích bận ⇒ 7; quyền vượt nguồn ⇒ 3; cap CNode chỉ READ ⇒ 3; slot ngoài bảng ⇒ 19; `Mutate`/`Rotate`/object khác ⇒ 18 (có đếm); thiếu/thừa MR ⇒ 5; CPTR cũ sau `Delete` ⇒ 8; cap chết ngoài slot `alive=0` |
| `ok 24 - capalloc bat bien` | `check_caps`/`check_bytes`/`check_ipc`/`check_objmem` = 1; `slot_rfree` **s = 12→12, c = 12→12**; `live_cap_count` trở về mốc; `cap_transfer` **installed +1, failed +0**; `cap_high` tăng (ID không tái dùng) |

Mỗi dòng `# capa:` bắt buộc được liệt kê trong `v2/test/boot/ktest.sh`
(`evidence_for ktest-user`); runner **đỏ** nếu thiếu/lệch (fail-closed). Tên TAP ở
`manifest_user`, số TAP ở `v2/test/boot/expected.txt` (`ktest-user 24 24`).

## 7. Đỏ-trước (D2-05)

- **Host** (`cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-core --lib`): trước khi
  cài, nhánh `CNode.*` trả `NotSupported` ⇒ **9 test capalloc đỏ** (so mã lỗi 18 với kỳ vọng);
  log `JOURNAL/kt68-do-truoc-host.log`.
- **QEMU**: cùng bản tạm đó ⇒ TAP 20..24 **đỏ** (`not ok`), log
  `JOURNAL/kt68-do-truoc-qemu.log`. Sau khi cài: 24/24 xanh (log `kt68-xanh-user-smp1.log`).

## 8. Giới hạn / nợ (không tô hồng)

- `CNode.Mutate` (3) và `CNode.Rotate` (6) **chưa cài** — giữ `NotSupported` (D2-53 Q11), có TAP.
- Không có `CNode.Insert` (spec không cấp số) và **không có `GetFreeSlot`**: capalloc quét slot
  bằng `Copy` + `SlotOccupied` — đúng “từ CNode rảnh”, nhưng là **policy** của server, không
  phải syscall. Với CNode lớn, quét tuyến tính; tối ưu (bitmap) là việc của policy, không đổi ABI.
- `n_caps` (bảng cap core) là **high-water**, không có free-list: ID cap không tái dùng (spec) ⇒
  chu kỳ cấp/thu hồi dài sẽ chạm `MAX_CAPS`; đây là giới hạn mô hình, đã có test host đo “không
  rò **sống**” (`live_cap_count`), không đo “không tăng high-water”.
- Capalloc chạy **một hart** (hart 0) như mọi pha U-mode hiện có; đa hart U-mode là nợ K6 (D2-94).
- `capa_server` là chương trình **test** (giao thức label `0x68x` không phải ABI); khi có IDL
  (KT-69) giao thức này sẽ được thay bằng stub sinh từ spec.
- `Revoke` trên cap nguồn Frame chỉ xoá **cap**, không finalise object (Frame do object layer sở
  hữu — D2-83); mapping đã tạo vẫn còn (client tự `Unmap`; TAP 20 unmap trước khi kiểm).

## 9. Số đo cổng (chép từ log, `JOURNAL/kt68-*`)

- TAP `ktest-user` **24/24**; matrix **12/12** (4 ảnh × smp 1/2/4); conformance **61/61 dòng,
  5/5 ca**; soak **0/1000** (BUILD_ID `02ed5a1810ba59ce`, sha256 `f6318b5d…`);
- gate `--fast` **PASS 661/87**, L1 wall **879 ms** / chạy thuần **0.86–0.79–0.85 s** (D2-100);
  clippy kernel **4/4 `EXIT=0`**; core host **42 passed** (9 test mới).
- Hai ngưỡng **không đổi**: `size_of::<Machine>() = 188.872 B` ≤ 196.608; ảnh test ≤ 262.144
  (runner in budget từng ảnh). Chi tiết + log: `v2/agent/JOURNAL/kt68-journal.md`.

## 10. Chạy lại

```bash
# test host core (gồm 9 test capalloc)
cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-core --lib

# TAP trên QEMU (4 ảnh × smp 1/2/4)
bash v2/test/boot/ktest.sh 1 matrix

# conformance + boot + so trace (12 boot)
bash v2/test/conformance/run.sh --boot --smp 2

# cổng
./v2/bin/v2 gate --fast
```
