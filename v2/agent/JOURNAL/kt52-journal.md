# KT-52 — Object layer `no_std` + Retype nội bộ (K3)

- **Ngày:** 2026-11-15
- **Task:** KT-52 (QUEUE v2) — object layer tối thiểu `no_std`, Retype **nội bộ**, VSpace/PageTable/Frame
  trên Untyped boot; **không** ABI, **không** `spec/`.
- **HEAD vào:** `072a73ea` (KT-51 DONE, mở KT-52) — nhánh `main`, working tree có sửa đổi
  `viscript/` **từ trước** (không chạm; chỉ stage `v2/`).
- **Trạng thái:** **DONE** — commit **`320daa59`** ([KT-52], đã push) + `[KT-52b]` (STATE/log).
  `reviewer-kt52` verdict **DRIFT** (không UNSAFE) — đã vá C1/C2/L1/L2 + L8, L7 chặn map 1 GiB, ghi nợ
  L3..L9; **300/300 boot** + gate **569/73** trên cây cuối trước commit.

## 1. Baseline + đỏ trước

- Baseline `./v2/bin/v2 gate --fast` ⇒ PASS, **569** test / 73 suite, L1 1.000 seed
  (`kt52-gate-baseline.log`).
- **Đỏ trước** (`kt52-red.log`): nâng `expected.txt`/`ktest.sh` lên **37 TAP** (thêm 7 TAP object,
  trap lồng dời 30 → 37) **trước khi** viết mã; chạy `bash v2/test/boot/ktest.sh 1 1` ⇒
  `not ok - ktest smp=1 boot 1: thiếu plan-1..37 ok-set-1..37 ok-count=30/37 ok30-object-vspace
  … ok37-nested`. Ảnh guard vẫn 3/3.
- Sau khi có mã: `kt52-ktest.log` (matrix 1 boot × `smp 1/2/4`) ⇒ **37/37 TAP**, 6/6 boot sạch;
  ảnh guard 3/3. 50 boot × matrix: `kt52-boot50.log` (chạy **sau** khi chốt mã, xem §5).

## 2. Thiết kế đã làm

| Tệp | Nội dung |
|---|---|
| `v2/kernel/vifast-plat-common/src/obj/mod.rs` | kiểu công khai (`ObjKind`, `ObjError`, `MapFlags`, `FlushRequest`, `PhysMem`, `CapId`, stats), `Machine` + Untyped/Retype/Revoke, hằng số `[TK]`, helper hình |
| `…/obj/arena.rs` | arena byte + cửa sổ PA của một Untyped: bump + hole free-list, **đệm căn lề là hole** (không có `gap` tự khai), `pa` watermark + PA hole |
| `…/obj/vspace.rs` | `map`/`unmap`/`protect`/`walk` + cấp bảng trung gian, rollback atomic, walk PTE |
| `…/obj/check.rs` | `check_objmem` (tương đương `check_vm` K2, `no_std`) + truy vấn/thống kê + drain `sfence` |
| `…/obj/tests/obj.rs` (host) | 10 test: Retype/Revoke/byte/PA/Frame 3 cỡ/map-unmap-protect/W^X 2 ca/check_objmem/atomic |
| `v2/kernel/vifast-plat-riscv64/src/obj.rs` | `PhysmapMem` (physmap), `init` 2 Untyped boot, `copy_kernel_root`, `switch_root`/`restore_kernel_root`, đọc/ghi VA user qua `sstatus.SUM` (unsafe trong `kernel/vifast-plat-*`, D2-17) |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_obj.rs` | TAP 30..36 + dòng bằng chứng `# obj init:`/`# retype:`/`# untyped bytes:`/`# frame:`/`# wx:`/`# map:`/`# leak:` |
| `ktest.rs`/`main.rs`/2 `lib.rs` | plan `1..37`, gọi object phase, obj init ở **cả** ảnh sản phẩm, `# obj:` product; trap lồng 30→37 |
| `expected.txt`/`ktest.sh`/`k3-riscv64.md` | 37 TAP + grep bằng chứng + §15 (bảng §2/§4 cập nhật) |

### 2.1 Untyped boot + byte (D2-66)

- Arena object = `align_up(mmu::arena_range().1, 4 KiB)`, **64 KiB** `[TK]` RAM sau `__kernel_end`
  (không section ELF ⇒ không vào budget). 2 Untyped:
  - `U0`: arena 64 KiB, cửa sổ PA `0x8200_0000` (căn 2 MiB) — PageTable/VSpace + Frame 4 KiB/2 MiB.
  - `U1`: arena 4 KiB, cửa sổ PA `0x8000_0000` (căn 1 GiB) — Frame 1 GiB (**nhãn**, D2-66).
- `PageTable`/root VSpace tiêu 4 KiB arena; `VSpace` metadata + `Frame` tiêu **0 byte**; `Frame.pa`
  là nhãn trong cửa sổ 1 GiB, cấp theo watermark + PA hole, không chồng **toàn cục**.
- **Bất biến byte (đã siết theo review lead):** byte đệm căn lề được ghi thành **hole** ngay khi
  cấp ⇒ `check_objmem` kiểm `Σ live + Σ hole == watermark` (không còn `gap` tự khai); mỗi live
  block/hole nằm trong `[0, watermark)`, pairwise không chồng, cộng lại **phủ kín** watermark.
  TAP 31 in `used0/after_retype/after_revoke/free0/free1` + `invariant=1 atomic=1 oom=1`.
- `Retype`/`Revoke` đều **atomic** kể cả `OutOfMemory` (TAP 31 cấp tới cạn arena, lần lỗi
  `NotEnoughMemory` không đổi `used/free`; revoke trả đúng mốc).

### 2.2 Frame 3 cỡ

- `size_bits` đọc từ `vifast_pte::FrameSize` (12/21/30); cỡ khác ⇒ `InvalidArgument`. Cấp **giảm
  dần** (không đệm căn lề ngầm; lệch ⇒ `Alignment`); chồng PA ⇒ `AlreadyMapped`.
- TAP 32: 2 MiB tại `pa` (căn 2 MiB) rồi 4 KiB; **ghi/đọc 8 B thật** tại `pa` qua physmap;
  `size_bits=13` atomic; Frame 1 GiB từ `U1` kiểm `pa` căn 1 GiB + cửa sổ (không chạm bộ nhớ).

### 2.3 VSpace + PTE + `sfence.vma` (KT-51)

- PageTable 4 KiB = 512 PTE × 8 B; hình/`walk_shape`/`PAGE_SIZE_BITS`/`PTE_LEVELS` từ `vifast-pte`;
  cờ PTE từ `vifast-plat-common::mmu` (không chép bit).
- `map` kiểm trước (atomic) → cấp slot/bảng còn thiếu → nối PTE → commit mapping/đếm Frame → đẩy
  `FlushRequest::Page`; lỗi giữa chừng rollback PTE + block + object slot. `unmap`/`protect` tương
  tự; `protect` kiểm W^X trên **phần còn lại** của Frame.
- Arch chạy `sfence.vma` thật (`run_flush`: `(va,asid)`/`(asid)`/`(*)`).
- **Phần cứng thật (TAP 35):** chép 512 root entry kernel vào root VSpace, `csrw satp` + `sfence`,
  map 4 KiB `U|R|W` ở `0x4000_0000`, S-mode ghi/đọc **qua VA user** bằng `sstatus.SUM`, `protect`
  sang `R|NX` (walk: PTE mất W), `unmap` + `sfence` (walk invalid), rồi khôi phục `satp` kernel
  **vô điều kiện** (kể cả nhánh lỗi; không in/log trong cửa sổ đã đổi `satp`), `kernel=1`.
- **W^X toàn cục (D2-67):** mỗi Frame có `maps/w/x`; map/protect `W` khi `mapped_x>0` hoặc ngược lại
  ⇒ `WxViolation` atomic; `Device+EXEC` ⇒ `WxViolation` (Sv39 không có bit Device — cờ chính sách).
  TAP 33/34 in `# wx:`; `check_objmem` recount từ mapping sống.

### 2.4 `check_objmem` (no_std)

Byte/arena (phủ kín, không gap), PA/Frame (căn cỡ, cửa sổ, không chồng, có cap sống = không mồ
côi), VSpace root/bảng nội bộ đúng chủ, mapping đi lại bằng PTE (`walk` khớp `phys/level/flags`),
W^X recount, cap trỏ object sống/đúng key. Trả `ObjStats`; chạy trong `init` **cả hai** ảnh và sau
mỗi pha TAP.

## 3. Vì sao object layer ở `plat-common` (không `unsafe`) + arch

- Logic object (dữ liệu, PTE, W^X, byte) **không** cần asm/MMIO ⇒ `vifast-plat-common::obj`
  `#![forbid(unsafe_code)]`, chạy host (test + Miri-able), đúng D2-72 Q12 và D2-17.
- Chỉ truy cập bộ nhớ vật lý/`satp`/`sfence`/`SUM` ở `vifast-plat-riscv64::obj` (danh sách đóng
  `kernel/vifast-plat-*`); R5 của cổng cấu trúc vẫn xanh.

## 4. TAP đã thêm (ảnh `vifast-kbin-ktest`, plan 1..37)

| TAP | Tên | Kiểm |
|---|---|---|
| 30 | `retype vspace` | 1 VSpace + 3 PageTable + 1 Frame; root 4 KiB, ASID nhãn, cap/key; walk rỗng; `check_objmem` |
| 31 | `untyped byte invariant` | `used/free` qua Retype/Revoke đúng mốc; `size_bits` lạ atomic; **OutOfMemory atomic** |
| 32 | `frame 3 sizes` | 4 KiB/2 MiB **thật** (ghi/đọc tại `pa`), 1 GiB nhãn/căn cửa sổ, `bad13` atomic |
| 33 | `wx violation x-then-w` | map `R|X` rồi `R|W` ⇒ `WxViolation`, không PTE/đếm mới |
| 34 | `wx violation device+exec` | `Device+EXEC` ⇒ `WxViolation`, Frame 0 mapping |
| 35 | `map/unmap 4 KiB` | satp+sfence+SUM thật, protect mất W, unmap invalid, khôi phục kernel |
| 36 | `0 ro` | `used=0`, `free=capacity`, `holes=0`, 0 Frame/PT/VSpace/mapping, `orphans=0` |
| 37 | `trap nested panic` | (dời từ 30 — không đổi nội dung) |

`ktest.sh` grep **đúng** các dòng bằng chứng và 7 dòng `ok 30..36`; `expected.txt` = `ktest 37 37`.

## 5. Bằng chứng (log thật trong `v2/agent/JOURNAL/`)

| Log | Nội dung |
|---|---|
| `kt52-gate-baseline.log` | `gate --fast` baseline PASS 569 test / 73 suite |
| `kt52-red.log` | **đỏ trước**: runner đòi plan 37, ảnh còn 30 TAP |
| `kt52-host.log` | `cargo test -p vifast-plat-common --test obj` ⇒ **13/13 PASS** (thêm L1/L2/L7) |
| `kt52-ktest.log` | matrix 1 boot (cây cuối): 6/6 boot sạch, **37/37 TAP** mỗi ảnh chính, guard 3/3 |
| `kt52-product.log` | ảnh sản phẩm `run.sh 2 2` ⇒ 2/2 sạch, `# obj:` in ra, `ktest-strings=0` |
| `kt52-budget.log` | `budget.sh` 3 ảnh (sản phẩm/ktest/guard) ≤ 262.144 B |
| `kt52-boot50.log` + `kt52-boot50-smp{1,2,4}.log` | **300/300 boot** (mỗi smp 100/100 sạch, 0 `not ok`; 3 luồng song song, ≤3 QEMU) trên **cây cuối** sau vá review |
| `kt52-gate.log` | `gate --fast` **trên cây cuối** — PASS **569 test/73 suite** (C1 review) |
| `kt52-image-sha.log` | sha256 3 ảnh cuối |
| `kt52-host.log` | host test **13/13** sau vá (thêm L1/L2/L7) |

### 5.1 Vì sao `# leak:` không còn cờ tự khai (C2)

`# leak: cap=… used=0 free=… holes=0 frames=0 pts=0 vspaces=0 maps=0 orphans=0 check=1`:
`used/holes` recompute từ bảng object/hole; `check=1` **chỉ** khi `check_objmem()` trả `Ok` (không có
`gap`/`bytes_ok/wx_ok/walk_ok` tự khai). Bất biến `Σ live + Σ hole == watermark` + `holes=0` ⇒
`watermark=0` ⇒ không byte nào bị giữ; `free=capacity` là hệ quả, không phải nguồn.

## 6. Nợ ghi rõ (không giấu; `k3-riscv64.md` §15.10)

1. Frame 1 GiB là **nhãn** trên `-m 128M` (D2-66); 4 KiB/2 MiB mới ghi/đọc thật.
2. Metadata object/cap trong `.bss`; Untyped hiện cấp **dữ liệu** object (PTE) + PA nhãn Frame —
   K3b KT-58 đưa header vào Untyped.
3. Chưa có child Untyped (`Retype(Untyped) ⇒ NotSupported`), CNode/MDB/IPC/sched (K3b).
4. Object op **single-hart** ở KT-52 (BSP), chưa có khoá — K3b.
5. Không thu hồi bảng trung gian rỗng sau `unmap` (không cần cho hình walk đúng).
6. `Device` không có bit PTE (Sv39) — cờ chính sách; fault W^X thành `FaultKind` là KT-56.

## 7. Review độc lập (`reviewer-kt52`, lead spawn) — DRIFT (không UNSAFE)

Verdict: lõi đúng (byte đi lại bảng thật, Frame 0 byte arena, W^X atomic + protect, map/unmap 4 KiB
phần cứng, 300 boot 100/100 ×3, host 10/10, `spec/` rỗng). **Đã vá trước commit**:

| Mục | Vấn đề | Vá |
|---|---|---|
| **C1** (chặn) | `kt52-gate.log` được trích **chưa tồn tại** (chỉ có baseline trước mã cuối) | chạy `gate --fast` trên cây cuối, chép log thật vào `JOURNAL/kt52-gate.log` (sau vá + 50 boot) |
| **C2** (chặn) | `# leak:` in hằng số tự khai `gap=0` + 3 cờ `bytes_ok/wx_ok/walk_ok` khởi tạo `true` không bao giờ `false` | bỏ `gap=0` và **bỏ 3 cờ** khỏi `ObjStats`; `# leak: … check=1` chỉ khi `check_objmem` trả `Ok`; sửa `ktest.sh` + §15.7/§15.8 |
| **L1** (DoD mồ côi) | `revoke(Untyped)` giải phóng Frame còn mapping (map chéo Untyped) ⇒ mapping trỏ object chết | pre-check **trước mọi mutation**: Frame còn mapping từ VSpace **khác** Untyped ⇒ `HasChildren`; test host `revoke_untyped_tu_choi_frame_map_cheo_untyped` |
| **L2** (DoD rò) | `sfence` rơi im lặng khi hàng đợi đầy (`unmap/protect/teardown`) | `push_flush -> bool` + `ensure_flush_slots` **trước mutation** ở `map/unmap/protect/revoke`; test host `hang_doi_flush_day_bao_loi_atomic` |
| **L8** | `report_pa1` hardcode + "kiểm tràn" vô nghĩa | TAP 32 dùng hằng số arch `aobj::OBJ_PA_BASE_U1` + kiểm căn 1 GiB |
| **L7** | cửa sổ PA `0x8000_0000` trùm RAM thật | `map` Frame 1 GiB ⇒ `NotSupported` tới K4/K5; test host `frame_1g_khong_duoc_map_khi_chua_co_backing`; §15.4/§15.10 |

**Ghi nợ rõ trong §15.10** (không vá): L3 `mapped_device` toàn cục (KT-56 làm rõ D2), L4 hai nguồn
ASID, L5 panic không khôi phục `satp` (fatal halt), L6 `copy_kernel_root` test-only chép PTE không
tracked + `map` không giới hạn dải VA (K3b), L9 nhánh "đệm thành hole" chưa có test (mọi cấp phát
KT-52 đều 4 KiB).

## 8. Kết quả cuối (sau vá review, trước commit)

- Host `kt52-host.log`: **13/13** PASS.
- ktest 1 boot × `smp 1/2/4` (`kt52-ktest.log`): **37/37 TAP + guard 3/3**, 6/6 boot sạch.
- **300/300 boot** (`kt52-boot50*.log`): mỗi `smp` 100/100 boot sạch, 0 `not ok`.
- Product `run.sh 2 2` (`kt52-product.log`): 2/2 sạch, `ktest-strings=0`, `# obj: healthy=1`.
- Budget (`kt52-budget.log`): product/ktest/guard đều ≤ 262.144 B.
- Gate `kt52-gate.log`: **PASS 569 test/73 suite**, L1 1.000 seed.
- Dòng `# leak` cuối: `cap=… used=0 free=… holes=0 frames=0 pts=0 vspaces=0 maps=0 orphans=0 check=1`
  (không cờ tự khai; `check=1` ⇔ `check_objmem() == Ok`).

## 9. Bước tiếp

- Chờ verdict reviewer độc lập (lead spawn), vá nếu DRIFT (chặn commit), rồi commit
  `[KT-52]` + `[KT-52b]` (chỉ `v2/`), push, báo hash + TAP + bất biến byte.
- KT-53 dùng lại: `obj::machine()` + `Retype(VSpace)` + `map` (đã có flush qua KT-51); cần thêm
  nạp ELF + trapframe U-mode. **Chú ý**: `map` Frame 1 GiB đang `NotSupported` (L7); VSpace user
  phải dựng root riêng (L6), ASID hợp nhất (L4).
