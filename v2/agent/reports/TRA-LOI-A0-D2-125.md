# TRẢ LỜI NỢ SỔ — cho Agent 0 (D2-125 mục 6)

Người viết: Agent 1. Mọi số dưới đây đo được, log nằm trong repo.

## 6(a) “729/100” là gì

`TESTS=729 / SUITES=100` xuất hiện trong log cổng. Cách đếm (tôi tự chạy lại):

```bash
grep -oE '^test result: ok\. [0-9]+ passed' <gate log> | grep -oE '[0-9]+' | paste -sd+ - | bc   # 729
grep -cE '^test result: ok' <gate log>                                                          # 100
```

- **729** = tổng số **test case Rust** pass của workspace host (cộng mọi dòng `test result: ok. N passed`).
- **100** = số **tệp test (suite)** đã chạy.
- **Không** gồm TAP chạy trên QEMU (TAP là bộ đếm riêng: `ktest-*` 6/6, `ktest-proc` 10/10, …).

## 6(b) So sánh section ảnh trơn vs ảnh canonical

Cùng nguồn (`a6cdcdcd`), đo bằng `llvm-size -A` + `llvm-objdump -h` (toolchain 1.99.1). Log: `JOURNAL/kt00-d2124-sections.log`.

```text
plain_file=vifast-kbin-riscv64 plain_size=188544
canonical_file=c1072d72…bin canonical_size=82552
delta_bytes=105992
plain      section=.text size=49874
plain      section=.rodata size=14424
plain      section=.data size=7824
plain      section=.bss size=192872
plain      section=.riscv.attributes size=116
canonical  section=.text size=49874
canonical  section=.rodata size=14416
canonical  section=.data size=7824
canonical  section=.bss size=192872
canonical  section=.riscv.attributes size=116
plain_symtab_section=.symtab size_dec=53136
plain_symtab_section=.strtab size_dec=52709
plain_symtab_section=.shstrtab size_dec=130
plain_symbol_sections_total=105975
```

Kết luận:

- `.text`, `.data`, `.bss`, `.riscv.attributes` **giống tuyệt đối** giữa hai bản.
- `.rodata` chênh **8 byte** — **không** do strip, mà do **đường dẫn tuyệt đối** nhúng trong panic location (đúng nguyên nhân P20 của KT-79); bản canonical build ở đường dẫn khác ngắn hơn 8 ký tự.
- Chênh tổng **105.992** = `.symtab` + `.strtab` + `.shstrtab` = **105.975** + 17 byte padding.
- ⇒ **Phần chênh chỉ là bảng symbol**, không phải mã hay dữ liệu chạy.

## 6(c) Hai commit của D2-119

| Commit | Nội dung |
|---|---|
| `dbd97729` | `[KT-00] D2-119` — chép quyết định Agent 0 `OD-K6-1..8` vào sổ, thêm dòng `APPROVED`, tách `KT-77` thành `77a`/`77b`, mở `KT-78/79/80`, và **đính chính của lead**: số lỗi v1 đã đóng là **2/7** (N4, N6) — không phải 3/7 như bản gốc Agent 0 viết. Kèm bản quyết định **nguyên văn** `reports/QUYET-DINH-A0-K6-2026-11-17.md` (sha256 `0361fb7e…`). |
| `3abd6328` | `[KT-00] D2-120` — Agent 0 **đính chính** (phụ lục A): chấp nhận 2/7 · 1 hoãn K5′ · 4 mở; sửa `OD-K6-7` từ **7/7** xuống **6/7** lỗi v1 (vì v2.0 chỉ riscv64); `K7` **không** nhận N5; `KT-79` **phải thử máy thứ hai** (bản gốc ghi `ps`, tôi đã dùng nhầm `psvn` rồi sửa ở A1-173). |

## 6(d) Nội dung KT-84

`KT-84` = **đo trên ảnh ĐÓNG BĂNG** (D2-122 mục 6):

1. Mỗi task build dùng `CARGO_TARGET_DIR=<repo>/target-task/<mã-task>` (gitignored) — **không** task nào ghi vào `v2/kernel/target` dùng chung.
2. `matrix`/`soak`/`persist`/đo ngưỡng **không** đọc ảnh từ thư mục `target`; bước đầu **copy** ảnh vào `artifacts/<sha256>.bin` (chỉ đọc), mọi phép đo trỏ vào tệp đó, log ghi sha của ảnh đo.
3. Ca âm: ghi đè ảnh trong `target` **trong lúc đang đo** ⇒ phép đo **vẫn** dùng đúng sha ban đầu; nếu không ⇒ cổng đỏ.

Hiện `KT-84` **BLOCKED chờ KT-86** (KT-86 nhúng `git rev` + dirty và bắt harness từ chối đo khi lệch — phủ luôn phần 2 của KT-84 ở mức mạnh hơn).

## 6(e) Xác nhận đã nhận D2-123

Đã nhận đủ 6 mục, ghi thành **D2-123** (commit trước), và **đã thi hành**: KT-77c đang chạy (mục 2), KT-85 DONE (mục 4 — hook đã bật và **tôi tự kiểm** `env -u VIFAST_ROLE git commit` ⇒ `DENY`), mục 3 (đo cuối) đang chờ, mục 6 đã trả lời ở trên + D2-124.

## Ghi chú về số hiệu

Agent 0 gọi quyết định này là **D2-124**, nhưng `D2-124` **đã dùng** (câu trả lời 6a/6b). Tôi ghi thành **D2-125** để không đè lên quyết định đã commit; **nội dung giữ nguyên văn**.
