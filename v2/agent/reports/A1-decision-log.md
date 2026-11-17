# Sổ quyết định của Agent 1 (thi hành thay Agent 0)

**Căn cứ**: LUẬT CHUNG do người dùng ban hành 2026-11-17 10:0x JST — "AGENT 1 QUYẾT ĐỊNH THAY AGENT 0".
**Hiệu lực**: mọi dòng (OS + ViScript), ghi vào sổ quyết định của dòng (tệp này cho dòng OS; dòng ViScript ghi trong `viscript/agent/`).
**Quy ước mã**: `A1-17x` (ví dụ `A1-171`).
**Nhãn bắt buộc**: mọi mục dưới đây mang nhãn **CHỜ A0 XÁC NHẬN** cho tới khi Agent 0 duyệt hồi tố (công nhận hoặc đảo lại).

## Phạm vi

### Agent 1 ĐƯỢC tự quyết (không phải hỏi A0)
1. **Nợ nhỏ** không cần thiết kế: sửa lỗi chính tả trong docs, sắp xếp lại log, đổi tên tệp nội bộ.
2. **Việc sổ**: cập nhật `QUEUE.md` trạng thái, `STATE.md`, `BLOCKED.md`, sổ quyết định, biên bản.
3. **Đo đạc**: chạy lại cổng, ma trận, soak, L1; chọn cấu hình đo trong phạm vi đã duyệt.
4. **Báo cáo**: viết báo cáo, briefing, tổng hợp.
5. **Thứ tự task** trong phạm vi Agent 0 đã duyệt (ví dụ đổi KT-79 trước KT-77a).
6. **Phân loại sự cố** khi đã có bằng chứng (log, diff, `file:dòng`): ví dụ "đây là flake hạ tầng", "đây là lỗi sản phẩm", "đây là DRIFT bằng chứng".

### Agent 1 KHÔNG được tự quyết (phải `BLOCKED-CHO-A0` + chuyển việc khác)
1. **Thiết kế, API, ABI** mới hoặc thay đổi.
2. **Chính sách crate, giấy phép, `unsafe`**.
3. **Đổi kỳ vọng của ca cũ, đổi ngưỡng, đổi cách đo ngưỡng**.
4. **Hạ phạm vi một cổng/mốc đã duyệt; tuyên bố đạt cổng/mốc.**

### Ranh giới mờ — cách xử lý
- **Thi hành quyết định đã duyệt sẵn** (ví dụ OD-K6-1 cho phép "hết ngân sách thì tự động rơi về (b)") **không** phải quyết định mới ⇒ Agent 1 **được thi hành**, nhưng phải ghi rõ "**thi hành OD-K6-1**" và vẫn liệt kê để A0 xác nhận.
- **Kết luận kỹ thuật có bằng chứng** ("không build được trên psvn vì thiếu target X, log ở `file:dòng`") là **phân loại sự cố** ⇒ Agent 1 được kết luận.
- **Kết luận về mức đạt của cổng/mốc** ("K6 ĐẠT/KHÔNG ĐẠT") ⇒ **Agent 1 không được tuyên bố**; chỉ được trình bày bằng chứng + đề xuất, chờ A0.

---

## Nhật ký quyết định

| Mã | Ngày | Nội dung | Bằng chứng | Trạng thái |
|---|---|---|---|---|
| _(chưa có)_ | | | | |
