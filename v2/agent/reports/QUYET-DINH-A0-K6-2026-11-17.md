# QUYẾT ĐỊNH AGENT 0 · ViFastOs v2 · CỔNG K6 · 17/11/2026

**Từ**: Agent 0 · **Gửi**: lead (điều phối dòng OS), Agent 2, reviewer
**Căn cứ**: BRIEFING-Agent0-K6.md (09:40 JST)
**Mã quyết định**: OD-K6-1 … OD-K6-8 · **Task mở**: KT-77 … KT-83

---

## §0. Trả lời 5 câu (§8 của brief, mỗi câu một dòng)

1. **K6**: **(a) có giới hạn thời gian.** Làm KT-77 để đưa shell vào ảnh sản phẩm. Nếu vượt ngân sách ở OD-K6-1 thì tự động chuyển sang (b), không cần hỏi lại.
2. **Build tái lập**: **(ii) sửa ngay, không chặn K6**, nhưng **chặn mọi bản phát hành** và mọi tuyên bố sha giữa các máy cho tới khi sửa xong. Trong lúc chờ thì áp dụng (i), tức ghi rõ sha chỉ có nghĩa trong một môi trường.
3. **N1/N2/N3/N7**: **chuyển sang K7 có tên** ("K7 · Đóng lỗi v1"), không chặn K6. Văn bản đóng K6 phải ghi đúng **"3/7 lỗi v1 đã đóng"** (xem đính chính ở §1).
4. **Reviewer**: **giữ bắt buộc** cho task chạm kernel/ABI/driver/harness/script kiểm/cổng. Task chỉ sửa tài liệu thì **bỏ review người**, chỉ chạy kiểm tự động. Đồng thời chuyển các dạng lỗi lặp lại ở §4 thành luật máy (OD-K6-5) để giảm chi phí.
5. **"v2 hoàn thiện"**: xem OD-K6-7 (một câu, đo được).

---

## §1. Đính chính brief

- §6 vấn đề 3 ghi "đóng K6 và ghi **4/7**". Nhưng v1 có 7 lỗi đã biết và còn **4** chưa đóng (N1, N2, N3, N7), nên số đã đóng là **3/7**. Lead xác nhận lại danh sách 7 lỗi kèm trạng thái từng lỗi trong KT-83. Nếu thực tế khác 3/7 thì ghi theo số đo được.
- P20: hai sha có cùng 188.552 byte, một sha 188.544 byte. Kích thước chênh **8 byte** gợi ý nguyên nhân là **chuỗi đường dẫn tuyệt đối** có độ dài khác nhau, được nhúng vào binary (panic location, debuginfo). Đây mới là giả thuyết; KT-79 phải chứng minh bằng diff byte trước khi sửa.

---

## §2. Quyết định

### OD-K6-1 · K6 theo hướng (a), có điều kiện rơi về (b)
- **Ngân sách KT-77**: tối đa **2 task × 3 h** làm (chưa tính review). Hết ngân sách mà chưa đạt tiêu chí ở OD-K6-2 thì dừng, đóng K6 theo (b) bằng văn bản hạ phạm vi trong brief §6, và S1 chuyển thành **mục đầu tiên của K7**.
- Nếu KT-77 phát hiện đường user mode **phụ thuộc test harness** thì đó là lỗi thiết kế, không phải việc nối dây. Khi đó dừng ngay, ghi BLOCKED-CHO-A0 kèm file:dòng của chỗ phụ thuộc. **Không** được vá bằng cách nhét harness vào ảnh sản phẩm.
- Lý do chọn (a): "tương đương v1" là hợp đồng **đã duyệt**, và v1 có `shell>`. Hạ phạm vi khi chưa thử là tạo tiền lệ xấu. Giới hạn thời gian là để chặn rủi ro mà phản biện §7 đã nêu đúng.

### OD-K6-2 · Định nghĩa "tương đương v1 ở mức người dùng" (đóng §9 gạch đầu dòng 3)
Trước khi viết mã, KT-77a phải tạo **spec/v1-parity.toml**, liệt kê từ tag `v1-archive`:
- danh sách lệnh shell của v1 (tên, đối số, đầu ra mong đợi);
- mỗi lệnh có **một kịch bản serial** (gửi gì, chờ chuỗi gì, timeout bao nhiêu).

**Tiêu chí K6 đạt (bổ sung vào 8/8 hiện có)**:
1. Ảnh **sản phẩm** khởi động tới `shell>` trên QEMU riscv64 với 1/2/4 hart.
2. Chạy hết kịch bản trong v1-parity.toml qua serial, **tối thiểu** gồm: liệt kê thư mục, đọc tệp, ghi tệp, đọc lại tệp đó **sau reboot** (dùng lại ảnh đĩa persist), chạy một chương trình (exec), thoát.
3. **Ca âm**: ảnh sản phẩm build với shell bị tắt thì kịch bản phải FAIL ở bước chờ `shell>`. Thiếu ca âm thì tiêu chí 1 không được tính.
4. Ảnh sản phẩm vào **ma trận** (lúc này thành 8 ảnh × 3 = 24 lần boot), soak **1.000** lần chạy kịch bản, 0 lỗi.
5. Ngưỡng giữ nguyên: ảnh ≤ 262.144, bộ nhớ tĩnh ≤ 196.608.
6. Ảnh sản phẩm **không** chứa mã của test harness. Kiểm bằng máy: grep symbol hoặc feature flag trong binary, có ca âm tương ứng.

Lệnh nào của v1 không làm được trong ngân sách thì ghi vào parity.toml với trạng thái `deferred-K7` kèm lý do. Lead và Agent 0 đọc danh sách này lúc duyệt.

### OD-K6-3 · Build tái lập (P20)
- Làm ở KT-79, **song song** với KT-77, không chặn K6.
- Thứ tự: (1) diff byte 2 ảnh khác sha để tìm nguyên nhân thật; (2) sửa theo nguyên nhân. Các biện pháp dự kiến: ghim toolchain bằng `rust-toolchain.toml`, `--remap-path-prefix`, `codegen-units=1`, cố định thời gian và thứ tự đầu vào, thư mục build chuẩn hoá.
- **Tiêu chí**: build sạch trên **2 máy khác nhau**, ở **2 đường dẫn thư mục khác nhau** ⇒ **cùng sha**, có script kiểm trong cổng. Ca âm: cố tình nhúng đường dẫn tuyệt đối thì script phải đỏ.
- Từ giờ tới khi KT-79 xong, mọi báo cáo nhắc tới sha phải ghi kèm **máy + đường dẫn build**.

### OD-K6-4 · K7 "Đóng lỗi v1" (tên chính thức)
Mỗi mục có tiêu chí đo được, viết sẵn để lead mở task:

| Mã | Tiêu chí đạt |
|---|---|
| N1 | Test đơn điệu **có migrate** thật giữa các hart, trên 2 và 4 hart; ca âm: tắt kiểm tra đơn điệu thì test bắt được vi phạm |
| N2 | Tái hiện được vi phạm T26 trên mã v1 (đỏ-trước), rồi **50 boot / 0 đỏ** trên v2 |
| N3 | Có checker "giao đúng một lần", có ca 4 client; ca âm: cố tình giao 2 lần thì checker phải đỏ |
| N7 | Test "3 port" dùng chung một cơ chế wake |
| S1 | Chỉ khi KT-77 rơi về (b) |

Văn bản đóng K6 **không được** dùng các cụm "sửa hết lỗi v1" hay "vượt v1".

### OD-K6-5 · Quy trình review và luật máy
Giữ reviewer bắt buộc (lý do: 10/10 task có lỗi thật bị bắt trước commit). Ghi nhận phản biện ở §9 là hợp lý, nên để giảm chi phí, các dạng lỗi lặp lại ở §4 được **chuyển thành luật trong cổng**:

1. **Số trong báo cáo do script sinh ra** từ log đã commit, không viết tay. Mỗi số trong báo cáo đi kèm `(log: đường-dẫn:dòng)`. Log nằm ngoài repo hoặc trong `/tmp` thì không được trích.
2. **Log phải được commit trước** commit báo cáo; kiểm bằng thứ tự commit.
3. **Bộ đếm báo "0" thì phải có ca âm** làm nó khác 0, nếu không thì cổng đỏ.
4. **Cấm so sánh yếu** kiểu `<=` hoặc `>=` trên bộ đếm khi đặc tả nói `==`. Reviewer có quyền đòi đổi.
5. **Driver**: mọi lệnh gửi thiết bị phải kiểm feature đã đàm phán. Thêm một **thiết bị giả nghiêm ngặt** trong mô hình giả lập, từ chối mọi điều đặc tả không hứa (QEMU tha lỗi nên không đủ làm bằng chứng).
6. **Tài nguyên phần cứng** (địa chỉ, IRQ): có bảng cấp phát một nguồn duy nhất và test kiểm chồng lấn.
7. Bản trích tiêu chí trong task cổng phải **giữ nguyên mọi cột** của bản gốc; kiểm bằng diff với thiết kế.
8. Script kiểm số (đã bị phá 3 lần): mỗi lần sửa phải kèm **ca phá** của lần trước làm ca hồi quy.

Phạm vi review: **mọi** task chạm kernel/ABI/driver/harness/script kiểm/cổng thì review người, và reviewer **phải cố tình phá**. Task chỉ sửa tài liệu thì chạy luật máy, không cần review người. Đánh giá lại sau 10 task nữa: nếu tỉ lệ DRIFT giảm dưới 50% thì xét hạ tiếp.

### OD-K6-6 · Sự cố QEMU mồ côi
- Harness phải chạy QEMU trong **process group riêng**, có `timeout` cứng và dọn dẹp khi cha chết (trap EXIT, kill cả group).
- Trước và sau mỗi phép đo thời gian: ghi số tiến trình QEMU đang chạy và load average vào log. Có QEMU lạ thì phép đo bị đánh dấu không hợp lệ.
- Các số thời gian đo trong 3 giờ bị ảnh hưởng: **không dùng**, giữ log nhưng gắn nhãn.

### OD-K6-7 · Định nghĩa "ViFastOs v2.0 hoàn thiện" (một câu)
> **v2.0 hoàn thiện khi: ảnh sản phẩm build tái lập byte trên 2 máy, khởi động trên QEMU riscv64 ở 1/2/4 hart tới `shell>`, chạy hết kịch bản `spec/v1-parity.toml` (không còn mục `deferred`) với soak 1.000/1.000, 7/7 lỗi v1 có ca đỏ-trước/xanh-sau, ABI đóng băng không đổi, hai ngưỡng cứng không nới, và mọi số trong báo cáo cuối do script trích từ log đã commit.**

- Phạm vi v2.0: **chỉ riscv64 trên QEMU**. x86_64, AArch64, phần cứng thật, ngắt thay cho poll, IPI cho lịch trình, 60 message register chưa dùng: thuộc **v2.1+**, không chặn v2.0.
- Đạt câu trên thì lead **dừng dòng** và gửi báo cáo kết thúc cho Agent 0.

### OD-K6-8 · ~14 nợ khác
Giữ trong sổ. Phân vào K7 **chỉ** những nợ nằm trong đường của OD-K6-7: fsync từ user mode, NoSpace/torn write, ma trận phủ ảnh persist, đa nhân trên ảnh persist, Tcb.Exec(path) nếu v1-parity cần. Các nợ còn lại để v2.1+.

---

## §3. Task mở

| Task | Ai | Nội dung | Điều kiện xong |
|---|---|---|---|
| **KT-77a** | Agent 2 | spec/v1-parity.toml từ tag v1-archive + script chạy kịch bản qua serial | Chạy được trên **ảnh test** hiện có (đã có shell) và ra kết quả; trích lệnh v1 kèm file:dòng |
| **KT-77b** | Agent 2 | Đường khởi động user mode + shell trong ảnh sản phẩm | Tiêu chí 1-6 của OD-K6-2. Hết ngân sách thì áp dụng OD-K6-1 |
| **KT-78** | Agent 2 | Harness QEMU: process group, timeout, dọn dẹp (OD-K6-6) | Ca âm: giết tiến trình cha thì 0 QEMU còn sót sau 5 s |
| **KT-79** | Agent 2 | Build tái lập (OD-K6-3) | 2 máy × 2 đường dẫn = 1 sha; ca âm đỏ |
| **KT-80** | Agent 2 | Luật máy 1-3, 7, 8 của OD-K6-5 vào cổng | Mỗi luật có 1 ca phá làm đỏ cổng |
| **KT-81** | Agent 2 | Thiết bị giả nghiêm ngặt + bảng tài nguyên (luật 5, 6) | 2 lỗi driver cũ (FLUSH chưa đàm phán, đòi độ dài) bị bắt ở mô hình |
| **KT-82** | lead | Văn bản đóng K6 (theo (a) hoặc (b)), ghi đúng số lỗi v1 đã đóng, P20 kèm caveat | Reviewer duyệt; số do script sinh |
| **KT-83** | lead | Mở K7 "Đóng lỗi v1" theo OD-K6-4 và OD-K6-8; xác nhận danh sách 7 lỗi v1 | Bảng 7 lỗi có trạng thái + file:dòng |

**Thứ tự**: KT-78 → KT-77a → KT-77b → KT-82. Chạy song song: KT-79, KT-80. KT-81, KT-83 làm sau khi K6 đóng.

**Không đổi**: ABI đóng băng (16 syscall, 19 mã lỗi, 64 MR), hai ngưỡng cứng, ma trận/soak/L1 hiện có.

---

## §4. Agent 0 sẽ kiểm khi duyệt K6
1. Log kịch bản serial trên ảnh sản phẩm: có `shell>` và đọc lại được tệp sau reboot, kèm ca âm.
2. Ảnh sản phẩm không chứa harness (bằng chứng máy).
3. Kích thước ảnh sản phẩm mới so với 262.144.
4. Danh sách `deferred-K7` trong v1-parity.toml (nếu có).
5. Văn bản đóng K6 ghi đúng số lỗi v1 đã đóng.

---

## §5. PHỤ LỤC A — ĐÍNH CHÍNH VÀ SỬA ĐỔI (Agent 0, 17/11/2026)

Phần §0–§4 ở trên giữ nguyên làm bản gốc. Phụ lục này **thay thế** ba điểm; khi hai chỗ khác nhau thì **phụ lục thắng**.

### A.0 Đính chính số lỗi v1 (Agent 0 tự nhận lỗi)
Agent 0 xác nhận: **2 đóng (N4, N6) · 1 hoãn K5' (N5) · 4 mở (N1/N2/N3/N7)**. Lỗi của Agent 0 ở §1: suy "đã đóng" **bằng phép trừ** (7 − 4 mở = 3), bỏ qua trạng thái **hoãn**. Ghi nhận đúng tinh thần §4 của briefing: con số truyền miệng vẫn phải đo lại.

### A.1 Sửa OD-K6-7 (định nghĩa v2.0 hoàn thiện)
Bỏ cụm "**7/7 lỗi v1** có ca đỏ-trước/xanh-sau"; thay bằng:

> **"6/7 lỗi v1 (N1, N2, N3, N4, N6, N7) có ca đỏ-trước/xanh-sau; N5 thuộc K5' x86_64, ngoài phạm vi v2.0."**

Lý do: v2.0 chỉ riscv64; giữ "7/7" thì định nghĩa **không thể đạt**.

### A.2 Sửa OD-K6-4 (K7)
**K7 không nhận N5.** Văn bản đóng K6 (**KT-82**) phải ghi **đúng ba trạng thái**: đóng / hoãn / mở, với số lấy từ **bảng của KT-83** (không lấy từ phụ lục này).

### A.3 Sửa KT-79 (build tái lập) — chưa chấp nhận "1 máy"
- Máy thứ hai: thử dùng **ps** (Agent 2 đã SSH được từ `mps`), build ở **đường dẫn khác** với máy hiện tại; toolchain ghim bằng `rust-toolchain.toml` cho **cả hai** máy.
- **Ra cùng sha** ⇒ **đạt đủ** OD-K6-3.
- **Không ra cùng sha** ⇒ báo **nguyên nhân bằng diff byte**; **không** được gộp vào kết luận "do khác máy".
- **Không build được** trên ps (thiếu target hoặc mạng) ⇒ **khi đó mới** ghi "đạt một phần: 2 đường dẫn, 1 máy", kèm **lý do cụ thể**.

### A.4 Thứ tự task (giữ nguyên)
**KT-78, KT-79 → KT-77a → KT-77b → KT-82**; các task khác không đổi.
