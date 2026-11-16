# KT-71 — tầng 2 VFS: journal + rename/truncate/double-indirect + fsck (K6)

Trạng thái: **phần cài + đo DONE**, chờ reviewer. Phạm vi: mọi thao tác **metadata** đi qua
**journal redo** (`intent → commit → áp dụng`) và **replay sau crash**; **rename** (kể cả **đè**),
**truncate** (về 0 **và** giải phóng block, kể cả bảng double-indirect), **double-indirect**;
**ghi qua cache (write-back)**: trang **bẩn** phải xuống backend **trước khi bị trục xuất**;
**fsck phát hiện + SỬA** bốn loại hỏng; **7 pha crash** trên QEMU thật. Không thêm syscall,
không sửa `spec/`, không nới ngưỡng.

## 1. Kiến trúc

```
                 ┌──────────────── Fs<D: BlockDev> (v2/lib/vifast-fs) ────────────────┐
 thao tác  ────▶ │ stage(lba, data)  →  journal[dữ liệu]                            │
 (create/write/  │ stage_inode/stage_bitmap                                             │
  rename/        │ commit():                                                            │
  truncate/…)    │   1. ghi dữ liệu journal  (LBA 13..)                                 │
                 │   2. ghi header VJ01 (targets + CRC32)  (LBA 12)                     │
                 │   3. **flush thiết bị**  ← KT-71 thêm: THÂN LOG phải bền TRƯỚC      │
                 │   4. ghi commit record VC01 (LBA 29)  ← điểm commit                  │
                 │   5. ghi các target (áp dụng)                                        │
                 │   6. flush; đọc lại từng target (bỏ cache) so CRC                    │
                 │   7. xoá VC01                                                        │
                 └──────────────────────────────────────────────────────────────────────┘
 mount: load_metadata → replay(): VC01 sống? → đọc thân log → CRC → áp dụng (idempotent) → xoá VC01
        CRC sai / header lệch seq ⇒ replay_status = 2, **không áp dụng gì**, xoá VC01
```

`v2/lib/vifast-fs` (port v1, D2-26) đã có **định dạng trên đĩa** VF04 (superblock + bitmap +
inode + journal 18 block + vùng data) và `rename`/`truncate`/`fsck`/`replay` — KT-71 **tận
dụng** đúng định dạng đó, chỉ thêm phần **thiếu** (xem §2) thay vì dựng FS thứ hai.

## 2. Phần KT-71 thêm (đỏ-trước: `JOURNAL/kt71-red-host.log`)

| # | Thay đổi | Ở đâu | Vì sao |
|---|---|---|---|
| 1 | **flush trước commit record** | `vifast-fs/src/tx.rs` `commit()` | Thiếu flush, đĩa có cache ghi có thể để commit record xuống **trước** thân log ⇒ crash cho ảnh "đã commit nhưng thân log hỏng"; CRC chỉ cứu khỏi áp dụng rác, còn thao tác **đã commit thì mất**. Có flush: "commit record sống ⇒ thân log sống" — bất biến write-ahead. |
| 2 | **`rename` đè** | `vifast-fs/src/dir.rs` `rename()` | Bản cũ trả `Exists`; POSIX (và mục tiêu KT-71) đòi **thay thế** đích: file↔file, dir↔dir **rỗng**; dir không rỗng ⇒ `NotEmpty`, file đè dir ⇒ `IsDir`, dir đè file ⇒ `NotDir`. Inode/block của đích được **giải phóng** (nlink−1, nếu 0 thì trả block + inode). |
| 3 | **`fsck_repair` + `FsckReport`** | `vifast-fs/src/fsck.rs` (mới) | `fsck()` cũ **chỉ phát hiện** (`Err(Corrupt)`). Cần **sửa** và **đếm**: block mồ côi, block đếm sai (con trỏ ngoài `size`, bảng thừa), inode không tham chiếu, link count sai, entry trỏ inode chết, size thư mục sai. Sửa đi qua journal (`stage`+`commit`), commit khi transaction gần đầy (`MAX_TX_WRITES=16`). Block bị **hai** nơi tham chiếu ⇒ đếm `unfixed` (không sửa mù). |
| 4 | **`PrefixDisk` + `run_op`** | `vifast-fs/src/crash.rs` (mới) | Mô hình crash: "đĩa bền theo **tiền tố** lệnh ghi" — `arm(limit)` chỉ cho `limit` lệnh ghi đầu tiên sống sót, phần đuôi mất **im lặng** (cache volatile), FS không thấy lỗi I/O. Không `unsafe`, không cấp phát, dùng được cả host lẫn kernel. |
| 5 | **`into_dev` + `inode_blocks_raw`** | `vifast-fs/src/{fs,dir}.rs` | `into_dev`: đóng FS giữ ảnh đĩa để **mount lại** (replay). `inode_blocks_raw`: chẩn đoán cấu trúc (test chứng minh double-indirect **hai cấp thật**), không đổi API đĩa. |
| 6 | **Ghi qua cache (write-back)** | `vifast-vfs/src/{cache,core,status,stats}.rs` | `PageCache` chỉ đọc (KT-70). Thêm `dirty` cho mục, `PageSource` (đối xứng `PageSink`), `mark_dirty`, `flush_page` (8 sector + **flush thiết bị**), `flush_all`; `Begin::Evict{dirty}`; `load()` **từ chối** nạp đè slot bẩn (`Busy=14`); giữ **bản ghi mục vừa trục xuất** để còn `flush_page(key)` được sau `begin_miss`. Bộ đếm mới: `dirty_marks/dirty_now/writes/flushes/flushed_pages`. |

## 3. Write-back — ngữ nghĩa đo được

- `fault/read` → `begin` (hit/ready/evict) → `load` (đọc backend) → chương trình ghi vào **Frame**
  → `mark_dirty(file,page)` → `flush_page` = ghi đủ `SECTORS_PER_PAGE=8` sector + `dev.flush()`
  → `clear_dirty` (sạch). Đóng/mở lại (`PagerCore` mới trên **cùng** thiết bị) đọc đúng dữ liệu.
- **Trục xuất**: `begin` trả `Begin::Evict{slot,key,dirty}`; nếu `dirty=1` thì `load()` trả
  `Busy` **cho tới khi** `flush_page(key)` — không có đường nào ghi đè dữ liệu chưa xuống đĩa.
  Trang sạch thì `load` nạp đè được ngay.
- Bộ đếm an ninh kiểu "luôn 0" đều có **ca âm**: `flush_page` ngoài cache ⇒ `InvalidArgument`,
  `mark_dirty` ngoài file ⇒ `InvalidArgument`, backend lỗi I/O ⇒ `RangeError` (và `reads`
  **không** tăng — không đếm thành công giả).

## 4. Journal + crash — mô hình và 7 pha

- **Điểm commit** = lệnh ghi `VC01` (sau flush thân log, trước mọi target).
- **Crash** = một **tiền tố** lệnh ghi của thao tác sống sót (mô hình **đơn giản hoá có chủ
  đích**: không tái sắp xếp lệnh ghi; hệ quả "commit record sống ⇒ thân log sống" là do flush
  ở bước 3, và ca âm CRC chứng minh replay **từ chối** thân log hỏng chứ không áp dụng bừa).
- **7 pha** là 7 tiền tố: `0`, `w/4`, `w/2`, `w-3`, `w-2`, `w-1`, `w` với `w = 7` lệnh ghi của
  một `write` 1 chunk (`2 data journal + 1 header + 1 commit + 2 target + 1 clear`). Sau mỗi pha:
  `replay` + `fsck_repair` phải cho FS **nhất quán** (`fsck()` sạch, `found=0`), `/keep` (đã
  commit **trước**) không mất, và thao tác dở phải **nguyên tử** (cũ hoặc mới, không nửa vời).
- **Ca âm journal** (D2-108B): commit record sống nhưng thân log bị ghi rác ⇒
  `replay_status=2`, nội dung **cũ**, `/keep` còn, `fsck` sạch. Nếu bỏ CRC, ca này sẽ áp dụng
  dữ liệu rác — đây là bằng chứng CRC không phải trang trí. Đo thêm **đĩa thô**: block dữ liệu
  của `/log` sau khi replay từ chối vẫn là nội dung **cũ** (`crc_disk=old`) — không tin lời khai
  của FS.
- **Ca âm ĐẢO THỨ TỰ (chứng minh flush-trước-commit-record)**: thêm mô hình cache ghi
  `CRASH_MODE=1`: lệnh ghi chưa flush nằm trong `PENDING` (đọc-thấy-ghi vẫn đúng); "chết" tại
  điểm crash ⇒ `crash_keep_last()` chỉ cho **lệnh ghi CUỐI** xuống đĩa — đúng ca commit record
  vượt thân log. Kịch bản: dừng ngay sau commit record (`writes-3` lệnh ghi). Kết quả đo:
  đĩa thô trước replay = **old** (`pre=old`, target chưa ghi), `replay_r=1` ⇒ replay **áp dụng**,
  `new_r=1`, đĩa thô sau = **new** (`post=new`) — vì thân log đã bền bởi flush trước commit
  record. **Mutation** (bỏ đúng một dòng `dev.flush()` trước `VC01`): host test
  `journal_dao_thu_tu_commit_record_phai_con_than_log_nho_flush` **đỏ** (`replayed` 2 ≠ 1,
  `JOURNAL/kt71-red-reorder.log`) và QEMU **`not ok 67`** với `replay_r=2 new_r=0 post=old`
  (`JOURNAL/kt71-red-reorder-qemu.log`) — bất biến này **có ca âm**, không phải tuyên bố suông.

## 5. Bằng chứng đo (QEMU thật, `bash v2/test/boot/ktest.sh 1 1`, TAP 65..69)

Nguồn: `JOURNAL/kt71-green-qemu-ktest.log`, `JOURNAL/kt71-ktest-matrix.log`. Mọi dòng `# fs:`
do **kernel in từ biến đo** (D2-102), hằng thiết kế in kèm `const:`; giá trị suy ra in kèm
**công thức + đầu vào cùng dòng** (D2-106).

| TAP | Dòng `# fs:` (đo) |
|---|---|
| 65 | `rename src=2 exp=2 replaced_gone=1 dir_over=1 used=6->2 impl=rename-overwrite`; `truncate used=6->2 freed=4 read_empty=true reuse=4 (=used_again-used_t0) used_again=6 used_t0=2 reuse_ok=true const:from_blocks=4`; `double index=143 used=6->9->6 delta=3 (=3: root+bang L2+data) root=37 l2=38 data_ptr=36 single=0 read=4 data=2CAP fsck=true` (root/l2/data_ptr đọc từ **đĩa thô**) |
| 66 | `writeback dirty_marks=0->1 dirty_now=0 writes=8 flushes=1 wrote=true backend_ok=true reopen_ok=true const:page=4096 const:sector=512` |
| 67 | `journal writes=7 issued=7 commit_prefix=4 (=writes-3: 2 target + 1 clear) replay=1 new=1 size=7 keep=true fsck=true crc_reject=2 crc_applied=0 crc_disk=old reorder_writes=5 kept=true pre=old replay_r=1 new_r=1 post=new keep_r=true` |
| 68 | 7 dòng `crash pha=0..6` (`content=old` ở pha 0..2, `new` ở pha 3..6, `found=0`, `keep=true`, `clean=true`) + `crash7 phases=7 writes=7 old=3 new=4 torn=0 consistent=7 threshold=4 last_old=3 atomic=1 replays=3` |
| 69 | `fsck detect=1 found=4 fixed=4 unfixed=0 orphan_blocks=1 orphan_inodes=1 bad_nlink=1 bad_blocks=1 used=2->4->2 keep=true fsck_ok=true second=0 unfixed_pos=true unfixed2=1 double_blocks2=1 second_bad=true const:disk_blocks=48` — `unfixed_pos` là **ca dương** của `unfixed` (hai inode cùng block: `unfixed2=1`, `fsck()` còn đỏ) |

Stack boot đo được: `# fs: stack entry used=39761`, `end used=40577` của 65.536 B — pha FS
**chỉ giữ MỘT** `Fs` 13 KiB (tạo trong `fs_phase`, "mount lại" bằng `initialize()` tại chỗ);
lý do: `Fs::mount` trả 13 KiB theo giá trị và các khung lồng nhau từng làm tràn stack 64 KiB
(đỏ QEMU đầu tiên: guard page fault ở `fs_phase+0x26`, ghi ở journal).

## 6. Ngưỡng và tài nguyên (không nới)

- `size_of::<Machine>()` **188.872 B** — pin D2-84 **196.608 B**, không đổi.
- Ảnh `ktest` `.data+.bss` đo sau KT-71: **242.840 B** ≤ trần **262.144 B** (thiết bị RAM 48
  block = 24.576 B + Frame 4 KiB nằm trong `.bss`, in kèm `const:disk_blocks=48`).
- Hình học FS: block 512 B, 48 block, vùng data `48 − 30 = 18` block; journal 18 block.
- `MAX_TX_WRITES=16` (một transaction tối đa 16 block) — `fsck_repair` commit khi `tx+4 ≥ 16`.

## 7. Giới hạn / nợ

- **Pager U-mode chưa ghi**: đường `mark_dirty/flush_page` đo trong kernel (cùng mã crate) và
  host; đường U-mode (pager ghi trang qua `Frame.Map` + shim) chờ **KT-72** (virtio-blk) — khi
  đó chỉ thêm `impl BlockDev`, không sửa `PagerCore`.
- Crash model là **tiền tố** (không mô phỏng tái sắp xếp); flush ở bước 3 là bất biến write-ahead
  bảo vệ trường hợp đó, và ca âm CRC phủ nhánh "thân log hỏng".
- Journal chỉ **redo** (không undo); thao tác nhiều chunk (`write` lớn) là nhiều transaction
  512 B, không có tính nguyên tử toàn file (giữ nguyên ngữ nghĩa v1 — đã ghi trong `lib.rs`).
- `fsck_repair` **không** sửa block bị hai inode cùng trỏ (đếm `unfixed` + `double_blocks`);
  không kiểm bảng thư mục trùng tên (ngoài phạm vi bốn loại hỏng của KT-71).

### 7.1 Bài học: `Fs::mount` 13 KiB làm tràn stack boot (bug thật, đã sửa)

- **Triệu chứng**: boot ảnh `ktest` chết ở `PANIC: guard page fault scause=0xf
  stval=0x80400808 sepc=fs_phase+0x26` — không phải panic của test mà là **ghi vào trang bảo vệ
  stack**; log trước đó chỉ tới TAP 64, **không** có dòng `# fs:` nào.
- **Cách phát hiện**: đối chiếu `sepc` với symbol (`llvm-nm -n` → `fs_phase+0x26`), rồi suy ra
  khung `fs_phase` > 64 KiB. Nguyên nhân: `Fs::mount` **trả `Fs` 13 KiB theo giá trị**; mỗi lần
  gọi có thêm bản sao trong khung của nó, cộng khung gọi ⇒ 4–5 bản × 13 KiB.
- **Cách sửa**: (a) pha FS giữ **đúng MỘT** object `Fs` tạo trong `fs_phase`; "mount lại" =
  `initialize()` **tại chỗ** (`fn remount`); (b) `#[inline(never)]` cho các pha/`mount`/`fsck_repair`
  để khung không bị gộp; (c) mô hình crash dùng **static + `AtomicU8`** (không bọc `Fs` lần hai);
  (d) in `# fs: stack entry/end` đo bằng `mmu::stack_range` ⇒ hiện 39.761/40.545 của 65.536 B.
  Host test không cần mẹo này (stack 2 MiB) nhưng cùng mã.
- **Phòng ngừa tái diễn**: quy ước "một object lớn ~13 KiB/pha trên kernel 64 KiB" + in
  `# fs: stack` ở **mọi** pha nặng; reviewer kiểm dòng này; nếu thêm object lớn thứ hai thì phải
  tách ảnh/khung chứ không nới stack.

### 7.2 Nợ có ngưỡng: ảnh `ktest` đã dùng 92,6 % `.data+.bss`

- Đo sau KT-71: **242.840 B / 262.144 B** (headroom **19.304 B**), phần tăng chủ yếu là thiết bị
  RAM 24.576 B + Frame 4 KiB + cache ghi tái sắp xếp (12 × 512 B). **Không nới trần**; task sau
  thêm gì vào ảnh test phải đo lại `budget.sh`, nếu chạm trần thì **tách ảnh** (như KT-66 đã tách
  `ktest-proc`).
