# KT-67a — invocation hẹp (D2-97): memsrv thật + client U-mode map/đọc/ghi Frame

Người làm: **Agent 2 dòng OS**. Ngày: 2026-11-16. HEAD khi mở task: `19341f07`
(`[KT-00] lat KT-67 DONE …; D2-97 invocation da co trong ABI; mo KT-67a`).

## 1. Phạm vi + quyết định

- **D2-97**: invocation **đã** nằm trong ABI (`spec/abi.toml` `[syscall].0` + `[object]`;
  `[method_signature]` 13 method, không phải 27 — xem §5 nợ 2). KT-67a **không** sửa `spec/`,
  không thêm syscall, không đổi 16 số hiệu, không đổi `MessageInfo` 35 bit.
- Danh sách trắng hẹp đã cài: `Untyped.Retype` (1.0), `Frame.Map` (11.0), `Frame.Unmap` (11.1).
  Mọi cặp khác ⇒ `NotSupported` **18** (D2-53 Q11), có đếm.
- “`VSpace.Map`” của D2-97 **không có method id** trong ABI hiện tại (`VSpace` id 9 chỉ có
  `GetAsid = 0`; method 1 **để trống theo D2-32**; PageTable.Map = 0 nhưng root PageTable không
  có cap) ⇒ dùng đúng method spec cấp cho thao tác: **`Frame.Map` = 11.0** (map chính Frame vào
  `VSpace` ở `MR0`). Không thêm id mới. Ghi ở `k6-memsrv.md` §7.1 / `k3b-core.md` §14.2.
- `CNode.Insert` **không có method id** trong bảng `CNode` (`spec/abi.toml`: Copy=0..Rotate=6,
  `spec/` đóng băng) ⇒ **chưa cài**, giữ `NotSupported`; core đã có API `copy/mint/delete/revoke`
  nên task sau chỉ gắn method id có sẵn. Xem §5.

## 2. Đỏ-trước (red-before, cả 3 đợt)

| Log | Cách tạo | Kết quả đỏ |
|---|---|---|
| `kt67a-red-no-invocation.log` | tạm bỏ nhánh invocation ở `syscall.rs` (hành vi cũ D2-53 Q11) | `# sys: nr=0 err=18` cho mọi `Call` non-Endpoint; **not ok 15..18**; `stats=0/0` |
| `kt67a-red-no-map.log` | chỉ giữ `Untyped.Retype`, bỏ `Frame.Map/Unmap` khỏi danh sách trắng | **ok 15**; `# inv: map … err=1`; **not ok 16/17/18** |
| `kt67a-red-retype-unregistered.log` | đường memsrv thật nhưng bridge chưa đăng ký cap Untyped | `# inv: … registered=0`; `# inv: retype … ty=Untyped err=1 stats=0/0`; **not ok 15..18** |
| `kt67a-red-satp-kernel-fault.log` | hook invocation **không** khôi phục root kernel (satp = root user) | `PANIC: trap LoadPageFault … stval=0xffffffc0804fb008` — bug thật, vá bằng `with_kernel_root` |

Hai log đầu là đỏ-trước đợt 1/đợt 2; hai log sau là đỏ-trước của “memsrv thật + client map”
(đợt 3) — đều **giữ trong cây** làm bằng chứng, không xoá.

## 3. Xanh — bằng chứng đo từ boot thật

Nguồn: `JOURNAL/kt67a-ktest-user.log` (ảnh `ktest-user`, `-smp 1`, QEMU 9.2 + OpenSBI
fw_dynamic; số đo do kernel in, D2-80).

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

- **Ca âm (reviewer M1/M2/D2-99g)**: `Frame.Map` bằng cap VSpace **chỉ READ** ⇒ `err=3`;
  `Frame.Unmap` bằng **Frame cap khác** ⇒ `err=1` và mapping Frame A **còn** (đọc lại MAGIC);
  `Call` với `caps=1` ⇒ `err=18`. Đo sau lượt: `frame_a_maps=0 frame_b_maps=0 walk_after=0`.
- **“Tự retype” nghĩa chính xác**: quyết định + yêu cầu cấp Frame đến từ **U-mode** (`Call`
  ⇒ core `invoke` kiểm cap/quyền **của chính caller** ⇒ `SyscallEnv::invoke` ⇒
  `mem_bridge::invoke`). `mem_bridge::invoke` là **hook arch của object layer** (D2-83: nơi duy
  nhất sở hữu Frame/PA), **không** phải `mem_bridge::grant` kernel-mediated của KT-67 (đường
  `grant` — kernel kiểm quyền client + cấp hộ cap — **không** nằm trên đường này).
- **PA đọc độc lập, không tautology**: kernel đọc `aobj::machine().read_phys_u64(pa)` với
  `PhysMem` = `PhysmapMem` (`v2/kernel/vifast-plat-riscv64/src/obj.rs:69-75`): `pa` →
  `mmu::physmap_alias(pa)` (nửa cao, **không** map trong VSpace user —
  `high_half_unmapped=1` khi nạp ELF) → `read_volatile` 8 B, chạy khi `satp` = **root kernel**
  (sau `enter_expect_fault` đã `restore_kernel_root`). Vì vậy giá trị `phys=0x1122334455667788`
  **không** đi qua PTE/mapping của client.
- **TAP**: `ktest-user` **18/18** (gồm ca âm M1/M2/D2-99g); `ktest` 65/65; `ktest-guard` 3/3;
  `ktest-proc` 10/10.
- **Matrix**: `JOURNAL/kt67a-ktest-matrix.log` — **12/12** boot sạch (smp 1/2/4 × 4 ảnh): ktest **65/65**, guard **3/3**, user **18/18**, proc **10/10**; TAP 15..18 xanh ở cả smp 1/2/4.
- **Gate `--fast`**: `JOURNAL/kt67a-gate.log` — **PASS** — fmt + clippy + luật cấu trúc + L0 + gen-check + L1 1.000 seed **1.118 ms** (661 test/87 suite, cùng mốc baseline); 6 test host mới của `vifast-core` chạy rời (`JOURNAL/kt67a-core-tests.log`: 25 test, 0 fail).
- **Soak ảnh sản phẩm**: `JOURNAL/kt67a-soak.log` — **0/1000**, wall **251 s** (3 lane), `build=02ed5a1810ba59ce` sha256 `7f7a7936…` `git=b5277cb7`; `soak: 0/1000 lỗi (PASS)` (D2-24/D2-74c).
- **Budget 2 ngưỡng**: `.data+.bss` (trần 262.144) đo từ ảnh: ktest **209.920**, guard
  **2.680**, user **202.408**, proc **206.560**; `size_of::<Machine>() = 188.872 B` ≤ pin
  **196.608 B** (`# user core: … static=188872 pin=196608`, smp 1/2/4), trần ảnh 262.144.
- **Test host mới**: 6 test trong `invoke.rs` (core **25** test, chạy `cargo test -p vifast-core`);
  `gate --fast` đếm **661/87** (cổng không chạy unit-test của `vifast-core` — cùng số với baseline).
- **Clippy kernel (riscv64, `-D warnings`)**: 4 tổ hợp `EXIT=0` — workspace, bin `ktest-user`,
  bin `ktest-proc`, `vifast-plat-riscv64 --features ktest,user`
  (`JOURNAL/kt67a-clippy-kernel.log`).

## 4. Tệp đã đổi

| Tệp | Thay đổi |
|---|---|
| `v2/kernel/vifast-core/src/invoke.rs` (**mới**) | toàn bộ đường invocation: giải mã `MessageInfo`, danh sách trắng, kiểm quyền (M2), ràng buộc frame (M1 ở bridge), cài cap đại diện + rollback 3 bước (L1/L2), `caps>0 ⇒ 18` (D2-99g), `INVOCATION_STATS`, 5 test host |
| `v2/kernel/vifast-core/src/machine.rs` | `drop_root_cap` — rollback cap gốc chưa vào slot (L2) |
| `v2/kernel/vifast-core/src/syscall.rs` | `SyscallEnv::invoke`/`release_object` (mặc định `NotSupported` — D2-53 Q11); nhánh syscall 0: Endpoint → IPC, còn lại → invocation |
| `v2/kernel/vifast-core/src/lib.rs`, `src/spec.rs` | `pub mod invoke`; tái xuất `CORE_OBJECTS`/`RESERVED_OBJECTS` (id object sinh từ spec — D2-11) |
| `v2/kernel/vifast-plat-riscv64/src/mem_bridge.rs` | `invoke`/`register_untyped`/`release_key` — object layer thi hành (`retype/map/unmap` + `sfence`), bảng một chiều core ObjId → cap Untyped |
| `v2/kernel/vifast-plat-riscv64/src/user.rs` | `UserMemAccess::invoke` + `with_kernel_root` (khôi phục root kernel rồi trả lại root user — vì `satp` = root user trong syscall) |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_inv.rs` (**mới**) | pha `invocation_phase` (TAP 15..18): cấp cap lúc “spawn” (Untyped/VSpace/Frame B/VSpace chỉ READ), nạp `inv.elf` ô 1, đăng ký bridge, đo PA qua physmap; tách khỏi `ktest_user.rs` để giữ R1 |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_user.rs` | `mod ktest_inv` + gọi pha (TAP 15..18); 1.532 → **1.191 dòng** (R1 ≤ 1.500) |
| `v2/kernel/vifast-kbin-riscv64/init/inv_user.S` + `init_user.rs` (**mới**, + `build.rs`) | chương trình U-mode: tự `Retype` Frame, `Map`, `sd/ld` thật, `Unmap`, đọc lại ⇒ fault, 2 method lạ ⇒ 18 |
| `v2/test/boot/ktest.sh`, `expected.txt` | manifest/evidence `# inv:` + PANIC mới + `ktest-user 18 18` (KT-54: cùng commit với kernel) |
| `v2/kernel/vifast-kbin-riscv64/src/ktest.rs` | **dọn nợ doc-lint của KT-67** (2 dòng `//!` ngăn `clippy::doc_lazy_continuation` khi clippy kernel bật `-D warnings`; chỉ comment, không đổi hành vi) |
| `v2/docs/k6-memsrv.md` §7, `v2/docs/k3b-core.md` §14 | docs (số mục: §14 là mục kế tiếp thật của k3b-core, không có §15/16) |

## 5. Nợ / BLOCKED (file:line)

1. **Chuyển cap Frame giữa hai TCB U-mode** — **không phải thiếu ABI** (D2-98): receive window
   **đã** có trong ABI (`spec/abi.toml` `[calling.riscv64].ipc_buffer`, dòng 58) và core đã có
   `tcb_set_recv_window`/`plan_cap_transfer`/`deliver_caps`; thiếu là **shim U-mode** —
   `syscall7` (`v2/kernel/vifast-plat-user/lib.rs:28`) không ghi header IPC buffer; chỗ **từ chối**
   `MessageInfo.caps > 0` hiện là nhánh invocation (`v2/kernel/vifast-core/src/invoke.rs`, trả 18),
   còn `build_message` (`v2/kernel/vifast-core/src/syscall.rs`) **không** kiểm `caps`. Vì vậy pha này **memsrv + client là một chương trình/cspace**
   (**KT-67b** — D2-98 — cài header/window trong shim để tách 2 tiến trình, **không** sửa spec).
   Phần tự retype/map/đọc/ghi/fault đã là thật.
2. **`CNode.Insert`** không có số method trong `spec/abi.toml` (`[object] CNode` 0..6; spec
   đóng băng) ⇒ giữ `NotSupported`. `Untyped.Retype` một mình đủ cho memsrv; khi cần
   Copy/Mint/Move/Delete/Revoke qua invocation, gắn id 0/1/2/4/5 (không thêm id mới) + TAP.
3. **`[method_signature]` có 13 method, không phải 27** (D2-97 ghi 27): hiện trạng
   `spec/abi.toml` đếm được 13 mục `[method_signature."…"]`. Không sửa spec; ghi để Agent 0
   đính chính D2-97 nếu cần.
4. TAP 15..18 dùng `enter_expect_fault` (test-only) cho fault cuối — chính sách U-mode của ảnh
   sản phẩm vẫn fatal (không đổi).

## 6. Reviewer checklist

- [ ] Không có thay đổi `spec/` (`git diff --stat -- spec/` rỗng); 16 syscall + `MessageInfo` 35
      bit không đổi; method ngoài danh sách trả 18 (TAP 18).
- [ ] D2-83: core không đọc/ghi PTE/byte; `invoke.rs` chỉ dùng `foreign_key` + `add_foreign_object`
      + `put_cap`; object layer nằm ở `mem_bridge`.
- [ ] Nguyên tử: Retype kiểm slot đích/quyền **trước** khi cấp; core lỗi ⇒ `release_object`.
- [ ] `with_kernel_root`: root user được trả lại trước `sret` (nếu sai ⇒ kernel chạy U-mode bằng
      root kernel).
- [ ] Red-before 4 log có trong cây; matrix 12/12; soak 0/1000; gate 661/87 (core 24 test rời); budget 2 ngưỡng;
      clippy kernel sạch: **4 tổ hợp** `-D warnings` riêng (`JOURNAL/kt67a-clippy-kernel.log`, mỗi tổ hợp có dòng `CLIPPY_<tên>_EXIT=0`).

## 7. Review vòng 1 (reviewer-kt67a) — verdict **DRIFT trung bình**, không UNSAFE — và các vá

| Mục | Nội dung | Vá |
|---|---|---|
| **M1** (chặn) | `Frame.Unmap` chỉ tìm theo `va`, không ràng buộc Frame ⇒ cap Frame bất kỳ gỡ được mapping của Frame khác | `mem_bridge.rs` `invoke`: `walk(vs, va)` phải valid + `walk.phys == frame_pa(frame)`; sai ⇒ `InvalidCapability` **1** / `NotMapped` **11**, mapping **không** gỡ. Test âm: Frame B (slot 18) ở VA của Frame A ⇒ `err=1`, đọc lại MAGIC OK (TAP 17) |
| **M2** (chặn) | `aux_vspace` không kiểm quyền ⇒ cap VSpace chỉ READ vẫn map/unmap | `invoke.rs` `aux_vspace`: đòi `RIGHT_WRITE` ⇒ `InsufficientRights` **3**. Test âm: VSpace chỉ READ (slot 19) ⇒ `err=3` |
| **M3** | Chữ ký tham số do Agent 2 tự đặt, spec chưa có | **D2-99** (lead) hợp thức hoá `MR0..MR3` + trả `a1`, `MR0` = slot đích/VSpace cap; **KT-67b** cập nhật `spec/` một lần. Docs sửa thành “quy ước D2-99”, **không** nói “không có ABI mới” |
| **D2-99(g)** | `caps>0` trả 5, không đếm | `invoke.rs`: `caps>0 ⇒ NotSupported` **18** + `bump(DENIED)` — TAP 18 có ca thứ ba `caps=1 ⇒ 18` (`stats=3`) |
| **L1** | `retype` bỏ qua `made == ty` | so loại object layer trả với loại yêu cầu; lệch ⇒ release + `NotSupported` |
| **L2** | rollback `put_cap` để object đại diện sống | thêm `Machine::drop_root_cap` (cap chưa vào slot) → `kill_foreign_object` → `release_object`; test host `drop_root_cap_chi_cap_chua_vao_slot` |
| **L3/L5** | Trích dẫn sai: `build_message` không từ chối `caps>0`; symbol là `tcb_set_recv_window` | sửa `k6-memsrv.md` §7.5, `k3b-core.md` §14.6, `BLOCKED.md`, journal |
| **L6** | HEAD/đếm tệp STATE cũ; log “clippy” chỉ là log build | STATE cập nhật HEAD `b5277cb7` + đếm tệp thật; `JOURNAL/kt67a-clippy-kernel.log` chạy lại **4 tổ hợp clippy `-D warnings`** với `CLIPPY_<tổ hợp>_EXIT=0` in trong log |

Ghi chú: reviewer thấy cây bị sửa **sau** khi báo freeze — lần này sửa **hết** (code + docs + log)
rồi mới báo lại. Chạy lại trên **cây cuối** (sau khi tách `ktest_inv.rs` cho R1): gate **PASS
661/87** L1 **1.118 ms** (`JOURNAL/kt67a-gate.log`), `cargo test -p vifast-core` **25/25** (6 test
mới, `JOURNAL/kt67a-core-tests.log`), clippy kernel **4 tổ hợp `EXIT=0`**
(`JOURNAL/kt67a-clippy-kernel.log`), matrix **12/12** (`JOURNAL/kt67a-ktest-matrix.log`), soak
**0/1000** wall **251 s** (`JOURNAL/kt67a-soak.log`), budget product **200.184**, ktest
**209.920**, guard **2.680**, user **202.408**, proc **206.560**.
