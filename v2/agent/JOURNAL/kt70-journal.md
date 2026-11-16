# KT-70 — pager U-mode + page cache `(file,page)` (K6 tầng 1)

Ngày: 2026-11-16. Người làm: Agent 2 dòng OS. Ngân sách 3 h (bắt đầu ~19:32 JST).
Trạng thái: **DONE phần cài + đo**, chờ reviewer. Tài liệu: `v2/docs/k6-vfs-pager.md`,
`v2/docs/k3b-core.md` §18.

## 1. Vào / ra

- Vào: HEAD `02aa96af` (baseline `gate --fast` **PASS 675 test/92 suite**, L1 1.000 seed
  **967 ms** — `JOURNAL/kt70-baseline-gate.log`); KT-69 DONE (IDL + ring). Trong lúc làm, nhánh
  `main` có thêm commit của tab ViScript (`1a275ffa`, `3dc13342`, `b1ee0b33`) — cây OS không
  chạm các đường đó; cổng cuối chạy trên `b1ee0b33`.
- Ra: pager + cache + 2 client U-mode, TAP 30..34 xanh trên QEMU thật; host **12** test mới
  (`vifast-vfs`) + **7** test IDL; không thêm syscall; `spec/` sạch.

## 2. Đỏ-trước (D2-05)

- Host: `cargo test -p vifast-vfs` trên mã cũ **không biên dịch** (`E0432` unresolved import
  `vifast_vfs`, `E0425` không có `PageCache`/`PagerCore`) — `JOURNAL/kt70-red-host.log`.
- QEMU: TAP 30..34 **đỏ cả 5** (`# pager: fault …` nhưng chưa có pager phục vụ; mapping_bad>0)
  — `JOURNAL/kt70-red-qemu-ktest-user.log`.
- Trong quá trình đỏ→xanh còn 3 lần đỏ QEMU trung gian (ghi ở §4): treo 20 s do **khoá core
  không tái nhập**, `StackOverlap` do `.bss` 32 KiB trong ảnh U-mode, và `InvalidArgument` do
  **cạn bảng mapping 64** của object layer.

## 3. Cài đặt

1. **`v2/lib/vifast-vfs`** (host-testable, `no_std`, `#![forbid(unsafe_code)]`):
   `layout` (hình học + mẫu `byte_at`), `disk` (`RamDev` 40 sector qua `AtomicU64` + `DevRef`
   impl `BlockDev`), `cache` (`PageCache<N>` LRU theo `stamp`, `keys_unique`), `core`
   (`PagerCore`: `begin/load`, `Begin::{Hit,Ready,Evict,Reject}`, `PageSink`), `status`
   (`Status` tra id theo tên trong `vifast_abi::ERRORS`), `stats` (`Stat` 22 bộ đếm, magic
   `0x4b54373050414745`, offset `0x80 + i*8`). 12 test host: hình học, seed, ghi/đọc/ngoài
   dải, mẫu byte, hit không đọc backend, LRU, dữ liệu nạp, ngoài file, một bản/khoá, stats.
2. **IDL**: `v2/lib/vifast-idl/idl/pager.vidl` (`interface pager version 1`, `# caps=none`);
   generator `gen_idl.py` sinh thêm `src/generated_pager.rs` (target thứ hai), stub encode/
   decode **đúng arity**, không `unsafe`; test `pager_stub_arity_va_roundtrip` (va 64-bit tròn
   MR0..MR3) + `generated_pager_khong_unsafe`.
3. **Pager U-mode** (`init/pager_server_user.rs`): map stats + 3 pool Frame + 5 Frame đĩa
   (`VA_DISK`) bằng invocation `Frame.Map`; `MappedDisk` impl `BlockDev` đọc/ghi **qua VA đã
   map** (không `.bss` lớn trong ảnh); seed 40 sector bằng chính `BlockDev::write`; vòng
   `nb_recv(P_EP_FAULT)` decode IDL → `begin/load` → `Frame.Map` vào VSpace client → `served`;
   `stop`/`Exit` fault được xử lý; `linger` 4 lượt rồi exit(0).
4. **Hai client U-mode** (`pager_client_common.rs` + `_a`/`_b`): map khối kết quả, mỗi bước
   `yield_now()` rồi validate **cả 4096 byte** so `byte_at(file,page,off)`, ghi bộ đếm
   `ok_pages/ok_bytes/bad_*`, đặt DONE, exit(0). B dùng bảng có trang ngoài file (99).
5. **Kernel rig** (`src/ktest_vfs.rs`): 3 VSpace/TCB/CNode + kernel TCB + 2 Endpoint + 3 pool
   Frame + 5 Frame đĩa + stats + 2 Frame kết quả; `deliver_and_pump` = `inject_fault` (core,
   `FaultKind::Vm`) + message IDL `pager.fault` + chạy pager tới `served`; `step_client` đo
   walk/PTE/PA, hit (reads không đổi), trục xuất (walk invalid), nạp lại (PA đổi), dữ liệu
   kernel đọc độc lập từ physmap, mã lỗi/Exit; TAP 30..34 + evidence `# pager:`.
6. **Harness**: `expected.txt` `ktest-user 34 34`; `ktest.sh` plan `1..34`, manifest 5 TAP mới,
   regex evidence fail-closed; PANIC mẫu `… (34 TAP, 0 fail)`; budget `.data+.bss ≤ 262.144`.

## 4. Bài học đo được (quan trọng cho task sau)

0. **Ca âm là bắt buộc**: `kernel_data_bad=0` chỉ là "không xảy ra trong ca xanh" ⇒ **không
   chứng minh gì**. TAP 35 thêm **hai ca âm thật**: (a) `Frame.Map` với aux = TCB ⇒ đo được
   `WrongObjectType (2)` (không phải `InvalidCapability` như dự đoán ban đầu — **sửa theo số
   đo**, D2-102); (b) `Frame.Map` vào VA đã map trong VSpace client B ⇒ `AlreadyMapped (10)`,
   kernel chứng minh PTE khối kết quả **không đổi** (`pa_same=1`, magic còn nguyên) + dữ liệu B
   vẫn đúng. Không thêm ca "VA kernel 0x8020_0000" vì dải đó chỉ trống trong VSpace user, không
   phải hàng rào kernel thật (bài học: đừng test một tính chất sai).

1. **Khoá core không tái nhập**: `core_bridge::with_machine(|m| deliver_and_pump(…))` rồi bên
   trong gọi `run_task` (mở `with_machine` lần hai) ⇒ **treo 20 s**. Sửa: mọi thao tác core đi
   qua closure ngắn, `run_task` chạy **ngoài** khoá. Đây là lỗi thiết kế test, không phải core.
2. **`last_fault` bị xoá ở syscall kế tiếp** của TCB nhận (`reset_ipc_error`): phải
   `take_last_fault` **ngay sau** bước chạy pager đã tiêu thụ fault entry, không đợi cuối pha
   (lần đầu đo ra `core_fault=0` dù fault đã đi qua core).
3. **`StackOverlap`**: `static RamDev` 28 KiB trong `.bss` ảnh U-mode làm ảnh vượt vùng user
   28 KiB (guard `0x4000_7000`). Sửa: kernel cấp 5 Frame đĩa, pager map tại `VA_DISK` và impl
   `BlockDev` trên VA — cũng chính là bằng chứng "backend thay thế được".
4. **Cạn `MAX_MAPPINGS = 64`** của object layer: hình học đầu (5+2 trang, cache 4, đĩa 7 Frame,
   VA ngoài block ELF) cần 61..63 mapping + bảng trang mới; `Frame.Map` cho client B trả
   `InvalidArgument`. **Không nới hằng số**: giảm hình học thiết kế (3+2 trang, cache 3, đĩa 5
   Frame) và đặt **mọi VA trong cùng 2 MiB block với ảnh ELF** (loader đã tạo bảng trang) ⇒ đo
   cuối pha `maps=61 ≤ 64`, `objects=81 ≤ 96`.
5. **D2-106 inline**: công thức + đầu vào in **ngay trên dòng** log (`evictions = misses −
   min(capacity,unique)`, `pa_changed (first/reload)`, `walk_gone (file,page,va)`,
   `hit_delta0 (reads_before/after)`, `p2_same (=p2_a==p2_b)`, `a_bytes (=a_ok*4096)`), không
   chỉ nằm ở tài liệu.
6. **Luật cấu trúc R1 (≤1500 dòng/tệp)**: `user.rs` vượt 1561 dòng sau khi thêm pha pager ⇒
   tách `pager_fault.rs` (statics + API `PagerFault`) và `tf_store.rs` (`save/saved` trapframe);
   `user.rs` còn **1494** dòng, `v2 check` PASS. Bài học: thêm tính năng vào tệp đã dài phải
   tách module **trước** khi chạm trần.
7. **Ổn định bảng điều khiển**: B hit `(0,2)` **trước** khi bị trục xuất, sau đó A nạp lại p0/p2
   (PA đổi) và B hit lại p2 trên bản A vừa nạp ⇒ chứng minh "một bản, nhiều mapping" **qua cả
   trục xuất lẫn nạp lại**, không chỉ lần đầu.

## 5. Bằng chứng (QEMU `bash v2/test/boot/ktest.sh 1 1`)

Log xanh: `JOURNAL/kt70-green-qemu-ktest-user.log`; boot đầy đủ: `JOURNAL/kt70-ktest-boot.log`.

```
# pager: fault kind=Vm va=0x40100000 client=0 access=Read reason=NotMapped core_fault=1 faults=10 ready=1 const:page=4096 const:sector=512
# pager: objmem objects=81 caps=69 frames=63 pt=12 vs=4 maps=61
# pager: cache const:capacity=3 hits=2 misses=7 evictions=4 reads=7 sectors=56 served=9 rejected=1 entries=3 unique=3 shared=2 reloads=2 clients=2 magic=0x4b54373050414745
# pager: map p2_a=0x82034000 p2_b=0x82034000 p2_same=1 hit_delta0=1 mapping_bad=0 walks_ok=9
# pager: evict evictions=4 walk_gone=1 pa_changed=1 reloads=2 kernel_data_ok=9 kernel_data_bad=0 a_done=5 b_done=4
# pager: data a_ok=5 a_bad=0 a_bytes=20480 a_bad_bytes=0 b_ok=4 b_bad=0 b_bytes=16384 b_bad_bytes=0
# pager: reject client=1 page=99 status=5 name=InvalidArgument reads_before=7 reads_after=7 delta=0 exit=0x80000005
# pager: exit pager=0 a=0 b=2147483653 steps=29 starts=3 exits=3
ok 30 - pager fault den server va nap trang
ok 31 - pager hit khong doc backend
ok 32 - pager eviction LRU khong mat du lieu
ok 33 - pager hai client chia se mot trang
ok 34 - pager page ngoai file tra ma
# pager: neg aux_status=2 name=WrongObjectType over_status=10 name=AlreadyMapped pa_same=1 (walk=0x8203e000 exp=0x8203e000) magic=0x4b543730434c4942 const:neg_over=10
ok 35 - pager ca am quyen va khong ghi de
# ktest-user: taps=35 ok=1
```

Công thức (D2-106): `sectors = misses × 4096/512 = 7 × 8 = 56`; `a_bytes = a_ok × 4096 =
5 × 4096 = 20480`; `b_bytes = 4 × 4096 = 16384`; `shared = 2` (hai lần B hit trang đang map cho
A); `reloads = 2` (A nạp lại p0 và p2); `exits = 3` (`pager=0, a=0, b=0x8000_0000|5`).

## 6. Cổng

| Hạng mục | Kết quả | Log |
|---|---|---|
| host `vifast-vfs` + `vifast-idl` | **12 + 7 test PASS** | `kt70-host-vfs.log` |
| QEMU 1 boot × 4 ảnh | **4/4 sạch**, `ktest-user` **35/35 TAP** (TAP 35 ca âm + gói vá F1..F5) | `kt70-ktest-boot.log`, `kt70-green-qemu-ktest-user.log` |
| matrix (4 ảnh × smp 1/2/4) | **12/12** boot sạch (sau F1..F5) | `kt70-ktest-matrix.log` |
| clippy kernel 4 tổ hợp `-D warnings` | **4/4 `EXIT=0`** (sau F1..F5) | `kt70-clippy-kernel.log` |
| `gate --fast` | **PASS 689 test/95 suite**, L1 1.000 seed **915 ms** — log trên **cây cuối** HEAD `43b19dfe` (sau gói vá F1..F5) | `kt70-gate.log` |
| L1 chạy thuần ×3 (D2-100) | **914 / 710 / 995 ms** (1.000 seed, máy ấm) | `kt70-l1-thuan.log` |
| soak sản phẩm **0/1000** | **wall 227 s** (4 boot/s), BUILD_ID `02ed5a1810ba59ce`, sha256 `ff99297c…` — **không đổi** so với KT-69; sau các gói vá test-only, build sạch (không feature) + `shasum` vẫn **`ff99297c…`** (build test ghi đè path `release/` từng cho hash giả `badfc5e9…` — đã ghi bài học) | `kt70-soak.log`, `kt70-product-sha.log`, `kt70-product-sha-f1f5.log` |
| budget `ktest-user` `.data+.bss` | **202.832 ≤ 262.144** | `kt70-ktest-boot.log` |
| `size_of::<Machine>()` | **188.872 ≤ 196.608** (không thêm field) | `kt70-gate.log` |

## 7. Tệp thay đổi

Thêm: `v2/lib/vifast-vfs/{Cargo.toml,src/*,tests/pager.rs}`, `v2/lib/vifast-idl/idl/pager.vidl`,
`v2/lib/vifast-idl/src/generated_pager.rs`, `init/pager_{common,server_user,client_common,
client_a_user,client_b_user}.rs`, `src/ktest_vfs.rs`, `src/pager_fault.rs` (statics fault VM,
tách khỏi `user.rs`), `src/tf_store.rs` (trapframe vừa dừng, tách khỏi `user.rs`),
`v2/docs/k6-vfs-pager.md`, journal này.
Sửa: `v2/lib/vifast-idl/src/lib.rs`, `v2/tools/vifast-gen/gen_idl.py`,
`v2/test/gen/run_idl_fixtures.sh`, `v2/kernel/vifast-kbin-riscv64/{Cargo.toml,build.rs}`,
`src/ktest_user.rs`, `src/{ktest_capa,ktest_ring,proc_demo}.rs` (nhánh `Stop::Fault`),
`vifast-plat-riscv64/src/user.rs` (`Stop::Fault`, `set_pager_faults`, `pager_fault`),
`v2/test/boot/{expected.txt,ktest.sh}`, `v2/Cargo.toml`, `v2/docs/k3b-core.md` §18.

Không chạm: `spec/`, `viscript/`, `review/`, `crates/` (tab khác). HEAD khi làm `1a275ffa`
(nhánh main có commit ViScript của tab khác — pathspec commit do lead thực hiện).

## 7b. Gói vá TAP 35 (lead cho mở băng test-only sau khi reviewer soi)

Delta so với mốc trước (chỉ ảnh test + harness + docs, **không** đổi ảnh sản phẩm):

- `v2/lib/vifast-vfs/src/stats.rs:64-68` — thêm `Stat::NegAux`/`NegOver`; `:73` `COUNT=24`;
  `:98-99` vào `ALL`.
- `v2/kernel/vifast-kbin-riscv64/init/pager_server_user.rs:138` `neg_done`; `:228-256`
  `negatives_once()` (gọi `:285` trong `fault()` lần đầu, sau khi cả hai client đã map khối
  kết quả): (a) `Frame.Map` target pool Frame, aux = `P_SELF` (TCB) → đo mã thật; (b) `Frame.Map`
  vào `VA_RESULT` của client B (đã map) → đo mã thật, PTE cũ phải nguyên.
- `v2/kernel/vifast-kbin-riscv64/src/ktest_vfs.rs:1155` đọc `NegAux/NegOver`; `:1286` dòng
  `# pager: neg …` (mã + tên + walk/exp PA + magic); `:1374-1389` TAP 35; inline công thức ở
  các dòng evidence `:1225-1260` (`p2_same (=p2_a==p2_b)`, `hit_delta0 (reads_before/after)`,
  `evictions (=misses-min(capacity,unique))`, `walk_gone (file,page,va)`, `pa_changed (first/
  reload)`, `a_bytes/a_b_bytes (=*4096)`, `delta (=reads_after-reads_before)`, `exits (=…)`).
- `v2/kernel/vifast-kbin-riscv64/src/ktest_user.rs:91` plan `1..35`; `:500` taps=35; `:505` PANIC.
- `v2/test/boot/{expected.txt:63 -> 35 35, ktest.sh:325 manifest TAP 35, :566 regex neg,
  :551-567 regex công thức + PANIC 35}`.
- `v2/docs/k6-vfs-pager.md` §5 (TAP 35 + công thức inline), `v2/docs/k3b-core.md` §18 (TAP 35),
  journal/STATE.

**Số đo ca âm (không sửa theo dự đoán — D2-102):** aux TCB ⇒ **`WrongObjectType` (2)** (core
kiểm loại cap phụ trước object layer), **không** phải `InvalidCapability` (1) như dự kiến ban
đầu; map vào VA đã map ⇒ **`AlreadyMapped` (10)**, `pa_same=1`, magic B còn nguyên. Nếu map
thành công thì `pa_same` sẽ 0 và TAP 35 đỏ ⇒ ca âm **thật**, không phải nhãn.

## 7c. Gói vá cơ học F1..F5 (reviewer DRIFT thấp–trung bình; lead mở băng lần cuối)

Reviewer kết luận sản phẩm/an ninh **đạt** (hit không gọi backend — chứng minh cấu trúc; `shared`
là cùng Frame vật lý; eviction không mất dữ liệu; `AlreadyMapped` kiểm trước mọi ghi PTE; TAP 35
phụ thuộc thật). Năm lỗi cơ học đã vá:

- **F1** `pa_p0_first` derive `Default` = 0 nhưng code so `NONE_U64` ⇒ lần p0 đầu rơi nhánh
  `else`, log in `first=0x0` **trông như số đo nhưng không phải**. Vá: khởi tạo tường minh
  `NONE_U64` sau `Evidence::default()`; log sau vá `first=0x82034000 reload=0x82036000` (PA thật).
- **F2** `starts=3` literal → `const:starts=3` (regex runner cập nhật).
- **F3** `const:clients=2`, `magic` kèm `const:magic_expected=…`; `unique=… (=entries(…) nếu
  keys_unique)` in công thức + đầu vào; bỏ số literal trong annotation `neg` (in qua `name={}`).
- **F4** aux của TAP 35 là **CNode cap** (`P_SELF` = slot 0 = `root_cap(cn_pager)`), **không
  phải TCB** — `expected.txt`, `k6-vfs-pager.md` §5, `pager_common.rs`, comment `ktest_vfs.rs`
  đã sửa. **Giữ mã đo `2`**: reviewer xác nhận `aux_vspace` (`invoke.rs:536`) trả
  `WrongObjectType` khi `c.ty != VSpace`; `InvalidCapability` chỉ cho slot ngoài range/cap chết
  — **kỳ vọng (1) ban đầu của lead là sai, agent giữ số đo là đúng** (D2-102).
- **F5** comment hình học còn "5 + 2 trang, cache 4, 7 Frame" ⇒ sửa `lib.rs` §, `pager_client_*_user.rs`,
  `pager_common.rs` (3 + 2 trang, cache 3, 5 Frame 4 KiB, 40 sector; pager CSpace 32 slot).

Xác nhận sau vá: QEMU `ktest.sh 1 1` **4/4 sạch**, `ktest-user` **35/35** (log
`kt70-ktest-boot.log` cập nhật run cuối); `gate --fast` **PASS 689 test/95 suite**, L1 **915 ms**
(HEAD `43b19dfe`); matrix **12/12**; clippy **4/4 `EXIT=0`**; ảnh sản phẩm build sạch
(không feature) + `shasum` = **`ff99297c…`** (không đổi) — `kt70-product-sha-f1f5.log`.

Bài học bổ sung: (a) **log phải là số đo hoặc hằng có nhãn `const:`** — giá trị mặc định của
`Default` không được đóng vai "chưa có"; (b) `expected.txt` là nguồn sự thật TAP, comment lệch
test là lỗi; (c) hash ảnh sản phẩm chỉ lấy sau build **không feature** — build test
(`--features ktest,user`) ghi đè cùng path `release/` và làm hash "đổi giả" (`badfc5e9…`);
sau build sạch vẫn `ff99297c…`.

## 8. Nợ chuyển KT-71 / KT-72

Xem §8 `docs/k6-vfs-pager.md`: journal/writeback trang bẩn, unmap khi tiến trình chết, nhiều
fault đồng thời, virtio-blk (KT-72 chỉ thêm `impl BlockDev`), method id IDL chưa vào `spec/`.
