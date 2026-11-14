# KT-51 — ASID thật + `sfence.vma` + shootdown ack (K3) — nhật ký

Người làm: **Agent 2 dòng OS** (phiên KT-51). HEAD vào: `d06607eb` (`[KT-00]` lật KT-50 DONE, mở
KT-51; parent code KT-50 `74c22d4b`). Working tree có `viscript/`, `review/`, `out-b.log`… của tab
khác — **không chạm**; chỉ stage `v2/`. `git rev-list --count HEAD..origin/main` = **0** trước khi
làm (kiểm lại trước push).

Nguồn: `review/agent0/K3-PLAN.md` KT-51 (§4.1), **D2-65 Q4/Q5** (ASID 8 bit + generation, W^X là
KT-56; invalidate `(asid,va)`/`asid`/`gen`; ack `fetch_max`, `Ok` chỉ khi đủ ack, chờ ack phải
drain), **D2-68** (hart gọi **local flush + ack ngầm**, **không** self-target; drain trong vòng
chờ; 4 mutation — KT-51 làm 3 mutation bắt buộc của task), **D2-72** Q4/Q5/Q6, **D2-74**
(`-icount shift=6` chỉ trong `ktest.sh`), **D2-76/D2-77** (IPI S-mode qua `sip.SSIP`; hart offline
không giữ kind), **D2-65 Q10** (mutation + Miri: Miri **không** áp kernel `no_std`, D2-72 Q12).

## 1. Baseline (trước khi sửa)

- `./v2/bin/v2 gate --fast` → **PASS 569 test / 73 suite**, 0 failed, L1 1.000 seed
  ([`kt51-baseline-gate-fast.log`](kt51-baseline-gate-fast.log)).
- Ảnh KT-50 (baseline) boot `ktest.sh 1 1` → **25/25 + 3/3**; working tree `v2/` sạch trước khi sửa.
- Kernel **không** nằm trong `gate --fast` (`v2/Cargo.toml` `exclude = ["kernel"]`): kiểm bằng
  build 3 ảnh + `ktest.sh` + `run.sh` + host test workspace `v2/kernel`.

## 2. Đỏ trước (test-first, không hạ ngưỡng)

Worktree tạm **tách hẳn** ở HEAD `d06607eb` (`git worktree add --detach`), chỉ chép **test/runner
mới** (không chép code): `v2/test/boot/ktest.sh` + `expected.txt` (30 TAP) và
`vifast-plat-common/tests/tlb.rs`. Kết quả **đỏ vì thiếu tính năng**:

1. Runner mới trên ảnh KT-50 cũ: `not ok - ktest smp=1 boot 1: thiếu plan-1..30 ok-set-1..30
   ok-count=25/30 ok25-shootdown ok26-stale ok27-outside ok28-rollover ok29-timeout ok30-nested
   tlb-init-evidence tlb-phase-evidence tlb-probe-evidence shootdown-rounds-evidence
   shootdown-peer-evidence stale-evidence stale-form-{Page,Asid,All} outside-evidence
   rollover-evidence timeout-evidence tlb-asid-evidence` — [`kt51-red-ktest.log`](kt51-red-ktest.log)
   (`# KTEST: 1/2`; guard cũ vẫn 3/3 nên lỗi **không** do hạ tầng).
2. Host test mới trên HEAD cũ: `unresolved import vifast_plat_common::tlb` + `no satp_asid in mmu`
   — [`kt51-red-host.log`](kt51-red-host.log).

Worktree đỏ đã xoá (`git worktree remove --force` + `git worktree prune`).

## 3. Thay đổi

### 3.1 `vifast-plat-common` (không `unsafe`, host test được)

- **`src/tlb.rs` (mới)**: `ASID_BITS = 8` ⇒ `ASID_COUNT = 256`, `ASID_MASK`, `SATP_ASID_SHIFT = 44`
  (trường ASID của `satp` 16 bit, kernel dùng 8 bit); **`AsidAllocator`** (free-list 4 × `AtomicU64`
  + `retired` + `generation/issued/exhaustions/frees`, khoá CAS; cấp ASID thấp nhất trước; cạn ⇒
  `generation += 1`, nạp lại 256, lease `rollover = true`; `free` = **về hưu** trong thế hệ, chặn
  double-free/sai thế hệ); **`FlushScope`** `Page{va}`/`Asid`/`All` + `code`/`decode`/`va`/`name`
  (một cửa mã hoá cho slot); **`TlbFlushRequest`** `{seq, asid, scope}`; **`TlbShootdownInfo`**;
  **hàm thuần** `acked_mask`, `targets` (loại hart gọi), `implicit_ack` (chỉ khi hart gọi thuộc
  `active_on`), `unused_online_harts`, `ack_complete`, `ack_missing`; **`TlbCounters`/`TLB_STATS`**
  PerHart (`sent/received/flushes/acks/timeouts/errors/page/asid/all/implicit/wrong`).
- **`tests/tlb.rs` (mới, 11 test)**: hằng số 8 bit + trường `satp`; 256 ASID **không trùng** trong
  thế hệ; cạn ⇒ `generation+1` + `rollover` + nạp lại; `free` không trả free-list + double-free;
  `free` sai thế hệ bị từ chối; `satp_sv39_asid` giữ MODE/PPN; mã hoá/giải mã 3 dạng + mã lạ;
  `acked_mask` theo `seq`; targets/implicit/ngoài `active_on`; bộ đếm PerHart + `flush_total`;
  cùng dãy thao tác ⇒ cùng kết quả.
- **`src/mmu.rs`**: `satp_sv39_asid(root, asid)` + `satp_asid(satp)` (chỉ đổi trường ASID).
- **`src/lib.rs`**: `pub mod tlb;` + phạm vi KT-51.

### 3.2 `vifast-plat-riscv64` (asm/CSR — `unsafe` thuộc D2-17)

- **`src/tlb.rs` (mới)**: `sfence.vma` **ba dạng** `sfence_page(asid, va)` (rs1 = VA, rs2 = ASID),
  `sfence_asid(asid)` (rs1 = `x0`), `sfence_all()` (rs1 = rs2 = `x0`), `flush_scope`; **lease kernel**
  `ASIDS`/`LEASE_ASID`/`LEASE_GEN`/`ACTIVE_ON`; `init()` (BSP: cấp lease, ghi `satp.ASID`, in report),
  `activate(hart)` (AP sau `adopt_satp`, kiểm `satp_asid`), `replace_lease(lease)` (rollover ⇒
  **một** `FlushScope::All`), `allocate_lease`/`free_lease`, `asid_state`; **slot yêu cầu/hart** 4
  `AtomicU64` (`asid/scope/va` → `seq` Release → `post(TlbFlush)`); **`handle_kinds`** gọi từ **mọi**
  đường drain (`ipi::drain_hart_current`), đọc slot (Acquire), **adopt** ASID yêu cầu, `sfence` đúng
  dạng, `ACKED.fetch_max(seq)`; **`shootdown(scope, timeout)`** (D2-78: **không** nhận mask — tự
  đọc `active_on()` bên trong): local flush → ack ngầm nếu hart gọi thuộc `active_on` → post cho
  các hart **khác** → chờ `ack_mask == active_on` với drain mỗi bước + deadline `rdtime`, trả
  `Completed`/`TimedOut`/`Busy`; `IN_FLIGHT` một shootdown tại một thời điểm; `test_active_on_set`
  (**chỉ ảnh test**, D2-78) để TAP 27 gỡ/khôi phục hart khỏi `ACTIVE_ON` thật; `service_until` cho
  hart phục vụ; `ack_mask`/`acked_seq`/`request_seq`/`last_shootdown` (bằng chứng TAP).
- **`src/mmu.rs`**: `set_satp_asid(asid)` (ghi `satp` giữ MODE/PPN, cập nhật `SATP_VALUE` cho
  `adopt_satp`; **không** `sfence` — lý do ở §14.1 docs), `current_satp_asid`; **chỉ ảnh test**:
  `PROBE_VA`/`PROBE_PA_A/B` + `PROBE2_VA`/`PROBE2_PA_C/D` (2 leaf 2 MiB RW NX map trong `mmu::init`,
  cùng bảng cấp 1 nên **không** thêm bảng/arena), `remap_probe`/`remap_probe2` (chỉ sửa một PTE leaf
  cấp 1), `probe_leaf_phys`/`probe2_leaf_phys`.
- **`src/ipi.rs`**: `drain_hart_current` gọi `tlb::handle_kinds(kinds)` (trap handler, vòng chờ khoá,
  `park_forever` đều xử lý `TlbFlush`).
- **`src/smp.rs`**: `secondary_setup` gọi `tlb::activate(hart)` — hart vào `active_on` **chỉ khi** đã
  nạp đúng ASID (nếu không, hart không online ⇒ TAP đỏ, không che).
- **`src/lib.rs`**: `pub mod tlb;`.

### 3.3 `vifast-kbin-riscv64` (ảnh)

- **`src/ktest_tlb.rs` (mới)**: pha TLB sau pha IPI, barrier vòng 21..120 + 129..135 + 140; TAP
  **25..29**; BSP đo + AP prime/verify (peer đọc lại mapping; **mọi** hart trong `active_on` phục
  vụ — D2-78); TAP 27 gỡ hart khỏi `ACTIVE_ON` thật rồi khôi phục (không mask hẹp); TAP 29 đặt
  `ACTIVE_ON` chứa hart offline thật; probe #2 tách khỏi probe #1 để không nhiễu TLB; mọi
  barrier/shootdown có timeout ⇒ hart lỗi **park** (TAP đỏ) không treo máy.
- **`src/ktest.rs`**: `mod ktest_tlb;`, kế hoạch `1..30`, gọi `tlb::init()` **trước** pha SMP (hart
  thứ cấp `adopt_satp` nhận ASID), gọi `ktest_tlb::tlb_phase()` sau `ipi_phase()`; trap lồng
  **25 → 30**; doc TAP 25..30.
- **`src/ktest_ipi.rs`**: AP sau pha IPI nối sang `ktest_tlb::secondary_phase(hart)`.
- **`src/main.rs`** (sản phẩm): `tlb::init()` sau `mmu::init()` + dòng `# tlb:` — ảnh sản phẩm dùng
  đúng đường ASID; không có shootdown/TAP.
- **`v2/test/boot/expected.txt`**: `ktest 30 30`; **`ktest.sh`**: grep TAP 25..30 + bằng chứng
  `# tlb init:`, `# tlb:`, `# tlb probe:`, `# shootdown rounds/peer:`, `# stale:` + 3 dòng
  `# stale form:`, `# outside:`, `# rollover:`, `# timeout:`, `# tlb asid:` **theo `-smp`**
  (`≥2` `mode=cross` `sent=100`; `1` `mode=self` `sent=0`); grep `outside` **không ghim số hart**
  (OpenSBI có thể boot hart khác 0 — KT-49; guest tự kiểm `acked == targets`).

### 3.4 Không đổi (đúng phạm vi)

Không sửa `spec/`, sim, `.scn`, `QUEUE.md`, `DECISIONS.md`, `AGENTS.md`, `viscript/`; không thêm
dependency; không `cfg(target_arch)`; `unsafe` chỉ trong `kernel/vifast-plat-*`; W^X/FaultKind
(KT-56), object layer (KT-52), U-mode (KT-53) **không** đụng.

## 4. Bằng chứng

### 4.1 Host test (D2-72 Q12)

`cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-plat-common` → **78/78** = 14 host KT-45
+ 12 IPI KT-50 + 8 MMU + 12 timer + 12 PLIC + 9 SMP + **11 TLB KT-51**
([`kt51-host-tests.log`](kt51-host-tests.log)).

### 4.2 Ktest — 3 cấu hình, ảnh cuối

`ktest.sh 1 matrix` trên ảnh cuối (sha256 ở §4.4) → smp1/2/4 đều **30/30 + guard 3/3**; log mẫu
[`kt51-ktest-smp1.log`](kt51-ktest-smp1.log), [`kt51-ktest-smp2.log`](kt51-ktest-smp2.log),
[`kt51-ktest-smp4.log`](kt51-ktest-smp4.log). Trích **nguyên văn** (không viết lại số):

```
# -smp 2 (2 hart — active_on = cả 2)
# shootdown rounds: mode=cross rounds=100 ok=100 acked_ok=100 sent=100 sent_expected=100 self_slot_wrong=0 local_verify_wrong=0 barrier_bad=0 targets=0x3 active_on=0x3 wait_ticks=99987992
# shootdown all: bad=0 served=1 expected_served=1 online=0x3
# shootdown peer: mode=cross received=100 flushes=100 page=100 asid=0 all=0 implicit=0 prime_wrong=0 verify_wrong=0 service_timeout=0
# stale: asid=0 va=0x80800000 forms=3 stale_seen=3 fresh_ok=3 wrong=0 note=stale-read-without-sfence-is-positive
# outside: mode=cross removed=0x1 removed_count=1 narrowed=0x2 targets=0x2 acked=0x2 caller_in=0 removed_acked_before=0x1 removed_acked_after=0x1 restored=1 restored_acked=0x3 outside_wrong=0 ok=1
# rollover: generation=1 exhaustions=1 rollover=1 flush=Ok asid=0 satp_asid=0 stale_seen=1 fresh_ok=1 free=255 issued=1 asid_ok=0x3 online=0x3 primed=0x4b51c000000000c2 wrong=0
# timeout: outcome=Timeout narrowed=0x5 targets=0x5 acked=0x1 missing=0x4 waited=1000147 probe=1000000 timeouts=1 mailbox_empty=1 recovered=1 wrong=0
# tlb asid: ok=0x3 online=0x3 phase=0x2 arrived=0x3 timeout=0 satp_asid_boot=0 lease_asid=0 generation=1
ok 25 - 100 vong shootdown Ok
ok 26 - stale mapping caught
ok 27 - hart ngoai active_on
ok 28 - rollover flush
ok 29 - timeout khong treo

# -smp 4 (D2-78: gỡ **2 hart phụ** khỏi ACTIVE_ON thật ở TAP 27)
# shootdown rounds: mode=cross rounds=100 ok=100 acked_ok=100 sent=300 sent_expected=300 self_slot_wrong=0 local_verify_wrong=0 barrier_bad=0 targets=0xf active_on=0xf wait_ticks=99987960
# shootdown all: bad=0 served=3 expected_served=3 online=0xf
# outside: mode=cross removed=0xc removed_count=2 narrowed=0x3 targets=0x3 acked=0x3 caller_in=1 removed_acked_before=0xc removed_acked_after=0xc restored=1 restored_acked=0xf outside_wrong=0 ok=1
# rollover: generation=1 exhaustions=1 rollover=1 flush=Ok asid=0 satp_asid=0 stale_seen=1 fresh_ok=1 free=255 issued=1 asid_ok=0xf online=0xf primed=0x4b51c000000000c2 wrong=0
# timeout: outcome=Timeout narrowed=0x11 targets=0x11 acked=0x1 missing=0x10 waited=1540659 probe=1000000 timeouts=1 mailbox_empty=1 recovered=1 wrong=0

# -smp 1 (không có hart đích — log ghi mode=self, không giả 2 hart)
# shootdown rounds: mode=self rounds=100 ok=100 acked_ok=100 sent=0 sent_expected=0 self_slot_wrong=0 local_verify_wrong=0 barrier_bad=0 targets=0x1 active_on=0x1 wait_ticks=40504
# shootdown all: bad=0 served=0 expected_served=0 online=0x1
# outside: mode=self removed=0x1 removed_count=1 narrowed=0x0 targets=0x0 acked=0x0 caller_in=0 removed_acked_before=0x1 removed_acked_after=0x1 restored=1 restored_acked=0x1 outside_wrong=0 ok=1
# timeout: outcome=Timeout narrowed=0x3 targets=0x3 acked=0x1 missing=0x2 waited=1000124 probe=1000000 timeouts=1 mailbox_empty=1 recovered=1 wrong=0
```

Ảnh **sản phẩm** (ASID thật, không shootdown): `run.sh 1 2` → **1/1 boot sạch**
(`# tlb: asid=0 generation=0 satp_asid=0 active_on=0x1` + `# smp:` đủ 2 hart) — log
[`kt51-product-boot-smp2.log`](kt51-product-boot-smp2.log).

### 4.3 3 mutation bắt buộc (KT-51) — diff + log đỏ

Mỗi mutation sửa **một** điểm trong `vifast-plat-riscv64/src/tlb.rs`, build lại ảnh test, chạy
`ktest.sh 1 2`, lưu **nguyên văn** log QEMU + diff, rồi **restore** (kiểm `diff -q` = khớp; grep
`MUTATION` trong nguồn = 0 trước khi build ảnh cuối).

| Mutation | Sửa | Đỏ đo được (log) |
|---|---|---|
| **M1 — bỏ `sfence.vma`** | ba helper thành no-op | TAP 25/26/27/28 đỏ: `local_verify_wrong=50`, peer `prime_wrong=50 verify_wrong=50`, `stale_seen=0` (fresh sau "flush" sai) — [`kt51-mutation-1-bay-sfence-qemu.log`](kt51-mutation-1-bay-sfence-qemu.log), [`.diff`](kt51-mutation-1-bay-sfence.diff) |
| **M2 — bỏ chờ ack** | không drain/không chờ, trả ngay | TAP 25 (`ok=0 acked_ok=0`), TAP 27/28/29 đỏ (`flush=Fail`; `timeout: waited=587 < probe`) — [`kt51-mutation-2-bo-cho-ack-qemu.log`](kt51-mutation-2-bo-cho-ack-qemu.log), [`.diff`](kt51-mutation-2-bo-cho-ack.diff) |
| **M3 — self-target** | hart gọi cũng bị gửi IPI + bỏ ack ngầm | TAP 25 (`sent=200` vs `sent_expected=100`, `self_slot_wrong=100`), TAP 27 (`caller_out_ok=0` — slot của hart gọi khác 0) — [`kt51-mutation-3-self-target-qemu.log`](kt51-mutation-3-self-target-qemu.log), [`.diff`](kt51-mutation-3-self-target.diff) |

**Giải thích vì sao M1 đỏ** (reviewer chỉ đúng chỗ này): đỏ **chính** ở lần đọc **đầu tiên sau
remap** — TLB còn translation cũ nên `fresh_a` sai ngay (TAP 26 `stale_seen=0` vì bước "fresh_a"
đã sai trước) và peer `verify_wrong` tăng. Ở TAP 25, 100 vòng xen kẽ `PA_B/PA_A`; khi **không**
`sfence`, entry TLB trỏ PA của vòng trước nên **một nửa** số vòng đọc ra marker cũ (nửa còn lại đọc
trúng PA vừa ghi) — `local_verify_wrong=50` là dấu hiệu đúng của "TLB còn cache bản cũ", không
phải flake (chạy lại cho cùng kết quả).

### 4.4 Ảnh + budget (D2-72 Q5 — trần 262.144 B, không nới)

- sha256 ảnh **cuối** (ghi trước + sau khi chạy 300 boot, phải khớp —
  [`kt51-image-sha.log`](kt51-image-sha.log), đuôi [`kt51-boot50-matrix.log`](kt51-boot50-matrix.log)).
- `bash budget.sh` từng ảnh: `.data+.bss` **product 3656 B / ktest 5608 B / guard 2680 B** (trần
  262.144), `.stack`/`.trap_stack`/`.hart_stacks` NOLOAD báo riêng — [`kt51-budget.log`](kt51-budget.log).
- Probe leaf chỉ ở ảnh test, cùng bảng cấp 1 đã có ⇒ **không** thêm bảng/arena; ảnh sản phẩm không
  có mapping/tham số test (chỉ +ASID statics nhỏ).

### 4.5 300 boot — 50 boot mỗi `N` = 1/2/4 (ảnh cuối, không retry)

`bash v2/test/boot/ktest.sh 50 matrix` (ảnh **sau vá D2-78**, sha256 đầu/cuối khớp
`kt51-image-sha.log`) → `# KTEST: 300/300 boot sạch` (`smp=1: 100/100`, `smp=2: 100/100`,
`smp=4: 100/100`), 0 `not ok`, `EXIT=0` ([`kt51-boot50-matrix.log`](kt51-boot50-matrix.log)).

**Bài học vận hành** (ghi lại, không giấu): lần chạy 300 boot **đầu** có 300/300 boot xanh và 0
`not ok`, nhưng tôi đã **sửa `ktest.sh` khi script đang chạy** ⇒ bash đọc lại phần cuối tệp và báo
`syntax error near unexpected token ')'` (dòng tổng kết không in, `EXIT=2`). Log đó **không** được
dùng làm bằng chứng; đã **chạy lại sạch toàn bộ 300 boot** với `ktest.sh` **đóng băng** (cùng sha
ảnh đầu/cuối), lấy đúng log/`EXIT=0`/`# KTEST: 300/300`. Từ đây: **không** sửa runner khi đang chạy
(bài học nối tiếp KT-50 "không redirect cùng tệp log cho nhiều lần chạy").

### 4.6 Cổng `gate --fast`

`./v2/bin/v2 gate --fast` → **PASS 569 test / 73 suite**, 0 failed ([`kt51-gate-fast.log`](kt51-gate-fast.log));
baseline cùng số ([`kt51-baseline-gate-fast.log`](kt51-baseline-gate-fast.log)).

## 5. Review độc lập

`reviewer-kt51` (read-only, lead spawn) — verdict **DRIFT** (không `UNSAFE`), xem
[`kt51-review.md`](kt51-review.md). Reviewer xác nhận: 3 dạng `sfence` đúng `rs1`/`rs2` (disasm),
stale test dương tính thật, D2-68 (local flush + ack ngầm, không self-target `sent=100`), rollover
một `All`, timeout `recovered=1`, M1/M2/M3 bắt lỗi thật + mã đã restore; tự chạy host 78/78, ktest
6/6 boot, gate PASS. **Vá trước commit**: **A (bắt buộc)** theo **D2-78** — `shootdown` **bỏ tham
số mask**, tự lấy `active_on()`; TAP 27 gỡ **2 hart phụ** (hoặc hart boot) khỏi `ACTIVE_ON` **thật**
rồi khôi phục (hart bị gỡ không ack/không bị chờ); TAP 25 chuyển sang `active_on` = toàn bộ hart
online (`sent = 100×(N−1)`, mọi hart phục vụ); TAP 29 đặt `ACTIVE_ON = {boot, offline}`;
`ktest.sh`/docs grep lại. **B (bắt buộc)** — mọi log/sha được trích **tồn tại thật**; 300 boot chạy
lại sạch sau vá với runner **đóng băng**; sửa link chết. **C/D/E** — docs ghi rõ TAP 29 chỉ đòi
`waited ≥ probe`; câu chữ M1 đổi sang `fresh_a`; comment `ipi.rs::local_flush` nói rõ KT-51 không
thay hàm đó. Sau vá: host 78/78, ktest 1 boot mỗi smp 30/30 + guard 3/3, 300 boot 300/300, gate 569.

## 6. Đề xuất

1. **KT-52 (object layer + Retype)**: dùng `AsidAllocator`/`VSpace` thật — mỗi VSpace một lease ASID
   qua `allocate_lease`/`free_lease`, `active_on` theo hart; `replace_lease` chỉ dành cho **kernel
   VSpace** (K3b) — khi có nhiều VSpace phải chọn ASID theo hart đang chạy.
2. **K3b**: bỏ giả định "một VSpace/một shootdown tại một thời điểm" (`IN_FLIGHT`) bằng hàng đợi
   shootdown hoặc `seq` theo VSpace; khi đó cần đo nhiều shooter đồng thời (ngoài KT-51).
3. **Đo latency shootdown thật** (ngoài icount): task riêng, cùng loại §10.5/§14.7 — không đổi
   D2-74.
4. **KT-56 (W^X/FaultKind)** sẽ đụng `set_satp_asid`/root PTE khi tách text/rodata: giữ nguyên
   bất biến `satp` chỉ đổi trường ASID.
5. **Đề nghị Agent 0**: sau commit + push, lật KT-51 **DONE**; **D2-78** (reviewer yêu cầu) đã áp
   đúng trong code + TAP + docs. Không cần D2 mới khác (D2-65 Q4/Q5 + D2-68 + D2-78 đủ), trừ khi
   Agent 0 muốn chốt thêm "đổi ASID không `sfence`" thành D2 cho K3b.

**Giới hạn còn lại** (không giấu, khớp docs §14.7): thời gian là **ảo** (`-icount shift=6`); một
VSpace kernel + một shootdown/lúc; `-smp 1` không có phép đo chéo; không đo latency/TLB hit-miss
thật; phép đo "hart ngoài `active_on`" dùng hart **khác** trên `-smp 4` chứ không phải nhiều shooter
đồng thời; 300 boot là **smoke** (soak 1.000 thuộc KT-57).
