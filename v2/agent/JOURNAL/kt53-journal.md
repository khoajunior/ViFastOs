# KT-53 — U-mode + 3 syscall + nạp `init` ELF (K3)

- **Ngày:** 2026-11-15
- **Task:** KT-53 (QUEUE v2) — `init` U-mode 3 syscall (`DebugPutStr` 12 / `DebugHalt` 13 /
  `Exit` 14 theo **QD §1 Q-A**), nạp ELF bằng `vifast-elf`, map **W^X**, trapframe U-mode + `sret`,
  cách ly U-mode ↔ kernel; **không** procsrv/shell (K3.5), **không** CNode/TCB/IPC (K3b), **không**
  `.scn` (KT-55), **không** `FaultKind` đầy đủ (KT-56).
- **HEAD vào:** `5927063b` ([KT-00] Agent 0 mở KT-53; code KT-52 `320daa59`) — nhánh `main`,
  `HEAD..origin/main = 0`; working tree có sửa đổi `viscript/` **từ tab khác** (không chạm; chỉ
  stage `v2/`).
- **Trạng thái:** **DONE** — reviewer-kt53 **DRIFT** (không UNSAFE) → đã vá 3 mục chặn (D2-79,
  bằng chứng/log, đủ 50 boot mỗi N) + 2 bằng chứng thêm (`sstatus` hai đầu, guard invalid) →
  commit `[KT-53]` + `[KT-53b]` (hash/HEAD ở `STATE.md`).

## 1. Baseline + đỏ trước

- Baseline `./v2/bin/v2 gate --fast` ⇒ PASS, **569** test / 73 suite, L1 1.000 seed
  (`kt53-gate-baseline.log`).
- Baseline `bash v2/test/boot/ktest.sh 1 1` ⇒ **37/37** TAP ảnh chính + **3/3** ảnh guard, 2/2 boot
  sạch (`kt53-ktest-baseline.log`).
- **Đỏ trước (host)** `kt53-red-host.log`: thêm `tests/user.rs` (11 test ngữ nghĩa U-mode/syscall)
  **trước khi** có `vifast-plat-common::user` ⇒ `unresolved import vifast_plat_common::user`
  (EXIT=101).
- **Đỏ trước (QEMU)** `kt53-red-ktest.log`: thêm `expected.txt` dòng `ktest-user 5 5` + runner đòi
  ảnh thứ ba + bin `vifast-kbin-ktest-user` **trước khi** có `src/ktest_user.rs` ⇒
  `can't find bin vifast-kbin-ktest-user at path …/src/ktest_user.rs`, `build ảnh test FAIL`
  (EXIT=1).

## 2. Thiết kế đã làm

| Tệp | Nội dung |
|---|---|
| `v2/kernel/vifast-plat-common/src/user.rs` | **Ngữ nghĩa** U-mode/syscall, `no_std`, **không `unsafe`**, host-testable: 3 số hiệu syscall + dải `0..=15`, bảng **19 mã lỗi** (khớp `spec/abi.toml`), `UserRange` (dải user), `UserTask` (state "TCB-like" + bộ đếm atomics), `UserMem` trait, `dispatch()` trả `Return/Exit/Halt` — syscall trong dải chưa cài ⇒ `NotSupported` (18), ngoài dải ⇒ `InvalidArgument` (5), **không panic** |
| `v2/kernel/vifast-plat-common/tests/user.rs` | 11 test host: kênh `a0/a1`, `Exit` lưu mã/trạng thái, `DebugHalt`, syscall lạ trong/ngoài dải, trần `DebugPutStr` 4096, lỗi từ lớp đọc user được truyền, 19 tên mã lỗi, `UserRange` (tràn số), fault state, `begin_run`/`record_ecall` |
| `v2/kernel/vifast-plat-riscv64/src/user.rs` | Loader `init` ELF (`vifast-elf::parse`) + map segment **theo quyền** (`U|R|X`/`U|R|NX`/`U|R|W|NX`), stack user 8 trang + guard hở, đi lại PTE đếm `wx_pages`/`flags_ok`, `check_objmem`; trapframe U-mode + `user_enter`/`user_exit_resume` (asm lưu/khôi phục ngữ cảnh kernel); `on_u_exception` (syscall + fault); `UserMemAccess` (kiểm dải + leaf `U|R`, đọc bằng `sstatus.SUM`); feature **`user`** (kéo `ktest`) — chỉ ảnh test user |
| `v2/kernel/vifast-plat-riscv64/src/lib.rs` | `#[cfg(feature = "user")] pub mod user;` + nhánh `trap_dispatch`: `Event::Exception` và `!exc.from_supervisor` ⇒ `user::on_u_exception` **trước** guard/S-mode (fault U-mode không rơi vào probe guard) |
| `v2/kernel/vifast-plat-riscv64/src/obj.rs` | `set_sum` ⇒ `pub(crate)` (dùng lại đúng cơ chế TAP 35 cho syscall `DebugPutStr`; `unsafe` vẫn chỉ trong crate arch) |
| `v2/kernel/vifast-plat-common/src/trap.rs` | `report_u_fault` — dump `PANIC: u-mode fault scause/stval/sepc/sstatus/from_u=1` + thanh ghi rồi `DebugHalt(1)` |
| `v2/kernel/vifast-kbin-riscv64/init/{init_user.rs,init_user.S,init.ld}` | Chương trình `init` user: marker `U: init hello from u-mode`, ghi/đọc `.data`, 2 syscall lạ (EID SBI + 99) kiểm mã **19 `RangeError`** (D2-79), marker `U: bad syscall err=19 ok`, `Exit(42)`; nhánh `a0=1` đọc `0x80200000` để test cách ly. Chỉ dùng thanh ghi caller-saved (không đụng s0..s11/gp/tp) |
| `v2/kernel/vifast-kbin-riscv64/build.rs` | Khi feature `user`: gọi **`$RUSTC`** (cargo cấp) biên dịch `init/init_user.rs` (+`.S`, `-T init.ld`, `--gc-sections`, `relocation-model=static`) thành `$OUT_DIR/init.elf`; không cần toolchain ngoài Rust, không sinh file trong cây nguồn |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_user.rs` | Ảnh test **user** riêng (5 TAP): nạp ELF, `sret` U-mode, 3 syscall + syscall lạ, `Exit(42)`, rồi lượt đọc địa chỉ kernel ⇒ `scause=13` + `PANIC: u-mode fault` (boot kết thúc có chủ đích) |
| `v2/test/boot/{ktest.sh,expected.txt}` | 3 ảnh test; `ktest-user 5 5`; build ảnh user riêng (`--features ktest,user`); budget + chuỗi `ktest`; grep bằng chứng từng dòng (`# init load`, `# init run`, `# user put_str`, `# user exit`, `# user badsys`, marker user, `# u-mode read`, `# u-mode fault`, `PANIC: u-mode fault`); cho phép **đúng** dòng PANIC đó; summary `N_BOOT*3` |
| `v2/docs/k3-riscv64.md` | §16 (phạm vi, nạp ELF, cơ chế `sscratch` + lý do, 3 syscall/lỗi ABI, cách ly/fault, TAP 1..5, budget, giới hạn, không thuộc KT-53) + §1/§2/§4/§5 cập nhật **bốn** ảnh |

### 2.1 Vì sao ảnh test **user** riêng

Fault U-mode là **fatal** (log + `DebugPanic` + halt) theo yêu cầu cách ly, nên test fault phải là
lần kết thúc boot — giống lý do ảnh guard (KT-46). 4 TAP U-mode còn lại chạy trước đó trong cùng
boot. Module U-mode nằm sau feature `user` nên ảnh sản phẩm/ảnh chính/ảnh guard **không link** nó;
37 TAP ảnh chính + 3 TAP ảnh guard (và budget của chúng) **không đổi** — không phải renumber.

### 2.2 Nạp `init` ELF + W^X

- `init.elf` là ELF64 LE `ET_EXEC` `EM_RISCV` với **3 `PT_LOAD`**: `.text` `R|X`, `.rodata` `R`,
  `.data` `R|W`; entry `0x40000000`; không dynamic/interp/TLS.
- `vifast-elf::parse` kiểm header/segment/`phoff`/`filesz`/`vaddr ≡ p_offset (mod 4 KiB)`/cửa sổ
  user `[0x4000_0000, 0x8000_0000)`/chồng lấn/entry; loader **kiểm lại** `W+X`, tự kiểm lại biên
  `bytes[file_start..file_start+copy_len]`, rồi map đúng quyền; object layer (D2-67) chặn W^X toàn
  cục theo Frame.
- **Root VSpace user chỉ chép nửa thấp root kernel** (`copy_kernel_low_root`: index `0..256` trừ
  index 1 cửa sổ user) — **không** chép nửa cao (kernel high alias/physmap), nên U-mode không có
  đường PTE tới nửa cao (bằng chứng `# init load: … high=1`, walk high alias + physmap invalid).
- Sau map, loader **đi lại PTE thật** (`walk`) đếm `wx_pages` (leaf `W∧X`) và `flags_ok`
  (leaf khớp quyền segment + `U`), in `# init load: … wx=0 flags=1 … check=1` — không dùng cờ tự
  khai.
- Stack user 8 trang `0x4000_8000..0x4001_0000` `U|R|W|NX`; guard `0x4000_7000` **không map**;
  ảnh ELF phải kết thúc trước guard.
- Frame/PageTable/VSpace lấy từ Untyped `U0` KT-52; không thêm arena/bảng mới.

### 2.3 Trapframe U-mode, `sret`, `sscratch` (cơ chế đã chọn + lý do)

**Chọn: `sscratch` luôn giữ đỉnh trap stack per-hart** (KT-45/KT-49), không đổi khi xuống U-mode:

- `sret` U-mode: `sstatus.SPP=0`, `sepc=entry`, `sp=stack user`; `sscratch` vẫn là đỉnh trap stack.
  U-mode không đọc/ghi được CSR supervisor nên không thấy con trỏ kernel; `tp=0`/`gp=0` (không lộ
  hart id/con trỏ kernel), trap handler U-mode **không** gọi `hart_id()`/PerHart — chỉ
  `TRAP_STATS`/`USER_TASK` toàn cục (tránh phụ thuộc `tp` user).
- Trap từ U-mode: `trap_entry` `csrrw sp, sscratch, sp` ⇒ `sp` = trap stack top, `sscratch` = sp
  user; frame lưu `x2` = sp user. Một CSR vừa đổi stack vừa giữ con trỏ kernel; trap từ S-mode
  dùng cùng đường (`sscratch` vẫn là đỉnh trap stack).
- `Exit` ⇒ về S-mode: handler đặt `sstatus.SPP=1` (asm resume của `trap_entry` **không** ghi
  `sstatus` — P2a KT-45), `sepc=user_exit_resume`, asm resume `sret`; `user_exit_resume` khôi phục
  `(ra, sp, gp, tp, s0..s11)` từ `USER_KERNEL_CTX` rồi `ret` về caller Rust — **không** quay lại
  U-mode.
- **Lý do không chọn `sscratch = con trỏ user`:** cần thêm nguồn lưu con trỏ kernel + phân nhánh
  trap S/U trong asm; trap từ S-mode vẫn phải có đỉnh trap stack. Cơ chế hiện tại ít trạng thái
  hơn, không lộ địa chỉ kernel, và `sstatus.SPP` đủ phân biệt nguồn trap.

### 2.4 3 syscall + mã lỗi

- `DebugPutStr` 12: kiểm dải user + **trọn một dải VA đã map** (bảng dải loader ghi lúc nạp: mỗi
  `PT_LOAD` + stack — không `walk` PTE vì root user **không** có physmap), trần 4096; đọc raw bằng
  `SUM` rồi **tắt ngay** (`sum_off=1`); `a0=0, a1=len`; lỗi dải ⇒ 5, ngoài dải đã map ⇒ 11.
- `DebugHalt` 13: log `# user debug_halt` rồi SBI SRST (test).
- `Exit` 14: `UserTask` ⇒ `Exited` + `exit_code`; trapframe về S-mode; **không** quay lại U-mode;
  chưa có TCB/`fault_ep` (K3.5).
- Trong dải `0..=15` chưa cài ⇒ 18; **ngoài dải** (99, EID SBI) ⇒ **19 `RangeError`** (D2-79) — **không panic**; U-mode không
  gọi được SBI vì `ecall` U-mode luôn `scause=8` vào dispatcher.

### 2.5 Cách ly + chính sách fault

- Root VSpace user chỉ chép **nửa thấp** root kernel (test-only KT-52 — index 0/2..255) để kernel
  sống khi `satp` trỏ root user; leaf kernel `U=0` và **nửa cao không map** ⇒ U-mode đọc
  `0x80200000` = **load page fault** `scause=13`, `stval=0x80200000`.
- Fault U-mode (mọi loại trừ `ecall`): ghi `USER_TASK` + `TRAP_STATS` (fatal), in
  `# u-mode fault: … from_u=1 kernel_addr=1 fatal=1`, rồi `PANIC: u-mode fault …` +
  `DebugHalt(1)` — **không treo, không resume** (không pager).

## 3. Đo / cổng (log thật trong `v2/agent/JOURNAL/`)

- `kt53-ktest.log`: matrix **1/2/4**, 1 boot mỗi cấu hình ⇒ **9/9** boot sạch: ảnh chính 37/37,
  guard 3/3, user 5/5.
- `kt53-host.log`: `cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-plat-common` ⇒
  **102** test PASS, trong đó `tests/user.rs` **11/11**.
- `kt53-product.log`: `run.sh 2 2` (sản phẩm, **không icount**) ⇒ **2/2** boot sạch.
- `kt53-budget.log`: 4 ảnh (`product`, `ktest`, `ktest-guard`, `ktest-user`) ≤ 262.144 B
  (`.data+.sdata+.bss+.sbss`), `.stack` báo riêng — PASS; số đo thật trong log (không chép vào
  tài liệu, QD §9 Q12). Ảnh sản phẩm/ktest/guard **không đổi** data+bss so KT-52 (U-mode chỉ nằm ở
  ảnh user).
- `kt53-image-sha.log`: sha256 4 ảnh trên **cây cuối sau revert mutation** (trước commit).
- `kt53-boot50-smp{1,2,4}.log`: **50 boot mỗi N=1/2/4** trên **cây cuối** (3 ảnh test/vòng; 3 job
  song song để vừa trần 3 QEMU) ⇒ **150/150 mỗi N, tổng 450/450 boot sạch**, 0 `not ok`
  (`# smp=N: 150/150`, `# KTEST: 150/150`).
- `kt53-boot50-tainted-midrun.log`: **KHÔNG TÍNH — T-INFRA**: lần chạy 50-boot đầu bị kill giữa
  lúc rebuild ảnh user (thêm `high=1`); binary/runner đổi giữa run ⇒ log chỉ là bằng chứng sự cố,
  **không** dùng làm DoD. `kt53-boot50-preD79-smp{1,2,4}.log` là bản 50/50 của cây **trước** vá
  D2-79/sstatus/guard (150/150 mỗi N) — cũng **không** tính cho cây cuối, giữ để đối chiếu.
- `kt53-gate.log`: `./v2/bin/v2 gate --fast` trên cây cuối sau vá/revert — **PASS 569 test/73
  suite**, L1 1.000 seed; baseline cùng số (`kt53-gate-baseline.log`).
- `kt53-mutation-khong-fault.log`: mutation âm tính cho TAP 5 (xem §5) — chạy trên cây cuối,
  revert + sha ảnh khôi phục trước gate/commit.

## 4. Review độc lập

- `reviewer-kt53` (lead spawn, read-only): **DRIFT** (không UNSAFE). Cách ly/W^X đạt: `ecall` U ⇒
  `scause=8`; U đọc `0x80200000` ⇒ `scause=13` + `stval` đúng + `PANIC … from_u=1` + halt; không
  map nửa cao (`U=0` leaf kernel); W^X 3 lớp + walk `wx=0`; `Exit` đúng QD §1 Q-A (không Reply);
  `build.rs` byte-identical 2 lần build; `gate --fast` PASS (cây trước vá).
- **Đã vá 3 mục chặn:**
  1. **D2-79** — số syscall ngoài `0..15` trả **19 `RangeError`** (không phải 5): dispatch
     `vifast-plat-common/src/user.rs`, `init/init_user.S`, TAP 4 + runner, docs §16.4, journal.
     Mã 5 giữ cho tham số sai **trong** syscall hợp lệ.
  2. **Bằng chứng**: journal chỉ ghi DONE khi cây cuối + gate xanh (hash commit ở STATE), không
     ghi khi còn đỏ; bỏ mọi dẫn log/mutation khi chưa tạo; host sửa **92 → 102** đúng log; boot50
     log cũ ghi rõ T-INFRA/không tính (không im lặng thay log).
  3. **Chờ đủ 50 boot `-smp 4`** trên cây cuối rồi mới ghi kết quả (3 job song song, tổng 450 boot).
- **Thêm 2 bằng chứng (đã làm):** (a) `sstatus` hai đầu — `enter` (ghi CSR lúc vào U) và `trap`
  (phần cứng sau `ecall` đầu) đều `SPP=0/SPIE=0/SIE=0/SUM=0/UXL=2`, in `# u sstatus:`;
  (b) TAP 1 khẳng định **leaf guard stack invalid** bằng `walk` (`guard_none=1`), kiểm cả
  `guard_unmapped` trong `LoadedInit`.
- Ghi nợ §16.8 (SUM một chiều, `sscratch` gián tiếp, cách ly 1 địa chỉ, U-phase `SIE=0` treo,
  `satp` sau Exit, `USER_RANGES`, store 15) — **không** vá ở KT-53.

## 5. Mutation / giới hạn (không giấu)

- **Mutation âm tính TAP 5 (đã chạy trên cây cuối):** đổi nhánh `a0=1` của `init` đọc địa chỉ
  **user** `0x40000000` thay vì kernel `0x80200000` ⇒ `init` đi tiếp `Exit(98)`, kernel in
  `ok 5 - u-mode kernel read fault` rồi `not ok 5 - u-mode kernel read fault: khong fault`;
  runner đỏ (`not ok - user smp=1 boot 1`, `KTEST: 2/3`) — log `kt53-mutation-khong-fault.log`.
  **Revert** `init_user.S` + rebuild ⇒ sha256 ảnh user **khôi phục đúng**
  (`e21f9ae6…`, `kt53-image-sha.log`), rồi mới chạy matrix/product/gate cuối.
- **Store U-mode `scause=15`** chỉ có cùng chính sách PTE `U=0`, **không** có TAP riêng (TAP 5 là
  read 13 theo tên test; ảnh guard đã đo store 15 từ S-mode) — ghi nợ §16.8.
- **Một tác vụ/1 hart (BSP)**: ảnh user không `smp::init`; chưa TCB/cap/`fault_ep`/`Exit` message
  (K3.5), chưa CNode/TCB/IPC/sched (K3b), chưa `.scn`/conformance (KT-54/55), chưa `FaultKind` đầy
  đủ + `spec/` (KT-56), chưa soak/K3-DOD (KT-57).
- **Loader test-only** lỗi giữa chừng không dọn object (đo `check_objmem` chỉ ở đường thành công);
  root VSpace vẫn chép root kernel (test-only, L6 KT-52); ASID VSpace nhãn chưa hợp nhất lease
  `tlb::AsidAllocator` (L4); `DebugGetInfo` (15) chưa cài; guard stack U-mode chưa có test tràn.
- **Không** sửa `spec/`/sim/`.scn`/QUEUE/DECISIONS/AGENTS/`viscript/`; `git diff spec/` rỗng;
  `unsafe` chỉ `kernel/vifast-plat-*` (D2-17); không `cfg(target_arch)`; không dependency ngoài
  allowlist (`vifast-elf` là crate nội bộ `no_std`, không dep).
