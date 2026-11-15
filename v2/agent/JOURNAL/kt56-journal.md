# KT-56 — `FaultKind` thật + W^X + `spec/abi.toml` một lần (K3, D2-56a/D2-67/D2-72 Q1)

Ngày: 2026-11-15. HEAD vào: `963c4149` ([KT-00] lật KT-55 DONE + mở KT-56); baseline
`gate --fast` PASS (`JOURNAL/kt56-baseline-gate.log`). Task **DUY NHẤT** của K3 sửa `spec/`
(chỉ `spec/abi.toml`, **một hunk** `FaultKind`). Working tree có `viscript/` của tab khác —
**không chạm**; chỉ stage `v2/` + `spec/` (+ `docs/abi.md` là golden sinh từ spec — xem §5).

## 1. Đỏ trước (D2-05)

`JOURNAL/kt56-red.txt`: `spec/abi.toml` chỉ khai `fault_kind` = `Exit`; `ktest_conf` phân loại
`fault` **từ PTE** (`classify_fault`), không phải trap; `mapped_device` **không tồn tại**
(nợ L3 KT-52); runner mới đòi TAP 38..42 và 7 TAP ảnh user trong khi kernel chưa in.

## 2. Làm

1. `v2/kernel/vifast-plat-common/src/fault.rs` (mới, `#![forbid(unsafe_code)]`):
   `FaultKind` **đúng** K2 + spec (Exit/Vm/UnknownSyscall/UserException), `FaultAccess`,
   `VmFaultReason`, `TrapFacts`/`PteFacts`, `classify_trap` (scause 12/13/15 → `Vm` đúng
   access, `va=stval`; scause 2/3/0/1/4/5/6/7/9/lạ → `UserException{ip=sepc, kind=scause}`;
   ecall 8 → `None`), `vm_reason` (NotMapped/Permission/Wx; privilege thắng W^X),
   `FaultStats`/`FAULT_STATS` (atomics).
2. `spec/abi.toml`: **một hunk** mở rộng `[method_signature.fault_kind]` + 3 biến thể mới;
   `git diff spec/` = 1 hunk (`JOURNAL/kt56-spec.diff`). `vifast-gen` sinh thêm
   `FAULT_KINDS` vào `v2/lib/vifast-abi/src/generated.rs` + bảng trong `docs/abi.md` ⇒
   `gen-check` **không còn rỗng** cho hunk này; `vifast-abi` test đòi 4 biến thể/trường.
3. `vifast-plat-riscv64/src/probe.rs` (chỉ ảnh test): thực thi **một** instruction
   load/store/jump tới VA biết là fault; `trap_dispatch` ghi `scause/stval/sepc/SPP` rồi đặt
   `sepc` về `probe_after` để resume — trap **thật**, không tiêm (D2-73).
4. `user.rs`: khi `Exit` ⇒ `FAULT_STATS.record(Exit{code})`; khi fault U-mode ⇒ khôi phục root
   kernel, walk PTE thật của VSpace, `classify_trap`, log `faultkind=…`; cờ test một lần
   `enter_expect_fault` cho phép TAP 5 recover (chính sách mặc định vẫn log+halt).
5. `ktest_conf.rs`: `vm_expect_fault` của `.scn` 25/26 đi **cùng probe**; ca 26 **scratch-map**
   đúng PA/cờ tại đúng VA kịch bản (`mmu::map_scratch_4k`, bảng cấp 0 dự phòng 4 KiB `.bss`),
   gỡ ngay sau trap; in `# faultprobe: … src=trap`.
6. `ktest_fault.rs` (mới): TAP 38..42 (`faultkind vm load/store/insn`, `wx violation qua ABI`,
   `device exec`); trap lồng dời 43.
7. `objects`: `Frame.mapped_device` + `map`/`protect` chặn Device↔exec **theo Frame** (D2-67
   "toàn cục"); `check_objmem` đếm lại; `ObjError::abi_code()`; host test
   `device_toan_cuc_khong_lan_exec_theo_frame`.
8. `ktest_user.rs` + `init_user.S`: TAP 5 phân loại kernel-read (recover), 6 `faultkind exit`,
   7 U-mode **thực thi** trang `.data` RW NX ⇒ `scause=0xc`, `Vm{Execute, Wx}`, PANIC cuối.
9. `expected.txt` 43/3/7, `manifest_*`, evidence + panic regex trong `ktest.sh` sửa **cùng lúc**.

## 3. Kết quả đo (log trong `JOURNAL/`)

- `kt56-ktest-matrix.log`: `ktest.sh 1 matrix` **9/9 boot sạch**, ảnh 43/3/7 TAP, ktest 43 TAP.
- `kt56-trace-compare.log`: sim↔QEMU **61/61 dòng, 5/5 ca** (22:16 23:19 24:7 25:7 26:12),
  `build=…` khớp ảnh (D2-80) — `fault` của ca 25/26 từ **trap thật** (log `# faultprobe`).
- `kt56-qemu-ktest-smp2.log`: `# faultkind: case=load … kind=Vm{…Read…NotMapped} src=trap`,
  `case=store`, `case=insn`, `case=wx-abi … abi=4`, `case=device-exec … check=1`.
- `kt56-qemu-user-smp2.log`: `# u-mode exit: code=42 faultkind=Exit{42}`;
  `# u-mode fault: scause=0xd … recover=1 faultkind=Vm{…Read, Permission}`;
  `# u-mode fault: scause=0xc … reason=Wx` + `PANIC: u-mode fault scause=0xc …`.
- `kt56-gate.log`, `kt56-budget.log`, `kt56-boot50.log`, `kt56-host-fault.log`: xem §4.

## 4. Ngân sách và host test

- Host `vifast-plat-common`: **11 suite/110 test** (thêm 6 `fault` + 1 `device …` so KT-55; log
  `kt56-host-fault.log`).
- `vifast-abi`: test `fault_kind_du_bon_bien_the_dung_spec` xanh (FAULT_KINDS khớp spec).
- Budget `ktest.sh` in từng ảnh (`kt56-budget.log`): 262.144 B trần, không nới.

## 5. Phạm vi `spec/` và ghi chú

- `spec/` chỉ `abi.toml`, **1 hunk**; `spec/kernel.toml` không đổi.
- `docs/abi.md` là **golden sinh từ spec** (target của `vifast-gen`); commit cùng để
  `gen-check` (chạy trong `gate --fast`) không đỏ. Đây là hệ quả bắt buộc của việc mở spec,
  không phải thay đổi tài liệu thiết kế; báo rõ cho lead.
- Không đụng `.scn` 22..26 (chỉ đổi **đường** sinh trace phía kernel), không đổi sim semantics.

## 6. Nợ/giới hạn

- Conformance ca 26 dùng scratch mapping (cùng PA/cờ, đúng VA); `reason` đọc từ **leaf thật**
  (`mmu::walk_sw`) sau khi cài scratch, không dựng PTE tay — xem docs §19.7.
- `mmu::map_scratch_4k` ghi **thẳng** PTE root kernel (bảng cấp 0 dự phòng), **bỏ qua** kế toán
  `mapped_w/mapped_x/mapped_device` của object layer; đây là **đường ghi bảng trang thứ hai**,
  test-only, gỡ ngay sau mỗi trap — ghi rõ ở docs §19.7.
- `UnknownSyscall` chỉ là biến thể ABI (procsrv K3.5), K3 không tạo từ phần cứng (D2-79).
- Fault S-mode probe được resume bởi cờ test; ảnh sản phẩm không có probe.
- `FaultKind.vspace` là nhãn ASID (không phải cap) — procsrv cần cap thật thì D2 mới.
- Chưa procsrv/TCB (K3.5/K3b), chưa soak (KT-57).

## 7. Review độc lập (`reviewer-kt56`, lead spawn) — APPROVE + 4 vá

Verdict: **APPROVE** — spec đúng 1 hunk; gen tất định 2 lần cùng sha256; `FaultKind` khớp K2
`tcb.rs`; fault từ trapframe thật (`scause/stval/spp`), `src=trap` chỉ là nhãn emitter; W^X atomic
+ ABI 4 + `mapped_device` toàn cục; cờ test `swap(0)` một lần; 9/9 boot + 43/3/7 TAP; gate 574;
phạm vi đúng D2-81. Vá trước commit:

| Mục | Vấn đề | Vá |
|---|---|---|
| **M1** (chặn) | docs overclaim "reason từ PTE phần cứng đọc"; code dựng `PteFacts` từ chính bộ cờ scratch ⇒ `reason` tautology | `probe_vm_fault` đọc **leaf thật** bằng `mmu::walk_sw(va)` (trước khi gỡ scratch), `# faultprobe` thêm `pte=0x…`; ca 26 thực đo `pte=0x208800c3` (V|R|A|D, không W/X) ⇒ `Permission` từ **entry thật**; docs §19.2/§19.7 sửa theo sự thật (bộ cờ scratch do test chọn, `scause/stval` độc lập) |
| **M2** (chặn) | trích `kt56-boot50.log` khi tệp chưa có | chạy lại **450 boot** (`ktest.sh 50 matrix`) trên **cây sau M1**, log thật `kt56-boot50.log` — không xoá DoD 50 boot/N |
| **L2** | nhánh `not(feature="ktest")` trả `UserException` cho **mọi** fault U-mode | dùng `classify_trap(..., pte=None)` cùng bảng spec (12/13/15 ⇒ `Vm`, còn lại `UserException`) |
| **L1/L3** | journal ghi sai 14 suite/104 test; `kt56-selftest.log` là log **bị bỏ chạy**; scratch bỏ qua kế toán object layer | sửa **11 suite/110 test**; chạy lại `selftest.sh` khi QEMU rảnh; docs §19.7 thêm mục 2 "đường ghi bảng trang thứ hai" (test-only, gỡ ngay) |

Sau vá: rebuild release + `ktest.sh 1 matrix` **9/9** (log `kt56-ktest-matrix.log` cập nhật),
trace compare **61/61** (`build=3e6ae37a278300df` — ảnh M1), gate lại 574 trước commit.

## 8. Kết quả cuối (sau vá review)

- `ktest.sh 50 matrix` (**450/450 boot sạch**, 50 × smp 1/2/4 × 3 ảnh; 0 `not ok`, 0 FAIL,
  0 TIMEOUT) — `JOURNAL/kt56-boot50.log`; ảnh M1 `build=3e6ae37a278300df`.
- `ktest.sh 1 matrix` sau M1: **9/9** (ảnh 43/3/7 TAP) — `JOURNAL/kt56-ktest-matrix.log`.
- `trace_compare`: **61/61 dòng, 5/5 ca**, `build=3e6ae37a278300df` —
  `JOURNAL/kt56-trace-compare.log`.
- `selftest.sh` **17/17** (chạy lại sau khi QEMU rảnh) — `JOURNAL/kt56-selftest.log`.
- `gate --fast`: **PASS 574 test/75 suite**, L1 1.000 seed — `JOURNAL/kt56-gate.log` (baseline 573;
  +1 test `vifast-abi::fault_kind_du_bon_bien_the_dung_spec`; kernel host 110 test chạy **ngoài**
  gate vì `v2/Cargo.toml` `exclude=["kernel"]` — `JOURNAL/kt56-host-fault.log`).
- `vifast-gen gen` 2 lần: sha256 `generated.rs`/`docs/abi.md` **không đổi** —
  `JOURNAL/kt56-gen-determinism.log` (D2-81).
- Budget: ktest data+bss **17.344 B**, user **11.752**, guard **2.680** (≤262.144; không nới) —
  `JOURNAL/kt56-budget.log`.
