# KT-64 — nhật ký (K3.5 tầng 3): `TlbFlush` thật + `SlotPool`→arena + `SchedContext.Bind` + `tcb_resume_with_regs`

**Vào**: READY (D2-87); HEAD `2e2631fa`; `HEAD..origin/main` = 0; baseline `gate --fast` **PASS**
(Theo STATE KT-63: 625 test/80 suite) — log `reports/kt64/kt64-baseline-gate.log`.
**Nhánh**: main; worktree có `viscript/`, `review/`, `.agents/`, `out-b.log` của tab khác — **không chạm**;
commit chỉ `v2/**` (D2-91: không `git add`, lead commit bằng pathspec).

## 1. Bốn mảng — làm gì, vì sao

| # | Mảng | Nợ đóng | Thiết kế chốt |
|---|---|---|---|
| 1 | `TlbFlush` thật | §6.10, §8.6.3, §11.5 | trait `TlbEnv` (arch `sfence.vma`); core: flush cục bộ **luôn**, post `TlbFlush` tới `active_on`, chờ đủ ack, `dispatch` **Busy** khi còn pending |
| 2 | `SlotPool` → arena | §6.2, §9.6, §12.8 | byte slot CNode Retype **trong arena** tại `offset + cnode_base`; `SlotPool` xoá; cửa sổ word = hàm thuần của offset |
| 3 | `SchedContext.Bind`/`Unbind` | §8.6.0 (F4), §11.5 | `ScData.bound` sống; 1:1 hai chiều; `Signal` chỉ đánh thức TCB đúng bound; delete/revoke gỡ mồ côi |
| 4 | `tcb_resume_with_regs` | §8.6.6, §11.5 | một cửa: 4 trạng thái cho phép, ghi regs **trước** `Ready`, giữ `exit_code`, không mở đường user |

## 2. Đỏ trước (D2-05)

Test viết **trước** khi port (`kt64_core_tlb_bind_resume.rs`, `kt64_core_parity.rs`). Lần chạy
đầu trên core HEAD-cũ: **E0432** `unresolved imports kc::TlbEnv, kc::TlbScope` (exit 101) —
`reports/kt64/kt64-red-before.log`. `git grep` tại HEAD `2e2631fa` cho
`sched_context_bind`/`sched_context_unbind`/`tcb_resume_with_regs`/`cnode_slot_span`/`TlbEnv` ⇒
**0 kết quả** (cùng log): tính năng **chưa có**, không phải test sai. Sau port: **12/12** core +
**2/2** parity xanh; không retry, không nới ngưỡng.

## 2b. Review độc lập vòng 1 — DRIFT (verdict của `reviewer-kt64`) và bản sửa

Reviewer **DRIFT**: không `unsafe`, không nới ngưỡng (tự đo 639/82, ktest 60/60 × 3 smp, soak
0/1000, `size_of` 188.872, ảnh ≤ 262.144) nhưng **1 lỗi CHẶN + 3 lệch vừa**:

| Mã | Lỗi | Sửa trong KT-64 |
|---|---|---|
| **F1 (chặn)** | cửa sổ word slot chỉ là hàm của `offset`; `Region` nào cũng `cursor=0` ⇒ hai Untyped cùng offset **alias** word ⇒ cap CSpace này đè CSpace kia, im lặng | **D2-92a**: `arena_slot_base(region, offset, slots)` = `region*192 + (offset+64)/48 + slot`; vùng A 3072 word chia đều 16 Region; test hồi quy `kt64_slot_hai_untyped_cung_offset_khong_alias` |
| **F2** | §13.2 ghi 3584/172.032/harness 512, code là 3072/147.456/harness 1024 | sửa docs §13.2 (+ §13.7 ghi dải-theo-Region là hằng, pin không đổi) |
| **F3** | lý do core-only cho TLB quá rộng (sim **có** giao thức shootdown KT-40) | sửa §13.1: chỉ core-only phần **flush phần cứng**; parity `seq/targets/pending/acked/Busy` chuyển **KT-64a (D2-92c)** |
| **F4** | nhánh `Busy` quá hạn là **dead code** (waiter tự ack hộ mọi hart) | **D2-92b**: `wait` **không** flush/ack hộ hart khác (chỉ rút hộp thư mình); `Busy` reachable; test `kt64_tlb_ack_khong_toi_tra_busy_khong_treo` (Busy + pending giữ nguyên + `dispatch` chặn + `tlb_flush` cũng Busy) |
| **F5** | bất biến `ScData.bound` 1:1 chưa vào `check_*`; chưa test `revoke` gỡ mồ côi | **D2-92d**: `check_notifications` kiểm **hai chiều** (SC.bound ↔ Notification.bound_sc, cap sống, đúng kiểu); test `kt64_bind_revoke_go_binding_mo_coi` |
| **F6** | ghi nhận: multi-hart chỉ host | thêm câu §13.1: nếu đường multi-hart lọt vào kernel ⇒ `debug_assert_eq!` hart + `sfence` sai hart ⇒ lộ ngay |

**Review vòng 2 (`reviewer-kt64b`) — CONFIRM 5/5** (F1 chặn alias thật, F4 `Busy` reachable,
F5 bất biến hai chiều + test revoke, số khớp, M1 đỏ đúng / M2 xanh không tuyên bố quá mức), kèm
**2 việc biên** đã vá:
- **F7**: `harness_slot_base` off-by-one — `MAX_SLOT_WORDS − 1 − index − slots`: khi harness đầy
  **đúng** 1024 word, base = **3071** ⇒ trùng word cuối vùng A (Region 15). Sửa bỏ `−1`:
  `base = MAX − index − slots`, `index + slots ≤ 1024` ⇒ `base ≥ 3072`; điều kiện `slots == 0` ⇒
  `None`; test biên (`đầy đúng ⇒ 3072`, `vượt ⇒ None`).
- **F8**: `kt64-mutation.log` trích `requests=0` nhưng bản log M1 giữ lại chỉ có summary ⇒ **chạy
  lại M1**, lưu **đầy đủ** boot log (`JOURNAL/kt64-mutation-m1-boot.log`, có dòng evidence
  `requests=0` + `not ok - core tlb flush ... req=0`), rồi khôi phục `ktest_core64.rs` (sha256
  khớp `5e15a8c4…`).

Sau sửa: `size_of::<Machine>()` **188.872** (không đổi — dải slot là **hằng**, không thêm trường);
**15/15** core test; mutation M1 (bỏ core `tlb_flush`) ⇒ TAP 59 **đỏ**; M2 (bỏ `sfence`) ⇒ TAP 59
xanh — **giới hạn QEMU TCG**, ghi nợ §13.9 mục 7 (hiệu lực `sfence` pin ở KT-51 TAP 25..29).

## 3. Thứ tự làm + bằng chứng

1. Core: `slotmem.rs` (thay `slotpool.rs`), `arena::Region::probe`, `machine_slot.rs` (tách R1),
   `machine.rs` (fields + retype/create + delete/revoke hooks), `tlb.rs` (mới), `core_sync.rs`
   (`sched_context_bind/unbind/bound` + drop binding), `core_tcb.rs` (`tcb_resume_with_regs`),
   `core_sched.rs` (dispatch chặn + mailbox), `tcbdata.rs` (`ScData.bound`), `footprint.rs`
   (29 dòng), `lib.rs`.
2. **Sửa lỗi tự gây trong lúc làm** (ghi để không tô hồng):
   - `init_zeroed` **quên** đặt `harness` (cận khác 0) ⇒ boot PANIC `arena harness hết chỗ` ở TAP
     45; sửa + chạy lại.
   - TAP 57 ban đầu gọi `with_machine` **lồng** trong closure `core_taps` (lock không re-entrant)
     ⇒ **treo** boot (TIMEOUT); tách TAP TLB ra **ngoài** khoá (file `ktest_core64.rs`) và đánh số
     lại 56..59 (thứ tự `ok` = thứ tự chạy).
   - Cửa sổ harness 512 word **không đủ** cho ca F1 của KT-63 (6 CNode × 96 slot) ⇒
     `HARNESS_SLOT_WORDS = 1024`; cửa sổ arena còn 3072 word (147.456 B) — vẫn ≫ nhu cầu.
   - TAP plan kernel `1..56` → `1..60`; ora/slot evidence + trap lồng 56 → 60.
3. Test host:
   - `kt64_core_tlb_bind_resume.rs` — **15/15** (TlbFlush 1 hart/nhiều hart/ngoài active_on +
     **Busy khi ack không tới**, slot arena + harness + không chồng + **hai Untyped cùng offset
     không alias (F1)**, bind 2 chiều + lỗi + signal đúng bound + **revoke gỡ mồ côi (F5)**,
     resume 4 trạng thái + 3 trạng thái cấm + message cap + pin D2-84).
   - `kt64_core_parity.rs` — **2/2** (bind+signal; resume) so mã lỗi + digest **từng bước**.
4. Kernel TAP (ảnh `ktest`, 56 → **60** TAP): `# slot:`, `# bind:`, `# resume:`, `# tlb:`
   (`ktest_core64.rs`); matrix **9/9**, soak **0/1000** (log `JOURNAL/kt64-ktest-matrix.log`).
5. **Mutation** (`JOURNAL/kt64-mutation.log`): M1 bỏ core `tlb_flush` ⇒ TAP 59 **đỏ** (`requests=0`,
   58/60); M2 bỏ `sfence.vma` ⇒ TAP 59 **không** đỏ (TCG không giữ translation cũ cho probe) ⇒
   ghi rõ giới hạn, hiệu lực `sfence` pin ở KT-51 (TAP 25..29 + conformance 24).

## 4. Hai ngưỡng (D2-85a) — không nâng pin

| Ngưỡng | Áp cho | KT-63 | KT-64 | Kết quả |
|---|---|---|---|---|
| 196.608 B (D2-84) | `size_of::<Machine>()` | 188.448 | **188.872** (còn 7.736 B) | ĐẠT |
| 262.144 B (D2-85a) | `.data+.sdata+.bss+.sbss` ELF | 199.696 / 209.856 / 2.680 / 200.584 | **200.120 / 209.856 / 2.680 / 201.008** | ĐẠT |

File ảnh: **183.272 / 573.136 / 53.808 / 335.920** B (`reports/kt64/kt64-budget.log`; chạy
`budget.sh` trên 4 ELF release). `SlotMem` 16.384 + `harness_region` 24 thay `SlotPool` 16.656;
`tlb_pending[8]` 384; `ScData` 56→64 (+256); 4 bộ đếm ⇒ scalars 263 → 295; bảng 27 → **29 dòng**.

## 5. Cổng

- `./v2/bin/v2 check` **PASS** (fmt + clippy + luật R1..R5 + spec/gen).
- `./v2/bin/v2 gate --fast` **PASS**: **642 test / 82 suite**, 0 failed; L1 1.000 seed **879 ms**
  (`JOURNAL/kt64-gate.log`).
- `ktest.sh 1 matrix` **9/9** boot sạch; `ktest` **60/60**, guard **3/3**, user **17/17**.
- `soak.sh 1000 3` **0/1000** (226 s, BUILD_ID `cd0eb37b03c98a15`).
- clippy kernel `-D warnings` sạch (workspace + `--features ktest,user`).

## 6. Nợ còn lại (đồng bộ §13.9)

1. Core **multi-hart** chưa nối IPI (kernel `active_on` giao hart đang chạy core ⇒ targets=0);
   đường post/chờ ack mới có test host. Nối khi core chạy TCB trên hart khác (KT-65+).
2. `TlbFlush` theo `Generation` (cạn ASID) vẫn của `tlb::shootdown` KT-51.
3. `SchedContext.YieldTo`/`Consumed` chưa port.
4. `tcb_resume_with_regs` chưa có đường VM/W^X fault U-mode trong ảnh sản phẩm (KT-65 procsrv).
5. **KT-64a** (sửa sim theo D2-89/D2-90) **vẫn mở** — KT-64 không sửa sim.
