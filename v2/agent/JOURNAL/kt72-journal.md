# KT-72 — K6: virtio-blk **thật** trên QEMU, **chỉ trong ảnh test `ktest-vblk`**
(PCI ECAM → virtqueue → `BlockDev` → FS trên đĩa; ảnh sản phẩm **không** có driver)

Ngày: 2026-11-16/17 (phiên Agent 2 mới). HEAD vào: **`f2edba6a`** (`[KT-00] D2-109 … mở KT-72`).
Task trong QUEUE: `KT-72 | K6 virtio-blk thật (QEMU) | KT-71 | 3 h | READY | đọc/ghi sector qua
virtqueue; TAP`.

## 1. Vào — baseline (D2-105: cây `v2/` không bị tab khác ghi trong phiên này)

```
$ ./v2/bin/v2 gate --fast          # kt72-baseline-gate.log
v2 gate --fast: PASS (gen-check ON từ KT-05; L1 sim 1000 seed)
```
Baseline = **703 test / 96 suite** (số của KT-71 — đếm lại bằng cùng công thức:
`TESTS=703` / `SUITES=96`, `kt72-baseline-summary.log`); sau task = **718 / 99**
(`kt72-gate-summary.log`, công thức đếm ghi trong tệp). Cây sạch trước khi bắt đầu ngoài các tệp
`viscript/`, `review/`, `.agents/` của tab khác (không chạm).

## 2. Đỏ trước (D2-05)

### 2.1 Host — test mới trên cây chưa có code

`cargo test -p vifast-virtio` ⇒ **không biên dịch** (module chưa có):

```
error[E0432]: unresolved import `vifast_virtio::features`
error[E0432]: unresolved import `vifast_virtio::pci`
error[E0432]: unresolved import `vifast_virtio::queue`
error: could not compile `vifast-virtio` (test "logic") due to 4 previous errors
```

Log: `kt72-red-host.log`.

### 2.2 QEMU — hai lỗi thiết kế **tự bắt** bằng boot thật

1. Bản đầu từ chối `next` lùi trong capability list (`next <= off ⇒ CapLoop`) — sai vì PCI không
   bắt buộc offset tăng dần. Boot in `# vblk pci: err=CapLoop`, TAP 1 đỏ.
2. Sau khi sửa, đọc lại BAR **raw** (chưa che 4 bit thuộc tính) ⇒ `assigned=0x40000000` nhưng
   `readback` lệch `assigned` vì còn 4 bit thuộc tính; boot in
   `# vblk pci: err=BarUnassigned stage=readback`.

Log: `kt72-red-cap-loop.log` (3 mốc: CapLoop → BarUnassigned → xanh TAP 1).

### 2.3 Mutation (chứng minh test không tautology)

- **Host**: đổi `avail_ring_off` lệch 2 byte ⇒ `layout_queue_dung_chuan` + `avail_va_used_ring`
  đỏ (`left: 130 / right: 132`), `kt72-mutation-host.log`.
- **QEMU**: bỏ kiểm dải trong `read_sector` ⇒ TAP 6 **`not ok 6 - vblk loi ngoai dai`**
  (`refused=Io refused_no_request=0`), `kt72-mutation-qemu.log`; sau đó revert + build lại.

## 3. Cài đặt (tệp + vì sao)

| Tệp | Nội dung |
|---|---|
| `v2/lib/vifast-virtio/` (**mới**, `no_std`, `forbid(unsafe_code)`) | logic thuần + 15 test host: `bar_probe` (mask→cỡ/kiểu, đúng **độ rộng** 32/64 bit), capability list (`CapLoop`/`CapMissing`, `notify_off_multiplier`), `PciCfg` trait + `write16` đọc-sửa-ghi, `VqLayout`/`SplitQueue` (byte layout, avail/used, `write_used` cho test), `ReqHeader`, `decode_config`, mã status, feature bit 32 |
| `v2/kernel/vifast-plat-riscv64/src/pci.rs` (**mới**, `cfg(feature="vblk")`) | ECAM `0x3000_0000` (map 2 MiB), quét bus 0, đo BAR bằng mask, **gán** BAR (firmware không enumerate), `command.MEMORY\|BUS_MASTER`, parse caps; lỗi `PciFail{error,stage}` |
| `v2/kernel/vifast-plat-riscv64/src/virtio_blk.rs` (**mới**) | trang DMA 4 KiB căn lề trong `.bss`, common-config, feature `VIRTIO_F_VERSION_1` duy nhất, queue 8, `submit` (fence → notify → poll used → kiểm `used->id`), `read/write/flush`, `read_sector_force` cho ca âm, bộ đếm + `stats()` |
| `v2/kernel/vifast-plat-riscv64/src/mmu.rs` | `cfg(feature="vblk")`: map ECAM + cửa sổ MMIO PCIe (2 MiB mỗi cái); **không** đổi mapping ảnh khác |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_vblk.rs` (**mới**) | ảnh `ktest-vblk`: 6 TAP + evidence `# vblk:`, `impl BlockDev` cho virtio, pha FS format/mount/ghi/remount/fsck + đọc thô LBA 0/1 |
| `v2/kernel/*/Cargo.toml`, `v2/Cargo.toml` | crate mới vào workspace host; feature `vblk = ["ktest"]` (chỉ ảnh test); bin mới `required-features = ["ktest","vblk"]` |
| `v2/test/boot/expected.txt`, `v2/test/boot/ktest.sh` | nhãn thứ **năm** `ktest-vblk 6 6`; manifest/evidence/PANIC/`-drive` (ảnh đĩa 4 MiB **mới mỗi boot**), build tách `--features ktest,vblk`; matrix tự thành 5×3 |
| `v2/docs/k6-virtio-blk.md` (**mới**), `v2/docs/k3b-core.md` §20 | thiết kế + số đo + nợ |

**Vì sao ảnh riêng** (task cho phép tách khi chạm trần): ảnh `ktest` đã **242840** trên trần
**262144** (D2-109); driver + trang DMA đẩy sát trần, và ảnh này cần `-drive` ⇒ tách
`ktest-vblk` (10976 B) để **12 boot matrix cũ giữ nguyên dòng QEMU**, ảnh sản phẩm **không có**
driver (KT-73 mở).

**Bài học đo được (xung đột VA THẬT)**: driver map cửa sổ PCIe 2 MiB tại **`0x4000_0000`**, đúng
VA mà TAP 35 ([`ktest_obj.rs:31`](../kernel/vifast-kbin-riscv64/src/ktest_obj.rs)) dùng để map thử
4 KiB trên bản sao root của kernel — nếu driver nằm trong ảnh `ktest` thì TAP 35 đỏ
(`AlreadyMapped`). Vì vậy driver nằm sau feature `vblk` và chỉ có trong ảnh `ktest-vblk`;
**không** phải "leaf khác" như một bình luận cũ nói (đã sửa ở `Cargo.toml`).

## 4. Bằng chứng đo (QEMU thật)

Log xanh: `kt72-green-qemu-ktest.log` (boot `ktest-vblk`, smp 1; matrix chạy thêm smp 2/4).

```
# vblk pci: functions=2 bus=0 dev=1 func=0 vendor=0x1af4 device=0x1001 bar=4 kind=mem64 size=16384 assigned=0x40000000 readback=0x40000000 command=0x6
# vblk cap: common=0x0 notify=0x3000 device=0x2000 isr=0x1000 mul=4 cap_bar=4 ecam=0x30008000 ecam_identity=1 bar_identity=1 cap_ranges_ok=1 overlap_checked=1 other_bars=0 reserved_ranges=5
# vblk dev: capacity=8192 blk_size=512 size_max=0 seg_max=254 num_queues=1 queue_max=256 queue_size=8 device_features=0x10130006e54 driver_features=0x100000200 flush_feature=1 status=0xf notify_off=0 notify_addr=0x40003000 config_generation=0
# vblk dma: desc=0x8020e000 avail=0x8020e080 used=0x8020e098 req=0x8020e200 data=0x8020e220 status_pa=0x8020e420 identity=1 version1=1 status_readback=0xf
ok 1 - vblk pci
ok 2 - vblk queue
# vblk io: sectors=8 const:passes=2 writes=16 reads=16 compared=8192 mismatched=0 first_diff=4294967295 requests=32 completions=32 const:first_lba=6000
ok 3 - vblk ghi doc lai
# vblk flush: calls=2 flushes=2 completions=2 device_status=0xf driver_ok=1 errors=0 timeouts=0
ok 4 - vblk flush
# vblk fs: phase=write formatted=1 wrote=69 fsck_clean=1 found=0 fixed=0 unfixed=0 const:payload_len=69
# vblk fs: phase=remount read_len=69 content_match=1 fsck_clean=1 const:payload_len=69
# vblk magic: lba0_read=1 lba0_zero=1 lba1_read=1 lba1_magic=VF04 lba1_version=4 lba1_total_blocks=8192
ok 5 - vblk fs tren dia
# vblk loi: capacity=8192 refused=OutOfRange refused_no_request=1 (=requests_after(131)-requests_before(131)) forced=Io forced_requests=1 (=requests_after(132)-requests_before(131)) recovered=1 device_status=0xf id_mismatch=0 timeouts=0 const:id_timeout_proven=0 errors_before=0 errors_after=1 errors_delta=1 (=after-before) fs_err_refused=Invalid
ok 6 - vblk loi ngoai dai
# vblk stats: requests=133 reads=59 writes=68 flushes=6 completions=132 errors=1 notifies=133 id_mismatch=0 timeouts=0 capacity=8192
PANIC: KT-72 vblk xong (6 TAP, 0 fail)
```

**Độc lập phía host** (`kt72-host-fsck.log`): ảnh đĩa QEMU để lại (`vblk-smp2-boot1.img`)
đọc bằng `vifast-fsutil`:

```
$ vifast-fsutil fsck …/vblk-smp2-boot1.img     → FSCK_EXIT=0
$ vifast-fsutil ls   …/vblk-smp2-boot1.img /   → 2 kt72.txt
$ vifast-fsutil get  …/vblk-smp2-boot1.img /kt72.txt → ViFastOs KT-72: du lieu nay nam tren dia virtio that, khong phai RAM.   GET_EXIT=0
```

## 5. Cổng (run cuối)

- `./v2/bin/v2 check` PASS (`kt72-check.log`).
- `gate --fast` **PASS 718 test / 99 suite** (`kt72-gate.log` + `kt72-gate-summary.log`), L1 cổng
  **900 ms**; L1 **thuần** 897/885/1107 ms (`kt72-l1-thuan.log`; D2-100 ghi cả hai).
- Matrix **15/15 boot sạch** (`kt72-ktest-matrix.log`): 4 ảnh cũ × smp 1/2/4 = **12/12** (như
  KT-71) + `ktest-vblk` × 3 = 3/3.
- Conformance **61/61 dòng + 5/5 ca** (`kt72-conformance.log`).
- Clippy host (trong gate) + **kernel 6 ảnh `EXIT=0`** (`kt72-clippy-kernel.log`).
- Budget: `ktest` **242840** (`kt72-budget-ktest.log`), `ktest-vblk` **10976**
  (`kt72-budget-vblk.log`), trần **262144** (`kt72-budget-product.log` cho ảnh sản phẩm:
  `data+bss=200184`).
- Ảnh sản phẩm build **sạch không feature** (D2-108A): sha256
  `2e9550bdb6b4948399b76628de680f744869e344245aac9a88449f96575572d0`, `.text` digest
  `build=02ed5a1810ba59ce text=49854` = KT-71 (`kt72-product-sha.log`, `kt72-product-digest.log`,
  `kt72-budget-product.log`). **Sau khi vá DRIFT (CAO-1/CAO-2) build lại sạch: sha256 và digest
  y hệt** ⇒ ảnh sản phẩm **không đổi một byte**; `strings | grep -ci 'virtio\|vblk'` = **0**.
- Soak **0/1000** trên ảnh sản phẩm (`kt72-soak.log`): chạy **trước** khi vá DRIFT; vì ảnh sản
  phẩm sau vá **giống hệt** (sha256 + digest ở trên), soak **không chạy lại** — ghi rõ để không
  ai đọc "soak 0/1000" thành đã soak bản driver mới (driver **không** nằm trong ảnh sản phẩm).

### 5.1 Dòng log nguyên văn (máy đọc được — D2-109C)

```
# kt72-budget-ktest.log:   data+bss=242840 budget=262144
# kt72-budget-vblk.log:    data+bss=10976 budget=262144
# kt72-budget-product.log: data+bss=200184 budget=262144
# kt72-gate-summary.log:   TESTS=718 / SUITES=99
# kt72-gate.log:           # L1: OK (900 ms, đo bởi cổng)
# kt72-l1-thuan.log:       897 ms / 885 ms / 1107 ms
# kt72-ktest-matrix.log:   KTEST: 15/15 boot sạch
# kt72-conformance.log:    trace-compare: 61/61 dòng khớp, 5/5 ca
# kt72-soak.log:           # soak: 0/1000 lỗi (PASS) / BUILD_ID=02ed5a1810ba59ce
# kt72-clippy-kernel.log:  6× EXIT=0
# kt72-host-fsck.log:      FSCK_EXIT=0 / GET_EXIT=0
# kt72-baseline-summary.log: TESTS=703 / SUITES=96
# kt72-qemu-probe.log:     BAR4: 64 bit prefetchable memory (not mapped)
#                          alias pcie-mmio @gpex_mmio_window
```

`v2/test/boot/expected.txt` có dòng `ktest-vblk 6 6` (runner đọc số từ tệp, không hardcode).

## 6. Kiểm HEAD bằng worktree sạch (D2-105b)

Cây chung có tệp của tab khác (`viscript/`, `review/`, `.agents/`, các bản `TRA-LOI-…` ở gốc) ⇒
**không** kết luận bằng `v2 check` trên cây bẩn. Kiểm HEAD `f2edba6a` bằng worktree sạch:

```
$ git worktree add --detach /tmp/kt72-wt f2edba6a
$ cd /tmp/kt72-wt && ./v2/bin/v2 check
```

Log: `kt72-worktree-head.log`, `kt72-worktree-check.log`. Sau commit của lead, việc kiểm commit
(tương đương) do lead thực hiện trên pathspec đã commit.

## 7. Kiểm chéo số (D2-109C)

`kt72-verify-numbers.sh` (chép trong `JOURNAL/`) đối chiếu từng token số trong
`k6-virtio-blk.md` / `k3b-core.md` §20 / `STATE.md` / journal này với **log đã nằm trong repo**:

```
$ bash v2/agent/JOURNAL/kt72-verify-numbers.sh
# verify: ok=<…> fail=0
```

Log: `kt72-verify-numbers.log`.

## 8. Nợ (ghi rõ)

1. Poll chưa ngắt (MSI-X/INTx); queue depth thực tế 1.
2. `flush`: request thật + hoàn tất (status OK) + dữ liệu host đọc lại được; **bền qua reboot**
   thuộc KT-75. Không tuyên bố hơn.
3. `id_mismatch`/`timeouts` **chưa có ca âm** (D2-108B) — ghi "chưa chứng minh"; `errors` có ca
   dương (ca ép + mutation).
4. PCI: một bus/một function, BAR gán cố định, map leaf 2 MiB — chưa phải enumerator tổng quát.
5. Driver **chưa vào ảnh sản phẩm** (feature `vblk` chỉ ảnh test) — KT-73.
6. Ảnh `ktest-vblk` chưa in `# conf image:` (digest) — thêm khi KT-73/75 cần neo ảnh.
7. Ảnh đĩa 4 MiB cố định trong runner; driver từ chối `blk_size ≠ 512`.

## 9. Tự rà trước khi báo reviewer (đã áp, có chạy lại đo)

1. **D2-102**: dòng evidence `fs_err_refused=Invalid` trước là **chuỗi viết tay** ⇒ đổi sang in từ
   `BlkError::to_fs_name()` (hàm ánh xạ **duy nhất**); `passes=2` là hằng thiết kế ⇒ đổi thành
   `const:passes=2` (và regex `ktest.sh` theo cùng một dòng). Đã chạy lại matrix + verify số.
2. **Không ghi `queue_select` giữa lúc request đang bay**: `notify_off` đọc **một lần** ở init và
   cache trong `State`; mỗi notify chỉ còn **một** ghi MMIO 16 bit vào cửa thông báo.
3. **`config_generation`** (spec §2.4.4): đọc **trước** và **sau** khi đọc `virtio_blk_config`;
   khác nhau ⇒ `Err("config_generation")`, không dùng capacity có thể bị xé.
4. Bỏ `let _ = CFG_DEVICE_ID` (import thừa) trong `pci.rs`; `ktest.sh` header sửa "bốn ảnh" →
   "năm ảnh" (thêm `ktest-vblk`).
5. Sau các sửa đổi: **matrix 15/15**, **clippy kernel 6 ảnh `EXIT=0`**, kernel `cargo fmt --check`
   `EXIT=0`, `v2 check` PASS, verify số **D2-110A: quét mọi số theo từng tài liệu, fail=0** — các log trong `JOURNAL/` đã chép **lại** đúng
   bản cuối (`kt72-ktest-matrix.log`, `kt72-green-qemu-ktest.log`, `kt72-clippy-kernel.log`,
   `kt72-host-fsck.log`, `kt72-verify-numbers.log`).

## 10. Vá DRIFT của reviewer (CAO-1, CAO-2, medium, thấp) — delta `file:line`

**CAO-1 — `FLUSH` không đàm phán feature** (spec §5.2.3):
- `lib/vifast-virtio/src/blk.rs:30-54` (mới): `F_FLUSH=9`, `FLUSH_MASK=0x200`,
  `negotiated_features()` (thuần: VERSION_1 + FLUSH nếu thiết bị có), `flush_negotiated()`;
  test host `flush_phai_dam_phan_feature` (device có bit 9 ⇒ `0x100000200`; không có ⇒ chỉ
  `0x100000000`, `flush_negotiated=false`).
- `kernel/vifast-plat-riscv64/src/virtio_blk.rs:331-350`: đàm phán theo hàm thuần, đọc lại **hai**
  trang, `driver_features=0x100000200`; `:637-644` `flush()` trả `Unsupported` nếu chưa đàm phán;
  evidence `flush_feature=1`.

**CAO-2 — BAR fail-open** (D2-110C):
- `lib/vifast-virtio/src/pci.rs:120-188` (mới): `range_overlaps`, `first_conflict`
  (`other_bar`/`reserved`), `PciError::BarConflict`; test host `chong_lan_bar_bi_bat` (chồng lấn
  BAR khác, chồng lấn vùng nền tảng, **kề nhau không bắt oan**, cỡ 0 ⇒ ĐỎ).
- `kernel/vifast-plat-riscv64/src/pci.rs:120-230`: `RESERVED_RANGES` (UART/PLIC×2/ECAM/RAM),
  `collect_other_bars` (**đo** BAR memory đã gán của mọi function khác rồi **khôi phục** nguyên
  giá trị), `mmu_range_conflict` (trang đã map phải là leaf cửa sổ PCIe của chính driver);
  `:300-320` kiểm **trước khi ghi BAR** ⇒ `BarConflict` fail-closed; report thêm
  `overlap_checked/other_bars/reserved_ranges`.
- TAP 1 đòi `overlap_checked=1 cap_ranges_ok=1`; evidence `other_bars=0 reserved_ranges=5`.

**Medium**:
- `used_pa` theo `qsize` thật (`virtio_blk.rs:566-571`), `queue_max ≥ 3`
  (`:364-370`); `queue.rs:151-172` `set_desc` trả `bool` (`#[must_use]`) + test
  `set_desc_ngoai_queue_size_khong_im_lang`.
- **D2-110A**: viết lại `JOURNAL/kt72-verify-numbers.sh` — quét **mọi số** của **từng** tài liệu
  (218 token/4 tài liệu), mỗi số phải có trong log của task **hoặc** mã nguồn, kèm danh sách
  required chống xoá; `92,6 %`/`19.304 B` đã **bỏ khỏi tài liệu** (không có trong log nào) và
  mọi số nghìn đổi về dạng khớp log (`242840`).
- **D2-102/106**: `const:sectors`, `const:payload_len`;
  `refused_no_request=1 (=requests_after(131)-requests_before(131))`,
  `forced_requests=1 (=requests_after(132)-requests_before(131))`,
  `errors_before=0 errors_after=1 errors_delta=1 (=after-before)`,
  `compared=8192 (=reads(16)*512)`.

**Thấp**: `Cargo.toml` sửa bình luận "leaf khác" ⇒ **cùng VA `0x4000_0000`** (lý do tách ảnh);
raw boot log đỏ chép đủ `kt72-red-boot-cap-loop.log` + `kt72-red-boot-bar-readback.log` (có
`TAP version 13`), tệp trích ghi rõ "BẢN TRÍCH"; capability kiểm **cả `offset+length ≤ bar_size`**
(CAO-2 evidence `cap_ranges_ok=1`); host test `blk_size_khac_512_bi_tu_choi` (ca âm).

**Nợ ghi đúng hơn**: TAP 6 **không** phải bằng chứng cho `id_mismatch`/`timeouts`
(`const:id_timeout_proven=0`, hai bộ đếm đã bị bỏ khỏi `ok`); **coverage tích hợp không có**
(không boot nào chạy driver + 70 TAP / U-mode / conformance cùng lúc); xung đột VA là **thật**
(`ktest_obj.rs:31`); kiểm chồng lấn **chưa** đọc DT/`ranges` — vẫn là hằng, nợ KT-73.
