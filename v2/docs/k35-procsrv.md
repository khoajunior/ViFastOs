# K3.5 — procsrv U-mode + shell tối thiểu (KT-65)

Trạng thái: **KT-65 DONE; KT-66 (cổng K3.5) bổ sung §9/§10** — chờ reviewer + Agent 0 lật QUEUE.
Tài liệu này mô tả **kiến trúc, giao thức và thiết kế cổng**; mọi số đo (TAP, gate, budget, soak,
L1) nằm trong `v2/agent/reports/K35-DOD.md` + `v2/agent/STATE.md` + `v2/agent/JOURNAL/kt66-*`,
không chép tay vào đây (QD §9 Q12).

Liên quan: QD §1 **Q-A** (fault `Exit` + `wait`), D2-25/D2-56a (`FaultKind::Exit`), D2-79 (syscall
14), D2-83 (core sở hữu TCB), D2-84/85 (pin `size_of::<Machine>()` + trần ảnh), D2-86..92,
**D2-93** (shim `vifast-plat-user` trong danh sách đóng D2-17, chỉ `ecall`).

## 1. Phạm vi

KT-65 là tầng 4 của K3.5: **lần đầu kernel chạy chương trình U-mode thật có vòng đời**:

1. **procsrv** — tiến trình U-mode Rust, ELF riêng nạp như `init`; nhận **fault chuyển tiếp** từ
   core qua endpoint, `Recv` một `FaultKind::Exit{code}` rồi (core đã) đưa TCB chết về `Inactive`;
   phục vụ **giao thức procsrv** `wait(pid)` / `kill(pid)` cho shell.
2. **shell tối thiểu** — tiến trình U-mode Rust, đọc dòng từ UART qua **message của kernel**
   (chưa có IRQ→user), chạy lệnh thật `help`, `echo <text>`, `ps`, `wait`, `kill`, `exit`.
3. **demo end-to-end trên QEMU**: boot → shell → `help`/`echo`/`ps` → con chạy rồi `Exit(42)` ⇒
   procsrv nhận `Exit`, `wait` trả `42`; thêm nhánh `kill`; TAP/evidence `# proc:` + `# shell:`.

**Không** thêm syscall, **không** procsrv trong kernel, **không** sửa `spec/`, **không** in bảng
`ps` giả: mọi trạng thái `ps` đọc từ `Machine` của core.

## 2. Thành phần và bản đồ mã

| Thành phần | Tệp | Điểm vào |
|---|---|---|
| procsrv (U-mode) | `v2/kernel/vifast-kbin-riscv64/init/procsrv_user.rs` | `_start` :133 |
| shell (U-mode) | `v2/kernel/vifast-kbin-riscv64/init/shell_user.rs` | `_start` :222; `run_line` :192 |
| con (U-mode) | `v2/kernel/vifast-kbin-riscv64/init/child_user.rs` | `_start` :14 |
| shim syscall an toàn | `v2/kernel/vifast-plat-user/lib.rs` | `syscall7` :28, `put` :73, `exit` :89 |
| build 3 ELF + shim | `v2/kernel/vifast-kbin-riscv64/build.rs` | `build_user_lib` :69, `build_user_elf` :105 |
| scheduler/rig kernel | `v2/kernel/vifast-kbin-riscv64/src/proc_demo.rs` | `build_rig` :156, `service` :267, `run` :408 |
| đa nhiệm U-mode (arch) | `v2/kernel/vifast-plat-riscv64/src/user.rs` | `run_task` :347, `on_u_exception` :846 |
| stack riêng dispatch syscall | cùng tệp | mục KT-65 :761, `dispatch_on_private_stack` :809 |
| ghép ảnh test (syscall/core) | `v2/kernel/vifast-kbin-riscv64/src/ktest_user.rs` | TAP 1..14, kết thúc sạch |
| **ảnh demo riêng** (KT-66) | `v2/kernel/vifast-kbin-riscv64/src/ktest_proc.rs` | `rust_main`, `fault_phase` (TAP 1..10) |
| stack syscall **per-hart** (KT-66) | `v2/kernel/vifast-plat-riscv64/src/user.rs` | mục KT-66, `init_syscall_stacks`, `print_stack_evidence` |

Vì sao có `vifast-plat-user`: `core::arch::asm!` là `unsafe`, mà luật **R5** (D2-17) chỉ cho
`unsafe` trong danh sách đóng — `kernel/vifast-plat-*` nằm trong danh sách; **D2-93** xác nhận
đích danh shim này. Toàn bộ `ecall` gom vào **một** crate nền tảng đó (không phải thành viên
workspace; `build.rs` biên dịch `rlib` cho riscv64 rồi `--extern` cho từng ELF). Shim **chỉ**
phát `ecall` + đọc thanh ghi trả về; **không** CSR đặc quyền, **không** `sfence`, **không** truy
cập vật lý — lệnh đặc quyền mới sẽ cần D2 mới. Ba chương trình U-mode còn lại **0 `unsafe`**.

## 3. Giao thức procsrv (không phải syscall mới)

Cspace của ảnh demo do kernel dựng (`build_rig`) gồm các cap endpoint cố định; user chỉ dùng
`NBRecv`/`NBSend`/`Yield`/`DebugPutStr`/`Exit` của bảng syscall hiện có:

| Kênh | Hướng | Label | MR |
|---|---|---|---|
| `EP_FAULT` (1) | kernel → procsrv | `FAULT_EXIT = 0x608` | MR0 = pid, MR1 = `exit_code` |
| `EP_PROC` (2) | shell → procsrv | `WAIT_REQ = 0x604`, `KILL_REQ = 0x605` | MR0 = pid |
| `EP_REPLY` (4) | procsrv → shell | `REPLY_WAIT = 0x606`, `REPLY_KILL = 0x607` | MR0 = pid, MR1 = code / `0` |
| `EP_EVENTS` (8) | user → kernel | `EV_UP/EV_FAULT/EV_WAIT/EV_KILL` | sự kiện cho TAP/evidence |
| `EP_PS` (5) | shell → kernel | `PS_REQ = 0x602` | — |
| `EP_REPLY` (4) | kernel → shell | `PS_REPLY = 0x603` | 2 word đóng gói 4 mục: `entry = (pid<<5) | (state<<1) | suspended` |

Luồng `Exit` (QD §1 Q-A):

1. Con gọi syscall 14 `Exit(42)`. Core `exit()` chuyển TCB sang
   `BlockedOnSend{fault: Exit{42}}` tới `fault_ep` (hoặc `Inactive` nếu không có `fault_ep`).
2. Kernel demo thấy `BlockedOnSend`, đọc `tcb_last_fault(procsrv)` **ngay** sau lượt `NBRecv`
   (vì `reset_ipc_error` xoá `last_fault` ở đầu **mọi** lệnh IPC kế — D2-25/D2-56a), rồi gửi
   message `FAULT_EXIT` tới `EP_FAULT`.
3. procsrv `NBRecv(EP_FAULT)` in `# proc: recv exit pid=… code=…` (không in trạng thái — TAP16
   đọc `Machine`), ghi bảng
   `(pid, code)` bằng `AtomicU64` (không `static mut`), gửi `EV_FAULT`, rồi **`Yield` ngay** để
   kernel kịp đọc `last_fault` trước khi procsrv chạm lệnh IPC kế.
4. `wait(pid)`: procsrv tra bảng, trả `REPLY_WAIT` với code (hoặc `0xFFFF` nếu chưa có). Đây là
   **giao thức procsrv**, không phải syscall.
5. `kill(pid)`: kernel nhận `EV_KILL`, gọi `Tcb.Suspend(Abort)` trên TCB đích (đường có sẵn,
   D2-83), procsrv trả `REPLY_KILL` code `0`; TAP kiểm cờ `suspended` đọc từ `Machine`.

`kill` hiện là **treo TCB** (Suspend/Abort + `suspended` do kernel kiểm), không phải thu hồi toàn
bộ tài nguyên của tiến trình. Nợ: procsrv trả lời **trước** khi kernel xác nhận (reply nằm ở
procsrv), nên `kill <pid sai>` vẫn có thể in `code=0` dù kernel không treo ai; muốn chặn phải
thêm lượt ACK kernel→procsrv hoặc để kernel làm người phục vụ kill — xem §8.4/§8.9.

## 4. Shell, UART và `ps`

- Chưa có đường **IRQ UART → user**. Phương án tối thiểu đúng đề bài: kernel đọc/đóng gói **từng
  dòng** trong kịch bản demo thành message `LINE = 0x601` (MR0..MR2 = byte little-endian, MR3 =
  độ dài) gửi `EP_UART`; shell `NBRecv` và chạy lệnh. Đây là **kênh message thật**, không phải
  shell giả trong kernel; khi có IRQ→user chỉ thay nguồn sinh message.
- Lệnh **thật** trong shell: `help` (in trợ giúp), `echo <text>`, `ps` (gửi `PS_REQ`, kernel đọc
  `Machine` rồi trả 4 mục đóng gói; shell tự định dạng/in), `wait <pid>`/`kill <pid>` (giao thức
  procsrv), `exit` (`Exit(0)`).
- Bằng chứng `ps` không phải bảng tĩnh: kernel đọc `Machine` tại `PS_REQ`, gửi reply đóng gói và
  **lưu snapshot** `(pid, state, suspended)`; shell tự định dạng/in. TAP 18 so **từng dòng**
  `pid=… state=…` trong output shell với snapshot đó (không chỉ đếm `n=4`); ngoài ra kernel đếm
  `ps_replies` (số reply đã gửi). Số `entries` trong log là `EV_PS` do **shell** báo, không phải
  kernel đếm từ reply.
- Mỗi tiến trình U-mode được **capture** output (`user::capture_begin` :206) để TAP/evidence
  kiểm chuỗi `# proc:`/`# shell:` do chính chương trình U-mode phát ra, không chỉ tin UART.

## 5. Scheduler demo (kernel)

`proc_demo::run` (:408) là scheduler **tuần tự** của ảnh test: vòng round-robin
`[procsrv, shell, con]`, mỗi lượt `task_running` → `user::run_task` → `service` (drain `EP_EVENTS`,
chuyển fault, trả `PS_REPLY`, bơm dòng UART kế) → `task_ready`. State TCB đi qua đúng một cửa
`transition_tcb`; danh sách TCB/trạng thái cho `ps` đọc từ `Machine` (không hardcode). Tổng số
bước có biên; hết biên ⇒ TAP đỏ, không treo.

Vì sao kernel làm scheduler: K3.5 **chưa** có scheduler U-mode preemptive trong core; làm theo
cách này giữ đúng ràng buộc "không syscall mới, không procsrv-in-kernel". Đây là **nợ kiến trúc**
đã ghi ở §8, không phải thiết kế đích.

## 6. Sự cố đã tìm và sửa: tràn trap stack 16 KiB

- Hiện tượng: sau lượt U-mode **thứ hai** (shell nhận message `LINE`), biến cục bộ của scheduler
  bị **zero** (`LoadedInit.root/asid`, `steps`, `tcb_index`), hệ quả sai lịch và cuối cùng `ret`
  về 0. Thêm canary/`sp` cho thấy **không** phải tràn kernel stack thường.
- Đo nước thấp trap stack (sơn `0xAA` vùng `[__trap_stack_bottom, __trap_stack_top)`) sau lượt
  shell cho kết quả **chạm đáy** trap stack ⇒ đường dispatch IPC trong trap handler dùng **quá**
  16 KiB (bất biến KT-49) và ghi lấn xuống kernel stack. Log đỏ-trước (LoadedInit bị zero,
  low-water = đáy, PANIC `sepc=0x0`) ở `JOURNAL/kt65-red-before-trapstack.log`; log sau sửa
  (low-water trên đáy, 24/24 TAP) ở `JOURNAL/kt65-after-trapstack.log`.
- Sửa: chuyển **toàn bộ dispatch syscall U-mode** sang **stack riêng 32 KiB** trong `.bss`
  (`user.rs` :761..:844), đổi `sp` bằng một asm block có lưu/khôi phục `sp`+`ra`, rồi gọi
  `abi::dispatch` như cũ. Trap stack giữ nguyên 16 KiB; low-water sau sửa nằm **trên** đáy.
- Hệ quả ngân sách: ảnh test `user` tăng `.bss` 32 KiB; vẫn dưới trần 262.144 B (số đo ở STATE/
  budget log). `size_of::<Machine>()` **không đổi** (chỉ thêm static của arch runtime).

## 7. Kết thúc boot và continuation

Sau TAP 21, `proc_demo::run` **không trả về** khung gọi mà gọi `fault_phase` (ktest_user.rs :484)
— continuation chạy TAP 22..24 (fault U-mode của KT-56) rồi kết thúc bằng
`PANIC: u-mode fault … reason=Wx` đúng chính sách fatal của K3. Lý do: khi trả về qua nhiều lượt
`user_enter`, khung `rust_main` từng nhảy `ra=0`; continuation giữ nguyên toàn bộ coverage cũ
(TAP 22..24 vẫn chạy thật) mà không phụ thuộc đường trả về đó. Đây là **biện pháp kiến trúc đã
ghi nợ**, không phải bằng chứng đường trả về đã đúng — xem §8.

## 8. Giới hạn / nợ (trung thực)

1. **Scheduler tuần tự trong kernel** cho ảnh demo, không phải scheduler U-mode preemptive của
   core; chưa có `Yield`/timer-driven preemption. Nợ: đưa vòng lặp này vào core/scheduler thật.
2. **Chưa có IRQ UART → user**: dòng vào do kernel bơm từ kịch bản. Nợ: PLIC UART IRQ + hàng đợi
   byte → message, giữ nguyên giao thức `LINE`.
3. **`wait` không chặn** ở procsrv: nếu pid chưa thoát trả `0xFFFF`; shell in ra. Nợ: `wait` chặn
   thật (Reply/Notification) khi có scheduler.
4. **`kill` = `Tcb.Suspend(Abort)`**, không thu hồi vspace/frame/cap của tiến trình; chưa có
   `finalise`/`Revoke cap TCB` đầy đủ cho tiến trình bị kill (QD Q-A). Nợ: đường teardown + trả
   tài nguyên; đồng thời `kill <pid sai>` chưa bị từ chối vì reply do procsrv trả trước khi kernel
   `Suspend` (TAP 20 chỉ chứng minh pid đúng bị treo thật).
5. **Đường trả về `rust_main` sau nhiều lượt `user_enter`** chưa được chứng minh; demo tránh nó
   bằng continuation. Nợ: điều tra `ra=0` (nghi vấn khung gọi/stack) và bỏ continuation.
6. **Headroom ảnh test mỏng** (239.216/262.144 B): 3 ELF + capture + stack dispatch 32 KiB.
   **ĐÃ GIẢI Ở KT-66** — tách ảnh demo thành ảnh thứ tư `vifast-kbin-ktest-proc` (§9); số headroom
   trả về đo trong `K35-DOD.md` §7, không chép tay ở đây.
7. Chưa có `ReplyRecv`/`Call` trong giao thức; chỉ `NBSend`/`NBRecv`/`Yield`.
8. **Bằng chứng low-water trap stack** đã gỡ khỏi cây cuối (instrumentation); muốn giữ thường
   trực cần **canary thật** — **ĐÃ LÀM Ở KT-66**: canary + nước thấp cho **stack syscall per-hart**
   (§9.4); low-water của trap stack 16 KiB vẫn là instrumentation khi điều tra, không thường trực.
9. Chưa có kiểm song song nhiều procsrv/shell; ảnh demo là một bộ ba cố định.

## 9. KT-66 — tách ảnh demo + stack syscall per-hart (canary)

### 9.1 Ảnh thứ tư `vifast-kbin-ktest-proc`

- Bin **mới** `src/ktest_proc.rs` (`[[bin]] vifast-kbin-ktest-proc`, required-features
  `ktest,user`) — **cùng** crate/feature với ảnh user, linker script dùng chung.
- Ảnh giữ **đúng mười ca cũ 15..24** của `ktest-user`, **đánh số lại 1..10** vì TAP của một ảnh
  phải liên tục `1..N`; tên TAP giữ nguyên. Ánh xạ là **thiết kế** (không phải số đo):

  | Cũ | Mới | Ca |
  |---:|---:|---|
  | 15 | 1 | procsrv/shell/con load u-mode |
  | 16 | 2 | procsrv recv fault exit |
  | 17 | 3 | shell help echo |
  | 18 | 4 | shell ps tu core |
  | 19 | 5 | shell wait ma thoat |
  | 20 | 6 | procsrv kill treo tcb |
  | 21 | 7 | shell exit va procsrv finalise |
  | 22 | 8 | u-mode kernel read fault |
  | 23 | 9 | faultkind exit |
  | 24 | 10 | faultkind wx from u |

- `ktest-user` còn **1..14** và kết thúc boot **sạch** bằng `PANIC: KT-66 u-mode pha syscall xong
  (14 TAP, 0 fail)` + `DebugHalt` (vẫn đúng **một** dòng PANIC theo mẫu runner — không phải fault).
- Ảnh demo chạy **cùng** mười ca đó với **script riêng**: `ktest_proc.rs` dựng rig tối thiểu cho
  `init`, chạy `init` **một lượt** (nguồn `exit_kind_ok` + `LoadedInit` cho fault phase), rồi
  `proc_demo::run` (TAP 1..7) và continuation `fault_phase` (TAP 8..10, PANIC `u-mode fault`).
- Matrix đầy đủ vì vậy là **4 ảnh × smp 1/2/4 = 12 boot** (trước KT-66: 3 × 3 = 9); runner kiểm
  **mọi slot** (`LABEL_COUNT`, `want_rows`), không bỏ ảnh nào. `expected.txt` thêm dòng
  `ktest-proc 10 10`, `ktest-user 14 14`; manifest/evidence/panic-pattern của runner đổi **cùng
  lúc** với kernel (luật KT-54).

### 9.2 Vì sao tách

Ảnh `ktest-user` trước KT-66 phải mang: `init` ELF + **3 ELF demo** (procsrv/shell/con) +
`proc_demo` + stack dispatch 32 KiB trong `.bss` ⇒ sát trần `262.144 B`. Tách theo trách nhiệm:
ảnh **syscall/core** (14 TAP, `init` + `Machine`) và ảnh **demo** (10 TAP, U-mode đa nhiệm).
Số headroom trả về và trần ảnh đo ở `K35-DOD.md` §7 (D2-72 Q5), không chép tay ở đây.

### 9.3 Stack dispatch syscall **per-hart**

- Thay `static SYSCALL_STACK` 32 KiB (KT-65, **một** stack dùng chung) bằng **một block 32 KiB
  cho mỗi hart online**, nằm trong **vùng NOLOAD per-hart** (`__hart_area_base`), record mới:
  `guard 4 KiB | kernel stack 64 KiB | trap stack 16 KiB | **syscall stack 32 KiB**`
  ⇒ `HART_STRIDE = 0x1_D000`, `HART_AREA_BYTES = 0xE_8000` (8 hart). Hằng số nằm ở
  `vifast_plat_common::smp` + `linker.ld` + macro `trap_entry` (dịch/cộng, không cần M) và được
  `mmu::init` **kiểm khớp linker** (panic nếu lệch).
- **Vì sao không lấy Frame từ arena:** `Frame` của object layer nằm trong cửa sổ PA
  `0x8200_0000`, chỉ chạm được qua physmap/nửa cao; `copy_kernel_low_root` **không** chép root
  index ≥ 256 vào root VSpace user, mà trap từ U-mode chạy với `satp` = root user ⇒ dispatch trên
  stack ở arena **store page fault thật** (`stval=0xffffffc082007ff8`). Vùng NOLOAD per-hart thì
  identity-mapped **thấp** (root index 2, có trong root user) nên dùng được đúng chỗ. Đây là
  **phát hiện đo được** (PANIC `stval=0xffffffc082007ff8` trong phiên; log thô bị ghi đè khi
  chuyển sang NOLOAD — bằng chứng kiến trúc `# mmu layout: … none_physmap=1`), ghi ở
  `K35-DOD.md` §9 và `JOURNAL/kt66-nhat-ky.md`.
- Chỉ `static` còn lại: bảng mô tả `base/top/canary_bad` (`[u64; MAX_HARTS]`, ~200 B) + scratch
  dispatch một lượt; **không** còn 32 KiB trong `.data+.bss`. NOLOAD báo **riêng** ở `budget.sh`
  (D2-72 Q5) — `syscall_stack=`, `hart_stacks=`, `noload_hart=`.

### 9.4 Canary + nước thấp (`low`)

- Mỗi block: hai word canary ở **đáy** (`STACK_CANARY` + `!STACK_CANARY`) và **sơn** `0xAA` toàn
  block 32 KiB lúc cấp. `low` = word đầu tiên khác mẫu sơn từ đáy lên ⇒ nước thấp **thật** của
  lần chạy; chưa dùng ⇒ `low = top`.
- Canary kiểm **sau mỗi lượt dispatch** và một lần nữa khi in evidence; tràn/underflow ghi đè
  canary ⇒ in `# stack: canary=1 hart=K` rồi **PANIC + halt ngay** (fail-closed — không chạy tiếp
  trên trạng thái hỏng; vá reviewer KT-66 M1), và evidence `canary=0` của runner đỏ.
- Bằng chứng (một dòng tổng + một dòng mỗi hart online; thay cho `:761` của KT-65):
  `# stack: noload=1 block=32768 harts=N distinct=1 canary=0 base=0x…`
  `# stack hart=K base=0x… top=0x… low=0x… used=… canary=…`
  `distinct=1` ⇔ **mọi cặp** hart có `base` khác nhau (smp 2/4 **không** dùng chung stack);
  runner kiểm cả **tập** hart 0..N-1 + `low ∈ [base, top]` + `top-base ≥ 32 KiB` bằng
  `check_stack_lines` (không chỉ đếm). Harness **fail-closed**: parse portable (bash `16#…`, không
  `strtonum`), thiếu dòng/hex hỏng ⇒ **ĐỎ**; tự kiểm bằng `ktest.sh --selftest-stack` (5 ca).
- **Phạm vi đo:** smp 2/4 **đã cấp** block riêng cho từng hart (`distinct=1`), nhưng scheduler demo
  chỉ chạy hart 0 nên **hart 1..3 chưa từng dispatch** (`used=0`) — nối IPI multi-hart là nợ #6
  (`K35-DOD.md` §12.2), không phải "đã chạy song song".

### 9.5 Phạm vi đỏ-trước của KT-66

Ba log đỏ-trước (mục D9 trong `K35-DOD.md`): **ảnh tách** (log 24 TAP cũ vs `expected.txt` mới 14
⇒ plan/ok-count đỏ), **stack dùng chung** (mọi hart trỏ block hart 0 ⇒ `distinct=0`, runner đỏ ở
`stack-distinct`), **canary** (ghi đè canary ⇒ `canary=1`, evidence đỏ). Không retry (D2-07).

## 10. Cách chạy và đo cổng K3.5

```bash
# một boot mỗi ảnh (ktest, ktest-guard, ktest-user, ktest-proc) ở smp=1
bash v2/test/boot/ktest.sh 1 1
# matrix đầy đủ (4 ảnh × smp 1/2/4 = 12 boot)
bash v2/test/boot/ktest.sh 1 matrix
# ngân sách **mỗi** ảnh (gồm ảnh demo) + NOLOAD báo riêng
bash v2/test/boot/budget.sh v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-proc ktest-proc
# conformance sim ↔ QEMU (dùng log matrix ở trên)
KT55_LOGDIR=/tmp/k3-kt66-matrix bash v2/test/conformance/run.sh --compare --smp 2
# soak ảnh sản phẩm (KHÔNG icount; không chạy song song build/gate)
bash v2/test/boot/soak.sh 1000 3
# L1 cổng (100k seed × smp 1/2/4/8) — chạy **một mình**, xem K35-DOD §6 về cách chia tiến trình
VIFAST_SEEDS=100000 VIFAST_SMP=N cargo test --manifest-path v2/Cargo.toml -p vifast-sim driver_smp -- --nocapture
# cổng bắt buộc cuối task
./v2/bin/v2 check && ./v2/bin/v2 gate --fast
```

TAP/evidence của KT-65/KT-66 nằm trong `v2/test/boot/ktest.sh` (manifest + regex `^…$`) và
`expected.txt` (`ktest 60`, `ktest-guard 3`, `ktest-user 14`, `ktest-proc 10`); số đo cổng ở
`v2/agent/reports/K35-DOD.md`, log thô ở `v2/agent/JOURNAL/kt66-*` + `STATE.md`.
