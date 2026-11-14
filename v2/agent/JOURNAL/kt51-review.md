# KT-51 — verdict review độc lập (`reviewer-kt51`) + các mục đã vá

Người review: **reviewer-kt51** (read-only, Agent lead spawn; 2026-11-15).
Verdict: **DRIFT** (không `UNSAFE`). Không commit trước verdict — mọi mục **bắt buộc** dưới đây đã
vá **trước** commit `[KT-51]`, kèm log chạy lại.

## Đã xác nhận đúng (không cần sửa)

- **3 dạng `sfence.vma` đúng `rs1`/`rs2`** — reviewer **disasm** ảnh test, xác nhận `rs1 = VA`,
  `rs2 = ASID`, không đảo thứ tự.
- **Stale test dương tính thật**: remap **không** sfence ⇒ hart đọc ra mapping cũ; sau `sfence`
  đúng dạng ⇒ mapping mới; log `stale_seen=3 fresh_b=3` (đủ 3 dạng).
- **D2-68**: hart gọi local flush + **ack ngầm**, **không self-target** (`sent=100`, không 200);
  drain mỗi bước trong vòng chờ; `ack_mask == active_on`.
- **Rollover**: cạn ASID ⇒ **một** `FlushScope::All`; ASID 0 phát lại; gen 0→1.
- **Timeout**: hart offline trong `active_on` ⇒ `Timeout` rồi `recovered=1`, **không treo**.
- **Mutation M1/M2/M3 thật** (bắt lỗi, không phải xanh hình thức); mã đã restore; ảnh cuối build từ
  mã sạch (grep `MUTATION` = 0).
- Reviewer tự chạy: host **78/78**, `ktest.sh 1 matrix` **6/6 boot**, `gate --fast` PASS.

## A (bắt buộc) — TAP 27 chưa chứng minh "ngoài `active_on`" ⇒ **D2-78**

Vấn đề: `smp.rs` cho **mọi** hart vào `ACTIVE_ON`; TAP 25/27 lại truyền **mask hẹp** vào
`shootdown`, nên `extras` vẫn nằm **trong** `active_on` toàn cục — test chỉ kiểm mask **tham số**,
không kiểm trạng thái thật. Cách vá (đúng D2-78):

1. **Bỏ tham số mask khỏi API công khai**: `shootdown(scope, timeout_ticks)` tự đọc `active_on()`
   **bên trong** tại thời điểm phát; không còn chỗ cho người gọi chờ/bỏ chờ sai tập hart. Mọi call
   site (`replace_lease`, TAP 25/27/28/29) đã đổi.
2. **TAP 27 dùng trạng thái thật**: API test-only `tlb::test_active_on_set(mask)` (chỉ ảnh `ktest`)
   gỡ **2 hart phụ** (`-smp 4`, ưu tiên) hoặc hart boot (`-smp 1/2`) khỏi `ACTIVE_ON`, lưu mask cũ;
   shootdown ⇒ hart bị gỡ **không** ack, **không** bị chờ, `acked == active_on()` mới; **khôi phục**
   rồi shootdown lại ⇒ mọi hart online ack. Bằng chứng in `removed=… removed_count=…
   narrowed=… targets=… acked=… caller_in=… removed_acked_before/after=… restored=1
   restored_acked=… ok=1`.
3. TAP 25 đổi theo: `active_on` = **toàn bộ hart online** ⇒ `sent = 100×(N−1)` (smp2 100, smp4 300)
   và **mọi** hart khác BSP phải phục vụ đủ 100 yêu cầu (`# shootdown all: bad=0 served=N−1`).
4. TAP 29: đặt `ACTIVE_ON = {boot, offline}` (trạng thái thật của VSpace có hart chết) ⇒ timeout;
   thu hẹp `{boot}` để chứng minh máy còn dùng được; khôi phục.
5. `ktest.sh` grep lại theo bằng chứng mới (bỏ `extras=0xc` cũ, thêm `removed_count`, `served`,
   `sent=100×(N−1)`); `# tlb init:` siết `active_on=0x(1|2|4|8)` (một hart boot).
6. Docs §14.3/§14.5/§14.7 cập nhật đúng "hart ngoài `active_on`" = **gỡ thật rồi khôi phục**.

## B (bắt buộc) — bằng chứng không tồn tại

- Đã tạo **log thật** trong `v2/agent/JOURNAL/`: `kt51-ktest-smp{1,2,4}.log` (trích từ boot 1 của
  lần 300 boot cuối), `kt51-gate-fast.log`, `kt51-boot50-matrix.log` (có `# KTEST: 300/300` +
  `EXIT=0` + sha256 đầu/cuối), `kt51-image-sha.log`, `kt51-budget.log`, `kt51-host-tests.log`,
  `kt51-product-boot-smp2{,-raw}.log`, `kt51-mutation-*`, `kt51-red-*`, `kt51-baseline-gate-fast.log`.
- Sửa mọi link chết: `tests/tlb.rs` trỏ `kt51-ktest-smp*.log` (không còn `kt51-tlb.log`); journal
  chỉ trích các tệp **có thật**; không viết "300/300" trước khi log tồn tại.
- Ghi lại **bài học vận hành**: lần 300-boot đầu bị `syntax error` do sửa `ktest.sh` **khi script
  đang chạy** (log không dùng làm bằng chứng) ⇒ chạy lại sạch với runner đóng băng.

## C/D/E (nit) — đã sửa

- **C**: docs ghi rõ TAP 29 chỉ đòi `waited ≥ probe` (100 ms **ảo**), giá trị thật (có thể vượt) in
  trong log — không ngụ ý đúng bằng 100 ms.
- **D**: câu chữ mutation M1: đỏ chủ yếu vì **lần đọc đầu sau remap** đã ra mapping cũ
  (`fresh_a` sai) và peer `verify_wrong` tăng; **không** dựa vào `stale_seen` (vốn bằng 0 khi
  thiếu sfence). Ghi trong docs §14.8 + journal §4.3.
- **E**: comment `plat-riscv64/src/ipi.rs` (`local_flush`) nói rõ KT-51 **không** thay hàm này;
  shootdown tự làm local flush + ack ngầm ở `crate::tlb`, còn `local_flush` chỉ giữ ngữ nghĩa
  "post kind cho chính mình không sinh IPI" (TAP 22).

## Chạy lại sau vá

- Host: `cargo test -p vifast-plat-common` → **78/78**.
- `ktest.sh 1 1|2|4` → **30/30 + guard 3/3** mỗi cấu hình (bằng chứng mới ở trên).
- `ktest.sh 50 matrix` → **300/300 boot sạch**, 0 `not ok`, `EXIT=0`, sha ảnh đầu/cuối khớp.
- `./v2/bin/v2 gate --fast` → **PASS 569/73**.
