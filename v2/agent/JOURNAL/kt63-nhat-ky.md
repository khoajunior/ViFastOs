# KT-63 — chuyển cap qua IPC + finalise IPC khi Revoke/Delete (K3.5 tầng 2)

- **Ngày**: 2026-11-16 (Asia/Tokyo).
- **Vào**: KT-62 DONE (HEAD `5380164b` lật KT-62; `HEAD..origin/main` = 0); `agentctl next` ⇒
  KT-63 (job `vifast-agentctl` không có trên PATH máy này — đọc `v2/agent/QUEUE.md` dòng 170:
  KT-63 READY, phụ thuộc KT-62 DONE). Baseline ghi theo STATE KT-62: `gate --fast` **607/79**,
  matrix **9/9**, soak **0/1000**.
- **Ra**: `size_of::<Machine>()` **188.448 B** ≤ **196.608** (pin **không** nâng); ảnh
  **199.696 / 209.856 / 2.680 / 200.584** ≤ **262.144**; parity **16/16** (5 kịch bản mới);
  core-only **13/13**; TAP `ktest` **56/56**, matrix **9/9**; soak **0/1000**; guard **3/3**; gate --fast
  **PASS 625 test / 80 suite** (607 + 18: 5 parity mới trong `kt60_core_parity` + 13 test
  `kt63_core_cap_ipc`).

## 1. Red-before (D2-05)

Viết test trước khi port: `v2/sim/vifast-sim/tests/kt63_core_cap_ipc.rs` gọi
`Message::with_cap_ids`, `Machine::{tcb_set_recv_window,tcb_recv_window,cap_transfer_stats,
cap_transfer_installed,cap_transfer_failed,cap_unwrapped}` — core HEAD cũ **không** có ⇒
`JOURNAL/kt63-red-before.log` (`E0599`, exit 101): đúng "tính năng chưa có", không phải test
sai. Sau port: **13/13** xanh (`kt63-core-tests.log`). Không retry; không nới ngưỡng.

## 2. Port từ sim (giữ nguyên ngữ nghĩa)

- `ipc.rs`: `CapSnapshot` (cap/obj/ty/rights/badge), `CapPlan` (install/unwrap/skipped),
  `plan_cap_transfer` **nguyên văn** sim; `Message` mang `caps: [CapId; 4]` + `snaps` ảnh chụp
  lúc `admit` (không copy object). `Message::{with_caps, with_cap_ids, cap_ids, snapshots,
  clear_caps}`; **5 cap ⇒ `InvalidArgument`** ngay khi dựng.
- `tcbdata.rs`: `TcbData.recv_window: Option<[SlotRef; 4]>` (None = chưa bind).
- `core_capxfer.rs`: `resolve_caps` (cap chết ⇒ `InvalidCapability` **trước** `msg_id`),
  `prepare_caps` (window chưa bind + có cap cần cài ⇒ `InvalidArgument` **trước** khi giao),
  `deliver_caps` (cài con **bỏ `RIGHT_GRANT`**, giữ badge; ô bận/thiếu GRANT/cap chết ⇒ thất
  bại **mềm**; `MessageInfo.caps`/`extra_caps_unwrapped`/`badge` cập nhật), `deliver_message`.
- Nối vào **mọi** đường: `call_msg`, `endpoint_send`, `recv`, `nb_recv`, `reply_with`,
  `deliver_queued`; `admit_message` kiểm `n_caps == info.caps()` + đủ ảnh chụp.
- Bộ đếm `cap_transfer_{installed,failed}`, `cap_unwrapped` + `cap_transfer_stats()` (cùng tên
  sim); footprint scalars 9 → 12 × `u64`.
- `core_finalise.rs`: `cancel_reply_object` (caller `Cancelled + Ready` qua **một** cửa
  `transition_tcb`, hook KT-21 trả donation), `clear_dead_reply_in_waiters` (Q7b),
  `finalise_endpoint_body` (đếm `ipc_dropped` + đánh thức Recv waiter), `finalise_notification_body`
  (đánh thức waiter). `Machine::delete` gọi cancel khi cap Reply chết; `Machine::{revoke,
  revoke_children}` gọi cùng bộ **trước** khi body về free-list (cả nhánh Untyped và cap thường).
- Tách `mdb.rs` (helper cây MDB) + `core_capxfer.rs` + `core_finalise.rs` để giữ R1 ≤ 1.500
  dòng (`core_ipc.rs` 1.631 → 1.421, `machine.rs` 1.548 → 1.478).

## 3. Sim **không** đánh thức waiter — đo trước khi chọn khác biệt có chủ đích

Probe tạm (đã xoá; dán nguyên văn output, `cargo test -p vifast-sim --test kt63_probe_tmp`):
Untyped chứa 3 Endpoint + Reply + Notification; t0 `BlockedOnReply(rep)`, t1 `BlockedOnRecv(ep1)`,
t2 `BlockedOnSend(ep3)` (1 message trong hàng), t3 vừa Recv xong; `revoke` cả cây:

```
deleted caps=5
t0: state=Ready err=Some(Cancelled)
t1: state=BlockedOnRecv { ep: 3, reply: None } err=None
t2: state=BlockedOnSend { ep: 11, call: false, fault: None } err=None
t3: state=Ready err=None
ep1 alive=false ep2 alive=false ep3 alive=false rep alive=false ntfn alive=false
 counters sent=2 delivered=1 dropped=1 pending=0
 checks ipc=Ok(()) reply=Ok(()) ntfn=Ok(())
```

⇒ sim: caller `Cancelled + Ready`; **Recv/Notification waiter vẫn treo** trên object đã chết
(sim không có ca). **D2-89 (lead chốt 2026-11-16, sau khi đo probe)**: chuẩn finalise IPC là
**anti-hang**; sim để waiter treo là **lỗi mô hình**. Core làm theo chuẩn: waiter `Cancelled +
Ready` (qua đúng một cửa, hook donation vẫn chạy) — đây là **lệch có chủ đích chờ đồng bộ**,
**không** phải "sim không làm nên core làm khác"; **cấm** dùng "sim không làm" làm lý do để core
treo. Parity tạm loại trừ đúng đường này; **KT-64a** (QUEUE, 2 h, sau KT-64) sửa sim + `.scn`
đỏ-trước + đưa đường này trở lại parity rồi bỏ loại trừ. Hành vi core pin bằng 2 ca core-only +
TAP 55; §12.5 docs ghi rõ KT-64a là nơi đóng lệch.

## 4. Parity sim↔core (`kt60_core_parity.rs`, 5 kịch bản mới)

- Thêm `WinFp`/`CapFp` + `View::{window, cap_stats}`; `Digest` thêm `windows` (3 TCB) +
  `cap_stats`; `Rig63`/`Op63`/`run63` (TCB **kernel object**, IPC object **retype từ Untyped**
  để revoke được, 2 Untyped, 3 receive window, 4 cap nguồn, 1 cap thiếu GRANT, 2 cap badge,
  1 cap chết).
- Kịch bản: (1) `kt63_parity_chuyen_cap_qua_ipc` — 22 bước: 4 cap bỏ GRANT/ô bận/thiếu GRANT/
  unwrap/window chưa bind/cap chết/Call+Reply mang cap; kết `cap_transfer_stats() = (9,2,8)`
  (cùng cửa sổ tái dùng — đúng ngữ nghĩa, không phải lỗi); (2) `kt63_parity_revoke_finalise_caller_hang`
  — Revoke cây Untyped khi caller/waiter/hàng: caller `Ready`, `last_ipc_error=Cancelled`, hàng
  rỗng `ipc_dropped=1`, TCB gửi còn `BlockedOnSend` **giống sim**; (3) `Delete` cap Reply;
  (4) `Revoke` cap Reply con; (5) Reply cap chết trong waiter (Q7b).
- Kết quả: **16/16** test trong file (11 cũ + 5 mới), digest khớp **sau từng bước** cả 2 phía,
  8 cờ bất biến xanh — `kt63-parity.log`.

## 5. TAP QEMU

| TAP | Bằng chứng `# ipc:` |
|---|---|
| 53 `core cap transfer` | `# ipc: cap-transfer installed=8 failed=1 unwrap=1 caps4=4 len=2 rights=0xb caps3=3 caps1=1 extra=1 badge=0xa5a5 check_ipc=1 check_caps=1` |
| 54 `core cap message` | `# ipc: cap-message direction=call-reply caller_caps=2 reply_caps=2 caller_bound=1 caller_ready=1 rights=0xb reply_caller=0 check_ipc=1 check_reply_links=1` |
| 55 `core revoke finalise` | `# ipc: revoke deleted=5 caller=Ready caller_err=Cancelled recv_waiter=Ready notif_waiter=Ready queue=0 queued=1 pending=0 sent=2 dropped=1 check_ipc=1 check_reply_links=1 check_notifications=1` |

- `ktest.sh 1 1` (smp1): 3/3 ảnh sạch, `ktest` **56/56** — `kt63-ktest-smp1.log`.
- `ktest.sh 1 matrix`: **9/9** boot sạch (smp 1/2/4 × ktest/guard/user; 56/3/17 TAP) —
  `kt63-ktest-matrix.log`. Trap lồng dời **53 → 56**; `expected.txt` = `ktest 56 56`;
  manifest +3 TAP; evidence `# core:` 11 → 11 + `# ipc:` 3 (68 mục ảnh chính).
- Ảnh: product **191.824 file / 199.696 data+bss**, ktest **533.536 / 209.856**, guard
  **53.808 / 2.680**, user **330.704 / 200.584** — `kt63-budget.log` (`budget.sh`).

## 6. Dấu chân (D2-84/D2-85a) — **không** nâng pin

| Ngưỡng | Áp cho | KT-62 | KT-63 |
|---|---|---|---|
| 196.608 B | `size_of::<Machine>()` | 180.168 | **188.448** (95,9 %; còn 8.160 B) |
| 262.144 B | `.data+.sdata+.bss+.sbss` ELF | 191.416 / 197.568 / 2.680 / 192.304 | **199.696 / 209.856 / 2.680 / 200.584** |

Bảng §7.3 cập nhật 27 dòng: `tcbs[64]` 45.056 → **49.664** (TcbData 704 → 776 B), `ipc_msgs[24]`
13.632 → **17.280** (QueuedMsg 568 → 720 B), scalars 239 → **263**; tổng 180.168 → **188.448**.
Test pin vẫn assert `padding ≤ 8` (hiện 1 B) ⇒ bảng thiếu dòng là đỏ.

## 7. Gate / soak

- `gate --fast`: **PASS 625 test / 80 suite**, 0 failed, L1 1.000 seed (879 ms) — `kt63-gate.log`.
- `ktest.sh 1 matrix`: **9/9** — `kt63-ktest-matrix.log`.
- `soak.sh 1000 3`: **0/1000**, BUILD_ID + sha256 ghi trong `kt63-soak.log`.
- `v2 check` (fmt + clippy + 5 luật cấu trúc + gen-check): PASS — `kt63-check.log`.

## 8. Vá reviewer-kt63 (DRIFT trung bình → hết)

Reviewer xác nhận `plan_cap_transfer` **khớp từng nhánh** (unwrap trước, thiếu GRANT/ô bận ⇒
mềm, window `None` + cap cần cài ⇒ `InvalidArgument`, ≤ 4, `rights & !GRANT` giữ badge), lỗi cứng
**trước** `msg_id`/pop, finalise **trước** free-list, `ipc_dropped` khớp `check_ipc`, D2-89 đạt,
footprint 188.448/padding 1. Ba lỗi **nguyên tử thật** ở đường mới đã vá:

- **F1** — `deliver_caps` gọi `push_cap` (assert `n_caps < MAX_CAPS`) mà không kiểm sức chứa; chuỗi
  “bind window → send 4 cap → delete cap window” lặp ~129 lần ⇒ panic **giữa message** sau `admit`
  và sau `take_waiter` ⇒ mất message/treo waiter. **Vá**: `prepare_caps` kiểm
  `n_caps + plan.installed() <= MAX_CAPS` ⇒ `NotEnoughMemory` **trước** `admit`; ca
  `kt63_core_bang_cap_day_tra_not_enough_memory_nguyen_tu` (512 cap thật; message không vào
  đường IPC, waiter còn `BlockedOnRecv`). `derive` cùng lỗ ⇒ **nợ riêng** §12.8.
- **F2** — receive window trỏ CNode đã chết: `cnode_slot` trả `Ok(None)` ⇒ plan cài;
  `push_cap` rồi `cnode_set_slot` lỗi ⇒ cap mồ côi, message đã `admit` nhưng không giao/drop,
  waiter đã take. **Vá**: từng ô đích phải `object_alive(dst.cnode)` + `check_cnode_slot` ⇒
  `InvalidCapability`/`RangeError` **trước** `admit`; ca
  `kt63_core_window_tro_cnode_chet_tra_invalid_capability` (retype CNode từ Untyped → bind window
  → `revoke` Untyped → send cap).
- **F3** — thiếu ca “cap nguồn chết giữa lúc xếp hàng và lúc giao”: thêm
  `kt63_core_cap_nguon_chet_giua_hang_va_giao_thi_that_bai_mem` ⇒ message **vẫn giao**,
  `info.caps = 0`, `cap_transfer_failed = 1`, `ipc_dropped = 0`.
- **D2-90** (lead chốt): anti-hang **phủ cả sender** `BlockedOnSend` trên Endpoint bị revoke.
  Core KT-63 **giữ hành vi sim** cho sender (còn treo) = **nợ D2-90** ghi tường minh; **KT-64a**
  phủ **cả ba** loại chờ + **bỏ mọi loại trừ parity** (docs §12.5/§12.8).

Sau vá: core-only **13/13**, parity **16/16**, gate **625/80**, matrix **9/9** (56/3/17), soak
**0/1000** (BUILD_ID `09647ba1765e054d`), budget **199.696 / 209.856 / 2.680 / 200.584**.

## 9. Nợ / khác biệt còn lại

- **Lệch có chủ đích chờ đồng bộ** (D2-89): waiter Recv/Notification **thức** khi object bị
  finalise — chuẩn là anti-hang; sim để treo là lỗi mô hình, **KT-64a** đóng lệch (sửa sim +
  `.scn` đỏ-trước + đưa vào parity, bỏ loại trừ §12.5).
- TCB gửi hàng bị Revoke vẫn `BlockedOnSend` trên Endpoint chết (`ipc_dropped` cộng đủ) — **giống
  sim**; chưa đánh thức người gửi (thuộc giao thức procsrv/KT-65).
- MR4..MR63 qua IPC buffer user (`length > 4` ⇒ 5) — nợ KT-64; invocation `NotSupported`
  (D2-53 Q11) giữ nguyên; `TlbFlush`/`SlotPool`/`Bind`/`resume_with_regs` — KT-64.
