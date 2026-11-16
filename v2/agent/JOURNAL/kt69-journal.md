# KT-69 — IDL tối thiểu + ring buffer chia sẻ (K6) — nhật ký

Trạng thái: **DONE phần cài + đo**, chờ reviewer verdict trước commit `[KT-69]`.
Thời gian: 2026-11-16 17:09– (Asia/Tokyo). HEAD vào: **`a9f80d83`** (KT-68b + D2-105).
Worktree: chỉ ghi `v2/`; `viscript/`, `review/`, `.agents/`, `out-b.log` (tab khác) **không chạm**.

## 1. Vào / baseline

- `git log -1` = `a9f80d83`; KT-67/67a/67b/68 DONE; KT-69 READY.
- Baseline `./v2/bin/v2 gate --fast`: **PASS 661 test/87 suite**, L1 1.000 seed wall **915 ms**
  (cây ấm) — `kt69-baseline-gate.log`.
- Đọc D2-97/98/99/101/102/103/104/105, D2-83/84/85/94/100, `spec/abi.toml`,
  `tools/vifast-gen/gen.py`, các crate kernel, KT-68 journal/docs.

## 2. Đỏ-trước (D2-05)

### 2.1 Host

Viết **trước** test của `vifast-ring` (7 integration + 2 lib) và `vifast-idl` (5 lib) khi chưa có
crate/lib: log `kt69-red-host.log` (491 dòng) = **compile FAIL** (`E0583` không thấy `generated.rs`,
`E0425` không thấy `Request`), tức test được viết trước phần cài. Sau khi cài: **14/14 xanh**
(`kt69-host.log` — 2 lib ring + 7 integration ring + 5 idl).

### 2.2 QEMU

Dựng đủ harness `ktest_ring.rs` (TAP 25..29, evidence `# idl:`/`# ring:`), hai chương trình
`ring_server`/`ring_client` ở dạng **stub** (chỉ `Exit(0)`), runner/plan đã cập nhật 29 TAP:
`bash v2/test/boot/ktest.sh 1 1` → `ktest-user` **24/29**, `not ok 25..29` (evidence đo được
toàn 0: `seen=0x0 attach_msgs=0 … size=0 open=0`) — `kt69-red-qemu-ktest-user.log` +
`kt69-red-qemu-runner.log`. TAP 20..24 (KT-68) vẫn xanh ⇒ hạ tầng đo đúng, chỉ thiếu phần cài.

## 3. Cài đặt

### 3.1 IDL + generator

- `v2/lib/vifast-idl/idl/ring.vidl`: interface `ringchan version 1`, 3 method (`attach` mang cap
  `frame` + `slots: u32`; `sync(phase:u32,count:u64)`; `finish(err:u32,sent:u64,got:u64)`), kiểu
  trả `u32`. Cú pháp dòng ghi trong chú thích đầu tệp.
- `v2/tools/vifast-gen/gen_idl.py`: parser chặt (1 interface, ≤4 scalar, ≤1 cap, tên/kiểu hợp
  lệ, **phải có ≥1 method mang cap**), sinh `v2/lib/vifast-idl/src/generated.rs`: hằng interface/
  method/`REPLY_BIT`/shift `MessageInfo`, encoder typed, decoder **arity chính xác** (D2-104) +
  chống tràn `u32`, reply typed + `decode_reply` chung. **Nguồn số method = tệp IDL** (spec
  không có bảng method cho interface mới, ghi rõ trong header sinh + docs; D2-102c).
- `./v2/bin/vifast-idl` (`gen`/`--check`); **nối vào `v2 check`** (`do_check`) + fixture âm
  `v2/test/gen/run_idl_fixtures.sh` (2 case: sửa tay stub; sửa IDL mà không sinh lại).
- Stub **không `unsafe`**, không syscall, không crate phụ thuộc; `#[rustfmt::skip]` từng item để
  `cargo fmt --check` và gen-check không đánh nhau (phát hiện: fmt làm lệch generated.rs).

### 3.2 Ring + shim

- `v2/lib/vifast-ring`: header 4 KiB (magic/version/size/mask/slot_bytes/head/tail/seq), khối
  stats 33 trường (bên nào ghi trường nấy), payload 8 slot × 32 B; `send`/`recv` **một slot**,
  `store_rel`/`acquire` qua trait `Mem`; đầy ⇒ **9**, rỗng ⇒ **14** (mã ABI có sẵn); `check()`
  bất biến; **không `unsafe`**. Test host: 7 integration (wrap 40/8, đầy/rỗng, thứ tự, chứng minh
  slot ghi **trước** publish head, stats một bên ghi/đọc, **2 thread thật** 2.000 message).
- `vifast-plat-user`: thêm `atomic_load_acquire`/`atomic_store_release`/`atomic_load_relaxed`/
  `atomic_store_relaxed` trên VA user (`AtomicU64`; D2-101: chỉ bộ nhớ user, không CSR/không lệnh
  đặc quyền; `fence` của Acquire/Release là lệnh U-mode hợp lệ, không phải `sfence`).

### 3.3 Rig + hai tiến trình

- `ktest_ring.rs`: load 2 ELF vào 2 VSpace (slot dải 1/2 tái dùng sau KT-67b), 2 CNode/2 TCB/2 SC,
  **2 Endpoint một chiều** (một Endpoint hai chiều sẽ để bên gửi đọc lại message của mình), cap
  Frame object-layer trong CSpace server + cap VSpace từng bên; chạy xen kẽ tới `Yield` (giữ
  trapframe), đo walk/PTE/PA/physmap + stats + cap transfer + bất biến.
- `ring_server_user.rs`: `Frame.Map` (số `vifast_abi::FRAME_MAP` sinh từ spec), `Ring::init`,
  gửi `attach` mang cap (send caps header), lấp đầy 8 + probe mã 9, mỗi `sync` gửi lô 4 + probe
  đầy, nhận `finish` → reply + `ServerDone`, **linger 4 lượt** trước `Exit` (D2-25: Exit của
  sender finalise IPC, huỷ message còn trong hàng — lần chạy đầu client mất reply `finish`).
- `ring_client_user.rs`: nhận `attach`, kiểm `MessageInfo.caps=1` + CPTR ô 4, `Frame.Map`,
  `Ring::open`, thử rỗng 3 lần (mã 14), tiêu thụ 4/lượt + kiểm `seq`/nội dung, `sync`, `finish`,
  nhận reply → `ClientDone`; stats chỉ ghi **sau** khi map (lần đầu ghi trước khi map ⇒ fault
  `0x420000f0`, đã sửa).
- `ring_common.rs`: adapter `Mem` + hình học + message `[seq, seq+1, !seq, 0x69]`.

### 3.4 Dọn tài nguyên (phát hiện + sửa)

Object layer chỉ **64 KiB arena + `MAX_OBJECTS = 96`**; các pha KT-67a/67b/68 để lại Frame/PT/
VSpace + cap tạm ⇒ pha ring **không load nổi** (`Obj(NoSlots)`, rồi `Obj` do bảng flush đầy).
Sửa **chỉ sau khi đo xong** từng pha (không đổi điều kiện TAP nào):

1. `LoadedInit` ghi `(VA, CapId)` từng trang; `release()` `unmap` → **drain flush** → `revoke`
   Frame → `revoke` VSpace (module mới `init_pages.rs`, tách khỏi `user.rs` để giữ R1).
2. capx/inv/capa `delete` cap tạm trong cspace gốc (13..28) sau TAP tương ứng; capa dọn trước,
   ring dùng lại 25..28 (cspace gốc chỉ **32 slot**, không dùng slot 32).
3. `MAX_OBJECTS`/`MAX_FLUSHES` **không đổi** (không nới ngưỡng); chỉ trả tài nguyên đã cấp.

## 4. Đo cuối (mọi số từ log)

| Hạng mục | Kết quả | Log |
|---|---|---|
| gate `--fast` | **PASS 675 test/92 suite** (661→675 test, 87→92 suite) — lần cuối sau vá DRIFT | `kt69-gate.log` |
| L1 (D2-100) | wall cổng **1.343 ms** (máy bận, ghi nhận D2-100); chạy thuần sau vá **894 / 884 / 922 ms** — cả hai < 60 s | `kt69-gate.log`, `kt69-l1-thuan.log` |
| ktest-user | **29/29 TAP**; `# idl:` seen=0x7, attach/sync/finish=1/10/1, acks=10; `# ring:` sent=recv=40, full_err=9/full_code=9/full_head=8, empty_err=3/empty_code=14, first=0 last=39 wrap=5 order=1 torn=dups=lost=bad=0, sum 820=820, peak=8, `installed=1 failed=0`, steps s12/c12; PANIC KT-69 đúng mẫu | `kt69-ktest-matrix.log` |
| matrix | **12/12** boot sạch (4 ảnh × smp 1/2/4) | `kt69-ktest-matrix.log` |
| soak | **0/1000** (wall **230 s**, BUILD_ID `02ed5a1810ba59ce` sha256 `5c8ce032…` — sha đổi so với trước vá vì module `init_pages` vào ảnh; `.text` digest không đổi) | `kt69-soak.log` |
| clippy kernel | **4/4 `EXIT=0`** (host workspace; kbin `ktest`; kbin `ktest,user`; plat-riscv64 `user`) | `kt69-clippy-kernel.log` |
| budget | `ktest-user` `.data+.bss` **202.792 B** ≤ 262.144; ktest 209.920; guard 2.680; proc 206.640; `size_of::<Machine>()` **188.872 B** ≤ 196.608 | `budget.sh`/log boot |
| conformance | 61/61 dòng + 5/5 ca (gate: `gen_golden` PASS; matrix ktest 65/65) | trong `kt69-gate.log`/matrix |

L1 được đo **trên cây ấm** (compile trước) theo D2-100; không dùng wall cổng làm số hiệu năng.

## 5. File (chỉ `v2/`)

**Mới**: `lib/vifast-idl/{Cargo.toml,idl/ring.vidl,src/lib.rs,src/generated.rs}`,
`lib/vifast-ring/{Cargo.toml,src/lib.rs,tests/spsc.rs}`, `tools/vifast-gen/gen_idl.py`,
`bin/vifast-idl`, `test/gen/run_idl_fixtures.sh`, `kernel/vifast-kbin-riscv64/src/ktest_ring.rs`,
`kernel/vifast-kbin-riscv64/init/{ring_common,ring_server_user,ring_client_user}.rs`,
`kernel/vifast-plat-riscv64/src/init_pages.rs`, `docs/k6-idl.md`, log `kt69-*`.
**Sửa**: `Cargo.toml` (2 member), `bin/v2` (idl-gen-check), `kernel/.../Cargo.toml` (2 dep),
`build.rs` (rlib + 2 ELF), `ktest_user.rs` (mod + TAP 29 + PANIC), `ktest_capa/capx/inv.rs`
(dọn tài nguyên + test-only), `plat-riscv64/{lib,user}.rs` (release/InitPages), `plat-user/lib.rs`
(atomic), `lib/vifast-abi/src/generated.rs` (hằng method theo tên sinh từ spec — sửa `gen.py`),
`test/boot/{expected.txt,ktest.sh}`, `docs/k3b-core.md` §17.

## 6. Vá sau review (reviewer-kt69: DRIFT **thấp** — kỹ thuật đạt hết, chỉ cách in)

Reviewer tự kiểm: SPSC đúng thứ tự; hai tiến trình cùng PA (`s_pa=c_pa=pa=0x8202a000`,
`b4=1` trỏ `frame_obj=44`); `wrap=5` thật (kernel tự chia `head/size` từ header physmap);
method id sinh bằng `enumerate` (không hardcode); `spec/` sạch; gate/matrix/conformance/soak
khớp; disassembly **0 lệnh đặc quyền**. Sáu mục đã vá **một đợt** (lead yêu cầu gộp):

| Mã | Vấn đề | Vá |
|---|---|---|
| **D1** | `Stat::EmptyCode` ghi **hằng** `EMPTY_CODE` thay vì mã thật | client lưu `first_code`/`last_code` **thật** từ `Ring::recv`; thêm `Stat::EmptyCodeLast` (STAT_COUNT 33→34); TAP 27 đòi **cả hai** = 14; evidence `empty_code_first/last` |
| **D2** | `wrap=5` không ghi công thức (D2-106) | evidence `wrap=head/size={} const:size={} head={}`; TAP 28 chia theo `header.size` **đo được** + đòi `header.size=RING_CAP`; giữ `slot0=32` làm chứng cứ thứ hai |
| **D3** | delta cap không in `pre`; `b4` không in `ty/alive` | in `installed={}->{} failed={}->{}` + `b4_ty=Frame b4_alive=1` |
| **N1** | TAP 25 dùng hình học do **chương trình tự ghi** | TAP 25 dùng `ring_check` + `header.size/slot_bytes` kernel đọc physmap; xoá `Slots/SlotBytes/RingCheck` khỏi TAP; client thôi ghi các trường đó |
| **N2** | comment “mỗi bên chỉ ghi trường của mình” sai | sửa comment: bảng sở hữu một bên (server: sent/full/sync/labels/wrap; client: recv/empty/order/hdrerr/…); client thôi ghi `LabelsSeen/AttachMsgs/Slots/SlotBytes/RingCheck/Wrap` |
| **N3** | `torn` thực chất là lỗi header | đổi tên `Stat::HdrErr`, evidence `hdrerr=`, TAP 28 `hdrerr=0`; cộng cả mã rỗng lệch |
| **N4** | thiếu `const:` cho `va`/`iface/version/methods` | `const:va=0x42000000`, `const:iface/version/methods` trong `# idl:` |
| **N5** | chỉ assert hình học runtime | `const _: () = assert!(SLOTS.is_power_of_two() …)` + `SLOT_BYTES % 8 == 0` trong `ring_common.rs` |
| **N6** | — | reviewer xác nhận **không có unsafe code** (hit `grep` là chú thích/string test) — không sửa |

Sau vá: host **675 test/92 suite**; matrix **12/12**, `ktest-user` **29/29** (evidence mới khớp
regex runner); clippy 4/4 `EXIT=0`; gate PASS (L1 wall **1.343 ms** — máy bận lúc đo, chạy thuần
ghi ở §4); **soak chạy lại 1000** vì sha256 ảnh sản phẩm đổi do module `init_pages` (BUILD_ID
`.text` giữ `02ed5a1810ba59ce`) — số cuối ở §4.

## 7. Nợ mở

1. Method id interface dịch vụ **không thuộc** `spec/abi.toml` (sinh từ IDL — mục 2.1 KT-69);
   muốn thành ABI lõi phải mở spec (D2-22).
2. **MR4..MR63** qua IPC buffer vẫn thiếu (IDL hiện ≤4 scalar) — nợ KT-67b.
3. **Chưa song song thật**: một hart, scheduler tuần tự; Acquire/Release có test 2 thread host
   nhưng QEMU chỉ chứng minh interleaving có kiểm soát.
4. **Policy cap copy** (KT-63): server vẫn giữ cap Frame nguồn; move cần `CNode.Delete` ngoài
   danh sách trắng invocation.
5. Ring capacity/slot_bytes là hằng dịch vụ, chưa cấu hình qua IDL (thuộc KT-70 nếu cần).
