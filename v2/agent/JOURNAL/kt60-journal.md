# KT-60 — K3b tầng 3: IPC/Reply/Notification/WaitAddr/fault (`no_std`)

**Commit:** `70fd5e45` `[KT-60]` + `[KT-60b]` (STATE) — pushed `main`, `HEAD..origin/main = 0`.

**Vào:** `agentctl next --project v2` ⇒ KT-60; HEAD `56083428` ([KT-00] lật KT-59 DONE, mở
KT-60). Baseline `gate --fast` **587 test/76 suite**, L1 1.000 seed; `ktest.sh 1 matrix` 9/9
(TAP 43/3/7). Đọc `review/agent0/K3-PLAN.md` Q13, D2-82/83/84, `docs/k3b-core.md` §6/§7,
sim `kobj.rs` + `ipc.rs` (oracle), `docs/k1c-{ipc-semantics,fault,notification,waitaddr}.md`.

**Bước (9/9):**
1. Baseline xanh (587); chép log.
2. **Red-before:** tạm trả core tracked về KT-59 (`git checkout` 4 tệp, giữ module mới
   untracked) → parity không biên dịch: `E0433: cannot find MessageInfo/Message in kc`
   (`kt60-red-before.log`, exit 101) → khôi phục đúng bản backup + `touch` (bẫy mtime: cargo
   tái dùng artifact cũ nếu không touch — đã gặp và xử lý).
3. Port core: `ipc.rs` (MessageInfo 35 bit/Message 64 MR/IpcArea/transfer), `objstate.rs`
   (body Endpoint/Reply/Notification + `IpcQueue` FIFO 8 + pool `QueuedMsg` + nút WaitAddr),
   `core_ipc.rs` (Call/Send/Recv/Reply/ReplyRecv/NBSendRecv/NBSendWait/Exit/inject_fault/
   unlink/donation), `core_sync.rs` (Notification + WaitAddr), `check_ipc.rs` (5 bất biến),
   `footprint.rs` (27 dòng + pin D2-84); `machine.rs` + `core_tcb.rs` + `core_sched.rs` +
   `lib.rs`; tách `captable.rs`/`tcbdata.rs` để giữ R1.
4. **Hai hook KT-59** nối tại `transition_tcb`: `release_donation_of` khi rời `BlockedOnReply`;
   xung đột `Reply.caller` ⇒ `IllegalOperation`; `finalise_tcb` gọi release trước unlink.
5. **Accessor sim chỉ đọc** (đóng nợ §7.5): `Machine::endpoint_queued` (nội dung message đang
   treo), `Machine::mail_items` (hàng hộp thư) — không đổi semantics; diff sim = thêm accessor.
6. Parity `tests/kt60_core_parity.rs`: 6 kịch bản KT-24/25+34/27/28/26/21 + 3 core-only + pin
   footprint; so mã lỗi + digest **sau từng bước**; 8 cờ bất biến hai phía. **11 test**.
7. Đỏ/mutation: M1 bỏ hook donation (kt21 bước 5 digest lệch); M2 bỏ kiểm Reply active trong
   `recv` (kt25 bước 10 kết quả lệch); M3 `word = badge` (kt27 bước 7 digest lệch). M3 lần đầu
   **không bị bắt** vì kịch bản chỉ có một badge không-zero → **thêm bước OR 0x10|0x02** (tăng
   phủ, ghi nhận; không hợp thức hoá) → M3 đỏ. Khôi phục + `sha256sum -c` OK.
8. Cổng: `gate --fast` **598 test/78 suite**, L1 855 ms (log `kt60-gate.log`); `ktest.sh 1
   matrix` **9/9**, TAP **43/43 + 3/3 + 7/7** (`kt60-ktest-matrix.log`); core bare-metal
   build+clippy xanh (`kt60-core-build.log`).
9. **Review độc lập** (reviewer-kt60, lead spawn) → chờ verdict trước commit; sau verdict:
   commit `[KT-60]` + `[KT-60b]` (STATE) + push, chỉ `v2/`.

**Bằng chứng chính:** `kt60-parity.log` (11 passed + bảng 27 dòng, TOTAL **180.168 B** ≤
196.608), `kt60-gate.log`, `kt60-ktest-matrix.log`, `kt60-core-build.log`,
`kt60-footprint.log`, `kt60-red-before.log`, `kt60-mutation-m{1,2,3}.log`.

**Dấu chân (D2-84 + D2-85):** 180.168 B = 91,6 % pin 196.608 B, còn 16.440 B **sau khi áp
D2-85b** (`MAX_MAIL_LOG` 64→32, −12.288 B). **Không nâng pin.** Điều kiện tiên quyết KT-61 +
phương án cắt còn lại (B
`MAX_QUEUED_MESSAGES` 24→16 + `MAX_ENDPOINTS` 16→8 −5.184 B; C body TCB sang arena 45.056 B;
D dự phòng `MAX_FINALISE` 256→64) ghi `docs/k3b-core.md` §8.5.

**Reviewer-kt60 (lead spawn) verdict DRIFT → vá 5/5:** F1 `tcb_suspend` gọi
`release_donation_of` trước unlink (+ ca kt21 suspend donee); F2 `tcb_set_priority`/
`set_sched_params` chặn donor (`IllegalOperation`, + ca kt21); F3 `block_current` với
`BlockedOnNotification` kiểm/ghi `waiter` + `Busy` (+ ca kt27); F4 `SchedContext.Bind(notification)`
chưa port — ghi nợ §8.6.0 rõ `bound_sc` là trường chết; F5 test pin hết tautology (assert
`padding ≤ 8`, seal scalars; regenerate log 27 dòng). D2-85a hai ngưỡng tách (Machine 196.608 /
ảnh link 262.144), D2-85b giảm `MAX_MAIL_LOG` 64→32 **đã áp**, D2-85c test pin không tautology.
Kèm chép đủ log `/tmp` vào `JOURNAL/` (baseline, sha, mut, red-before, gate, parity, matrix,
footprint, core-build) — không còn log ngoài `JOURNAL/`.

**Nợ KT-61 (docs §8.6):** chuyển cap (receive window/`plan_cap_transfer`); finalise IPC object
khi Revoke (huỷ caller/waiter/hàng); `SchedContext.Bind(notification)`; `TlbFlush` placeholder;
xoá cap Reply khi caller chờ (`Cancelled+Ready`); `tcb_resume_with_regs`; phủ thêm Reply cap
trong `Endpoint.recv`/WaitAddr toàn phần; nối syscall/đo ảnh link.

**Biên task:** không đụng `spec/`, không đổi semantics sim `src/` (chỉ accessor đọc), không
`.scn`/QUEUE/DECISIONS/AGENTS/`viscript/`, không `unsafe` mới, kernel không đổi (chưa nối core
— KT-61).
