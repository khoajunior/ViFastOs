# KT-55 — ghi chú kỹ thuật (cho reviewer/người sau)

## 1. Lỗi platform `W|X` — có **thật** trước khi sửa, cách phát hiện, vì sao KT-52 review không bắt

- **Triệu chứng (đỏ trước):** `cargo test -p vifast-plat-common --test obj wx_ca_3_cung_map_w_x_bi_tu_choi_atomic`
  ⇒ `panicked at tests/obj.rs:469:38: called Result::unwrap_err() on an Ok value: ()`.
  Bản sửa nằm ở `kernel/vifast-plat-common/src/obj/vspace.rs` — **hai** điểm ghi PTE duy nhất:
  `map` (trước mọi cấp bảng/PTE/đếm) và `protect` (trước `find_mapping`/mutation) từ chối
  `flags.write && flags.exec` ⇒ `ObjError::WxViolation` (cùng mã K2).
- **Vì sao là lỗi thật:** D2-67 phát biểu W^X **toàn cục** cho mỗi Frame (`mapped_w == 0` hoặc
  `mapped_x == 0`). Mapping **một lá** `W|X` làm cả `mapped_w` và `mapped_x` cùng tăng ⇒ chính
  object layer tạo trạng thái mà `check_objmem()` coi là sai. K2 từ chối từ trước.
- **Vì sao KT-52 review không bắt:** TAP 33 (X rồi W) và TAP 34 (Device+EXEC) chỉ kiểm vi phạm
  **chéo giữa hai mapping**; chưa ca nào map **một** lá `W|X`. Bất biến trong `check_objmem` đọc
  theo **bộ đếm Frame** nên khi không có lá W|X nào được tạo, nó vẫn xanh; review đọc D2-67 theo
  hướng "hai mapping" nên không thấy lá đơn. Conformance (mô hình K2 trả `WxViolation` cho chính
  bước `vm_map ... rights=READ|WRITE attrs=WB|EXEC` của `.scn` 23) mới lộ ra.
- **Không nới D2-67:** chỉ thêm từ chối; không đổi `MapFlags`, không đổi `check_objmem`, không đổi
  `spec/`; test KT-52 14/14 + `ktest.sh` 38/38 xanh sau sửa.

## 2. `check_trace` của `ktest.sh` chống log rỗng/thiếu dòng thế nào

- Với **từng** ca 22..26: đòi đúng một dòng `# conf: scn=N name=… steps=K ok=…`; số dòng
  `# trace: N …` phải **bằng** `K` (K do boot tự khai) ⇒ thiếu dòng hoặc log rỗng ⇒ `trace<scn>=x/K`.
- Mọi dòng `# trace:` phải khớp `^# trace: <số> <số> <op> … => <kết quả>$` (regex neo cả dòng).
- Evidence bắt buộc còn đòi **nội dung** đại diện từng ca: `22 1 map …`, `22 … walk … NotMapped`,
  `23 … WxViolation`, `23 … check => ok`, `25 … fault … NotMapped`, `26 … fault … Permission`,
  `26 … check => ok`, và với `-smp ≥ 2`: `24 … shootdown … => ok`; với `-smp 1`: `skipped=smp1`.
- `trace_compare.py` so **nội dung** 61 dòng theo thứ tự, không chỉ đếm: sim là phía được
  `check_scn.py --trace` kiểm **độc lập** từ `.scn` (tập bước chiếu + giá trị quan sát), nên hai
  phía cùng rỗng hoặc cùng bịa đều không thể qua.

## 3. Ca 24 trên `-smp 1`

- Kernel in `# conf: scn=24 name=vm-tlb steps=0 skipped=smp1` (không giả lập 2 hart); runner đòi
  đúng dòng đó ở smp=1 và dòng `shootdown` thật ở smp≥2. So trace dùng log smp=2 (và đã so thêm
  smp=4). Đây là **giới hạn phép đo** ghi ở `docs/k3-riscv64.md` §18.9(2).

## 4. `fault` của kernel là phân loại PTE, không phải trap

- `classify_fault`: `walk` PTE thật — invalid ⇒ `NotMapped`; lá hợp lệ thiếu bit quyền theo
  `access` ⇒ `Permission`. **Không** phải `FaultKind` (KT-56) và **không** phải trap page-fault
  thật (bằng chứng trap là KT-46 guard + KT-53 U-mode). Ghi rõ §18.9(1).

## 5. `reviewer-kt55` (DRIFT, không UNSAFE) — 7 mục đã vá

- **Mục 1:** `k3-riscv64.md` §17 + `expected.txt` comment còn "37 TAP" ⇒ đã đổi hết sang **38** (TAP
  37 = conformance, 38 = trap lồng) và ghi rõ nhãn TAP 37 phụ thuộc `smp`.
- **Mục 2 (D2-80):** log QEMU phải có `# conf image: build=<16 hex> text=<len>` = **FNV-1a 64 trên
  `.text` đã nạp** của chính ảnh; `ktest.sh` + `trace_compare.py --image` bắt buộc đối chiếu với ELF
  (`image_digest.py`) — xem `kt55-splice-check.log`: chép 61 dòng trace sim vào log guard bị **từ
  chối** cả hai phía.
- **Mục 3:** `check_trace` không chỉ đếm `steps` tự khai: `steps>0` (trừ `skipped`), **đúng tập**
  `scn=NN step=K` theo `check_scn.py --steps <scn>` (nguồn độc lập), không trùng/không thiếu, đúng 5
  dòng `# conf:`.
- **Mục 4:** `trace_compare.py` tính số ca khớp từ `--require` (không in cứng "5/5").
- **Mục 5:** nhãn TAP 37 smp=1 = `conformance 4/5 + 1 skipped`; manifest `__CONF37__` thay theo smp.
- **Mục 6:** `kt55-red-diff.log` ghi rõ là log **tái lập** (log gốc 13:33 bị boot sau ghi đè) và tái
  lập bằng log guard (QEMU thật, không có `# trace:`), cùng kết quả "trace rỗng ⇒ đỏ".
- **Mục 7:** §18.2 ghi rõ bước `vm_touch` **không được phủ** (dòng `fault` phát tại `vm_expect_fault`).

## 6. W|X — reviewer xác nhận lỗi thật (ghi theo yêu cầu)

Reviewer dựng pre-image: Frame mới (`mapped_w=mapped_x=0`) **lọt cả 3 check** cũ; `map` tăng cả hai
bộ đếm; `check_objmem` từ chối `w>0 && x>0`. `.scn` ca 23 bước 7 (`READ|WRITE` + `WB|EXEC` trên Frame
mới) là vector đầu tiên chạm đúng. **KT-52 không bắt vì khoảng trống phủ**: chỉ test X→W (TAP 33) và
`Device+EXEC` (TAP 34), không có **một request `W|X`**. Đây là giá trị thực của conformance — không
phải nới ngưỡng, không đổi `spec/`.
