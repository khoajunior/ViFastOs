# KT-50 — IPI thật + drain hộp thư per-hart (K3) — nhật ký

Người làm: **Agent 2 dòng OS** (phiên KT-50). HEAD vào: `6268e4d5` (`[KT-00]` mở KT-50; parent
code KT-49 `1cc67908`). Working tree có `viscript/` của tab khác — **không chạm**; chỉ stage `v2/`.
`git rev-list --count HEAD..origin/main` = **0** trước khi làm.

Nguồn: `review/agent0/K3-PLAN.md` KT-50 (§4.1), **D2-68** (local flush + ack ngầm, **không**
self-target; 4 mutation; dạy "chờ ack mà không drain hộp thư ⇒ deadlock"), **D2-70** (bài học
ngưỡng KT-43 — KT-50 không đổi ngưỡng nào), **D2-72** Q4 (matrix `1/2/4`, `MAX_HARTS = 8`), Q5
(budget 262.144 B), Q7 (địa chỉ hardcode ACLINT), Q12 (Miri không áp kernel), D2-74 (`-icount
shift=6` chỉ trong `ktest.sh`).

## 1. Baseline (trước khi sửa)

- `./v2/bin/v2 gate --fast` → **PASS 569 test / 73 suite**, 0 failed
  ([`kt50-baseline-gate-fast.log`](kt50-baseline-gate-fast.log)).
- Kernel **không** nằm trong `gate --fast` (`v2/Cargo.toml` `exclude = ["kernel"]`) — như KT-49,
  kernel kiểm bằng build 3 ảnh + `ktest.sh` + `run.sh` + host test của workspace `v2/kernel`.
- Ảnh KT-49 (baseline) boot `ktest.sh 1 matrix` **20 TAP + 3 TAP guard** — log baseline trong
  worktree tạm ở §2 (`kt50-baseline-ktest.log`).
- Working tree `v2/` sạch trước khi sửa; `viscript/`, `review/`, `out-b.log`… của tab khác
  **không chạm**.

## 2. Đỏ trước

Dựng worktree tạm ở HEAD lúc đó (`1eeb1b23`; phần `v2/` **không đổi** so với `6268e4d5` của KT-49 —
chỉ có commit ST của dòng ViScript), **chỉ chép test/runner mới** rồi chạy — đỏ vì
**thiếu tính năng**, không vì hạ/nới ngưỡng:

1. Host test mới `vifast-plat-common/tests/ipi.rs` (10 test) → không biên dịch (`unresolved import
   vifast_plat_common::ipi`) — [`kt50-red-host.log`](kt50-red-host.log).
2. Runner mới (`ktest.sh` + `expected.txt` 25 TAP) trên ảnh KT-49 (20 TAP):
   `not ok - ktest smp=2: thiếu plan-1..25 ok-set-1..25 ok20-ipi-delivered … ok25-nested
   ipi-*-evidence` — [`kt50-red-ktest.log`](kt50-red-ktest.log).
3. Baseline thật (runner cũ trên ảnh cũ) vẫn xanh 20/20 + 3/3 — [`kt50-baseline-ktest.log`](kt50-baseline-ktest.log).

Worktree đỏ đã xoá (`git worktree remove --force`, prune).

## 3. Thay đổi

### 3.1 `vifast-plat-common` (không `unsafe`, host-test được)

- **`src/ipi.rs` (mới)**: hằng **SBI IPI** (EID `0x735049` "sPI", FID 0, `hart_mask_base = 0`),
  `SIE_SSIE`, hình học **ACLINT MSWI** (`MSWI_HART_STRIDE = 4`, `mswi_offset/address`,
  `msip_pending` — dữ kiện nền tảng + host test, arch **không** chạm CLINT, xem §3.4);
  **`IpiKind`** `{TlbFlush, VmKick, Wake}` + **`KindSet`** (bitmask 3 bit, bỏ bit lạ);
  **`IpiMailbox`** per-hart **coalesce theo kind** (`post` trả bit mới/gộp, `drain` một `swap(0)`,
  bộ đếm `posted/coalesced/drains/delivered/empty_drains`, bất biến `posted = delivered + pending`);
  **`PerHartIpiMailboxes`** + `IPI_MAILBOXES`; **`IpiCounters`/`PerHartIpiStats`/`IPI_STATS`**
  (`sent/coalesced/local/errors/last_error/traps/delivered/spurious/kinds[3]`); **`delivery_for`**
  (self-target ⇒ `Local` — D2-68); **`wait_with_drain`** (drain trước mỗi bước, timeout luôn kết
  thúc) + `deadline_after`.
- **`src/trap.rs`**: `INTERRUPT_SUPERVISOR_SOFTWARE = 1`; `Event::Ipi` (nhánh **riêng**);
  `TrapAction::Ipi`; `decode_scause` tách **tường minh** 1/5/9 (`Ipi`/`Timer`/`Interrupt{9}`);
  `decode_trap`/`handle_trap` passthrough; tài liệu module cập nhật (IPI không fatal).
- **`src/lib.rs`**: `pub mod ipi;` + tài liệu phạm vi KT-50.
- **`tests/ipi.rs` (mới, **12 test** — 10 đầu + 2 theo D2-77/D4)**: hằng SBI/MSWI; `KindSet` bit/ngoài dải; **coalesce không mất
  kind** (5 trùng + 1 khác ⇒ 2 bit, `coalesced=4`, một drain giao 2, `posted = delivered + pending`);
  drain mỗi kind đúng một lần; **self-target local** (`sent`/`traps` = 0); bộ đếm
  `sent/coalesced/traps/delivered/spurious/kinds`; PerHart không lẫn + hart ngoài dải;
  `wait_with_drain` **drain trước mỗi bước** (kể cả khi ready ngay); **timeout phải hết** (đồng hồ
  giả, `drain` vẫn chạy); decode **không nhầm** timer/external/exception + `scause = 0x8000_0000_0000_0001`;
  **D2-77** (post hart offline ⇒ `Offline`, không SBI, không giữ kind, `sent=0`) và **D4**
  (`unpost` gỡ bit khi SBI lỗi, giữ bất biến `posted = delivered + pending`).

### 3.2 `vifast-plat-riscv64` (asm/CSR/SBI — `unsafe` thuộc D2-17)

- **`src/ipi.rs` (mới)**: `sbi_send_ipi` (`ecall` EID `0x735049`, `a0/a1` đều `inout` vì SBI trả
  error/value); `send_ipi_raw`/`send_ipi` (đường test/raw có đếm `sent`); **trạng thái ngắt phần
  mềm** đọc/xoá bằng **`sip.SSIP`** (xem §3.4): `ipi_pending_current`, `clear_ipi_current`;
  `enable_ipi` (`sie.SSIE` + `sstatus.SIE`) / `disable_ipi` / `set_global_irq`; **`post`**
  (không self-target ⇒ local flush khung KT-51 + `local`, remote ⇒ đặt kind **trước** rồi SBI IPI,
  trùng ⇒ coalesce); `drain_current`/`on_ipi_interrupt` (đọc SSIP **trước** khi xoá ⇒ `spurious`);
  `pending_of`; **`wait_drain`** (bọc `wait_with_drain`, timeout `rdtime`).
- **`src/lib.rs`**: `pub mod ipi;`; `trap_dispatch` thêm nhánh `Event::Ipi` (drain + ghi
  `TRAP_STATS` mã 1); `park_forever` **drain** hộp thư của chính hart mỗi vòng (D2-68: hart park
  vẫn nhận kind, không phụ thuộc trap); `TrapAction::Ipi` trong match `unreachable`.
- **`src/smp.rs`**: `boot_hart()` (hart boot thật, không hardcode 0).
- **`src/mmu.rs`**: **giữ nguyên** mapping (không map CLINT); cập nhật tài liệu lý do (§3.4).

### 3.3 `vifast-kbin-riscv64` (ảnh test)

- **`src/ktest_ipi.rs` (mới)**: pha IPI sau pha SMP, vòng barrier 7..13; TAP **20..24** (bảng
  §13.5 của `v2/docs/k3-riscv64.md`); BSP đo + in bằng chứng `# ipi:`, `# ipi delivered:`,
  `# ipi phase1 wait_ticks=`, `# ipi drain:`, `# ipi self:`, `# ipi coalesce:`, `# ipi distinct:`;
  AP tham gia theo `is_peer` (hart online nhỏ nhất khác BSP — `-smp 4` chỉ một cặp đo chéo), mọi
  barrier **timeout ⇒ park** (không để hart rời pha giữa chừng làm bẩn phép đo — bài học đo được
  ở bản thử: `delivered > traps`).
- **`src/ktest_smp.rs`**: `bsp_barrier`/`ap_barrier` thành `pub(crate)`; `rust_secondary_main` gọi
  `ktest_ipi::secondary_phase(hart)` thay vì park ngay.
- **`src/ktest.rs`**: `mod ktest_ipi;`, kế hoạch `1..25`, gọi `ipi_phase()` sau `smp_phase()`, dời
  trap lồng sang `ok 25`.
- **`v2/test/boot/expected.txt`**: `ktest 25 25` (guard 3/3 không đổi).
- **`v2/test/boot/ktest.sh`**: grep TAP 20..25 + bằng chứng IPI **theo `-smp`**: `≥ 2` dùng
  `mode=cross` (`delivered=100/traps=100/kinds=34/33/33`, drain `mutual=1`, coalesce
  `sent_delta=2`), `1` dùng `mode=self-ipi` (`delivered=0/traps=100`, drain `mode=self-wait`,
  coalesce `mode=mailbox`) — hai cấu hình **cùng** bất biến, không "xanh hình thức".

### 3.4 Deviation **đo được**: ACLINT MSWI → `sip.SSIP`

Bản thử đầu tiên map trang ACLINT MSWI 4 KiB `Device RW` rồi handler đọc `0x0200_0004` (MSIP hart
1) từ S-mode. Kết quả: **load access fault** (`scause = 5`, `tval = 0x0200_0004`) — PMP của
firmware **không** mở ACLINT cho S-mode; vì trap xảy ra **trong** handler IPI, `trap_entry` phát
hiện trap lồng ⇒ `PANIC: nested trap` + halt. Bằng chứng QEMU `-d int`:
[`kt50-aclint-pmp.log`](kt50-aclint-pmp.log). Vì vậy:

- `mmu::map_mmio` **không** map CLINT (giữ nguyên như KT-47/48 — timer qua SBI);
- đọc/xoá trạng thái ngắt phần mềm bằng **`sip.SSIP`** (privileged spec: S-mode được đọc/ghi; cùng
  trạng thái với MSIP mà `scause = (1<<63)|1` phản ánh);
- hằng ACLINT/`mswi_*` **vẫn** ở `plat-common` (dữ kiện nền tảng + host test), tài liệu §13.4.

Đây là deviation **đo được**, không nới ngưỡng nào; đã ghi `v2/docs/k3-riscv64.md` §13.4 + STATE.
**Agent 0 chốt D2-76** (`d598bf2d`): `sip.SSIP` là **nguồn sự thật** cho IPI S-mode, **cấm** map
CLINT/MSWI vào MMIO kernel; gửi SBI EID `0x735049`, xoá pending `csrc sip, SSIP`, nhận
`scause=(1<<63)|1` — không phải workaround.

Ghi chú phạm vi bằng chứng (lead yêu cầu, đã vào §13.7 khoản 7): `traps_delta = 0` khi `SIE = 0`
chỉ chứng minh **ngắt không được nhận** (không vào `stvec`), **không** chứng minh IPI không tới —
kind vẫn tới hộp thư và được drain trong vòng chờ; muốn chứng minh "không IPI nào tới" phải đọc
pending + đếm riêng, thuộc KT-51.

## 4. Bằng chứng

### 4.1 Cổng và host test (sau khi vá verdict)

- `./v2/bin/v2 gate --fast` → **PASS 569 test / 73 suite**, 0 failed, L1 1.000 seed
  ([`kt50-gate-fast.log`](kt50-gate-fast.log) — log **sau vá**).
- Host test kernel (`cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-plat-common`) →
  **67/67** = 14 host + **12 IPI (mới)** + 8 mmu + 12 timer + 12 PLIC + 9 SMP
  ([`kt50-host-tests.log`](kt50-host-tests.log)).
- Ảnh sản phẩm: `run.sh 1 2` → **1/1 boot sạch**, `ktest-strings=0`, TAP `ok 1 - boot 1 (smp=2)`
  ([`kt50-product-boot-smp2.log`](kt50-product-boot-smp2.log)) — chứng minh `park_forever` drain
  không làm hỏng ảnh sản phẩm.
- Budget (D2-72 Q5, trần 262.144 B, `.data+.bss`): sản phẩm **2840 B**; `ktest` **4304 B**;
  `ktest-guard` **2688 B**; `.stack`/`.trap_stack`/`.hart_stacks` NOLOAD báo riêng
  ([`kt50-budget.log`](kt50-budget.log)) — **không** đổi cấu hình budget.

### 4.2 Ktest — matrix `1/2/4`, 50 boot mỗi N (300 boot)

`bash v2/test/boot/ktest.sh 50 matrix` → **300/300 boot sạch** (`# KTEST: 300/300`,
`smp=1/2/4: 100/100` mỗi cấu hình), **0 `not ok`**, TAP **25/25** ảnh chính + **3/3** ảnh guard
([`kt50-boot50-matrix.log`](kt50-boot50-matrix.log)). Đây là lần chạy **sau khi vá verdict** (ảnh
`sha256 bfdd7785…` ktest / `8b088aa3…` guard, khớp `kt50-ipi.log`), trên máy có **một** run khác
đang chạy song song trong ~6 phút đầu (đã kill đúng PID khi phát hiện; log vẫn đủ 300 dòng `ok`).

TAP IPI theo cấu hình (log mẫu `kt50-ktest-smp{1,2,4}.log`):

```
# -smp 1 (mode=self-ipi, không có hart đích — TAP 20 tên "traps (self)" theo D5)
# ipi delivered: mode=self-ipi sent=100 delivered=0 traps=100 coalesced=0 spurious=0 kinds=0/0/0 timeout=0
# ipi drain: mode=self-wait mutual=0 traps_delta=0/0 timeout_ended=1 timeout_drains=9413
# ipi self: ok=0x1 sent_delta=0 traps_delta=0
# ipi coalesce: mode=mailbox posts=6 coalesced=4 pending=0x3 peer_delivered=2 ok=1
# ipi distinct: ipi_traps=100 interrupts_delta=100 timer_delta=0 plic_delta=0 last_scause=0x8000000000000001
ok 20 - 100 ipi traps (self)

# -smp 2/4 (mode=cross, 2 hart — TAP 20 tên "delivered")
# ipi delivered: mode=cross sent=100 delivered=100 traps=100 coalesced=0 spurious=0 kinds=34/33/33 timeout=0
# ipi drain: mode=cross mutual=1 boot_ok=1 peer_ok=1 traps_delta=0/0 timeout_ended=1
# ipi self: ok=0x3/0xf local=1 sent_delta=0 traps_delta=0 kinds_tlb=2
# ipi coalesce: mode=cross posts=6 coalesced=4 sent_delta=2 pending=0x3 peer_bits=0x3 peer_delivered=2 ok=1
# ipi distinct: ipi_traps=100 interrupts_delta=100 timer_delta=0 plic_delta=0 last_scause=0x8000000000000001
ok 20 - 100 ipi delivered
```

Bản đầy đủ 3 cấu hình (nguyên văn, không sửa số): [`kt50-ipi.log`](kt50-ipi.log).

`# ipi phase1 wait_ticks=50000157` (10 MHz) ở `-smp 2` là **thời gian ảo** của pha 100 delivery
(`-icount shift=6` chia lượt vCPU ~50 ms/delivery) — ghi để người đọc hiểu chi phí, **không** phải
ngưỡng DoD (§13.7 khoản 2).

### 4.3 Ảnh và budget sau khi đóng băng mã

`sha256` ảnh test của lần chạy 300 boot **sau vá**: `bfdd77854f15ed6da8b188a082912aaeae49b17c474068c1c46f4d757ad49c53`
(ktest, 131.120 B) và `8b088aa325ea5e1c532911b71269f9e182af738555ce2c5eb41d2bf17566911f` (guard,
53.832 B) — build lại từ nguồn cuối cho **cùng hash**, nên log 300 boot ứng đúng mã commit.
(Lần đầu `f8b94520…`/`d56ecf91…` là trước vá verdict — không dùng làm bằng chứng cuối.)

## 5. Review độc lập

`reviewer-kt50` (read-only) tự chạy host **65/65**, `ktest.sh 1 matrix` **6/6 (25/25 + 3/3)**, gate
**569**; kết luận **DRIFT nhẹ — không UNSAFE**, lõi đạt (`sip.SSIP` đúng, không map CLINT,
self-target không gửi SBI, drain trước mỗi bước, coalesce đúng). 6 mục đã vá:

| # | Mục | Vá |
|---|---|---|
| D1 | comment nói ngược **D2-76** (`plat-common/src/ipi.rs`, `lib.rs`) | hằng ACLINT còn lại chỉ là **dữ kiện nền tảng + host test**; arch dùng `sip.SSIP`; **cấm map** CLINT — sửa `ipi.rs` (module doc + `MSWI_*` + `msip_pending`), `lib.rs` |
| D2 | số cũ trong `k3-riscv64.md` (§4 "mã khác 5/9 vẫn fatal"; `1..20`/`20 TAP`/`ok 20 - trap nested panic`/`ktest 20/20`) và comment `ktest.sh` | đồng bộ **25 TAP** + "khác 1/5/9" + `ok 25 - trap nested panic` + `1..25` |
| D3 | **D2-77**: `post` tới hart **valid nhưng chưa online** | `classify_post(sender,target,online_mask)` thuần ở `plat-common`; arch `post` trả `Invalid` + log `# ipi: hart N chua online` (`IPI_ERR_HART_OFFLINE`), **không** gọi SBI, **không** đặt kind vào hộp thư; **test** host mới (sent=0, hộp thư rỗng) |
| D4 | lỗi SBI khi gửi | `IpiMailbox::unpost(kind)` **gỡ bit vừa post** + `errors+=1`; test host `unpost` giữ bất biến `posted = delivered + pending` |
| D5 | nhãn TAP 20 ở `-smp 1` (`delivered=0` dễ đọc sai) | TAP đổi tên theo cấu hình: `-smp ≥ 2` = `100 ipi delivered`; `-smp 1` = `100 ipi traps (self)`; `ktest.sh` grep theo `IPI_TAP20` (khai báo **trước** vòng TAP) |
| D6 | `kt50-boot50-matrix.log`/`kt50-ipi.log` chưa có (log 300 boot bị hai lần chạy ghi cùng tệp) | chạy lại **300 boot sạch** sau khi vá, chép log thật + `kt50-ipi.log` (trích pha IPI từ boot thật) vào JOURNAL; nếu hỏng giữa đường ghi đúng số đã chạy |

Host test sau vá: **67/67** (14 host + **12 IPI** + 8 mmu + 12 timer + 12 PLIC + 9 SMP).

## 6. Đề xuất

1. **KT-51 (ASID + shootdown + ack)**: dùng đúng khung KT-50 — `IpiKind::TlbFlush` đã có kind +
   drain; KT-51 thay `local_flush()` (hiện chỉ đếm) bằng `sfence.vma` thật, thêm `acked_mask` +
   `active_on`, ack ngầm cho hart gọi (D2-68), **timeout bằng `wait_drain`** (đã có primitive) và
   4 mutation theo D2-68. Không cần đụng SBI/CSR/trap một lần nữa.
2. **Reset hộp thư lúc hart start** (nối D2-77): `post` đã từ chối hart offline nên hiện không có
   bit mồ côi; khi có hotplug/restart (ngoài K3), `secondary_setup` phải `drain`/xoá hộp thư trước
   khi hart nhận việc — ghi thành điều kiện của task hotplug, không thêm bây giờ.
3. **Đo latency IPI thật** (ngoài `-icount`): KT-50 chỉ đo **ngữ nghĩa** (không mất/không nhân đôi)
   dưới thời gian ảo; muốn số jitter/latency thật phải là task riêng (cùng loại §10.5), không đổi
   D2-74.
4. **Nhiều người gửi đồng thời trên phần cứng**: TAP chỉ đo **một** peer ở `-smp 4`; host test
   kiểm MPSC/atomic. KT-51/57 (shootdown nhiều hart) sẽ là phép đo tự nhiên cho nhiều sender; nếu
   cần đo sớm, thêm phase 2 peer trong task riêng — **không** nới ngưỡng TAP hiện tại.
5. **`-smp 1` self-IPI**: giữ `mode=self-ipi` + TAP `100 ipi traps (self)` (D5) — nếu Agent 0 muốn
   cấm hẳn self-IPI ngay cả đường test thì phải bỏ phép đo 1 hart và ghi rõ matrix 1 chỉ kiểm
   self-target/coalesce; hiện giữ vì nó đo được **đường SBI + trap + drain** thật trên 1 hart.
6. **Chi phí thời gian ảo**: pha 100 delivery ~5 s thời gian ảo (mỗi delivery một lượt M-mode +
   `sret` + trap dưới icount). Nếu KT-57 soak 1.000 boot thấy wall-time tăng, tách phép đo 100
   delivery sang ảnh test riêng hoặc giảm số vòng **chỉ khi** Agent 0 duyệt đổi DoD — không tự
   giảm.
7. **Đề nghị Agent 0**: lật KT-50 sang **DONE** sau khi commit + push (verdict `reviewer-kt50` =
   DRIFT nhẹ, 6/6 mục đã vá); ghi nhận D2-76 (đã có) là nguồn sự thật ACLINT. Không mở lại
   `spec/`, không cần D2 mới cho KT-50 ngoài D2-76/D2-77 đã chốt.

**Giới hạn còn lại** (không giấu): 300 boot là **smoke** (không phải soak 1.000 của KT-57); phép đo
IPI chỉ một peer; `traps_delta=0` không chứng minh "không có IPI tới" (chỉ chứng minh không nhận
ngắt — §13.7 khoản 7); `TlbFlush` chưa xả TLB thật (KT-51).
