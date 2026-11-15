# KT-63 — ghi chú phân bổ commit (trung thực, không sửa lịch sử đã push)

- **`4dd5014f`** (`[KT-00] D2-90 …`) do lead tạo trong lúc KT-63 đang làm; commit này **vô tình
  chứa toàn bộ cây làm việc KT-63** (kernel `ipc/core_capxfer/core_finalise/mdb/tcbdata/
  machine/footprint`, test parity + core-only, TAP 53..55, manifest/expected, docs §12, journal,
  STATE). Nội dung đã được review độc lập (`reviewer-kt63`, DRIFT trung bình) và vá F1/F2/F3.
- **`489b0883`** (`[KT-63] chuyen cap qua IPC … finalise IPC khi Revoke, TAP 56, gate 625`) chứa
  **bản vá sau review**: F1 (bảng cap đầy ⇒ `NotEnoughMemory` trước `admit`), F2 (window trỏ CNode
  chết ⇒ `InvalidCapability` trước `admit`), F3 (ca cap nguồn chết giữa hàng), + 3 test core-only
  (10 → 13) + docs D2-90/§12.5/§12.8.
- Không rewrite `4dd5014f` vì đã push `origin/main`; giữ lịch sử tuyến tính. Khi đọc lịch sử:
  feature KT-63 nằm ở `4dd5014f`, bản vá review ở `489b0883`; cả hai đều qua gate **625/80**,
  matrix **9/9**, soak **0/1000** trên cây cuối.
