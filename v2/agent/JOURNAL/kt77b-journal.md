# KT-77b — ảnh sản phẩm có U-mode + shell (OD-K6-2)

Ngày: 2026-11-17 · Task: KT-77b · Ngân sách OD-K6-1: dự kiến dùng **1 task ≈ 1,5 h** (chưa tính
review), dừng trong hạn (không rơi về (b)).

## Đạt

1. **Ảnh sản phẩm canonical có shell**: `39ca3cc6…`, 176.768 B, `build=6e90c73c43f9b4aa`,
   `text=106764` (`kt77b-logs/kt77b-build-shell-canonical.log:5`). Build hai lần ở hai
   `CARGO_TARGET_DIR` khác nhau ⇒ **cùng sha** (`kt77b-build-canonical.log:12`).
2. **Boot tới `shell>`** ở smp 1/2/4; `# shell: up` do shell U-mode in
   (`kt77a-serial-1-help-serial-custom-smp{1,2,4}.log:79,84,87,89`).
3. **Parity serial 4/4 active × 3 cấu hình hart**, exit 0 (`kt77b-parity-smp124.log:1-50`).
4. **Ba ca âm**: shell tắt ⇒ exit 5 (`kt77b-neg-shell-off.log:25`); detector bắt ảnh test
   (22 > 0) trong khi ảnh sản phẩm 0 (`kt77b-harness-scan.log:3-11`); sai expect ⇒ exit 3
   (`kt77b-neg-wrong-expect.log:23`).
5. **Ngân sách**: ảnh 176.768 ≤ 262.144; `.data+.bss` 203.232 ≤ 262.144; `size_of::<Machine>()`
   = 189.384 ≤ 196.608 **không đổi** (`kt77b-budget.log:2-3`, `kt77b-run-exit-smp1.log:74`).
6. **Tách phụ thuộc harness**: `user = ["ktest"]` (crate arch) ⇒ nay `user = ["umode"]` +
   `ktest` giữ riêng; `obj.rs` bốn hàm kernel đổi sang `any(ktest, umode)`; kbin thêm feature
   `product-shell = ["user"]`. Ảnh sản phẩm **không** có marker harness.
7. **Không hồi quy ảnh test**: `ktest-proc` build lại sau thay đổi vẫn 10/10 TAP + PANIC W^X
   chủ đích (`kt77b-regress-ktest-proc.log:121,129,142`).
8. `./v2/bin/v2 check` PASS, `./v2/bin/v2 gate --fast` PASS (`kt77b-v2-check.log`,
   `kt77b-gate-fast.log`).

## Không đạt (trung thực)

- **OD-K6-2 tiêu chí 2 mới đạt phần `active`**: `ls`/`cat`/ghi tệp/đọc lại sau reboot/`exec` là
  `deferred-K7` **trong spec đóng băng**; "đọc lại sau reboot" cần FS + ảnh đĩa persist trong ảnh
  sản phẩm — ngoài ngân sách hai task.
- **Tiêu chí 4** (ma trận + soak 1.000) do lead đo riêng.
- **Nợ K7**: ngắt RX thường trực khi ở U-mode (trap từ U-mode không đổi `satp`); ELF đủ symbol
  (KT-84); ảnh test đổi `.text` vì `shell.elf` + prompt (ma trận phải chạy lại).

## Quyết định kỹ thuật đáng ghi

- **Không** ghi `BLOCKED-CHO-A0`: phụ thuộc tìm được là **gán nhãn feature sai** cho bốn hàm
  kernel (`copy_kernel_low_root`/`switch_root`/`restore_kernel_root`/`set_sum` nằm sau
  `cfg(ktest)`), không phải U-mode gọi mã test. Sửa **thu hẹp**: ảnh sản phẩm không link harness;
  ảnh test không đổi ngữ nghĩa (đã boot lại).
- **Prompt `shell>` in thành dòng riêng** (không phải tiền tố cùng dòng) để `expect` neo theo
  dòng của `spec/v1-parity.toml` không trượt.
- **Byte serial đi qua cửa sổ RX ở S-mode** rồi đóng trước khi hạ U-mode (lý do `satp`), byte đến
  lúc cửa sổ đóng nằm ở FIFO 16550 và được vét cạn — nợ K7 đã ghi.
- Bơm payload lặp 8 lần là **thiết kế của `v2/test/parity/stdin_wrap.sh:32-43`** (không phải lỗi
  runtime); `serial_min_count` của spec đã tính.

## Bằng chứng (đã commit cùng commit)

`v2/agent/JOURNAL/kt77b-logs/` (build canonical ×2, budget, parity 3 cấu hình hart, ba ca âm,
detector, boot tới exit ở 1/2/4 hart, regress ktest-proc, v2 check, gate --fast) +
`v2/agent/JOURNAL/kt77b-harness-scan.sh` + `v2/docs/k6-product-shell.md`.
