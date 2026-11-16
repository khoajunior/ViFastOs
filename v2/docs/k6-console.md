# K6 — virtio-console + UART IRQ → user (KT-73)

Trạng thái: **DONE phần cài + đo** (K6, sau KT-72). Ảnh test riêng **`ktest-con`**; shell U-mode
của `ktest-proc` đọc **byte từ ring UART** (đóng nợ "shell chưa UART IRQ" từ K3, bản trước).

Quy ước tài liệu (D2-112): **mọi số đo nằm trong khối** ```text scope=<tệp-log>``` **và mỗi dòng
là trích nguyên văn** từ đúng tệp đó; ngoài khối chỉ có **định danh** (hex, số hiệu spec dạng
`x.y`, mã task, ngày, tên chip). Không có "haystack chung" để hoán số giữa các ảnh, và script kiểm
(`JOURNAL/kt73-verify-numbers.sh`) không có danh sách số được miễn.

## 1. Phạm vi và nợ được đóng

| Nợ | Trạng thái sau KT-73 |
|---|---|
| "shell chưa UART IRQ" (K3) | **ĐÓNG** — shell đọc byte từ ring do **ngắt RX** của UART sinh ra; không còn message `LINE` do kernel cắm sẵn. "Bàn phím" = **đường RX thật của UART tương thích 16550** (RBR → IER → PLIC → ring); nguồn byte của **ca kiểm** là loopback của thiết bị (§3), **không** phải người gõ phím |
| "driver chỉ ở ảnh test" (D2-111) | **MỞ có điều kiện** — driver console vẫn không vào ảnh sản phẩm; lý do + `file:line` ở §6 |
| "coverage tích hợp không có" (D2-111) | **ĐÓNG một phần** — một ảnh chạy **cùng lúc** UART IRQ + U-mode đọc ring + driver virtio-console + ca âm |
| đòi `used->len == len` cho `tx` (lỗi lặp D2-110B) | **ĐÓNG** — §4.3 |

## 2. UART RX IRQ → ring → U-mode

```text scope=kt73-qemu-ktest-con.log
# con plic: base=0xc000000 ndev=95 hart=0 ctx=1 threshold=0 priority_uart=1 enable_ctx=1 enable_mmode=0 claim_idle=0
# con ring: pa=0x82000000 phys=0xffffffc082000000 va=0x43000000 const:slots=32 const:slot_bytes=32 head=0 tail=0 check=1 const:capacity=768
# con uart: irqs=31 bytes=30 const:payload=30 chunks=30 const:chunks_want=2 head=30 const:head0=0 tail=0 used=30 seq=30 empty_polls=1 empty_bytes=0 drops=0 timeouts=0 inject_ok=1 inject_ticks=64 rx_begin=1 rx_end=1
```

Đường đi: byte tới `RBR` → `IER.RX` → PLIC line của UART → `sie.SEIE`/`sstatus.SIE` →
`trap_dispatch` → `plic::on_external_interrupt` → claim → `IIR` mã nguồn RX ⇒
`uart_console::on_rx_irq` → drain `RBR` (có trần) → `conchan::pack` → `Ring::send` (ring của KT-69,
**không** layout mới) → complete. Kernel **map** Frame ring vào VSpace của chương trình U-mode tại
`VA_CON_RING` (R|W|U, NX); U-mode **không** nhận cap, **không** tự map, **không** lệnh đặc quyền.
Cửa sổ ngắt **hẹp** (`rx_begin`/`rx_end`), `rx_end` **vét cạn** `RBR` sau khi đóng cổng nên byte
đến muộn không mất; handler có trần nên không quay vô hạn.

Chương trình U-mode (**consumer**) đọc ring và in marker; kernel so **nguyên chuỗi** với giá trị
nó tính từ payload đã bơm:

```text scope=kt73-qemu-ktest-con.log
# con u: map=1 exit=0 match=1 head=30 tail=30 used=0 captured=48 prefix="# con: u up.# con: u bytes=30 sum=2363 first=99 "
```

### 2.1 Shell U-mode đọc bàn phím thật (`ktest-proc`)

`proc_demo.rs` bơm **từng dòng** script vào UART **thật** (loopback) trong cửa sổ ngắn; byte quay
lại `RBR` sinh ngắt RX → ring → shell đọc. TAP `shell help echo` chỉ xanh khi **đường**
UART → ring → shell đúng (`ok_uart_path`):

```text scope=kt73-qemu-ktest-proc.log
# proc plic: base=0xc000000 ndev=95 hart=0 ctx=1 threshold=0 priority_uart=1 enable_ctx=1 enable_mmode=0 claim_idle=0 const:uart_irq=10
# proc ring: pa=0x81200000 phys=0xffffffc081200000 va=0x43000000 const:slots=32 const:slot_bytes=32 head=0 tail=0 check=1 mapped_shell=1 const:capacity=768
# con uart: irqs=45 bytes=45 const:script_bytes=45 lines=6 inject_ok=1 drops=0 timeouts=0 head=45 tail=45 used=0 empty_polls=0 const:va=0x43000000 ring_ok=1
ok 3 - shell help echo
```

## 3. Nguồn byte của ca kiểm: loopback 16550

QEMU `virt` không nối bàn phím vào UART khi log đi qua `-nographic`. Để ca kiểm **tất định** mà vẫn
đi **đúng** đường phần cứng, kernel bật **loopback** của UART (bit loopback của `MCR`) rồi ghi byte
vào `THR`: byte quay lại `RBR` **qua thiết bị**, sinh **ngắt RX thật**. Nếu `IER.RX`, PLIC enable,
`sie.SEIE`/`sstatus.SIE`, claim/complete hoặc handler sai thì **không byte nào** vào ring. Byte bơm
**không** ra console (loopback chặn SOUT).
**Giới hạn trung thực**: nguồn byte là **thiết bị UART của QEMU**, không phải bàn phím người; muốn
nguồn người thật chỉ cần nối stdin/chardev khác — driver không đổi.

## 4. virtio-console (device id `0x1003`)

### 4.1 Tái dùng hạ tầng PCI của KT-72 (không nhân đôi)

`vifast-virtio::pci` thêm `VirtioKind{Blk,Console}` + `find_function`; kernel `pci::init_for(kind)`
là **một** đường quét ECAM → đo BAR bằng mask → **kiểm chồng lấn** (BAR function khác +
`RESERVED_RANGES` + leaf page table) → gán BAR → đọc capability. Console gán BAR ở
`0x4020_0000` (leaf thứ hai của cửa sổ PCIe); hằng `PCI_DRIVER_WINDOW_BYTES` mô tả đúng số leaf
driver tự map — lệch ⇒ `BarConflict` fail-closed.

**Vì sao miễn trừ này KHÔNG fail-open**: `mmu_range_conflict` chỉ bỏ qua leaf khi **cả ba** đúng —
leaf nằm trong **đúng** hai leaf driver tự map, là `leaf`, và **identity** (`walk.phys ≤ page` và
`page − walk.phys < leaf_bytes`). Hai lớp kiểm không bị nới và chạy **trước khi ghi BAR**.

```text scope=kt73-qemu-ktest-con.log
# con pci: vendor=0x1af4 device=0x1003 bus=0 dev=1 func=0 bar=4 kind=mem64 size=0x4000 assigned=0x40200000 readback=0x40200000 command=0x6 ecam=0x30008000 other_bars=0 reserved=5 overlap=1 caps_in_bar=1 ecam_id=1 bar_id=1 const:assign=0x40200000
```

### 4.2 Đàm phán feature (D2-110B)

Driver chọn **duy nhất** `VIRTIO_F_VERSION_1` ⇒ **không** `MULTIPORT`/`SIZE` ⇒ theo spec chỉ một
port: `rx` = queue đầu, `tx` = queue thứ hai; **không** gửi lệnh control, **không** đọc
`virtio_console_config`. `queue_index` cho cổng khác cổng đầu trả `None` ⇒ `ConsoleError::MultiPort`
(fail-closed). `queue_size` driver đặt theo `queue_max` **đọc từ thiết bị**; `notify_off` đọc
riêng từng queue.

```text scope=kt73-qemu-ktest-con.log
# con queue: num_queues=64 rx_max=128 rx_size=8 rx_notify_off=0 tx_max=128 tx_size=8 tx_notify_off=1 notify_mul=4 device_features=0x10130000006 driver_features=0x100000000 const:version1=1 multiport=0 status=0xf generations=0/0 rx_buf=0x80253400 tx_buf=0x80253440 rx_desc=0x80253000 tx_desc=0x80253200
```

### 4.3 `used->len` của `tx` — bài học D2-110B lần hai

Spec định nghĩa `used.len` là số byte thiết bị ghi vào buffer **device-writable**; queue `tx`
**không** có buffer đó ⇒ `len` **không** mang nghĩa "đã nhận bao nhiêu byte" (QEMU ghi rỗng). Bản
đầu đòi `len == want` ⇒ TAP đỏ **dù** host nhận đủ byte. Sửa: `tx_used_len_sane(len, want)` chỉ
kiểm `len ≤ want`; **bằng chứng phân phối lấy từ host** (§5.4). Với `rx`, `len` **có** nghĩa và
được kiểm `≤` cỡ đệm.

```text scope=kt73-qemu-ktest-con.log
# con tx: one=Ok(14) two=Ok(10) writes=2 write_bytes=24 const:tx_want=24 notifies=4 last_tx_used_len=0 const:tx_len_note=0..=24 driver_status=0xf
# con rx: got=14 const:want=14 ok=1 busy=0 errors=0 timeouts=0 id_mismatch=0 last_rx_used_len=14 armed=1 head=2/2
```

### 4.4 Ca âm

- **Không có dữ liệu**: `read_try` (chờ không tick) trả `Busy` **ngay**, không treo.
- **Ring đầy** (kênh byte): slot đầy bị **từ chối** với mã `NotEnoughMemory`, **không** ghi đè;
  kernel đọc lại **đúng từng byte** theo thứ tự.

```text scope=kt73-qemu-ktest-con.log
# con neg: busy=1 const:busy_code=14 read_try=Busy ring_slots_ok=32 const:slots_want=32 ring_bytes_ok=768 const:bytes_want=768 drops=2 const:drops_want=2 full_code=9 const:full_want=9 consumed=768 order_ok=1 head=62 tail=62 used=0 timeouts=0
```

## 5. TAP của ảnh `ktest-con`

```text scope=kt73-qemu-ktest-con.log
ok 1 - con uart irq ring
ok 2 - con u-mode doc ring
ok 3 - con pci
ok 4 - con ghi doc
ok 5 - con ca am
PANIC: KT-73 con xong (5 TAP, 0 fail)
```

### 5.1 Ca âm của chính TAP `shell help echo` (mutation, có log)

Bỏ lời gọi `uart_console::inject_loopback` trong `proc_demo.rs` (không bơm byte) rồi boot
`ktest-proc` ⇒ `irqs`/`bytes` bằng không, shell không in gì, TAP đỏ (`kt73-mutation-tap3.log`).
Mutation hoàn nguyên; ảnh rebuild có sha **trùng** bản đóng băng (`kt73-image-sha.log`) ⇒ không có
đường thứ hai đưa dòng lệnh tới shell. Cùng cách, mutation hằng leaf ⇒ `BarConflict` đỏ
(`kt73-mutation-barconflict.log`).

### 5.2 Bộ đếm nào **chưa** có ca âm (D2-108B)

| Bộ đếm | Ca dương | Ca âm | Dùng làm bằng chứng? |
|---|---|---|---|
| `irqs`/`bytes`/`chunks` | ngắt RX thật + byte vào ring | **mutation TAP**: bằng không | **có** |
| `empty_polls` | gọi handler khi FIFO rỗng | — | **có** |
| `drops`/`full_code` | ring đầy | — | **có** |
| `busy` | `read_try` không dữ liệu | — | **có** |
| `timeouts`/`id_mismatch` | không | **không có** | **KHÔNG** — giữ trong `ok` như lưới hồi quy, không claim |

### 5.3 Bơm byte host → `rx` **đồng bộ theo log**

Guest in marker `# con: rx armed` sau khi arm descriptor `rx`; runner thấy dòng đó mới ghi byte vào
FIFO của chardev — không `sleep` mù, không flaky theo tốc độ máy.

### 5.4 Bằng chứng **độc lập phía host**

Runner tạo `chardev pipe` mỗi boot; guest ghi byte qua `tx`; host đọc tệp `.out` **sau khi QEMU
thoát** và coi boot là **FAIL** nếu thiếu (biến `con_host` trong `ktest.sh`):

```text scope=kt73-ktest-matrix.log
# con host: tx_file=/tmp/k3-ktest/con-smp1-boot1.out bytes=24 ok=1
```

Nội dung tệp `.out` (trích trong repo `kt73-con-chardev.out`): `KT73-CON-WRITE0123456789`.

## 6. Ảnh sản phẩm: driver console **không** vào (BLOCKED có `file:line`)

**Không khả thi trong KT-73**, và **không** nới trần:

1. Ảnh sản phẩm **không có chương trình U-mode** — `v2/kernel/vifast-kbin-riscv64/src/main.rs`
   `rust_main` chỉ boot kernel rồi `halt` (`main.rs:360-361`); U-mode nằm sau
   `#[cfg(feature = "user")]` (`v2/kernel/vifast-plat-riscv64/src/lib.rs:100-101`, kênh UART ở
   `lib.rs:95-96`) và driver console sau `#[cfg(feature = "vcon")]` (`lib.rs:110-111`). Đưa driver
   vào ảnh sản phẩm là **code chết không người nhận** (D2-08).
2. Thêm code vào ảnh sản phẩm sẽ đổi `.text`/BUILD_ID đang là neo của soak — chỉ nên làm khi có
   **người nhận thật** (KT-74).
3. Trần **không** phải lý do (§7).

**Hệ quả kiến trúc cho KT-76:** ảnh sản phẩm **không** có U-mode ⇒ **K6 "tương đương v1" sẽ không
có shell trên ảnh sản phẩm** nếu KT-74..75 không đưa U-mode vào ảnh sản phẩm. KT-76 **không thể**
nhận "tương đương v1" chỉ vì `ktest-proc` có shell (shell đó ở **ảnh test**). Điều kiện hợp lệ:
(a) KT-74 đưa U-mode/shell vào ảnh sản phẩm, hoặc (b) KT-76 ghi rõ "tương đương v1 **ở mức tính
năng kernel**, chưa có shell trên ảnh sản phẩm" + `BLOCKED`. Cùng lý do: **build ảnh sản phẩm phải
là lệnh riêng** (không kèm `--features`) — build chung invocation với `--features ktest` làm ảnh
sản phẩm mang code test (đã tự gặp, sha đổi; `JOURNAL/kt73-journal.md` §5).

## 7. Ảnh và trần (không nới)

```text scope=kt73-ktest-matrix.log
# image=ktest path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest file=712968 data+bss=242840 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-guard path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-guard file=54072 data+bss=2680 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-user path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-user file=598064 data+bss=202832 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-proc path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-proc file=435872 data+bss=206736 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-vblk path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-vblk file=132104 data+bss=10976 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-con path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-con file=352240 data+bss=214152 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
```

Ảnh `ktest` không đổi; ảnh sản phẩm: `.text` **trùng** KT-71/72 (số trong khối dưới đây), soak
chạy đúng bản `sha256=545cfb64…` (**cùng byte** với bản này), nhưng **ELF sha256 đổi so KT-72**
(`2e9550bdb6b49483…` → `545cfb64…`) — **không xác định được** phần đổi từ artifact trong repo
(ELF KT-72 không có trong repo; `.rodata`/`.data` nằm ngoài digest `.text`), nên **không suy đoán
nguyên nhân**: chỉ nói sha đổi, `.text` trùng, soak chạy trên **chính** sha đó.

```text scope=kt73-product-digest.log
build=02ed5a1810ba59ce text=49854
```

```text scope=kt73-product-sha.log
545cfb64ff7067e76500b56c90234a5bc33aff7ad793ea6f9a9da127368c2bea  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

```text scope=kt73-soak.log
# image=product path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64 file=188552 data+bss=200184 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# soak: 0/1000 lỗi (PASS)
```

## 8. Cổng (log trong repo)

```text scope=kt73-gate-summary.log
TESTS=726 / SUITES=99
```

```text scope=kt73-conformance.log
ok - trace-compare: 61/61 dòng khớp, 5/5 ca (22:16 23:19 24:7 25:7 26:12), build=3cc6bf36f3ac98b8
```

```text scope=kt73-ktest-matrix.log
# KTEST: 18/18 boot sạch (smp=matrix, NBOOT=1, ảnh/boot=ktest ktest-guard ktest-user ktest-proc ktest-vblk ktest-con)
```

## 9. Chạy lại

```bash
bash v2/test/boot/ktest.sh <NBOOT> matrix          # sáu ảnh, smp khác nhau
bash v2/agent/JOURNAL/kt73-verify-numbers.sh       # kiểm số theo D2-112
bash v2/agent/JOURNAL/kt73-verify-selfbreak.sh     # tự phá năm lớp
```
