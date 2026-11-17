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

## Vòng vá reviewer KT-77b (2026-11-17, sau DRIFT) — KHÔNG commit (A1-177)

**Mục 1 — sha phải sinh từ cây đã commit.**
- Build `product-shell` từ **commit `12fbff3e`**, cây `v2/kernel` sạch (`git status` trống):
  **hai lần, hai `CARGO_TARGET_DIR` tạm khác nhau ⇒ cùng `sha256=07c34fc6…`**, `size=176768`,
  `build=6e90c73c43f9b4aa` (khớp sha reviewer tự build). Ảnh copy vào
  `v2/artifacts/07c34fc6….bin`; mọi log parity/negative mới đều in `image_sha256=07c34fc6…`.
- Ảnh không shell cùng cây: `c1072d72…`, 82552 B (không đổi byte). Ảnh test canonical (ca âm
  detector): `81d8d0ed…`, 205376 B, `build=862591498553d445` (khớp lần build trước ⇒ cùng cây).
- **Nhãn**: `39ca3cc6…` = **bản trước commit, KHÔNG dùng làm bằng chứng/matrix/soak**
  (khác `07c34fc6` đúng vùng panic-location như reviewer chỉ).
- **Phát hiện thêm (nợ KT-79/85)**: build cùng commit ở **đường dẫn khác** (`git worktree
  ~/kt77b-head-wt`) cho `c65ee0f6…` — cùng size, cùng `.text` digest, khác `.rodata`; chuỗi khác
  là `/Users/mps/kt77b-head-wt/v2/kernel/vifast-kbin-riscv64/init/procsrv_user.rs` ⇒ `build.rs`
  gọi `$RUSTC` trực tiếp cho ELF U-mode nên **không** nhận `--remap-path-prefix`. Đã ghi vào
  docs §10 + §2.
- Log build WIP bị KT-77c làm bẩn (`2a1bbe55…`, `169ff6cb…`, `62e86bd8…`) ⇒ **đã xoá**, không dùng.

**Mục 2 — literal dòng evidence (D2-102).** `proc_rt.rs`: thêm `const INPUT_MODE`, `const INJECT_COUNT`,
`const RX_PATH`; dòng `# product shell:` in `const:input_mode=…`/`const:inject=…`; dòng
`# product rx:` in `const:rx_path=…` (không còn literal `input=serial no-inject=1`/`path=…`).
Biên dịch kiểm bằng `CARGO_TARGET_DIR=/tmp/kt77b-patch-check` (không đụng ảnh phát hành). **Chưa
commit** ⇒ ảnh bằng chứng vẫn là `07c34fc6` (bản trước vá); sau commit phải build lại.

**Mục 3 — ca âm shell tắt sạch.** Ba lần chạy: `qemu_con_sot_total` lần lượt 2/3/2 nhưng
`qemu_cua_script=0`, `verdict=OK` before+after cả ba lần ⇒ QEMU sót là của **tab khác** đang đo
song song, không phải script này. Log dùng: `kt77b-neg-shell-off.log` (exit 5); hai lần trước giữ
ở `kt77b-neg-shell-off-r2.log`/`-r3.log` (có dòng đếm QEMU toàn máy khác không).

**Mục 4 — `probe` (D2-123(1)).** `kt77b-harness-scan.sh` thêm hai lớp: (a) chuỗi đủ điều kiện
`vifast_plat_riscv64::probe`, `probe::on_trap`, `probe_fault_entry`, `src/probe.rs` (chỉ hiện khi
còn symtab); (b) **feature-graph** `cargo tree -e features -i vifast-plat-riscv64`: sản phẩm có
`ktest=0 umode=1` (probe không biên dịch), ca âm `ktest=1`. Kết quả: product `hits=0`, ảnh test
`hits=22`, probe gate `ok` cả hai phía. Không dùng literal `probe` trần (PLIC in sẵn
`plic: probe khong khoi phuc priority irq`).

**Mục 5 — metric ngưỡng.** Docs §8 thêm một câu: `.data+.bss` (cổng ảnh, QD §3.5) ≠ "bộ nhớ tĩnh"
OD-K6-2 (`size_of::<Machine>()`, core, không đổi).

**Chạy lại bằng chứng trên ảnh mới `07c34fc6…`:** parity serial 1/2/4 hart `KET_LUAN=ok`
(smp4 còn `qemu_con_sot_total=0 qemu_cua_script=0`); boot-tới-exit 1/2/4 hart (halt `shell_exit=0`,
`fault_seen=1` = `Exit{0}`); ca âm sai expect `exit 3`; budget đo lại; detector 4/4 `ok`.

**Xung đột tiềm ẩn (báo lead):** KT-77c đang sửa **cùng crate** (`Cargo.toml`, `main.rs`,
`proc_abi.rs`, `proc_rt.rs`, `proc_fs.rs` mới, plat `lib.rs`/`mmu.rs`). Tôi chỉ sửa **dòng evidence +
ba hằng** trong `proc_rt.rs`; nếu KT-77c ghi đè, cần ghép lại — hiện vá vẫn còn trong tệp.
