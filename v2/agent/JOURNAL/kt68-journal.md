# KT-68 — capalloc: cấp/thu hồi capability theo cây MDB

Ngày: 2026-11-16 (Agent 2 dòng OS). HEAD khi bắt đầu: `3ea7176d` (sau KT-67b DONE).
Trạng thái: **DONE** — TAP **24/24** (`ktest-user`, thêm 5 TAP), matrix **12/12**, gate **PASS**,
soak **0/1000**, clippy kernel **4/4 `EXIT=0`**, hai ngưỡng giữ nguyên.
Tài liệu: [`../../docs/k6-capalloc.md`](../../docs/k6-capalloc.md), [`../../docs/k3b-core.md`](../../docs/k3b-core.md) §16.

## 0. Việc 0 — sửa **một dòng** harness `run.sh --boot` (phát hiện từ KT-67b)

**Đỏ-trước** (`JOURNAL/kt68-runboot-do.log`): `bash v2/test/conformance/run.sh --boot --smp 2`
chạy sim + `check_scn` xong rồi chết ở bước QEMU:

```text
# qemu: ktest.sh 1 matrix (3 ảnh × smp 1/2/4)
not ok - ktest.sh matrix FAIL (xem /tmp/kt68-boot-red/ktest-matrix.log)
bash: v2/test/boot/ktest.sh: No such file or directory
EXIT=1
```

Nguyên nhân: `run.sh` đã `cd "$V2_ROOT"` (=`…/v2`) rồi vẫn gọi `bash v2/test/boot/ktest.sh`
⇒ `v2/v2/test/...`. **Vá 1 dòng** + sửa chú thích “3 ảnh” → “4 ảnh” (runner có 4 ảnh từ KT-66):

```diff
-    echo "# qemu: ktest.sh 1 matrix (3 ảnh × smp 1/2/4)"
-    if ! (cd "$V2_ROOT" && bash v2/test/boot/ktest.sh 1 matrix) >"$LOGDIR/ktest-matrix.log" 2>&1; then
+    echo "# qemu: ktest.sh 1 matrix (4 ảnh × smp 1/2/4)"
+    if ! (cd "$V2_ROOT" && bash test/boot/ktest.sh 1 matrix) >"$LOGDIR/ktest-matrix.log" 2>&1; then
```

**Xanh-sau** (`JOURNAL/kt68-runboot-final.log`): `bash v2/test/conformance/run.sh --boot --smp 2`
→ matrix **12/12 boot sạch** + `trace-compare PASS 61/61 dòng, 5/5 ca`, `EXIT=0`
(BUILD_ID `051a43bed94160db` — ảnh `ktest` không đổi vì nhánh `user` không link vào ảnh đó).
Cờ `--boot` **giữ nguyên** (không xoá cờ chết) vì đã chạy được và là đường duy nhất
sim↔QEMU trong một lệnh.

## 1. Số method `CNode` — **đọc spec trước khi cài**

`spec/abi.toml` `[object].CNode` (frozen): `0 Copy, 1 Mint, 2 Move, 3 Mutate, 4 Delete, 5 Revoke,
6 Rotate`.

> Mô tả task ghi `Revoke (4), Delete (5)` — **sai thứ tự so với spec**. KT-68 làm theo spec:
> `Delete=4`, `Revoke=5`. Nếu theo mô tả thì `Delete` sẽ chạy `Revoke` và ngược lại. Đã **sinh**
> bảng `CNODE_METHODS` từ spec (`v2/tools/vifast-gen`) và test host
> `so_method_cnode_khop_spec` khoá lại — không chép tay (D2-11).
>
> `Mutate`/`Rotate` có số nhưng **chưa cài** ⇒ `NotSupported` (18) + có đếm. `CNode.Insert`
> không có số trong spec (D2-98) ⇒ vẫn không tồn tại.

## 2. Sản phẩm code (chỉ `v2/`)

| Tệp | Thay đổi |
|---|---|
| `kernel/vifast-core/src/invoke.rs` | 5 method `CNode.*` trên nhánh `(CNode, _)`; layout MR0..MR3 (D2-99); hằng `CNODE_*`; bộ đếm `copy/mint/moved/delete/revoke`; test host |
| `kernel/vifast-core/src/machine.rs` | `Machine::move_cap` (D2-44, port từ sim): dst bận ⇒ `delete` trước, `src==dst` ⇒ 17, giữ `CapId`/cha/quyền/badge |
| `kernel/vifast-core/src/capalloc_tests.rs` (mới) | **9 test host**: Copy/Mint/Move/Delete/Revoke/cây 2 tầng/xuyên CNode/ca âm/cap chết/capalloc chu kỳ |
| `kernel/vifast-core/src/lib.rs` | `#[cfg(test)] mod capalloc_tests;` |
| `lib/vifast-abi/src/generated.rs` + `tools/vifast-gen/gen.py` | sinh `CNODE_METHODS` (+ các bảng `*_METHODS` khác) từ `[object]`; số phải liền mạch 0..N (fail-closed) |
| `kernel/vifast-plat-user/lib.rs` | thêm `nb_recv` (syscall 4) — chỉ `ecall`, **không** lệnh đặc quyền (D2-101); sửa trùng `SYS_NB_RECV` |
| `kernel/vifast-kbin-riscv64/init/capa_server_user.rs` (mới) | capalloc server U-mode: quét slot rảnh bằng `Copy`+lỗi 7, `Revoke`/`Delete`, thu hồi cap nguồn xuyên CNode |
| `kernel/vifast-kbin-riscv64/init/capa_client_user.rs` (mới) | client U-mode: nhận slot, map/ghi/đọc/unmap, dựng cây 2 tầng, cấp lại cùng slot, CPTR cũ ⇒ 8 |
| `kernel/vifast-kbin-riscv64/src/ktest_capa.rs` (mới) | rig 2 CNode/2 TCB/2 Endpoint/Frame + scheduler round-robin + TAP 20..24 + `# capa:` |
| `kernel/vifast-kbin-riscv64/src/ktest_user.rs` | plan `1..24`, gọi pha capalloc, `# ktest-user: taps=24 ok=1`, PANIC mới |
| `kernel/vifast-kbin-riscv64/src/ktest_inv.rs`, `ktest_capx.rs` | sau khi đo xong TAP, **trả VSpace cũ** về object layer — arena 64 KiB chỉ còn 8 KiB trước pha capalloc (2 VSpace mới cần ~16 KiB) |
| `kernel/vifast-kbin-riscv64/build.rs` | build 2 ELF mới `capa_server.elf`/`capa_client.elf` |
| `test/boot/ktest.sh`, `expected.txt` | thêm 5 TAP + 9 dòng `# capa:`, PANIC mới, `ktest-user 24 24`, evidence count 30→**49** |
| `test/conformance/run.sh` | vá 1 dòng §0 |
| `docs/k6-capalloc.md` (mới), `docs/k3b-core.md` §16 | thiết kế/bằng chứng/nợ |

**Không** sửa `spec/`, **không** thêm syscall, **không** thêm `unsafe`, **không** nới ngưỡng.

## 3. capalloc làm gì (đường U-mode, không kernel-mediated)

Hai ELF riêng, hai cspace, hai VSpace, hai TCB; kernel làm scheduler tuần tự round-robin
(`NBRecv` + `Yield`, không chặn cứng):

1. client `NBSend(EP_REQ, caps=1)` mang **cap CNode của chính nó** (header IPC buffer, KT-67b);
2. server `NBRecv` → cap CNode client trong **receive window** (slot 8); quét slot `1..15` của
   CNode client: `Copy` Frame của server → slot; lỗi **7 = đã dùng** ⇒ thử tiếp; trả slot;
3. client `Frame.Map` cap vừa nhận → ghi `0x1122334455667788` → đọc lại → `Frame.Unmap`;
4. client tự dựng **cây 2 tầng** (`grant → con → cháu`) + nhánh khác bằng `CNode.Copy` trên cap
   CNode của mình; yêu cầu server `Revoke(slot)` (xoá đúng 2 cap) + `Delete(slot)` (trả slot);
5. cấp lại ⇒ **cùng slot**; client map lại, ghi `0x8877665544332211`; kernel đọc **physmap** tại
   PA của Frame ⇒ đúng (bằng chứng độc lập, không tin lời chương trình);
6. server `Revoke` **cap nguồn** của mình ⇒ xoá bản sao **xuyên CNode**; CPTR cũ của client ⇒ 8.

Bằng chứng đo trên QEMU (log `kt68-xanh-user-smp1.log`, kernel in — D2-80):

```text
# capa: u-mode server_exit=0 client_exit=0 steps=13 src_obj=34 src_ty=Frame src_rights=0xf src_alive=1
# capa: cap slot client4..7=1111 server8=1 copies=5 revokes=2 deletes=3 mints=0 moves=0 denied=4
# capa: walk va=0x41000000 valid=1 pa=0x82047000 expected_pa=0x82047000 phys=0x8877665544332211 client_vspace=64
# capa: pre empty_client=1 src_alive=1 walk_before=0 post caps=1 ipc=1 bytes=1 obj=1
# capa: mint badge=0xabc rights=0x3 badge_frame=1 copy_rights=0x3 move_src_empty=1 same_cap=1 parent_ep=1 cleanup=1 caps=1
# capa: revoke slot=6 deleted=2 subtree=1 sibling=1 live_before=46 live_after=48 delete_reparent=1 cross_cnode=1 src_alive=1 caps=1 revokes=4
# capa: am empty=1 occupied=1 rights=1 range=1 ro_cnode=1 mutate=1 rotate=1 other=1 denied=1 short=1 long=1 stale=1 dead_alive=1 caps=1
# capa: rfree s=12->12 c=12->12 live=46->46 cap_high=47->64 installed+=1 failed+=0
# capa: invariants caps=1 ipc=1 bytes=1 obj=1 frames=72
ok 20 - capalloc cap vong
ok 21 - capalloc mint copy move
ok 22 - capalloc revoke cay delete reparent
ok 23 - capalloc ca am
ok 24 - capalloc bat bien
# ktest-user: taps=24 ok=1
PANIC: KT-68 u-mode pha syscall, invocation, chuyen cap va capalloc xong (24 TAP, 0 fail)
```

## 4. Đỏ-trước (D2-05)

Host (`kt68-do-truoc-host.log`): tạm để nhánh `CNode.*` trả `NotSupported` (đúng hành vi trước
KT-68) ⇒ **9/9 test capalloc đỏ**, ví dụ:

```text
test capalloc_tests::cnode_copy_giu_quyen_va_badge ... FAILED
assertion `left == right` failed
  left: Err(18)
 right: Err(3)
test result: FAILED. 0 passed; 9 failed; 0 ignored; 0 measured; 33 filtered out
```

QEMU (`kt68-do-truoc-qemu.log`): cùng bản tạm ⇒ **TAP 20..24 đều `not ok`**:

```text
not ok 20 - capalloc cap vong: server/client/PTE/PA/cap sai
not ok 21 - capalloc mint copy move: mint/copy/move sai
not ok 22 - capalloc revoke cay delete reparent: revoke/delete sai
not ok 23 - capalloc ca am: ca am tra sai ma
not ok 24 - capalloc bat bien: ro cap/slot hoac bat bien do
# ktest halt
```

Sau khi cài: **24/24 xanh** (log §3). Không retry (D2-07); các lần boot đỏ ở trên là **bản tạm
có chủ đích**, không phải flake.

## 5. Bất biến & hai ngưỡng

- `check_caps`/`check_bytes`/`check_ipc`/`check_objmem` xanh sau mọi pha; `Revoke` đúng cây con,
  không lan nhánh khác; `Delete` reparent MDB; cap chết **không** nằm trong slot (bất biến bắt
  được — test host chứng minh `cap_at ⇒ 1` + `check_caps` đỏ; trên ABI, CPTR cũ ⇒ 8).
- **Không rò**: `slot_rfree` s/c **12→12**, `live_cap_count` **46→46**; `cap_high` 47→64 là
  high-water do **ID cap không tái dùng** (spec `id_reuse = "never"`) — ghi nợ, không phải rò.
- `size_of::<Machine>()` **không đổi 188.872 B** ≤ pin **196.608 B**; ảnh test các ảnh vẫn dưới
  trần **262.144 B** (runner in budget từng ảnh); **không** nới ngưỡng nào.
- Conformance **61/61 dòng, 5/5 ca** (sim ↔ QEMU, smp 2); matrix **12/12** (4 ảnh × smp 1/2/4),
  `ktest-user` **24 TAP** ở cả 1/2/4.

## 6. Cổng cuối + log (số chép từ log, không viết tay)

- **Matrix**: `ktest.sh 1 matrix` → **12/12 boot sạch** (4 ảnh × smp 1/2/4; `ktest-user` **24
  TAP** ở cả ba cấu hình) — `kt68-ktest-matrix.log`.
- **Conformance**: `run.sh --boot --smp 2` → **61/61 dòng khớp, 5/5 ca**, `trace-compare PASS`
  (`kt68-runboot-final.log`).
- **Soak**: `soak.sh 1000 3` → **0/1000**, wall **290 s**, BUILD_ID `02ed5a1810ba59ce`,
  sha256 `f6318b5d2ee91a9b5f1bbb8756b30aab69a6a082cff0bc35117713d21526236c` (`kt68-soak.log`).
  `.text=49854`, `.rodata=14424`, `.data=7824`, `.bss=192360`; ảnh sản phẩm **188.544 B**.
  Sau khi vá clippy (chỉ tệp test `ktest_capa.rs` — **không** vào ảnh sản phẩm) rebuild lại đúng
  sha256 `f6318b5d…` (`kt68-product-sha.log`) ⇒ soak 0/1000 áp cho **đúng** artifact cuối.
- **Gate `--fast`**: **PASS**, `v2 gate --fast: PASS`, **661 test / 87 suite**, L1 wall **879 ms**
  (`kt68-gate.log`). **D2-100**: chạy **thuần** cùng lệnh 3 lần = **0.86 / 0.79 / 0.85 s**
  (`kt68-l1-thuan.log`) — lệch < 2×, cả hai số đều ≪ 60 s ⇒ không giảm seed.
- **Core test host**: `cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-core --lib` →
  **42 passed** (9 test capalloc mới) — `kt68-core-tests.log`.
- **Clippy kernel** `-D warnings`: 4 tổ hợp **`EXIT=0`** (workspace host; kbin `ktest`;
  kbin `ktest,user`; plat-riscv64 `user`) — `kt68-clippy-kernel.log`. Lần đầu tổ hợp
  `ktest,user` **EXIT=101** (3 lint trong `ktest_capa.rs`); đã sửa (collapse if / direct return /
  `needless_range_loop`) rồi chạy lại 4/4 sạch.
- **`v2 check`**: PASS (fmt/clippy/luật/ABI/scn/report/gen-check) — trong `kt68-gate.log`.
- **Nit tự phát hiện (test-only)**: test host `capalloc_cap_thu_hoi_cap_lai_cung_slot` khẳng định
  `stats().mint >= 1` — bộ đếm **toàn cục** nên phụ thuộc thứ tự chạy (flaky khi chạy riêng tổ
  hợp). Đã bỏ điều kiện không thuộc test đó; sản phẩm/ảnh không đổi, gate không đổi (file
  `#[cfg(test)]` không được biên dịch vào ảnh/gate).

## 7. Nợ mở (có chủ, không tô hồng)

1. `CNode.Mutate` (3) / `Rotate` (6) chưa cài — `NotSupported` có TAP; cần D2 mới nếu muốn.
2. Không có `GetFreeSlot`: capalloc quét tuyến tính (policy của server), O(N) mỗi lần cấp.
3. `n_caps` high-water: ID cap không tái dùng (spec) ⇒ chu kỳ dài chạm `MAX_CAPS`; cần free-list
   kèm thế hệ nếu muốn thu hồi ID (không đổi ABI nhưng là thay đổi model).
4. Capalloc một hart; U-mode đa hart (D2-94) vẫn là nợ K6.
5. Hai ELF mới chỉ vào `.rodata` ảnh test (không vào `.data/.bss`); ảnh `ktest-user` vẫn ≤ trần.
6. `capa_server` là chương trình test (label `0x68x`); KT-69 (IDL) sẽ thay bằng stub sinh.

## 8. Vá literal evidence (D2-102) — sau verdict sơ bộ, cây đóng băng lần 2

Lead chọn áp **một đợt vá cơ học** (D2-102: evidence phải `writeln!` từ biến đo; hằng thiết kế
phải mang nhãn `const:…`; mã lỗi in qua `user::sys_err`; method id in qua hằng sinh từ spec).
Phạm vi **đúng** các mục đã liệt kê — không đổi logic, không thêm/bớt test:

| Tệp | Hunk (dòng sau vá) | Nội dung |
|---|---|---|
| `kernel/vifast-kbin-riscv64/src/ktest_capa.rs` | ~544, ~565, ~657, ~715, ~770, ~796 | `# capa: invariants` `frames_live=1` → `frames={}` đo từ `check_objmem().counts.frames` (log mới: `frames=72`); bỏ literal `parent=1` (đã có `parent_ep={}` đo); `# capa: revoke` 4 literal `subtree=1 sibling=1 cross_cnode=1 src_alive=1` → `{subtree_ok} {sibling_ok} {cross_ok} {src_alive_after}`; thêm biến `sibling_ok`/`src_alive_after`; `ok` thêm `&& sibling_ok` |
| `kernel/vifast-kbin-riscv64/src/ktest_inv.rs` | ~270, ~285, ~295, ~306 | 4 literal KT-67a: `method=0/0/1` → `invoke::METHOD_{RETYPE,MAP,UNMAP}`; `err=18` → `user::sys_err(0)` (render **y hệt** 0/0/1/18 ⇒ regex + trích dẫn docs không đổi, nhưng giá trị giờ đọc từ hằng/đo) |
| `test/boot/ktest.sh` | ~533 | 2 regex: dòng `mint` (bỏ `parent=1 `), dòng `invariants` (`frames_live=1` → `frames=[0-9]+`) |

**Delta chính xác**: `JOURNAL/kt68-delta-va-literal.diff` (so bản reviewer đã xem với bản sau vá;
1 hunk `ktest.sh`, 4 hunk `ktest_inv`, 6 hunk `ktest_capa`; tổng **+24/−11** — `ktest_capa.rs`
+14/−5, `ktest_inv.rs` +8/−4, `ktest.sh` +2/−2 — chỉ đổi cách in + 2 biến bool, không đổi nhánh
thực thi nào).

**Tự grep sau vá** (`JOURNAL/kt68-grep-evidence.txt`): quét **32** format-string evidence thuộc
`# capa:/# inv:/# capx:/# mem:/# stack:` trong `kernel/vifast-*`. Sau khi trừ hex `0x…` và
placeholder `{}`:
- **0** literal dạng **claim pass/fail** (`=1 /=0 /=empty`) còn lại trong `# capa:`/`# inv:`/
  `# capx:` của phần KT-68 (sau vá).
- Còn lại **đầu vào/thiết kế** (không phải claim): `# capa: revoke slot=6` (slot đem revoke —
  input của test), `# capx: … policy=copy` (nhãn policy, lead đã nêu là dạng được phép), các
  `slot=…`/`va=0x…`/`bits=…` trong `# capx:`/`# inv:`. `# mem:`/`# stack:` còn literal **có từ
  trước KT-68** (`# mem: init untyped=2 …`, `# stack: noload=1 …`) — **ngoài phạm vi** đợt vá
  (lead giới hạn “không sửa gì khác”); đề nghị một nit riêng nếu muốn áp D2-102 cho KT-52/67.

**Kiểm lại sau vá** (cùng cây):
- matrix `ktest.sh 1 matrix` → **12/12 boot sạch**, `ktest-user` **24/24** × smp 1/2/4
  (`kt68-ktest-matrix.log`); evidence `# capa:` render **giá trị đo** (mint/revoke/invariants —
  xem log `kt68-xanh-user-smp1.log`, dòng `frames=72`).
- clippy combo `ktest,user` **`EXIT=0`** (lần đầu sau vá lỗi thứ tự biến `src_alive_after`, đã sửa
  rồi chạy lại — không đổi logic; log cuối `kt68-clippy-user-sau-va.log`).
- ảnh sản phẩm rebuild **đúng sha256 `f6318b5d…`** ⇒ soak 0/1000 + gate 661/87 giữ nguyên hợp lệ
  (chỉ ảnh test `ktest-user` đổi phần in; ảnh sản phẩm không link nhánh `user`).
- `v2 check`/gate không đổi phạm vi (không compile feature `user`/`ktest`); `git ls-files --others
  --exclude-standard v2` = **0** (mọi tệp mới đã `git add -N`), `git diff --cached` rỗng.

## 9. Reviewer (điền sau verdict) — xem mục cuối
