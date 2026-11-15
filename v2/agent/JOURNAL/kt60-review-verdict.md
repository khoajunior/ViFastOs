# KT-60 — verdict reviewer độc lập (reviewer-kt60, do lead spawn)

**Verdict:** **DRIFT** (trung bình–cao, ngữ nghĩa; không UNSAFE). Số liệu khai báo của Agent 2
là **thật** (11 test, gate 598/78, ktest 9/9, footprint 192.456 B); digest chặt (Endpoint hàng
chờ đủ nội dung, accessor sim chỉ thêm 41 dòng); 2 hook KT-59 đúng chỗ; mutation thật.
**5 lỗ hổng phải vá hết trước commit.**

| Mã | Lỗ hổng | Vá |
|---|---|---|
| **F1** | `tcb_suspend` thiếu `release_donation_of` (sim gọi **trước** unlink) ⇒ T0 treo vĩnh viễn khi T1 donee bị Suspend(Abort) | thêm `release_donation_of(obj)?` trước `unlink_tcb_queues` + ca kt21 (suspend donee, digest + trạng thái) |
| **F2** | `tcb_set_priority`/`set_sched_params` thiếu guard `is_donation_donor` ⇒ donor đổi ngân sách donee giữa chừng | thêm guard `IllegalOperation` đúng thứ tự sim + ca kt21 |
| **F3** | `block_current(BlockedOnNotification)` không ghi/kiểm `waiter` ⇒ Signal không đánh thức; check một chiều không bắt | kiểm `Busy` trước, ghi `waiter` sau transition + ca kt27 (gồm Busy) |
| **F4** | `SchedContext.Bind(notification)` chưa port; digest so `bound_sc` luôn None | **ghi nợ rõ §8.6.0** ("trường chết, không phải parity có nghĩa") — không port trong KT-60 |
| **F5** | test pin `footprint` tautology (`padding = total - used` rồi assert tổng); log footprint stale 26 dòng | assert `padding ≤ 8 B` + seal `scalars` công thức; regenerate log 27 dòng (padding 1) |

**D2-85 (Agent 0 đã chốt + push):** (a) pin 196.608 B cho `size_of::<Machine>()`, trần
262.144 B cho **ảnh link** — hai ngưỡng, cấm đọc lẫn; (b) **bắt buộc** `MAX_MAIL_LOG` 64→32
trước KT-61 (đã áp: −12.288 B, footprint **180.168 B**); (c) test pin không tautology.

**Bằng chứng đã đối chiếu:** `kt60-footprint.log` cũ (26 dòng, padding 129) **stale** so test
hiện tại (27 dòng, padding 1) → đã regenerate; `k3b-core.md` dẫn `kt60-ktest.log` → sửa
`kt60-ktest-matrix.log`; chép `kt60-baseline-gate.log`, `kt60-core-sha-before.txt`,
`kt60-mut-before.sha` + red-before + M1..M3 từ `/tmp` vào `JOURNAL/`.

**Sau vá:** parity 11 test xanh, gate **598/78**, footprint **180.168 B** (D2-84 pin còn 16.440
B), ktest 9/9 (kernel không đổi), core bare-metal build+clippy xanh. Nợ còn lại §8.6
(chuyển cap/finalise IPC object/Bind SC/tb_resume_with_regs/phủ Reply cap & WaitAddr toàn phần).
