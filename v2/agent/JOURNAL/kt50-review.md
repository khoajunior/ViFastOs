# KT-50 — verdict review độc lập (`reviewer-kt50`)

- **Kết luận: DRIFT nhẹ — không UNSAFE.** Lõi đạt; 6 mục phải vá trước commit.
- Reviewer tự chạy (không dùng log của Agent 2): host **65/65** (trước vá), `ktest.sh 1 matrix`
  **6/6** (25/25 + 3/3), `./v2/bin/v2 gate --fast` **PASS 569**; kiểm `sip.SSIP` đúng, **không**
  map CLINT, self-target **không** gửi SBI, drain **trước mỗi bước** (đếm 9.413 lần), coalesce đúng.
- Thời điểm: sau khi KT-50 xong mã, **trước** commit (đúng yêu cầu task).

## 6 mục DRIFT và cách vá

| # | Mục | Cách vá | Tập tin |
|---|---|---|---|
| **D1** (bắt buộc) | comment **nói ngược D2-76** — `plat-common/src/ipi.rs:4-14`, `:55-56` nói "MMIO ACLINT"/"kernel đọc-ghi word"/"cần map MSWI"; `plat-common/src/lib.rs:44` nói hằng MSWI đọc/ghi per hart | Hằng ACLINT còn lại **chỉ** là dữ kiện nền tảng + host test; arch dùng **`sip.SSIP`**; **cấm map** CLINT (D2-76) — sửa module doc, doc `MSWI_HART_STRIDE`/`MSWI_WINDOW_BYTES`/`msip_pending`, `lib.rs` | `plat-common/src/{ipi,lib}.rs` |
| **D2** (bắt buộc) | số cũ: `k3-riscv64.md:280` "mã khác 5/9 (vd IPI 1) **vẫn fatal**" (ngược §13.2); `:128,:147,:506,:607,:774,:777` còn "20 TAP"/`ok 20 - trap nested panic`; `ktest.sh:194,203` comment "20 TAP"/"trap lồng (20)" | Đồng bộ **25 TAP**: "khác 1/5/9", `1..25`, `ok 25 - trap nested panic`, `ktest 25/25`; comment runner 25 TAP + TAP 20..24 | `v2/docs/k3-riscv64.md`, `v2/test/boot/ktest.sh` |
| **D3** (D2-77) | `post` tới hart **valid nhưng chưa online** phải trả `Invalid` + log, **không** SBI, **không** giữ kind; cần test | `classify_post(sender,target,online_mask)` thuần ở `plat-common`; arch `post` dùng nó, log `# ipi: hart N chua online`, `IPI_ERR_HART_OFFLINE`; test host mới: `sent=0`, hộp thư rỗng | `plat-common/src/ipi.rs`, `plat-common/tests/ipi.rs`, `plat-riscv64/src/ipi.rs` |
| **D4** | lỗi SBI khi gửi: gỡ bit vừa post + `errors+=1` | `IpiMailbox::unpost(kind)` (giảm `posted`, giữ bất biến `posted = delivered + pending`); arch gọi khi SBI lỗi; test host `unpost` | `plat-common/src/ipi.rs`, `plat-riscv64/src/ipi.rs`, `tests/ipi.rs` |
| **D5** | TAP 20 ở `-smp 1` có `delivered=0` nên tên `100 ipi delivered` dễ đọc sai | Tên theo cấu hình: `≥ 2` = `100 ipi delivered`; `1` = `100 ipi traps (self)`; `ktest.sh` grep theo biến `IPI_TAP20` khai báo **trước** vòng TAP | `ktest_ipi.rs`, `ktest.sh`, `k3-riscv64.md` §13.5 |
| **D6** (bắt buộc) | `kt50-boot50-matrix.log` + `kt50-ipi.log` chưa có; log 300 boot lần trước bị hai lần chạy ghi cùng tệp | Chạy lại **300 boot sạch** sau khi vá; chép log thật + `kt50-ipi.log` (trích pha IPI từ boot thật) vào `v2/agent/JOURNAL/` | `v2/agent/JOURNAL/kt50-{boot50-matrix,ipi}.log` |

## Xác nhận của lead

- **Câu 1 (deviation ACLINT)**: chấp nhận, đã ghi **D2-76** (`sip.SSIP` là nguồn sự thật; **cấm**
  map ACLINT MSWI; gửi SBI EID `0x735049`; xoá pending bằng `csrc sip, SSIP`).
- **D3** đã chốt thành **D2-77** (post tới hart chưa online ⇒ `Invalid`; lỗi SBI ⇒ gỡ bit).
- Yêu cầu thêm vào §13.7: `traps_delta=0` khi `SIE=0` **chỉ** chứng minh ngắt không được nhận,
  **không** chứng minh IPI không tới — đã ghi (khoản 7).

## Sau khi vá

- Host test kernel: **67/67** (14 host + 12 IPI + 8 mmu + 12 timer + 12 PLIC + 9 SMP).
- `ktest.sh 1 matrix`: **6/6** (25/25 + 3/3) — TAP 20 in `100 ipi traps (self)` ở `-smp 1` và
  `100 ipi delivered` ở `-smp 2/4`.
- 300 boot + gate 569: xem `kt50-boot50-matrix.log` / `kt50-gate-fast.log` (số thật trong log,
  không chép tay — QD §9 Q12).
