# KT-77c — FS + virtio-blk trong **ảnh sản phẩm** (D2-123 mục 2)

Ngày: 2026-11-17 · Task: **KT-77c** · Ngân sách: **3 h** (bắt đầu ~13:15 JST, hết hạn ~16:15) ·
Dừng trong hạn: **~1 h 05 m** (kết thúc đo lúc ~14:20). Không xin thêm ngân sách.

Tài liệu bằng chứng: **`v2/docs/k6-product-fs.md`** (mọi số đo trích từ log trong
`v2/agent/JOURNAL/kt77c-logs/`).

## Đạt

1. **Ảnh sản phẩm có FS**: `sha256=64a18ebf…cb0`, **205.440 B**, `build=1e84e92dc17e58ab`,
   `text=130062`; boot tới `shell>`, mount virtio-blk (`-drive`) hoặc volume RAM (không đĩa).
2. **`ls`/`cat`/`write` thật** trên ảnh sản phẩm; **ghi rồi reboot trên cùng tệp ảnh đĩa đọc lại
   đúng** (`cat /p.txt` → `hello77`) ở **1/2/4 hart**; sha ảnh đĩa **không đổi** giữa hai boot.
3. **Parity**: `ls`/`cat` chuyển `active` (giữ `source`), chạy **6/6 active** × **smp 1 và 4**, exit 0.
4. **Tái lập byte**: hai `CARGO_TARGET_DIR` khác nhau, **cùng** đường dẫn nguồn ⇒ **cùng sha**;
   ảnh **không** feature vẫn đúng baseline `c1072d72` (đo **trước** thay đổi của KT-86).
5. **Ngưỡng**: ảnh 205.440 ≤ 262.144; `.data+.bss` 223.696 ≤ 262.144; `size_of::<Machine>()`
   = 189.384, `pin` 196.608 **không đổi**; **không** nới `MAX_*`/`TOTAL_BLOCKS`/trần nào.
6. **Ca âm**: (a) không đĩa ⇒ `medium=ram volatile=1 why=khong-co-pci`; đĩa toàn rác ⇒ format lại;
   đĩa 4 KiB ⇒ `why=dia-qua-nho` — cả ba **không treo**; (b) ghi + `fsync` rồi reboot ⇒ byte bền;
   (c) build `product-shell` **không** `product-fs` ⇒ parity `ls`/`cat` **đỏ** (`not ok 5`, `not ok 6`).
7. **Marker harness = 0** trên ảnh sản phẩm; ca âm `ktest-proc` = **25 hit**; probe-gate
   `ktest=0 umode=1`.
8. **Không hồi quy ảnh test**: `ktest-proc` **10/10 TAP** + PANIC W^X chủ đích — **không** sửa kỳ
   vọng nào của harness.
9. **Bẫy feature** (giống KT-77b): `vblk = ["ktest"]` ⇒ tách `block = []` + `vblk = ["block",
   "ktest"]`; **không** `BLOCKED-CHO-A0` vì driver **không** có `cfg(feature = "ktest")` (chỉ nhãn
   feature sai).

## Không đạt / ghi nợ (trung thực)

- **Tiêu chí 2 chỉ đạt một phần**: spec **không có** mục `ghi tệp` và `đọc lại sau reboot` riêng
  (28 mục cố định) ⇒ chỉ chuyển được **`ls`/`cat`**. "Ghi + đọc lại trong một boot" đo bằng `send`
  của mục `cat`; "đọc lại **sau reboot**" đo bằng bằng chứng riêng (hai boot, cùng ảnh đĩa) và ghi ở
  `k6-product-fs.md` §6 — **không** thể đo trong một mục của spec (mỗi mục = một boot).
- **`exec`/`runfs`/`run`** giữ `deferred-K7` (D2-123(2) cho phép).
- **Không** chạy ma trận 24 boot + soak 1.000 (D2-123(3): chạy **một lần** trên ảnh cuối, sau KT-77c).
- **Không** sửa nợ A1-184 (`build.rs` không truyền `--remap-path-prefix` cho ELF U-mode ⇒ tái lập
  byte chỉ đúng khi **cùng** đường dẫn nguồn). Lead đã cho phép bỏ nếu > 20 phút; ưu tiên tiêu chí FS.
- **`./v2/bin/v2 check`: 1 bước FAIL** — "luật máy K6" với **49 lỗi đều ở
  `v2/agent/reports/TRA-LOI-A0-D2-125.md`** (tệp của agent khác, **ngoài** vùng ghi của tôi).
  fmt / clippy / R1..R5 / gen-check / L0 build **OK**; tài liệu của tôi (`k6-product-fs.md`,
  journal này) **0 lỗi** (`docs_checked=5 pre_existing=3`). Log: `kt77c-logs/kt77c-v2-check.log`.
- **Ảnh sẽ đổi sha** khi KT-86 land (thêm dòng `# build: rev=…` vào `src/main.rs`, D2-125.3) — KT-86
  đã báo; mốc baseline `c1072d72` trong tài liệu của tôi **chỉ đúng trước** thay đổi đó.

## Quyết định kỹ thuật đáng ghi (để task sau không lặp lỗi)

1. **`Fs` (13 KiB) phải ở `.bss`, không ở stack**: trả nó qua bốn tầng theo giá trị làm **tràn guard
   page 64 KiB** (đúng lỗi KT-75 ghi ở `ktest_persist.rs:915`). Thêm
   `vifast-plat-riscv64/src/static_slot.rs` (`StaticSlot<T>`, API an toàn, "một lần ghi") ⇒ chỉ
   chuyển **tham chiếu**; `unsafe` nằm trong crate thuộc D2-17 (luật R5 **không** cho `unsafe` ở
   `vifast-kbin-riscv64`).
2. **Không chèn dòng vào giữa tệp nguồn đang link vào ảnh không feature**: một dòng `//` làm lệch
   số dòng panic-location ⇒ `.rodata` đổi ⇒ baseline đổi sha (đo được: `169ff6cb…`, lệch 5 byte).
   Cách làm: sửa `#[cfg]` **tại chỗ**, chú thích ở **cuối tệp**, `mod` mới ở **cuối** `main.rs`.
3. **`ino` không được OR vào MR word của tên** (`p.txt` in thành `s.txt` khi `ino = 3`): đặt `ino`
   ở byte cao **word 0** của reply `ls`.
4. **Payload serial lặp mỗi lệnh hai lần** cho bằng chứng (lệnh đầu có thể mất — nợ K7 của KT-77b);
   parity thì `stdin_wrap.sh` bơm tám lần.
5. **Ca âm của "tắt FS"**: dùng `--cfg shell_fs` do `build.rs` đặt theo `product-fs` ⇒ shell của ảnh
   test **không** có `ls`/`cat`/`write` ⇒ `help cmds=6` **giữ nguyên** ⇒ **không** phải sửa
   `proc_demo.rs`/`v2/test/boot/ktest.sh` (đây là lý do TAP 3 của `ktest-proc` vẫn xanh).

## Ảnh hưởng tới tệp của người khác

- `v2/kernel/vifast-kbin-riscv64/build.rs`: tôi thêm `--cfg shell_fs` + `build_user_elf_with` cho
  **đúng** `shell.elf`; **đã** thông báo `agent2-kt86` (KT-86 cũng ghi tệp này) và **đóng** vùng ghi
  của mình sau 14:00.
- `v2/kernel/vifast-kbin-riscv64/src/main.rs`: chỉ `mod proc_fs;` ở **cuối** tệp + chú thích.
- **Không** chạm: `v2/docs/k6-product-shell.md`, `.git/hooks/**`, `v2/tools/git-guard/**`,
  `v2/test/rules/**`, `v2/bin/v2`, `v2/tools/repro-*`, `v2/agent/STATE.md`, `v2/agent/BLOCKED.md`,
  `crates/`, `viscript/`, `docs/v1/`, `docs/v2/`, `v2/test/boot/**` (kỳ vọng harness **không** sửa).
- **Không** commit, **không** `git add` (A1-177/A1-180, D2-123.4).

## Bằng chứng (tệp trong `v2/agent/JOURNAL/kt77c-logs/`)

`kt77c-build-fs-1.log`, `kt77c-build-fs-2.log` (hai target dir, cùng sha), `kt77c-build-baseline.log`
(`c1072d72`), `kt77c-build-negative-nofs.log`, `kt77c-budget.log`, `kt77c-boot-{write,reboot-read}-
smp{1,2,4}.log`, `kt77c-boot-nodisk-smp1.log`, `kt77c-boot-disk-broken-smp1.log`,
`kt77c-boot-disk-too-small-smp1.log`, `kt77c-parity-smp{1,4}.log`, `kt77c-parity-neg-nofs.log`,
`kt77c-regress-ktest-proc.log`, `kt77c-harness-scan.log`, `kt77c-v2-check.log`,
`kt77c-crash-and-lsbug-transcript.log`; script: `../kt77c-boot.sh`, `../kt77c-evidence.sh`.
