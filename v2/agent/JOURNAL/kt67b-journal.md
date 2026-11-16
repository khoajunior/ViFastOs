# KT-67b — header IPC buffer U-mode + chuyển cap Frame giữa **hai tiến trình** (K6, D2-98/D2-99)

- **Người làm**: Agent 2 dòng OS. **Ngày**: 2026-11-16.
- **HEAD khi làm**: `959d8f5d` (`[KT-00]` mở KT-67b) → `1bde39be` (lead push **D2-100** giữa task);
  nhánh `main`.
- **Phạm vi commit**: `v2/` + **`spec/abi.toml` đúng một lần** (D2-99) + `docs/abi.md` (sinh) +
  `v2/lib/vifast-abi/src/generated.rs` (sinh). **Không** chạm `viscript/`, `crates/`, `review/`,
  `.agents/`, `out-b.log`.
- **Kết quả**: TAP **19** trên QEMU (2 ELF/2 TCB/2 cspace/2 VSpace), matrix **12/12**, gate **PASS
  661/87**, soak **0/1000**, clippy kernel **4/4 `EXIT=0`**, core test **32** (7 test mới),
  baseline gate PASS trước khi sửa (`kt67b-baseline-gate.log`).

## 1. `spec/` — mở đúng một lần (D2-99): 3 entry + 3 dòng comment

`git diff spec/abi.toml` = **một tệp**, `+15 −3`, chỉ 3 entry trong `[method_signature]` (kèm
**3 khối comment** của chính 3 entry: `Retype` thay 1 dòng cũ bằng 2 dòng ⇒ +2/−1, `Map` +1,
`Unmap` +1 — tổng **+4/−1 dòng comment**); không đổi
`[syscall]`/`[error]`/`[object]`/`[message]`/`[calling]`/`[debug_info]`:

| # | Entry | Thay đổi | Comment đổi kèm |
|---|---|---|---|
| 1 | `Untyped.Retype` | `params` thêm `dst` (sau `cap`): `["cap","dst","type","size_bits","flags"]`; `returns = ["slot"]` (từ `[]`) | thay comment cũ *“Giá trị trả `cap` chưa có nguồn thiết kế … để K1a chốt riêng (D2-35)”* (mâu thuẫn trực tiếp với `returns=["slot"]`) bằng 1 dòng dẫn D2-99: `dst`=MR0, `type`=MR1, `size_bits`=MR2, `flags`=MR3, `a1`=slot |
| 2 | `Frame.Map` (**mới**) | `params = ["frame","vspace","va","flags"]`, `returns = []` | 1 dòng `# D2-99(d): 11.0; MR0 = cap VSpace, MR1 = va, MR2 = flags` |
| 3 | `Frame.Unmap` (**mới**) | `params = ["frame","vspace","va"]`, `returns = []` | 1 dòng `# D2-99(d): 11.1; MR0 = cap VSpace, MR1 = va; leaf tại va phải thuộc đúng cap Frame (f)` |

Tổng diff: **`+15 −3`** (3 entry + 4 dòng comment thêm/1 dòng thay — không dòng nào khác).

Sinh lại: `./v2/bin/vifast-gen gen` → `v2/lib/vifast-abi/src/generated.rs`
(`METHOD_SIGNATURES` 13 → 15), `docs/abi.md`; `./v2/bin/vifast-gen --check` **PASS**.

**F4 (reviewer)**: entry `Frame.Map` còn **1 dòng `note`** (`Map Frame vào VSpace; cap VSpace ở
MR0 phải có RIGHT_WRITE (D2-99(e))`) — metadata `note` mọi entry khác trong spec đều có, không
phải chữ ký mới; **nằm trong phạm vi lần mở spec duy nhất theo D2-99** và được ghi ở đây để
“3 entry” không bị đọc thành “3 entry + note không khai”. `Frame.Unmap` **không** có `note`. Lead đã xác
nhận việc **giữ 3 dòng comment** (spec không tự nói dối) với điều kiện journal ghi chính xác —
bảng trên là bản ghi đó.

## 2. Pin máy kiểm — **không** thay bằng số viết tay

Hai chỗ có pin cứng đếm chữ ký (đều lỗi thời khi spec mở; **không** sửa thành “15”):

1. `v2/test/abi/check_abi.py`: bỏ `len(methods) == 13`; `expected` = các khoá
   `[method_signature."X.Y"]` **đọc từ spec lúc parse**, so **tập** với entry có `params`
   (bắt entry thiếu `params`/entry lạ) + cross-check **số phần tử** `METHOD_SIGNATURES` trong
   `generated.rs` (nguồn thứ hai cùng pipeline). Thêm chữ ký sau này **không phải sửa pin**.
2. `v2/lib/vifast-abi/src/lib.rs` (test `chu_ky_method_sinh_tu_spec_va_co_read_state`, đổi tên từ
   `…_du_13…`): đếm `[method_signature."` bằng `include_str!("../../../../spec/abi.toml")` rồi so
   với `METHOD_SIGNATURES.len()`; thêm kiểm D2-99 (`Retype` có `dst`, trả `["slot"]`, có
   `Frame.Map/Unmap`) + tên chữ ký không trùng.

## 3. Shim U-mode (`v2/kernel/vifast-plat-user/lib.rs`)

- VA cố định `IPC_BUFFER_VA = 0x4002_0000`, 4 KiB, loader map `R|W|U|NX` + zero cho **mọi** ELF
  U-mode; header 10 word: `magic(b"KT67b")`, `info` (bản ghi 35 bit), **window 4 CPTR**, **send
  caps 4 CPTR**.
- API mới: `init_ipc_buffer`, `set_recv_window([CPTR;4])`, `recv_window`, `set_send_caps(&[CPTR])`,
  `send_caps`, `info(label,caps,no_reply,length)`/`info_label`/`info_caps`/`info_length`,
  `note_received`/`received_info`, `read_received_caps() -> ReceivedCaps{count, slots}`,
  `call`/`nb_send`/`recv`, `peek_u64`/`poke_u64`, `yield_now`.
- Chương trình U-mode **không** cần `unsafe`/`asm!` (R5): mọi `ecall` + đọc/ghi bộ nhớ đi qua shim
  (`kernel/vifast-plat-*` thuộc danh sách đóng D2-17).

## 4. Core (`vifast-core`)

- `ipc.rs`: `IPC_HEADER_MAGIC/…_WORDS/…_W_*`, kiểu `IpcHeader`.
- `SyscallEnv::ipc_header` **mặc định `None`** (ảnh cũ/Nền tảng khác giữ nguyên hành vi).
- `syscall.rs`: `apply_ipc_header` (4 CPTR → `cap_slot` → `tcb_set_recv_window_obj`) và
  `build_message(m, tcb, info, MR*, header)` — `caps>0` ⇒ CPTR nguồn **từ header**, rỗng/chết ⇒
  `InvalidCapability` 1, không header ⇒ `InvalidArgument` 5. Nhánh gửi 0/1/2/5/6/7 dùng
  `build_with_header`; nhánh nhận 3/4 áp window **trước khi giao**; Notification bỏ qua;
  **invocation giữ nguyên** `caps>0 ⇒ 18` + đếm (D2-99(g)).
- `core_ipc.rs`: thêm `tcb_set_recv_window_obj(ObjId, …)` (thân kiểm dùng chung bản `SlotRef`
  của KT-63: CNode sống, 4 ô khác nhau).
- **F2 (reviewer, theo D2-101)**: 3 chương trình U-mode (`memsrv.elf`, `client.elf`,
  `inv.elf`) **không** có `unsafe`/asm; disassemble `llvm-objdump -d --no-show-raw-insn` cả ba
  (log `kt67b-udisasm-no-priv.log`): hợp mnemonic = `addi auipc beqz bgeu bltu bne bnez ecall j
  jalr ld li lui mv pause ret sd slli srli` ⇒ **chỉ `ecall` + load/store/ALU thường**, **0** dòng
  `csrr*/csrw*/csrs*/csrc*/sret/mret/wret/wfi/sfence.vma/satp` — đúng D2-101 (shim U-mode **được**
  `unsafe` cho `ecall` + `read_volatile/write_volatile` trên địa chỉ user, **cấm** lệnh đặc
  quyền/CSR/physmap).
- 7 test host mới (`JOURNAL/kt67b-core-tests.log`): magic khớp `b"KT67b"`, window vào TCB, CPTR
  ngoài bảng ⇒ 1, trùng ô ⇒ 5, message lấy cap từ header, `caps>0` không header ⇒ 5, CPTR rỗng ⇒ 1;
  tổng `cargo test -p vifast-core` = **32 passed**.

## 5. Arch (`vifast-plat-riscv64`)

- `user.rs`: loader map thêm **trang IPC buffer** (1 Frame 4 KiB, cùng bảng cấp 1 với ảnh/stack ⇒
  không thêm bảng trang); `MAX_USER_RANGES` +1; `user_range_mapped()` (một nguồn `active_ranges`).
- `ipc_buffer.rs` (**tệp mới**, tách khỏi `user.rs` vì **R1**: `user.rs` chạm 1.542 dòng > 1.500):
  đọc 10 word bằng `sstatus.SUM`, magic khớp mới trả `IpcHeader`; `#[cfg(feature = "user")]`
  (ảnh sản phẩm/ktest chính không link).

## 6. Hai tiến trình thật (`src/ktest_capx.rs` + `init/{memsrv,client}_user.rs`)

- **2 ELF riêng** (`memsrv.elf`, `client.elf` — 8.560 B mỗi tệp), **2 `load_init_slot`** (2 VSpace,
  2 bảng dải), **2 TCB**, **2 CNode/cspace riêng** (A: 0 self/1 EP/2 VSpace A/3 Untyped/5 Frame/
  6 Frame-thiếu-GRANT, window 8..11; B: 0 self/1 EP/2 VSpace B, **window 4..7**).
- Luồng: A `Untyped.Retype` Frame (1.0) → ca âm `caps=1` + CPTR rỗng ⇒ **1** → `NBSend` mang cap
  (có `GRANT`) → `NBSend` mang cap **thiếu `GRANT`** (chuyển mềm 0 cap) → `NBSend` `caps=0`;
  B `Recv` (kernel áp window **của B** từ header) → cap cài vào **cspace B** slot 4 → `Frame.Map`
  vào VSpace B tại `0x4100_0000` → `poke/peek` 8 B → `Frame.Unmap` bằng chính cap nhận → map lại
  → ghi giá trị 2 → `Recv` 2 message đối chứng → `Exit(0)`.
- **Phát hiện khi làm (ghi lại vì là bẫy thật)**: bản đầu cho memsrv `Exit(0)` ngay sau khi gửi ⇒
  client `Recv` **treo** vì `Machine::exit` gọi `unlink_tcb_queues` **huỷ message đang chờ của
  người gửi** (D2-25: “hàng chờ cũ bị huỷ trước khi giao fault”). Sửa đúng ngữ nghĩa: memsrv kết
  thúc bằng **`Yield`** (giữ TCB sống, message ở lại hàng Endpoint), kernel test đưa A về `Ready`
  rồi chạy B. Log chẩn đoán (trước khi xoá) cho thấy `pushed=3 popped=0 removed=3` — bằng chứng
  cơ chế, không phải “client hỏng”.

## 7. TAP 19 + evidence

Ảnh `ktest-user`: `1..19`; TAP mới `ipc capx hai tien trinh`. Log boot thật (smp 1/2/4 giống nhau):

```text
U: memsrv chuyen cap Frame
U: client nhan cap va map/ghi/doc
# capx: pre cspace_b4=empty cspace_b5=empty walk=0 a5=Frame src_alive=1 policy=copy
# capx: recv slot=4 ty=Frame rights=0xb obj=29 from_obj=29 from_rights=0xf installed=1 failed=1
# capx: map va=0x41000000 pa=0x82030000 bits=12 walk=1 walk_pa=0x82030000 phys=0x8877665544332211 caps=1 ipc=1 obj=1 src_alive=1
ok 19 - ipc capx hai tien trinh
# ktest-user: taps=19 ok=1
PANIC: KT-67b u-mode pha syscall, invocation va chuyen cap xong (19 TAP, 0 fail)
```

Diễn giải: `pre` = **trước** `Recv` của B, cspace B **rỗng**, VSpace B **chưa** map VA;
`recv` = cap ở **cspace B** slot 4 (`rights=0xb` = `RIGHTS_ALL & !RIGHT_GRANT`), cùng object Frame
với A (`from_obj`), `installed=1` (đúng **1** cap cài — chống “chuyển ngầm nhiều lần”),
`failed=1` (cap thiếu `GRANT` bị bỏ **mềm**, message vẫn giao); `map` = kernel walk VSpace **B**
hợp lệ, PA thật, kernel đọc **physmap** = giá trị **B** ghi (`attrs` không in, PTE do kernel walk),
`check_caps`/`check_ipc`/`check_objmem` xanh, `policy=copy`/`src_alive=1` = memsrv **vẫn giữ** cap
nguồn (ABI/KT-63 là **copy**, không phải move — “chuyển hết” cần `CNode.Delete`, xem nợ).

**F3 (reviewer)**: dòng `# capx: pre …` **trước đây là literal** trong `ktest_capx.rs`; nay in
**giá trị thật** của phép đo trước (`pre_b4_empty`, `pre_b5_empty`, `pre_walk`, loại+còn-sống cap
A sau khi gửi) qua cùng `writeln!` như `# mem:`/`# inv:` — regex `ktest.sh` vẫn khớp y hệt:
`pre cspace_b4=empty cspace_b5=empty walk=0 a5=Frame src_alive=1 policy=copy`. TAP 19 vẫn kiểm
các biến boolean (không kiểm chuỗi), nên dòng evidence và phép kiểm ràng buộc nhau.

Manifest/expected cập nhật cùng commit: `v2/test/boot/expected.txt` (`ktest-user 19 19`),
`ktest.sh` (`manifest_user` + 3 regex `# capx:` + `taps=19` + mẫu PANIC mới).

## 8. Red-before (D2-05)

**Log boot thật** = `kt67b-red-before-ktest-user.log` (**dòng 158**: `# u-mode exit: code=71
faultkind=Exit{71} …`; **dòng 159**: `not ok 19 - ipc capx hai tien trinh: memsrv khong Yield`);
wrapper của `ktest.sh` cho riêng boot đó = `kt67b-red-before-matrix1.log` (trước tên
`…-caps-rejected.log` — **đổi tên F6** vì nó chỉ là wrapper, không phải log boot; chuỗi
`cspace_b4=empty` trong wrapper là **chẩn đoán của harness** “thiếu evidence#35”, **không** phải
số đo). Cách làm: tạm tắt `UserMemAccess::ipc_header` (đúng trạng thái trước KT-67b) ⇒ memsrv
`NBSend caps=1` nhận `InvalidArgument` **5** (không có CPTR nguồn/window), thoát **71**; TAP 1..18
vẫn xanh ⇒ đỏ **đúng chỗ**.

## 9. Số đo (chép **sau** khi log tồn tại)

- **Gate `--fast`** (`kt67b-gate.log`, lần chạy **sau vá F1..F6**): **PASS** — fmt + clippy + 5
  luật + abi/scn/report/gen-check + build + L0 (**661 test / 87 suite**, bằng mốc KT-67a) + L1
  1.000 seed; **L1 wall = 1.183 ms** (log: `--- L1: OK (1183 ms, đo bởi cổng)`) — cây ấm, số
  đúng. **D2-100 — ghi cả số xấu để không ai đọc nhầm**: lần gate **trước** (cùng mã, lúc máy bận)
  in **171.115 ms**, đã kiểm là **artifact biên dịch/khoá build trong cửa sổ đo**, không phải sim:
  chạy thuần cùng lệnh 3 lần = **0.80 / 0.82 / 0.85 s**, tái lập đúng thứ tự cổng (L0 rồi L1) =
  **0.85 s**, và 2 lần sau vá = **1.00 / 1.01 s**. Không dùng 171 s làm số hiệu năng; chạy thuần
  ≪ 60 s (AGENTS §6 không phải giảm seed).
- **Core test**: `cargo test -p vifast-core` **32 passed / 0 failed** (`kt67b-core-tests.log`).
- **Matrix**: `ktest.sh 1 matrix` → **12/12 boot sạch** (ktest 65/65, guard 3/3, user **19/19**,
  proc 10/10 × smp 1/2/4) — `kt67b-ktest-matrix.log`.
- **Soak**: `soak.sh 1000 3` → **0/1000**, wall **229 s**, BUILD_ID `02ed5a1810ba59ce`,
  sha256 `435e0f9dfc4aaada68dbba9552119c298e07ea8596dff56cc8dbc7e7618f114b` — **đúng** sha256
  ảnh sản phẩm hiện tại (`kt67b-soak.log`; chạy lại trên cây cuối, không dùng số của lần soak
  trước khi tách `ipc_buffer`).
- **Clippy kernel**: 4 tổ hợp `-D warnings` (workspace; kbin `ktest`; kbin `ktest,user`;
  plat-riscv64 `user`) đều `CLIPPY_*_EXIT=0` — `kt67b-clippy-kernel.log`.
- **Hai ngưỡng (D2-85a)**: `size_of::<Machine>()` **không đổi 188.872 B** ≤ pin **196.608 B**
  (TAP `# core: link` vẫn in `bytes=188872 pin=196608`); ảnh `ktest-user` `.data+.bss` =
  **202.488 B** ≤ **262.144 B**; `ktest` 209.920; `ktest-proc` 206.624; ảnh sản phẩm: xem
  `# soak:` BUILD_ID/sha256. Hai ELF mới chỉ vào `.rodata` (+17.120 B file ảnh), **không** vào
  `.data/.bss` (budget không đổi so với KT-67a).

## 10. Việc phải sửa ngoài phần tính năng (nit reviewer KT-67a + hạ tầng)

1. `v2/agent/BLOCKED.md:273` — câu *“`build_message` … từ chối `MessageInfo.caps > 0`”* **sai**
   (chỗ từ chối là nhánh invocation `invoke.rs`, D2-99(g)); đính chính + ghi rõ nguồn.
2. `v2/kernel/vifast-core/src/invoke.rs` — comment “`returns=[]` nên `a1` không phải hợp đồng
   ABI” → nay D2-99 chốt `returns=["slot"]`, sửa thành **hợp đồng ABI**.
3. `v2/kernel/vifast-kbin-riscv64/src/ktest_inv.rs` — bỏ nhãn **hard-code** `ro_vspace=1
   caps_msg=1` khỏi dòng `# inv: deny2` (bằng chứng thật là mã lỗi 3/18 do chương trình kiểm);
   sửa regex evidence tương ứng trong `ktest.sh`.
4. `v2/test/scn/01..03` — `Untyped.Retype` nay có `dst` (D2-99) nên 3 fixture **chỉ thêm** `dst`
   vào **dòng `setup`** (`+1/−1` mỗi tệp): **không** ca nào bị bỏ/đổi, không đổi kỳ vọng
   `=> ok`; conformance **không đổi** (`61/61 dòng, 5/5 ca`, `scn-check 27 tệp/16 nhóm`). Chọn
   `dst=<tên cap đã khai>`; **không** khai thêm cap kiểu `Frame` (bộ chạy `kt11_scn` dựng cap theo `caps` và
   “chỉ object lõi mới dựng trực tiếp” ⇒ sẽ panic — đã thử, ghi lại để người sau không lặp).
5. `v2/kernel/vifast-plat-riscv64/src/user.rs` tách `ipc_buffer.rs` (R1 ≤ 1.500 dòng) và
   `#[cfg(feature = "user")]` cho module mới (ảnh sản phẩm không link) — phát hiện vì
   `v2 check` R1 đỏ ở 1.542 dòng và build sản phẩm lỗi `unresolved import crate::user`.

## 11. Nợ (không tô hồng)

1. **Move cap** chưa có: policy là **copy** (KT-63). “Memsrv không giữ cap” chỉ đúng nếu có policy
   move ⇒ cần `CNode.Delete`/`Revoke` có method id trong invocation (ngoài danh sách trắng).
2. **MR4..MR63** qua IPC buffer vẫn chưa đọc/ghi (header mới chỉ cho window + cap nguồn).
3. **`CNode.Insert`** vẫn `NotSupported` (không có method id, D2-98).
4. **Multi-hart U-mode**: rig chạy tuần tự 1 hart dưới điều khiển kernel test; chưa có preemption
   U-mode thật (hai tiến trình = hai TCB/cspace/VSpace chạy lần lượt, không phải SMP scheduling).
5. `spec/` để `[object].Frame` vẫn `stability="reserved"`/`unsupported=true` (D2-98 giữ phạm vi
   “một lần”); **không** tự đổi — nếu cần, D2 mới.

## 12. Verdict reviewer — **DRIFT** (không UNSAFE), vá **F1..F6** (2026-11-16)

Reviewer `reviewer-kt67b`: nội dung lớn **đạt** (spec `+15/−3` đúng 3 entry; `.scn` `+1/−1` mỗi
tệp, conformance **61/61 + 5/5 không đổi**; hai cspace thật với `pre` assert trước `Recv`,
`installed==1`, `failed==1`, `src_alive`; D2-100 hai số L1). Vá:

| Mục | Vá | Bằng chứng |
|---|---|---|
| **F1** | `git add -N` **mọi** tệp mới (4 mã nguồn + 11 log/journal) | `git status --porcelain v2/ \| grep -c '^??'` = **0**; `git status --porcelain` chỉ còn `M`/`A` của task (ràng buộc: 2 lần trước HEAD không build vì thiếu `-N` — lần này kiểm trước khi báo) |
| **F2** | theo **D2-101**: disassemble 3 ELF U-mode, log `kt67b-udisasm-no-priv.log`; docs `k6-memsrv.md` §8.2b + `k3b-core.md` §15.6 | hợp mnemonic 3 ELF = `addi auipc beqz bgeu bltu bne bnez ecall j jalr ld li lui mv pause ret sd slli srli`; **0** dòng `csrr*/csrw*/csrs*/csrc*/sret/mret/uret/wfi/sfence.vma/satp`; mỗi ELF có `ecall` (memsrv 24 lệnh) |
| **F3** | dòng `# capx: pre …` in **giá trị thật** (`pre_b4_empty`/`pre_b5_empty`/`pre_walk`/loại+còn-sống cap A) qua `writeln!` | boot lại: `# capx: pre cspace_b4=empty cspace_b5=empty walk=0 a5=Frame src_alive=1 policy=copy` (regex `ktest.sh` khớp y hệt; TAP 19 vẫn kiểm biến boolean) |
| **F4** | ghi rõ `Frame.Map` có **1 dòng `note`** trong phạm vi lần mở spec D2-99 (metadata như mọi entry; `Frame.Unmap` không có) | §1 của journal + bảng §8.1 `k6-memsrv.md` |
| **F5** | giữ nguyên `.scn`, ghi rõ “chỉ thêm `dst` vào dòng `setup`, không ca nào bỏ/đổi” | `git diff v2/test/scn/` = 3 tệp `+1/−1`; conformance **61/61 dòng, 5/5 ca**; `kt11_scn` **9/9**, `unmodeled_setup==3`; `scn-check 27 tệp/16 nhóm` |
| **F6** | đổi tên `kt67b-red-before-caps-rejected.log` → **`kt67b-red-before-matrix1.log`** (chỉ là wrapper `ktest.sh`); evidence boot thật trỏ `kt67b-red-before-ktest-user.log` **dòng 158/159** | dòng 158 `# u-mode exit: code=71 faultkind=Exit{71} …`, dòng 159 `not ok 19 - … memsrv khong Yield`; docs §8.6 + §15.6 cập nhật tên |

**Chạy lại sau vá** (cùng cây): matrix **12/12** (`kt67b-ktest-matrix.log`), gate **PASS** (L1
**1.183 ms**), clippy kernel **4/4 `EXIT=0`**, `v2 check` **PASS**, `cargo test -p vifast-core`
**32/32** (không đổi — không chạm core), soak **giữ nguyên hợp lệ** (chỉ đổi `.text` ảnh
`ktest-user`; ảnh sản phẩm sha256 vẫn `435e0f9d…`, BUILD_ID `02ed5a1810ba59ce`).

## 13. Review

- Verdict: **DRIFT → vá F1..F6 xong** (bảng trên); chờ **confirm ngắn** của lead rồi commit
  `[KT-67b]` (pathspec `v2/ spec/abi.toml docs/abi.md`).
