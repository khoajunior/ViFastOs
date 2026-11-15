# KT-65 — procsrv U-mode + shell tối thiểu (K3.5 tầng 4)

Ngày: 2026-11-16. Dòng OS, Agent 2. Trạng thái: **DONE (đề xuất) — chờ reviewer + Agent 0**.
Tài liệu kiến trúc: [`v2/docs/k35-procsrv.md`](../../docs/k35-procsrv.md).

## 1. Baseline trước task

- `gate --fast`: **647 test/83 suite**, L1 1.000 seed (STATE KT-64a).
- `ktest.sh 1 matrix`: **9/9** boot; TAP **60/60** (ktest), **3/3** (guard), **17/17** (user).
- `soak.sh 1000 3`: 0/1000 (STATE KT-64a).

## 2. Đỏ-trước

1. Chưa có procsrv/shell/con; `vifast-kbin-ktest-user` chỉ có `init` + 17 TAP, không có
   `# proc:`/`# shell:`; matrix vẫn 9/9 vì chưa có test mới.
2. Test sim mới `v2/sim/vifast-sim/tests/kt65_procsrv.rs` được viết trước phần kernel; chạy trên
   sim hiện có (đã có `last_fault`/`Suspend`) nên xanh — đây là **pin hợp đồng**, không phải
   red-before của tính năng kernel.
3. Đỏ thật của đường kernel xuất hiện khi ghép demo: **tràn trap stack 16 KiB** (mục 4).
   Log đỏ-trước giữ nguyên tại `JOURNAL/kt65-red-before-trapstack.log`: (A) sau lượt shell,
   `LoadedInit.root` bị zero và low-water trap stack = **0x80411000** (đáy); (B) hệ quả trả về
   `PANIC: trap InstructionPageFault scause=0xc sepc=0x0`. Log sau sửa:
   `JOURNAL/kt65-after-trapstack.log` (low-water **0x804148c0**, 24/24 ok). Lưu ý: `# trap low water`
là **instrumentation chẩn đoán đã gỡ** khỏi cây cuối (không reproducible trực tiếp); muốn giữ
bằng chứng thường trực cần **canary thật** — đưa vào KT-66 cùng stack per-hart/arena.

## 3. Đã làm

- **3 ELF U-mode Rust** (nạp bằng loader `init` sẵn có, mỗi tiến trình một VSpace/asid):
  `procsrv_user.rs` (fault Exit + giao thức `wait`/`kill`), `shell_user.rs` (`help`, `echo`, `ps`,
  `wait`, `kill`, `exit`), `child_user.rs` (`Exit(42)`).
- **Shim ABI U-mode** `v2/kernel/vifast-plat-user/lib.rs` (asm `ecall` + `put`/`exit`), build
  thành `rlib` riscv64 trong `build.rs` rồi `--extern` cho 3 ELF. Lý do: `asm!` là `unsafe`, luật
  R5/D2-17 chỉ cho `unsafe` trong `kernel/vifast-plat-*`; 3 chương trình còn lại **0 `unsafe`**.
- **Scheduler demo kernel** `src/proc_demo.rs`: rig core (CNode 64, 6 endpoint, 5 TCB/SC, cspace
  chung), vòng round-robin 3 tiến trình, `service()` drain sự kiện, chuyển fault `Exit` thành
  message IPC, trả `PS_REPLY` đóng gói từ `Machine`, bơm dòng UART theo kịch bản; TAP 15..21.
- **Đa nhiệm U-mode** trong `vifast-plat-riscv64/src/user.rs`: `Stop{Exit,Yield,Blocked,Halt}`,
  lưu/khôi phục trapframe, intercept `Yield`, `Exit` trả về scheduler, capture output user.
- **Ghép ảnh test**: TAP 15..21 (KT-65) + continuation `fault_phase` giữ nguyên TAP 22..24 của
  KT-56; plan `1..24`; evidence/threshold trong `ktest.sh`/`expected.txt`.
- **Test host**: 2 test sim (`kt65_procsrv.rs`) + 2 test core (`core_tcb.rs`) pin hợp đồng fault
  `Exit`/`last_fault`/`Suspend(Abort)`.
- **Docs**: `v2/docs/k35-procsrv.md` (kiến trúc, giao thức, nợ trung thực).

## 4. Lỗi/lỗi nền đã tìm và sửa

1. **Tràn trap stack 16 KiB** (lỗi nền thật): sau lượt U-mode thứ hai (shell nhận message `LINE`),
   biến cục bộ scheduler bị zero (`LoadedInit.root/asid`, `steps`, `tcb_index`) rồi `ret` về 0.
   Đo nước thấp trap stack (sơn `0xAA`) cho thấy handler chạm đáy ⇒ dispatch IPC trong trap dùng
   >16 KiB. **Sửa**: dispatch syscall U-mode chạy trên **stack riêng 32 KiB** trong `.bss`
   (`user.rs` mục KT-65), giữ nguyên trap stack 16 KiB của KT-49. Sau sửa, nước thấp nằm trên đáy.
2. **Không trả về `rust_main`**: sau nhiều lượt `user_enter`, khung gọi từng nhảy `ra=0`; demo
   không trả về mà gọi continuation `fault_phase` (TAP 22..24 rồi PANIC fatal của KT-56). Đây là
   biện pháp kiến trúc, **chưa** chứng minh đường trả về đúng — ghi nợ §8 tài liệu.
3. `static mut` trong chương trình U-mode ⇒ R5; đổi sang `AtomicU64`/`AtomicUsize`.
4. Clippy kernel: `needless_range_loop` (capture_begin) và `while_let_loop` (service) — đã sửa;
   `int_plus_one` `slotmem.rs:179` sửa 1 dòng theo yêu cầu lead (diff hành vi tương đương, ảnh
   **byte-identical** — soak log xác nhận cùng sha256). Phạm vi: `cargo clippy -p vifast-core
   --all-targets -- -D warnings` (host) sạch; với target no_std, `--all-targets` **không áp
   dụng** (thiếu crate `test` — lỗi môi trường, không phải lint), nên plat/kbin dùng clippy
   lib/bin + `-D warnings`; shim `vifast-plat-user` được rustc biên dịch với `-D warnings`.

## 5. Kết quả đo (log trong cùng thư mục)

| Hạng mục | Kết quả | Log |
|---|---|---|
| `gate --fast` | **PASS — 649 test/84 suite** (baseline 647 + 2 test sim), L1 1.000 seed 884 ms | `kt65-gate-fast.log` |
| `ktest.sh 1 matrix` | **9/9 boot sạch**; TAP **60/60 + 3/3 + 24/24** | `kt65-ktest-matrix.log` |
| QEMU mẫu `ktest-user` | **24/24 ok, 48 evidence**, kết thúc `PANIC: u-mode fault … reason=Wx` | `kt65-qemu.log` |
| `soak.sh 1000 3` | **0/1000 lỗi**, wall **225s**, `BUILD_ID=6290c3b0198d37ac`, image sha256 `f2ccaed7542c892d…` | `kt65-soak.log` |
| Budget | product **200.120**, ktest **209.856**, guard **2.680**, ktest-user **239.216** — tất cả ≤ trần **262.144 B** | `kt65-budget.log` |
| `size_of::<Machine>()` | **188.872 B** ≤ pin **196.608 B** (không đổi) | `kt65-qemu.log` dòng `# user core:` |
| Clippy kernel | core/plat/kbin `-D warnings` sạch; core `--all-targets` sạch sau khi sửa `int_plus_one` `slotmem.rs:179` | `kt65-clippy.log` |

Bằng chứng chuỗi thật (trích `kt65-qemu.log`):

```
# proc: load procsrv=[…] asid=2 … shell=[…] asid=3 … child=[…] asid=4 …
# proc: recv exit pid=26 code=42
# proc: wait pid=26 code=42
# shell: wait pid=26 code=42
# proc: kill pid=28 code=0
# shell: ps n=4
  pid=26 state=Inactive
# proc: exit recv code=42 state=Inactive faults=2 captured=1
# proc: sched steps=31 loads=3 events=8 faults=2 ps=4 invariant=1
ok 15 … ok 24
```

## 6. Tệp thay đổi

- `v2/kernel/vifast-kbin-riscv64/{build.rs, src/ktest_user.rs, src/proc_demo.rs,
  init/{procsrv_user.rs,shell_user.rs,child_user.rs}}`
- `v2/kernel/vifast-plat-riscv64/src/user.rs`
- `v2/kernel/vifast-plat-user/lib.rs` (mới)
- `v2/kernel/vifast-core/src/core_tcb.rs` (2 test)
- `v2/sim/vifast-sim/tests/kt65_procsrv.rs` (mới, 2 test)
- `v2/test/boot/{ktest.sh, expected.txt}`
- `v2/docs/k35-procsrv.md`, `v2/agent/{STATE.md, JOURNAL/kt65-*}`
- Không chạm `spec/`, `v1/`, `viscript/`, tài liệu thiết kế.

## 7. Vá theo reviewer-kt65 (DRIFT thấp, chặn commit)

Đã sửa: (1) bỏ token `state=Inactive` do procsrv tự in — TAP16 đọc `child_state` từ `Machine`;
(2) `help cmds=6` + TAP17/evidence; (3) doc nói đúng `entries` là `EV_PS` do shell báo;
(4) TAP18 lưu snapshot `(pid,state,suspended)` tại `PS_REPLY` và so **từng dòng** output shell;
(5) EV_KILL kiểm `tcb_suspend` (`suspend_ok=1`) + TAP20 đòi `suspend_ok` và `state=Ready`;
(6) procsrv luôn gửi đủ 4 MR (không cắt zero); (7) comment `17 TAP`→24, expected; (8) ghi phạm vi
clippy + `-D warnings` cho shim. Còn nợ: `kill <pid sai>` chưa từ chối (reply procsrv trước khi
kernel xác nhận), low-water chỉ là instrumentation đã gỡ → canary thật ở KT-66.

## 8. Headroom ảnh test và việc tiếp theo

`ktest-user` còn **22.928 B** dưới trần (239.216/262.144) — mỏng nhất từ trước tới nay vì ảnh demo
gánh thêm 3 ELF + capture + **stack dispatch 32 KiB**. **Quyết định KT-66 (Agent 0): chọn (a),
chưa triển khai trong KT-65**; không hạ stack bằng cách đoán low-water. Hai phương án đã nêu:

- **(a) Tách ảnh demo**: trả `ktest-user` về 17 TAP cũ, chuyển procsrv/shell/con sang ảnh thứ
  tư `vifast-kbin-ktest-proc` chạy bằng script riêng (không thêm nhãn vào matrix). Ảnh này từng
  dựng và chính lỗi tràn trap stack làm nó treo; sau khi sửa, đường này khả thi. Ưu điểm: trả
  ~40 KiB headroom cho ảnh test chính; nhược điểm: thêm một lần boot ngoài matrix.
- **(b) Giảm stack dispatch**: đo low-water **bên trong** stack riêng (sơn 0xAA như đã làm với
  trap stack), hạ 32 KiB → mức đo + biên an toàn; hoặc cấp stack từ arena thay vì `.bss` (không
  tính vào `data+bss`) — cần chứng minh arena identity-mapped và không đụng ngân sách Untyped.

Nợ kỹ thuật khác:

1. Đưa scheduler tuần tự vào core (preemptive) và `wait` chặn thật.
2. IRQ UART → message cho shell (giữ giao thức `LINE`).
3. `kill` teardown đầy đủ (vspace/frame/cap) thay vì chỉ `Suspend(Abort)`.
4. Điều tra `ra=0` khi trả về qua nhiều lượt `user_enter`, bỏ continuation.
