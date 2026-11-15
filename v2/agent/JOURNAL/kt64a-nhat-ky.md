# KT-64a — đồng bộ sim theo D2-89/D2-90 + parity `TlbFlush` theo D2-92c

**Ngày:** 2026-11-16 (Asia/Tokyo). **Vào:** KT-64 DONE, HEAD `99cf1c2e`, `HEAD..origin/main` = 0.
**Ra:** sim = core = **một nguồn sự thật** cho finalise IPC (ba loại chờ) và parity `TlbFlush`;
**không còn loại trừ parity** ở §12.5/§13.1. `spec/` không đổi; không `unsafe`; không `#[ignore]`.

> **Quy ước số liệu (sửa sau review, mục 5):** mọi con số dưới đây **chép từ log SAU KHI log
> in dòng PASS** (không viết trước). Thời điểm PASS ghi kèm ở §6.

## 0. Bốn mục tiêu — kết quả

| # | Mục tiêu (brief) | Kết quả | Bằng chứng |
|---|---|---|---|
| 1 | Sửa **sim** theo D2-89/90: `Revoke/Delete` object có waiter (`BlockedOnRecv`, `BlockedOnNotification`, `BlockedOnSend`) ⇒ `Cancelled` + `Ready`, huỷ hàng, đếm drop; `BlockedOnReply` giữ nguyên | **XONG** — `kobj.rs::finalise_ipc_waiters` + `wake_cancelled`, gọi trong `revoke_children` **trước** khi body về free-list | `tests/kt64a_scn.rs`; `kt60_core_parity` 18/18; `JOURNAL/kt64a-red-scn.log` |
| 2 | `.scn` **đỏ-trước** cho đúng 3 loại chờ; chạy trên sim **cũ** ⇒ đỏ; sửa ⇒ xanh | **XONG** — `27-finalise-ba-loai-cho.scn` (26 ⇒ **27 tệp**, **16 nhóm**) + runner đọc tệp; trên HEAD cũ **0/1**, sau sửa **1/1** | `JOURNAL/kt64a-red-scn.log`, `JOURNAL/kt64a-red-parity.log` |
| 3 | Bỏ loại trừ parity `kt60`/`kt64`: phủ finalise-có-waiter **và** finalise-có-sender | **XONG** — `kt63_parity_revoke_finalise_caller_va_sender` (caller + sender, chốt cả pha **trước** `Revoke`), `kt64a_parity_finalise_recv_waiter`, `kt64a_parity_finalise_notification_waiter`; digest thêm `Tcb.last_ipc_error`; §12.5 viết lại | `kt60_core_parity` **18/18** (16 → 18) |
| 4 | Parity `TlbFlush` (`seq/targets/pending/acked/Busy`) sim ↔ core; thiếu chi tiết thì **thêm vào sim** | **XONG có giới hạn quan sát ghi rõ** — kịch bản **C** (2 test) + 2 chi tiết sim thêm (`tlb_pending_ack`, shootdown `targets ⊆ {hart gọi}` xong ngay ở `begin`); `targets/acked ≠ 0` của core **chưa** đọc được ⇒ so tập hart bị post + `pending` + bộ đếm + sự kiện, **không** tuyên bố so mask (nợ §13.9 mục 5) | `kt64_core_parity` **4/4**; §13.1 F3 có bảng 5 trường + **nợ quan sát** |

## 1. Đỏ trước (D2-05) — đo trên **mã nguồn HEAD cũ**

Worktree `git worktree add /tmp/kt64a-redwt2 99cf1c2e`, chép **chỉ** ba tệp test mới + `.scn` 27
(mã nguồn sim/core **giữ HEAD**) ⇒ chạy đúng "sim cũ":

- `tests/kt64a_scn.rs` **0/1** — panic tại `kt64a_scn.rs:237`: `t0 (Recv waiter)` còn
  `BlockedOnRecv{ep: 5}` (ep **đã chết**) thay vì `Ready`; log `JOURNAL/kt64a-red-scn.log`
  (một lần chạy chỉ hiện panic **đầu tiên** — đúng dòng 237 của tệp hiện tại, không sửa sau khi chạy).
- `kt60_core_parity` **15/18** — 3 ca finalise đỏ, đủ **ba** loại chờ + `last_ipc_error`:
  sender `BlockedOnSend{ep: 12, call: false, fault: 0}` vs `Ready`; Recv waiter
  `BlockedOnRecv{ep: 11, reply: 13}` + `last_errs = None` vs core `Ready` + `Some(15)`
  (= `KError::Cancelled`); Notification waiter `BlockedOnNotification{ntfn: 14}` + `None` vs
  `Ready` + `Cancelled`.
- `kt64_core_parity` **không biên dịch** (`E0599`: `Machine::tlb_pending_ack` — chi tiết sim
  KT-64a **thêm vào** để so `pending`; tính năng chưa có, không phải test sai).

Log gộp: `JOURNAL/kt64a-red-parity.log`. Sau bản sửa: **1/1 + 18/18 + 4/4** xanh. Không retry lấy
xanh; không nới ngưỡng.

## 2. Sim sửa gì (D2-89 + D2-90)

`sim/vifast-sim/src/kobj.rs`:

1. `finalise_ipc_waiters(obj)` — gọi trong `revoke_children` cho **mọi** object chết, **sau** vòng
   đếm `ipc_dropped` và **trước** khi body về free-list:
   - `Endpoint`: Recv waiter (`e.recv`) **và mọi** sender trong hàng gửi (`e.send`) — chỉ đánh
     thức TCB còn **đúng** trạng thái trỏ object này (`BlockedOnRecv{ep}`/`BlockedOnSend{ep}`);
   - `Notification`: `n.waiter` (`BlockedOnNotification{ntfn}`).
2. `wake_cancelled(tcb, khớp)` — đặt `last_ipc_error = Cancelled` rồi `→ Ready` qua **một** cửa
   `transition_tcb` (hook KT-21 tự trả donation, `sched_sync` đưa vào ready queue).
3. Message của sender bị huỷ vẫn đếm `ipc_dropped` ở vòng có sẵn (D2-90: drop **có đếm**);
   `NBSend` (sender **không** chặn) xếp hàng nhưng vẫn `Ready` ⇒ **không** bị đánh thức
   (điều kiện `BlockedOnSend` + khớp `ep`).

Không đổi ngữ nghĩa `BlockedOnReply` (KT-25 đã đúng): caller vẫn `Cancelled + Ready` qua
`cancel_reply`; TCB chờ Notification **sống** vẫn `BlockedOnNotification` (chốt trong kịch bản
`kt63-revoke` — chứng minh **không** wake nhầm).

## 3. Core sửa gì (đóng **nợ D2-90**, không phải nới ngữ nghĩa)

`kernel/vifast-core/src/core_finalise.rs::finalise_endpoint_body`: KT-63 đã đúng cho
caller/Recv/Notification; KT-64a đánh thức **cả sender `BlockedOnSend`** trong hàng (duyệt
`e.send`, chỉ `blocking`, khử trùng, khớp `ep`) — `Cancelled + Ready`. D2-90: "**trước** KT-64a
core được phép giữ hành vi sim (treo sender) **với điều kiện** ghi nợ tường minh" ⇒ sau KT-64a
điều kiện đó **hết** hiệu lực; đây là **cùng một chuẩn**, không phải tính năng mới. KT-63
waiter/caller **giữ nguyên**; core vẫn `#![forbid(unsafe_code)]`; `size_of::<Machine>()` không đổi.

**QEMU (review mục 3):** TAP 55 **cùng số TAP** nay kiểm **sender** trên nhân thật:
`ktest_core.rs` thêm `t2_free` (`Ready` + `Cancelled`) vào `ok55` + evidence
`# ipc: revoke … notif_waiter=Ready sender=Ready sender_err=Cancelled …`; `ktest.sh` (regex
evidence) + regex evidence trong `ktest.sh` (**cùng** commit — luật KT-54); `expected.txt`
**không** đổi (plan 60 + số mục evidence giữ nguyên, chỉ **một** dòng evidence dài thêm). Log QEMU:
`JOURNAL/kt64a-ktest-matrix.log` (dòng evidence thật của `-smp 1`).

## 4. Parity `TlbFlush` (D2-92c) — kịch bản C, và **giới hạn quan sát** (sửa sau review mục 1)

**C1** (1 hart, `active_on = {0}`): sim `begin`+`wait` ↔ core `tlb_flush` (một cửa) ⇒ `seq` bằng;
`report.active_on == info.targets`; `report.targets == tlb::targets(info.targets, caller)`;
`report.acked == tlb::targets(info.acked, caller)`; `local`; `completed`; `messages ==
tlb_remote_posts()`; `pending` rỗng hai bên; số flush **theo hart** bằng. Ở C1 cả `targets` và
`acked` đều **0** — đây là ca **duy nhất** đọc được hai trường report **literal**.

**C2** (2 hart, `active_on = {0,1}`): begin ⇒ `pending` **hai vai** bằng (`tlb_pending` +
`tlb_pending_ack` của sim ↔ `tlb_pending` của core), `Busy` khi begin lần hai **cùng hart gọi**
(cùng mã ABI, seq không tăng), hart đích tự phục vụ (`process_mail` ↔ `tlb_service`,
`tlb_ack_count` ↔ `tlb_serviced`, hộp thư rỗng dần), `wait` ⇒ `pending` sạch hai bên; `targets`
**≠ 0** so **gián tiếp** qua đúng tập hart **bị post** (hộp thư từng hart) + `info.messages ==
tlb_remote_posts()`.

> **Nợ quan sát (trung thực, không tô hồng):** `TlbFlushReport.targets/.acked` với giá trị
> **≠ 0** của core **chưa** đọc được qua `begin`+`wait` — `tlb_flush` nhiều hart **luôn `Busy`**
> khi chưa ai chen `tlb_service` (D2-92b). Vì vậy C2 **không** so mask `acked`; nó so **đúng tập
> hart bị post** + `pending` hai vai + bộ đếm ack + sự kiện `acked == targets`. Muốn so mask thật
> phải thêm report cho `wait`/`flush` (đổi API core) — **không** làm trong KT-64a; nợ ở §13.9
> mục 5. Docs §13.1 F3 đã **hạ claim** tương ứng (không còn câu "core.targets == …" cho test
> không đọc).

Hai chi tiết **sim thiếu** ⇒ thêm vào sim (không hạ core): (i) `Machine::tlb_pending_ack(hart)` —
vai "hart đích đang nợ ack" của `pending`; (ii) shootdown **không còn hart nào phải ack**
(`targets ⊆ {hart gọi}`) **xong ngay ở `begin`**, không giữ ô `pending` rỗng (core: `targets == 0`
⇒ không `pending`; kernel KT-51 cũng `completed` ngay vì ack ngầm đủ). Hệ quả: **một** assert của
`kt40_tlb::loi_bien_hart_va_vspace` đổi theo ngữ nghĩa mới; luật chống ghi đè `Busy` **vẫn pin**
trong cùng test bằng shootdown **có** hart đích.

**Phạm vi mô hình (không phải "đã kiểm"):** shootdown **đồng thời** hai hart (sim cho — KT-40
`deadlock_hai_hart_cho_nhau_poll_tu_phuc_vu`; core/kernel `IN_FLIGHT` giữ **một** ⇒ `Busy`) —
parity **chỉ** so `Busy` ở ca **cùng hart gọi**; đường đồng thời là **phạm vi mô hình** của sim K2.

## 5. `.scn` — 26 ⇒ **27 tệp**, **16 nhóm** (không nhóm mới)

`v2/test/scn/27-finalise-ba-loai-cho.scn` (tag `cap-revoke` + `notification`; 4 bước: `recv` ep0,
`recv` ntf0, `send` ep1, `invoke CNode.Revoke cnode=c1 slot=u1`), `expect_states = 3 × Ready`.
`check_scn.py`: hằng số **26 → 27** (siết, không nới) + thông điệp DoD 27/27. Runner
`tests/kt64a_scn.rs` đọc **nguyên** tệp (không sửa), so kết quả **từng bước** + 6 bất biến sau
mỗi bước + hậu điều kiện D2-89/90 (`Ready`, `Cancelled`, `ipc_dropped == 1`, `ipc_pending == 0`,
3 object chết, cap con bị xoá). Tệp **phân biệt thật** cũ/mới: trên sim cũ runner đỏ (§1).

## 6. Cổng + hồi quy (số chép **sau khi log PASS**)

| Bước | Kết quả | Thời điểm PASS (theo log) | Log |
|---|---|---|---|
| `gate --fast` (baseline KT-64) | **PASS 642/82**, `.scn` 26 tệp/16 nhóm | trước task | (baseline STATE KT-64) |
| `/tmp` red runs (HEAD cũ) | `kt64a_scn` 0/1; `kt60` 15/18; `kt64` E0599 | trước bản sửa | `JOURNAL/kt64a-red-*.log` |
| `cargo test -p vifast-sim` (toàn bộ, mã mới) | **454 passed / 0 failed** | sau bản sửa | `JOURNAL/kt64a-sim.log` |
| `gate --fast` (sau KT-64a) | **PASS 647 test/83 suite**, 0 failed, L1 1.000 seed/999 trace (833 ms), gen-check ON, `.scn` **27 tệp**/16 nhóm/0 blocked, fixtures âm PASS | lần chạy cuối | `JOURNAL/kt64a-gate.log` |
| `ktest.sh 1 matrix` (QEMU, **TAP 55 mới**) | **9/9** boot; TAP **60/60 + 3/3 + 17/17**; ảnh ktest **573.744 B**/209.856, guard 53.808/2.680, user 335.928/201.008, product 187.936/200.120 | sau khi sửa TAP 55 | `JOURNAL/kt64a-ktest-matrix.log` |
| `soak.sh 1000 3` | **0/1000** lỗi (1000/1000 boot ok), wall **224 s**, `dirty-kernel-paths=1` ⇒ chạy trên core **đã sửa**; `BUILD_ID=6290c3b0198d37ac` | log bắt đầu `2026-11-15T20:20:35Z` (05:20:35 JST), PASS ≈ **05:24 JST** | `JOURNAL/kt64a-soak.log` |
| conformance 5/5 | **không đổi trace** (`.scn` 22..26 + kernel trace không chạm) ⇒ không chạy lại; matrix chạy kèm kiểm `# trace`/`# conf:` | — | — |

Đường **kernel** có đổi (core finalise IPC + TAP 55) nên **có** chạy matrix + soak (không miễn).

## 7. Vòng review — 5 mục DRIFT và cách đóng

| # | Reviewer nêu | Đã làm |
|---|---|---|
| 1 | Parity TLB "5 trường" chưa đạt: C2 không đọc `core.targets/acked` | **Chọn (b) hạ claim**: §13.1 F3 bảng ghi rõ C1 chỉ đọc literal với giá trị **0**; C2 so **tập hart bị post** + `pending` + bộ đếm + sự kiện; thêm **"Nợ quan sát"** (§13.1 + §13.9 mục 5). Test: bỏ assert "core: acked == targets" (chỉ dùng dữ liệu sim) → thay bằng chú thích nợ + assert quan sát được; chú thích `targets ≠ 0` so gián tiếp |
| 2 | `kt64a-red-scn.log` panic dòng 241 ≠ assert hiện tại 237 | **Regenerate** trên worktree HEAD với **đúng test hiện tại** ⇒ panic `kt64a_scn.rs:237`; ghi rõ "một lần chạy chỉ hiện panic đầu tiên; đủ 3 loại chờ ở log parity" |
| 3 | TAP 55 QEMU không kiểm **sender** (D2-90) | Thêm `t2_free` vào `ok55` + evidence `sender=Ready sender_err=Cancelled`; sửa regex evidence `ktest.sh` cùng commit (`expected.txt` **không** đổi); **matrix lại 9/9** (TAP 55 in đúng dòng mới) |
| 4 | `STATE.md` BUILD_ID/ảnh còn số KT-64 | Cập nhật **187.936 / 573.744 / 53.808 / 335.928**, `data+bss` 200.120 / 209.856 / 2.680 / 201.008, BUILD_ID soak `6290c3b0…`, ghi rõ `size_of::<Machine>()` **không đổi** |
| 5 | Timeline soak: số ghi trước log PASS | §0 + §6 ghi rõ "số chép **sau khi log in PASS**", kèm thời điểm log; không dùng số ước lượng ở bất kỳ mục nào |

## 8. Không làm / biên

- Không sửa `spec/`, `viscript/`, QUEUE/DECISIONS/AGENTS, không `git add`, không retry, không
  `#[ignore]`, không nới ngưỡng, không `unsafe` mới.
- `Unbind` vẫn core-only (sim không có) — **không** phải loại trừ finalise/TLB của KT-64a.
- Nợ còn lại (ghi §13.9): mask `targets/acked ≠ 0` của core chưa quan sát được (nợ quan sát);
  shootdown đồng thời = phạm vi mô hình; `active_on` nhiều hart thật trong nhân (nợ cũ KT-64).
