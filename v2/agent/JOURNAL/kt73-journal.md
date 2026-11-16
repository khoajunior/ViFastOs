# KT-73 — journal (K6: virtio-console + UART IRQ → user)

Quy ước (D2-112): **mọi số đo nằm trong khối trích nguyên văn** (````text scope=<tệp-log>`), mỗi dòng đúng
một tệp; ngoài khối chỉ có định danh (hex, số hiệu spec `x.y`, mã task, ngày, tên chip). Nhờ vậy
script kiểm không có "haystack chung" và không có danh sách số được miễn.

## 1. Quyết định

- **Dùng ring KT-69 nguyên trạng** (`vifast-ring`: magic/header/slot/`head`/`tail`, mã `Full`/
  `Busy`); byte được đóng gói vào **slot bốn word** bằng codec mới `vifast-virtio::conchan` —
  **không** đổi layout ring.
- **Tái dùng PCI/virtio của KT-72**: thêm `VirtioKind{Blk,Console}` vào `vifast-virtio::pci`,
  `pci::init_for(kind)` trong kernel — **một** đường ECAM/BAR/capability, không copy.
- **Feature negotiation (D2-110B)**: chỉ `VIRTIO_F_VERSION_1`; không `MULTIPORT`/`SIZE` ⇒ một
  port, `rx` = queue đầu, `tx` = queue thứ hai; cổng khác cổng đầu ⇒ `MultiPort` fail-closed.
- **Ca kiểm UART**: bơm byte qua **loopback 16550** (đường ngắt/thanh ghi thật), không phải bàn
  phím người — khai rõ ở `docs/k6-console.md` §3.
- **Ca kiểm console cần host**: `virtio-serial-pci` + `chardev pipe` + `virtconsole`, bơm byte
  **đồng bộ theo marker trong log** (không `sleep` mù).

## 2. Red-before (host) và các lỗi **thật** đã bắt

1. Red-before trên cây HEAD: test logic mới ⇒ `unresolved import ... VirtioKind` (EXIT khác không) ghi ở
   `kt73-red-host.log`; sau khi cài: host unit + logic PASS (`kt73-green-host.log`).
2. `BarConflict stage=mmu`: miễn trừ leaf PCIe ban đầu chỉ phủ leaf đầu ⇒ console báo xung đột
   trước khi chạy driver. Sửa: `PCI_DRIVER_WINDOW_BYTES` mô tả **đúng** số leaf driver tự map;
   vẫn fail-closed, hai lớp kiểm chồng lấn không bị nới. **Log**: `kt73-mutation-barconflict.log`
   (mutation: hằng leaf = một leaf ⇒ `# con pci: FAIL error=BarConflict stage=mmu`, TAP đỏ; hoàn
   nguyên ⇒ sha ảnh trùng `kt73-image-sha.log`).
3. **`used->len` của `tx` (D2-110B lần hai)**: driver đòi `len == want` ⇒ TAP đỏ **dù** host nhận
   đủ byte; QEMU ghi **rỗng** vì `tx` **không** có buffer device-writable. Sửa: `tx_used_len_sane`
   chỉ kiểm `len ≤ want`; bằng chứng phân phối lấy từ **tệp host** (§4).
4. Kỳ vọng số slot sai: loopback sinh **một ngắt mỗi byte** ⇒ invariant đổi thành
   `head ≥ ceil(bytes / chunk) && head == chunks`.
5. `-device virtserialport` cho `Guest_connected=false` (`tx` len rỗng, rx không tới); dùng
   `virtconsole` trên bus ảo của thiết bị + `chardev` pipe thì cả rx và tx chạy.

## 3. Tệp chính

| Nhóm | Tệp |
|---|---|
| Kernel driver | `kernel/vifast-plat-riscv64/src/uart_console.rs`, `virtio_console.rs`, `plic.rs`, `pci.rs`, `mmu.rs` |
| Logic thuần | `lib/vifast-virtio/src/console.rs`, `conchan.rs`, `pci.rs` (+ test `tests/logic.rs`) |
| Ảnh test | `kernel/vifast-kbin-riscv64/src/ktest_con.rs`, `init/con_client_user.rs`, `build.rs`, `Cargo.toml` |
| Shell/proc | `kernel/vifast-kbin-riscv64/init/shell_user.rs`, `src/proc_demo.rs`, `src/ktest_proc.rs` |
| Harness | `test/boot/ktest.sh`, `test/boot/expected.txt` |
| Docs | `docs/k6-console.md`, `docs/k3b-core.md` §21, `agent/STATE.md`, `agent/BLOCKED.md` |

## 4. Đo

```text scope=kt73-qemu-ktest-con.log
# con uart: irqs=31 bytes=30 const:payload=30 chunks=30 const:chunks_want=2 head=30 const:head0=0 tail=0 used=30 seq=30 empty_polls=1 empty_bytes=0 drops=0 timeouts=0 inject_ok=1 inject_ticks=64 rx_begin=1 rx_end=1
# con u: map=1 exit=0 match=1 head=30 tail=30 used=0 captured=48 prefix="# con: u up.# con: u bytes=30 sum=2363 first=99 "
# con pci: vendor=0x1af4 device=0x1003 bus=0 dev=1 func=0 bar=4 kind=mem64 size=0x4000 assigned=0x40200000 readback=0x40200000 command=0x6 ecam=0x30008000 other_bars=0 reserved=5 overlap=1 caps_in_bar=1 ecam_id=1 bar_id=1 const:assign=0x40200000
# con tx: one=Ok(14) two=Ok(10) writes=2 write_bytes=24 const:tx_want=24 notifies=4 last_tx_used_len=0 const:tx_len_note=0..=24 driver_status=0xf
# con rx: got=14 const:want=14 ok=1 busy=0 errors=0 timeouts=0 id_mismatch=0 last_rx_used_len=14 armed=1 head=2/2
# con neg: busy=1 const:busy_code=14 read_try=Busy ring_slots_ok=32 const:slots_want=32 ring_bytes_ok=768 const:bytes_want=768 drops=2 const:drops_want=2 full_code=9 const:full_want=9 consumed=768 order_ok=1 head=62 tail=62 used=0 timeouts=0
PANIC: KT-73 con xong (5 TAP, 0 fail)
```

```text scope=kt73-qemu-ktest-proc.log
# con uart: irqs=45 bytes=45 const:script_bytes=45 lines=6 inject_ok=1 drops=0 timeouts=0 head=45 tail=45 used=0 empty_polls=0 const:va=0x43000000 ring_ok=1
ok 3 - shell help echo
```

```text scope=kt73-ktest-matrix.log
# image=ktest path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest file=712968 data+bss=242840 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-guard path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-guard file=54072 data+bss=2680 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-user path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-user file=598064 data+bss=202832 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-proc path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-proc file=435872 data+bss=206736 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-vblk path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-vblk file=132104 data+bss=10976 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# image=ktest-con path=/Users/mps/dvchd/ViFastOs/v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-ktest-con file=352240 data+bss=214152 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# KTEST: 18/18 boot sạch (smp=matrix, NBOOT=1, ảnh/boot=ktest ktest-guard ktest-user ktest-proc ktest-vblk ktest-con)
# con host: tx_file=/tmp/k3-ktest/con-smp1-boot1.out bytes=24 ok=1
```

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

```text scope=kt73-gate-summary.log
TESTS=726 / SUITES=99
```

```text scope=kt73-conformance.log
ok - trace-compare: 61/61 dòng khớp, 5/5 ca (22:16 23:19 24:7 25:7 26:12), build=3cc6bf36f3ac98b8
```

Ghi chú câu sha (D2-112(E)): ảnh sản phẩm có `.text` **trùng** KT-71/72 và soak chạy trên **chính**
`sha256=545cfb64…`; **ELF sha256 đổi so KT-72** (`2e9550bdb6b49483…`) nhưng **không xác định được**
phần đổi từ artifact trong repo (ELF KT-72 không có trong repo; `.rodata`/`.data` ngoài digest
`.text`) ⇒ **không suy đoán nguyên nhân**.

## 5. Nợ và BLOCKED

1. **Console trong ảnh sản phẩm: BLOCKED** — ảnh sản phẩm không có U-mode/shell
   (`main.rs:360-361`, `lib.rs:95-96`, `lib.rs:100-101`, `lib.rs:110-111`); trần không phải lý do.
   **Hệ quả kiến trúc cho KT-76**: K6 "tương đương v1" sẽ **không** có shell trên ảnh sản phẩm nếu
   KT-74..75 không đưa U-mode vào ảnh sản phẩm.
2. Nguồn byte ca kiểm là loopback 16550 (đường ngắt thật, không phải người gõ).
3. Console **poll** chưa dùng ngắt; chưa có ca **IRQ storm** thật của UART.
4. Coverage tích hợp mới **một** ảnh.
5. `timeouts`/`id_mismatch` chưa có ca âm ⇒ không dùng làm bằng chứng (`k6-console.md` §5.2).
6. **Ảnh sản phẩm phải build lệnh riêng** (không kèm `--features`): build chung invocation với
   `--features ktest` làm ảnh sản phẩm mang code test — đã tự gặp (sha sai), phát hiện bằng so
   sha, build lại sạch. Cùng lớp bẫy D2-108A.
7. **Bài học verify-script (lỗi tầng hai)**: log của lần break/mutation nằm trong nguồn ⇒ giá trị
   "cố tình sai" thành nguồn hợp lệ ⇒ PASS giả. Bản D2-112 **loại** `*red*`, `*mutation*`,
   `*verify*`, `*break*` khỏi nguồn; mỗi khẳng định có **phạm vi tệp riêng**; tự phá năm đường
   (§7).

## 6. Vì sao tách ảnh `ktest-con`

Ảnh `ktest` đã sát trần từ KT-72 (số nguyên văn trong khối ảnh ở §4); thêm driver console +
chương trình U-mode vào cùng ảnh sẽ vượt trần (cấm nới). Tách ảnh riêng giữ nguyên `ktest` ⇒ các
boot cũ và conformance không đổi; ảnh mới còn dư trần (khối ở §4).

## 7. Kiểm số (D2-112) + tự phá

```bash
bash v2/agent/JOURNAL/kt73-verify-numbers.sh   # PASS/FAIL
bash v2/agent/JOURNAL/kt73-verify-selfbreak.sh # phá năm đường, ghi log
```

Bản D2-112 thay bản cũ (bị reviewer phá bốn đường: hoán số giữa ảnh, ngoại lệ chứa chữ số, log
mutation là nguồn, STATE chỉ quét khối đầu). Bản mới: **mỗi dòng số đo phải trích nguyên văn từ
đúng một tệp log**; ngoài khối chỉ định danh theo lớp; nguồn loại log đỏ/mutation/verify/break;
STATE quét toàn bộ trừ mục lịch sử của task khác.

## 8. Lệnh và log

```bash
bash v2/test/boot/ktest.sh <NBOOT> matrix
bash v2/agent/JOURNAL/kt73-verify-numbers.sh
```

Log: `kt73-red-host`/`kt73-green-host`, `kt73-qemu-ktest-con|proc|vblk`, `kt73-ktest-matrix`,
`kt73-con-chardev.out`, `kt73-product-*`, `kt73-budget-product`,
`kt73-gate`/`kt73-gate-summary`/`kt73-check`/`kt73-clippy-kernel`/`kt73-soak`/`kt73-conformance`/
`kt73-sim-trace`/`kt73-l1-thuan`, `kt73-mutation-tap3`, `kt73-verify-numbers`,
`kt73-verify-selfbreak`, `kt73-image-sha`.
