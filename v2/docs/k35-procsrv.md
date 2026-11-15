# K3.5 — procsrv U-mode + shell tối thiểu (KT-65)

Trạng thái: **DONE (đề xuất)** — chờ reviewer + Agent 0 lật QUEUE. Tài liệu này mô tả **kiến trúc
và giao thức**; mọi số đo (TAP, gate, budget, soak) nằm trong `v2/agent/STATE.md` +
`v2/agent/JOURNAL/kt65-*`, không chép tay vào đây (QD §9 Q12).

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
| ghép ảnh test | `v2/kernel/vifast-kbin-riscv64/src/ktest_user.rs` | gọi demo :471, `fault_phase` :484 |

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
   **Quyết định cho KT-66** (Agent 0, chưa triển khai trong KT-65): tách ảnh demo thành ảnh thứ
   tư `vifast-kbin-ktest-proc` chạy script riêng, matrix giữ 9/9; **không** hạ stack bằng cách
   đoán. Xem `JOURNAL/kt65-procsrv.md` §7.
7. Chưa có `ReplyRecv`/`Call` trong giao thức; chỉ `NBSend`/`NBRecv`/`Yield`.
8. **Bằng chứng low-water trap stack** đã gỡ khỏi cây cuối (instrumentation); muốn giữ thường
   trực cần **canary thật** — đưa vào KT-66 cùng stack per-hart/arena.
9. Chưa có kiểm song song nhiều procsrv/shell; ảnh demo là một bộ ba cố định.

## 9. Cách chạy và đo

```bash
# một boot mỗi ảnh (ktest, ktest-guard, ktest-user) ở smp=1 — có TAP/evidence KT-65
bash v2/test/boot/ktest.sh 1 1
# matrix đầy đủ (3 ảnh × smp 1/2/4)
bash v2/test/boot/ktest.sh 1 matrix
# ngân sách ảnh
bash v2/test/boot/budget.sh v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-user ktest-user
# soak (không chạy song song build/gate)
bash v2/test/boot/soak.sh 1000 3
```

TAP/evidence của KT-65 nằm trong `v2/test/boot/ktest.sh` (manifest + regex `^…$`) và
`expected.txt` (`ktest-user 24 24`); log mẫu và số đo ở `v2/agent/JOURNAL/kt65-*` + STATE.
