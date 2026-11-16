# KT-70 — pager + page cache tầng 1 (K6)

Trạng thái: **phần cài + đo DONE**, chờ reviewer. Phạm vi: K6 tầng 1 của VFS —
**pager U-mode** nhận fault trang của client, **page cache** theo `(file_id, page_no)` với
trục xuất **LRU**, **backend thay thế được** (RAM hôm nay, virtio-blk ở KT-72), **hai client
U-mode** chia sẻ **một bản** trang. Không thêm syscall, không sửa `spec/`, không nới ngưỡng.

## 1. Kiến trúc

```
client A (U-mode)          kernel (S-mode)                 pager server (U-mode)
  load VA chưa map  ──trap──▶ classify_u_fault
                              FaultKind::Vm{NotMapped}   ──inject_fault──▶ fault entry (fault_ep)
                              (core ghi last_fault)      ──message IDL───▶ pager.fault(client,file,page,va)
                                                                           │ begin(file,page)
                                                                           │  hit  ⇒ slot có sẵn
                                                                           │  miss ⇒ đọc 8 sector
                                                                           │          từ BlockDev
                                                                           │ Frame.Map(slot→VSpace client)
                              resume đúng instruction ◀──served(slot)──────┘
  đọc lại VA ⇒ dữ liệu đúng
```

- **Fault do phần cứng**: trap U-mode (`scause=0xd/f`) → `vifast_plat_riscv64::user` phân loại
  `FaultKind::Vm{vspace, va, access, reason}` và **giữ `sepc`** tại instruction lỗi; kernel
  `inject_fault(tcb_client, …)` đưa fault vào `fault_ep` của tiến trình (core, D2-25).
- **Vì sao thêm message IDL?** `Recv` của ABI **không** mang payload fault (chỉ `last_fault`).
  Cùng tiền lệ procsrv (KT-65, `FAULT_EXIT`): kernel gửi tiếp một message `pager.fault` chứa
  đủ `(client, file, page, va)` bằng **stub sinh từ IDL**; pager lấy `last_fault` của chính nó
  để xác nhận fault đã đi qua core (`core_fault=1` trong bằng chứng).
- **Trả lời**: pager `Frame.Map` Frame của slot vào VSpace client tại đúng VA bị fault, rồi
  gửi `served(client,page,slot,status)`. Kernel **resume** client tại `sepc` đã lưu; instruction
  load chạy lại và đọc dữ liệu vừa map. Không copy dữ liệu trang vào client.

## 2. Giao thức IDL — `v2/lib/vifast-idl/idl/pager.vidl`

`interface pager version 1`, `# caps=none` (không truyền cap qua IPC), method id sinh tự động
0..4 theo thứ tự khai báo; stub `no_std`, **không `unsafe`**, encode/decode **đúng arity**
(D2-104 — `pager_stub_arity_va_roundtrip` kiểm `va` 64-bit tròn qua MR0..MR3):

| id | method | tham số | trả |
|---|---|---|---|
| 0 | `fault` | `client u32, file u32, page u32, va u64` | `u32` (0 = nhận) |
| 1 | `served` | `client u32, page u32, slot u32, status u32` | unit |
| 2 | `ready` | — | `u32` |
| 3 | `finish` | `faults, hits, misses u32` | `u32` |
| 4 | `stop` | — | `u32` |

Sinh lại: `./v2/bin/vifast-idl gen` (target thứ hai `src/generated_pager.rs`); `--check` và
fixture âm nối vào `v2 check` như KT-69 (`v2/test/gen/run_idl_fixtures.sh` copy `idl/*.vidl` +
`src/generated*.rs`).

## 3. Page cache (host-testable, `v2/lib/vifast-vfs`)

- Khoá `(file_id, page_no)`; **một mục/khoá** (`keys_unique`), mỗi mục giữ **slot** = Frame
  trong pool; dữ liệu trang nằm trong Frame, cache chỉ giữ metadata.
- `begin`: `Hit{slot}` ⇒ **không** đọc backend (đếm `hits`); miss ⇒ `Ready{slot}` (slot trống)
  hoặc `Evict{slot,key}` (chọn **LRU** theo `stamp`, caller phải gỡ mapping cũ **trước** khi
  ghi đè); trang ngoài file ⇒ `Reject{InvalidArgument}` và **không** đọc backend.
- `load`: đọc `PAGE_SIZE / SECTOR_SIZE = 4096/512 = 8` sector qua `BlockDev` vào `slot`, cài
  cache, đếm `misses/reads/sectors`, `reloads` nếu khoá đã từng có mặt (nạp lại sau trục xuất).
- Hình học **thiết kế** (`const:` trong bằng chứng — không phải ngưỡng nới):
  `PAGE_SIZE=4096`, `SECTOR_SIZE=512`, `SECTORS_PER_PAGE=8`, `FILE0_PAGES=3`, `FILE1_PAGES=2`,
  `CACHE_CAPACITY=3`, `DISK_SECTORS=(3+2)*8=40` sector = 5 Frame 4 KiB.
  Ngưỡng cache **3** đủ chứng minh trục xuất (chạm 5 trang khác nhau) và vừa **bảng mapping
  của object layer (64)** — xem §6, không nới hằng số nào.

## 4. Backend thay thế được — dùng lại `vifast_fs::BlockDev`

Không tạo trait thứ hai: pager dùng đúng trait của FS (`read/write` sector, `BLOCK_SIZE=512`,
`FsError`) — D2-106 "một đường đọc".

| backend | ở đâu | ghi chú |
|---|---|---|
| `RamDev` + `DevRef` | host (`v2/lib/vifast-vfs/src/disk.rs`) | 40 sector mẫu, interior mutability `AtomicU64`, không `unsafe` |
| `MappedDisk` | pager U-mode (`pager_server_user.rs`) | 5 **Frame kernel cấp** map tại `VA_DISK`; pager tự seed 40 sector bằng chính `BlockDev::write` (chứng minh mọi thao tác đi qua trait) |
| virtio-blk | KT-72 | chỉ thêm `impl BlockDev` — **không sửa pager** |

## 5. Bằng chứng đo (QEMU `bash v2/test/boot/ktest.sh 1 1`, TAP 30..34)

Log xanh: `JOURNAL/kt70-green-qemu-ktest-user.log`; đỏ-trước: `JOURNAL/kt70-red-qemu-ktest-user.log`.

| TAP | Dòng bằng chứng (đo từ biến) | Nguồn |
|---|---|---|
| 30 | `# pager: fault kind=Vm va=0x40100000 client=0 access=Read reason=NotMapped core_fault=1 faults=10 ready=1 const:page=4096 const:sector=512` | trap thật + `last_fault` core |
| 31 | `# pager: cache const:capacity=3 hits=2 misses=7 evictions=4 reads=7 sectors=56 served=9 rejected=1 entries=3 unique=3 shared=2 reloads=2 clients=2 magic=0x4b54373050414745` | stats trong Frame |
| 31 | `# pager: map p2_a=0x82034000 p2_b=0x82034000 p2_same=1 hit_delta0=1 mapping_bad=0 walks_ok=9` | walk PTE hai VSpace |
| 32 | `# pager: evict evictions=4 walk_gone=1 pa_changed=1 reloads=2 kernel_data_ok=9 kernel_data_bad=0 a_done=5 b_done=4` | walk + physmap |
| 30/32 | `# pager: data a_ok=5 a_bad=0 a_bytes=20480 a_bad_bytes=0 b_ok=4 b_bad=0 b_bytes=16384 b_bad_bytes=0` | client tự validate từng byte |
| 34 | `# pager: reject client=1 page=99 status=5 name=InvalidArgument reads_before=7 reads_after=7 delta=0 (=reads_after-reads_before) exit=0x80000005` | `vifast_abi::ERRORS` |
| 35 | `# pager: neg aux_status=2 name=WrongObjectType over_status=10 name=AlreadyMapped pa_same=1 (walk=0x8203e000 exp=0x8203e000) magic=0x4b543730434c4942 const:neg_over=10` | mã lỗi **thật** + walk PA |
| — | `# pager: exit pager=0 a=0 b=2147483653 steps=29 starts=3 exits=3` | mã thoát |

Công thức (D2-106) — mọi số dưới đây là **số đo**, không viết tay:

- `sectors = misses × (PAGE_SIZE/SECTOR_SIZE) = 7 × (4096/512) = 7 × 8 = 56`
  (`reads = misses = 7`; mỗi miss đọc đúng một trang).
- `a_bytes = a_ok_pages × PAGE_SIZE = 5 × 4096 = 20480`; `b_bytes = 4 × 4096 = 16384`
  (client đếm byte đã đối chiếu với mẫu `byte_at(file,page,offset)`, không phải suy diễn).
- `p2_same = 1` ⇔ PA walk ở VSpace A **bằng** PA walk ở VSpace B (`0x82034000`), tức **một**
  Frame, hai mapping; `shared = 2` ⇔ hai lần B hit trang đang map cho A (không nạp lại).
- `evictions = 4` với `const:capacity=3`; `reloads = 2` ⇔ A nạp lại **p0** (sau khi B trục xuất)
  và **p2** (sau khi B trục xuất tiếp); `pa_changed = 1` ⇔ PA của p0 lần sau **khác** lần đầu.
- `exits = 3` ⇔ `pager_exit==0` + `a_exit==0` + `b_exit==(0x8000_0000|5)`.
- `kernel_data_ok = 9` ⇔ kernel đọc **độc lập** 64 byte đầu mỗi trang từ Frame qua physmap và
  so mẫu cho **cả 9 lần phục vụ** (5 của A + 4 của B).
- `evictions = misses − min(capacity, unique) = 7 − min(3,3) = 4` — in **công thức + đầu vào
  ngay trên dòng log** (D2-106), không chỉ ở tài liệu.
- `pa_changed` in kèm `first=0x…`/`reload=0x…`; `walk_gone` in kèm `(file,page,va)` đã mất
  mapping; `hit_delta0` in kèm `reads_before/reads_after`; `p2_same` in kèm `(=p2_a==p2_b)`.
- **Ca âm (TAP 35)** — đo **mã lỗi thật**, không đoán trước: `Frame.Map` với aux = **CNode cap**
  của pager (`P_SELF`) ⇒ **`WrongObjectType` (2)** (core kiểm **loại cap phụ** trước object layer; kỳ vọng ban đầu
  `InvalidCapability` đã **sửa theo số đo**, D2-102); `Frame.Map` vào VA **đã map** trong VSpace
  client B ⇒ **`AlreadyMapped` (10)**, và kernel chứng minh PTE khối kết quả **không bị ghi đè**
  (`pa_same=1`, PA walk = PA cấp; `magic` của B còn nguyên) + `b_bad_bytes=0`. Một bộ đếm luôn 0
  mà **không có ca âm** thì không chứng minh gì — đây là bài học ghi ở journal §4.

Kịch bản đo (bảng điều khiển trong `v2/kernel/vifast-kbin-riscv64/init/pager_common.rs`):

- A: `(0,0),(0,1),(0,2)` — làm đầy cache 3 ô.
- B: `(0,2)` **hit** (cùng Frame của A), `(1,0)`, `(1,1)` — miss, trục xuất LRU p0 rồi p2.
- A: `(0,0)` **nạp lại** (PA đổi), `(0,2)` **nạp lại**.
- B: `(0,2)` **hit lần hai** trên bản A vừa nạp.
- B: `(0,99)` — ngoài file ⇒ pager từ chối `InvalidArgument` (5), kernel dừng B với
  `0x8000_0000|5 = 0x80000005`, **không** resume.

## 6. Tài nguyên & ngưỡng (không nới)

Đo cuối pha: `# pager: objmem objects=81 caps=69 frames=63 pt=12 vs=4 maps=61`.
Giới hạn object layer **giữ nguyên**: `MAX_OBJECTS=96`, `MAX_MAPPINGS=64`, arena U0 64 KiB.
Ban đầu hình học lớn hơn (5+2 trang, cache 4, đĩa 7 Frame, VA ngoài block ELF) làm **61..63
mapping** và `Frame.Map` của client B trả `InvalidArgument`; cách xử lý là **giảm hình học
thiết kế** (3+2 trang, cache 3, đĩa 5 Frame, mọi VA trong cùng 2 MiB block với ảnh ELF ⇒ không
cấp thêm bảng trang), **không** sửa `MAX_MAPPINGS`/`MAX_OBJECTS`/ngưỡng nào.

Ảnh `ktest-user`: `.data+.bss = 202.832 B ≤ 262.144`; `size_of::<Machine>() = 188.872 B ≤
196.608` (không thêm field core — static của pager nằm trong crate plat/user).

## 7. Đỏ-trước (D2-05)

- Host: `cargo test -p vifast-vfs` trên mã HEAD cũ **không biên dịch** (`E0432/E0425` — crate
  `vifast-vfs` chưa có) — `JOURNAL/kt70-red-host.log`.
- QEMU: TAP 30..34 **đỏ cả 5** (fault chưa tới pager / mapping sai) —
  `JOURNAL/kt70-red-qemu-ktest-user.log`. Sau đó xanh từng TAP; log xanh ở §5.

## 8. Nợ / giới hạn (chuyển KT-71)

1. **Chưa có journal/writeback**: cache chỉ đọc; trang bẩn, flush, fsync thuộc KT-71.
2. **Chưa có nhiều fault đồng thời** (một hart, scheduler tuần tự); hàng đợi pager xử lý tuần tự
   tối đa 4 message/lần chạy.
3. **Chưa có virtio-blk** — KT-72 chỉ thêm `impl BlockDev`, không sửa pager (hợp đồng đã kiểm
   bằng việc pager chỉ gọi trait).
4. **Chưa có `Frame.Unmap` tự động khi client chết**: pager gỡ mapping khi trục xuất; dọn theo
   tiến trình (Exit) thuộc KT-71.
5. Method id interface dịch vụ vẫn sinh từ IDL, **không** thuộc `spec/abi.toml` (nợ KT-69).

## 9. Chạy lại

```bash
cargo test --manifest-path v2/Cargo.toml -p vifast-vfs -p vifast-idl   # host: 12 + 7 test
bash v2/test/boot/ktest.sh 1 1                                         # 4 ảnh, TAP 30..34
bash v2/test/boot/ktest.sh 1 matrix                                    # 12 boot (smp 1/2/4)
./v2/bin/v2 gate --fast                                                # cổng
```
