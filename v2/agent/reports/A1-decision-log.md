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
| **A1-171** | 2026-11-17 | **Thi hành** thứ tự Agent 0 đã duyệt: chạy **KT-78 ∥ KT-79** (song song), vùng ghi tách rời (`v2/test/boot/**` cho KT-78; toolchain/build config + `v2/tools/**` cho KT-79). Không phải quyết định mới — OD-K6-1/A.4 đã ghi “KT-78, KT-79 → KT-77a”. | `reports/QUYET-DINH-A0-K6-2026-11-17.md` §3 + phụ lục A.4; D2-105 (một người ghi) ⇒ tách vùng ghi để không đụng nhau | **CHỜ A0 XÁC NHẬN** |
| **A1-177** | 2026-11-17 | **Sự cố index dùng chung (D2-91 tái diễn)**: tệp của **KT-80** (`v2/tools/verify-numbers.py`, `v2/test/rules/*`, `v2/bin/v2`, `v2/docs/k6-machine-rules.md`) bị **dòng ViScript** cuốn vào commit `4cea479c` `[ST-165]` — nội dung **có** trong lịch sử nhưng **sai nhãn task**. Không rewrite được (đã push). Cách xử lý: (a) ghi nhận tại đây để truy vết; (b) commit bổ sung `[KT-80]` mang log + docs còn lại (`47e18d9e`) để tối thiểu phần lớn KT-80 nằm đúng nhãn; (c) **luật mới**: agent **tuyệt đối không `git add`** (kể cả `git add -N`), chỉ lead stage; và lead **kiểm `git show --stat <commit> -- v2`** trên mọi commit (kể cả của dòng khác) để phát hiện nhiễm. | `git log --oneline -1 -- v2/tools/verify-numbers.py` → `4cea479c [ST-165]`; `git show --stat 4cea479c -- v2` rỗng (tệp vào ở commit đó nhưng không hiện trong stat lọc — xác nhận bằng `git log -- <path>`); `47e18d9e` mang 12 tệp KT-80 | **CHỜ A0 XÁC NHẬN** |
| **A1-176** | 2026-11-17 | **Việc sổ**: chuyển KT-78 sang **DONE** (task đã qua **2 vòng review**: vòng 1 DRIFT LOW + vòng delta DRIFT LOW, cả 3+2 mục đã vá và tôi **tự kiểm lại** bằng lệnh độc lập: `QG_FORCE_NO_SETPGRP=0` ⇒ `not ok 8 / cases=8 ok=7`, mặc định ⇒ `ok=8/8`; ảnh phát hành `fd202276` không đổi). KT-79 giữ **REVIEW** vì còn phụ thuộc quyết định B1 của A0 (ảnh phát hành). | `8664d69f`; log `kt78-guard/selftest-guard-{default,override0}.log`; `v2 check` PASS trong worktree sạch | **CHỜ A0 XÁC NHẬN** |
| **A1-175** | 2026-11-17 | **D2-108A tái diễn lần 3** — build canonical của KT-79 ghi đè `v2/kernel/target/…/release/vifast-kbin-riscv64` (82552 B / `c1072d72…`) trong lúc KT-78 đang đo, buộc reviewer-kt78 **không tái lập được** byte ảnh `fd202276…`/188552 và phải bỏ chạy lại matrix. Luật: (i) **mọi** build repro/canonical phải dùng `CARGO_TARGET_DIR` **riêng**; (ii) script phải in `shasum` của ảnh phát hành **trước và sau**, **fail-closed** nếu đổi; (iii) ca âm: trỏ target dir vào repo ⇒ báo lỗi; (iv) “ảnh phát hành **không bao giờ** bị build repro ghi đè”. Đồng thời: **lead** phải cấm chạy repro build song song với measurement (đây là lỗi điều phối của tôi). | reviewer-kt78: artifact lúc **10:46:05** = `82552 B`/`c1072d72…`; hiện tại (11:0x) = `188552 B`/`fd202276…` (đã về); `kt78-product-sha.log` ghi `fd202276…`/188552/`build=ef566cfa583e34bb` | **CHỜ A0 XÁC NHẬN** |
| **A1-174** | 2026-11-17 | **Build config, không phải thiết kế**: KT-79 đặt `rust-toolchain.toml` ở **gốc repo** ⇒ Cargo áp nó cho **mọi** thư mục con, gồm cả dòng **ViScript** (`crates/`, `viscript/`). Trên máy này **không đổi gì** (stable = 1.99.1 = bản ghim) nên chưa hồi quy; nhưng trên máy khác (psvn stable = 1.100.0) dòng ViScript sẽ bị **ép** xuống 1.99.1 — thay đổi chính sách build của **dòng khác**, ngoài phạm vi OD-K6-3 và ngoài quyền Agent 1 (D2-121: Agent 1 không đổi chính sách của dòng khác). ⇒ Yêu cầu **thu hẹp**: ghim đặt ở `v2/kernel/rust-toolchain.toml` (hoặc `v2/rust-toolchain.toml`) và gọi **tường minh** `cargo +1.99.1`/`RUSTUP_TOOLCHAIN` trong `v2/tools/repro-*` + `v2/bin/v2`; kiểm lại sha canonical **không đổi** sau khi dời. | `rust-toolchain.toml` hiện ở **gốc repo** (`channel = "1.99.1"`); cargo tìm file ghim ở thư mục hiện tại **và mọi thư mục cha**; `ls ~/.rustup/toolchains` → có `1.99.1-aarch64-apple-darwin` (active) và `stable`/`nightly` cùng 1.99.1 ⇒ hôm nay chưa hồi quy | **CHỜ A0 XÁC NHẬN** |
| **A1-173** | 2026-11-17 | **Sửa lỗi giao việc của Agent 1**: máy thứ hai cho KT-79 (OD-K6-3, D2-120.4) là **`ps@ps`** (Linux x86_64), **không phải `psvn`** (Windows 11). Nguyên nhân: lần thăm dò đầu tôi dùng `ssh ps` với user mặc định `mps` ⇒ tailnet từ chối; tôi thấy `Host psvn / User ps` trong `~/.ssh/config` và chuyển sang psvn. Đúng phải là `ssh ps@ps`. ⇒ Kết quả Windows của KT-79 (thứ tự monomorphization khác) là **dữ kiện phụ bị thay thế**, không phải kết luận OD-K6-3. Mở **KT-79b** trên `ps@ps`. | `ssh ps@ps 'uname -sm'` → `Linux x86_64`; 12 core; 53G free; `rustc 1.93.1` distro (**không** rustup); `rustup target list --installed` **không có** riscv64; `tailscale status` liệt kê `ps` = linux và `psvn` = windows là hai máy khác nhau | **CHỜ A0 XÁC NHẬN** |
| **A1-172** | 2026-11-17 | **Phân loại sự cố** (không phải quyết định): máy thứ hai `psvn` = **Windows 11**, shell mặc định PowerShell, có `rustc/cargo/rustup/git`, WSL chỉ có `docker-desktop` (Stopped), **chưa có repo**. ⇒ KT-79 phải build bằng `cargo` trực tiếp (không dùng `v2/bin/v2` là bash), chuyển commit bằng `git bundle`+`scp`. Đây là dữ kiện để KT-79 chọn đường, **không** phải kết luận “không build được”. | lệnh thăm dò trực tiếp: `ssh psvn 'powershell -NoProfile -Command "(Get-CimInstance Win32_OperatingSystem).Caption"'` → `Microsoft Windows 11 Home`; `Get-Command rustc,cargo,wsl,git` → 4 đường dẫn; `wsl -l -v` → chỉ `docker-desktop Stopped`; `Test-Path C:\ViFastOs` → `False` | **CHỜ A0 XÁC NHẬN** |
