# BLOCKED-CHO-A0 — việc Agent 1 KHÔNG được tự quyết (D2-121)

Mỗi mục: câu hỏi cụ thể + bằng chứng + đề xuất. Agent 1 **không** tự chốt các mục này.

---

## B1 · Ảnh nào là **ảnh phát hành** của v2.0? — **ĐÓNG 2026-11-17 (D2-122.2): chọn (B) canonical**

**ĐÃ GIẢI**: Agent 0 chọn **(B)** — ảnh sản phẩm = đầu ra pipeline canonical, áp dụng **ngay**; xuất 2 tệp cùng lần build (ảnh phát hành strip + ELF đủ symbol), ghi sha cả hai; `fd202276` giữ log nhưng **không** dùng làm bằng chứng v2.0; **không** chạy lại soak/matrix/persist trên `c1072d72` ngay — chạy **một lần** trên ảnh canonical **có shell** sau KT-77b; ngưỡng ≤ 262.144 đo trên ảnh canonical. Cấu trúc chống tái diễn: **KT-84** (D2-122.6).

**Bối cảnh cũ (giữ để tra cứu)**: OD-K6-3 đã đạt trên **bản canonical** (`sha256=c1072d72…`, `size=82552`, `-C strip=symbols` + remap + metadata chuẩn hoá) ở **2 máy × 2 đường dẫn**. Nhưng **ảnh phát hành hiện tại** là bản **trơn** `fd202276…/188552` (giữ symbol) — bản này **chưa** chứng minh tái lập.

Hai đường, chỉ Agent 0 chọn được:

| | (A) Giữ bản trơn `fd202276` | (B) Chuyển ảnh phát hành sang canonical `c1072d72` |
|---|---|---|
| Tái lập byte | **chưa** chứng minh | **đã** chứng minh (2 máy × 2 đường dẫn) |
| Kích thước | 188.552 | **82.552** |
| Bằng chứng đang có | soak `0/1000`, matrix `21/21`, persist `7/7` trên **chính sha này** | **phải chạy lại toàn bộ** trên sha mới |
| `.text` digest | `build=ef566cfa583e34bb text=49874` | `build=2cbe9bbe339d7c02 text=49874` |
| Rủi ro | “tương đương v1” với một artifact **không** tái lập được | phải soak lại; strip làm mất symbol (khó debug) |

**Đề xuất của Agent 1 (không tự quyết)**: chọn **(A) cho v2.0**, và ghi trong văn bản đóng K6 rằng tái lập byte đạt trên **pipeline canonical** (`v2/tools/repro-build.sh`) chứ **chưa** cho ảnh phát hành; nợ P20 chuyển sang v2.1 với tiêu chí “ảnh phát hành = ảnh canonical”. Lý do: (B) buộc chạy lại soak 1.000 + matrix 21 + persist 10 boot, và strip làm mất khả năng đọc symbol khi có lỗi.

**Bằng chứng**: `v2/agent/JOURNAL/kt79-repro-check.log`, `kt79b-ps-builds.log`, `kt79b-macos-canonical.log`, `kt79-negative.log`; `v2/docs/k6-repro.md` §5.1, §6.

---

## B2 · Windows — **ĐÓNG 2026-11-17 (D2-122.4): thành nợ v2.1**

Host **Windows** (`psvn`) **không** hội tụ sha: khác **thứ tự monomorphization** (`core::fmt::Display::fmt` do crate khác sở hữu). Nguyên nhân là lựa chọn của rustc/LTO theo host, **không** phải mã nguồn. Muốn hội tụ cần `-Z build-std` (nightly) hoặc chuẩn hoá tầng linker.

**Câu hỏi**: v2.0 có cần tái lập trên **Windows** không? Nếu không, ghi “v2.0 chỉ tái lập trên Linux + macOS” là đủ.
**Đề xuất**: **không** cấp ngân sách; ghi giới hạn host Windows vào `k6-repro.md` §7 và nợ v2.1.

---

## B3 · K6 ĐẠT/không ĐẠT — chờ bằng chứng KT-77b (mở 2026-11-17)

Agent 1 **không** được tuyên bố mốc. Khi KT-77b xong hoặc hết ngân sách (OD-K6-1), Agent 1 sẽ trình: log serial trên ảnh sản phẩm, ca âm tắt shell, bằng chứng máy “không chứa harness”, kích thước ảnh, danh sách `deferred-K7`. **Quyết định đạt/không đạt thuộc Agent 0.**
