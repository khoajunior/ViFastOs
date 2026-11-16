# KT-75 — `test-persist` bảy pha trên virtio-blk (K6) — nhật ký

**Vào**: KT-74 DONE (D2-115); nền `84e0df15` (trong lúc làm, dòng ViScript commit `93e1d418` chỉ
chạm `viscript/agent/` — không ảnh hưởng cây OS); gate baseline PASS; ba tệp nợ phải đọc trước:
D2-115 (luật bất nhất spec), D2-113F (self-break trên **bản copy**), D2-112 (script kiểm số theo
**scope từng khẳng định**, cấm ngoại lệ chữ số, cấm log mutation làm nguồn, cấm suy đoán sha).

## 1. Thiết kế

- **Ảnh test riêng** `ktest-persist` (`v2/kernel/vifast-kbin-riscv64/src/ktest_persist.rs`,
  `required-features = ["ktest","vblk"]`) — không thêm feature mới, không sửa `vifast-fs`/
  `vifast-vfs`/`spec/`.
- **Kịch bản ở LBA=0**: sector FS dành riêng (`stage()` từ chối LBA=0; `format` chỉ ghi số không
  một lần; `Fs::initialize` chỉ đọc LBA=0 khi superblock hỏng — `fs.rs:800`). Runner ghi kịch bản
  trước **mỗi** boot; kernel in lại **nguyên văn** (kể cả `nonce`) ⇒ lệch kịch bản là đỏ ngay.
- **Mười boot** trên **một** file ảnh đĩa: pha **ghi** ở boot này, pha **đọc lại** ở boot sau;
  mỗi pha một hành vi khác nhau (tạo/ghi đè/append/truncate/rename/xoá+tạo lại/double-indirect).
  Mọi pha kết thúc bằng `Fs::fsync()` rồi `halt`.
- **Nhật ký pha `/pstate`** (append **bốn byte**/bản ghi): boot đọc lại cuối cùng **đọc lại từ đĩa** rồi mới
  in `phases`/`ok` ⇒ con số tổng không thể đến từ biến RAM.
- **Ca âm `flush`**: cache ghi-bay-hơi mô phỏng (`PersistDev`) giữa FS và virtio-blk; bỏ **đúng**
  hiệu lực `flush` ⇒ `pending_blocks` lớn hơn không, `device_flushes=0`, `pushed=0`; boot sau
  **không** thấy tệp, sha256 ảnh **không đổi**. Giới hạn (mô hình, không phải QEMU tự mất dữ liệu)
  ghi ở `docs/k6-persist.md` §9 và `agent/BLOCKED.md`.
- **Kiểm chứng host**: sau **mỗi** boot, `vifast-fsutil fsck/ls/stat/get` trên **cùng** ảnh, so
  từng byte với mô hình Python **cài đặt độc lập**; `fsutil_mutated=0` chứng minh kiểm host không
  ghi vào ảnh. Thêm tệp test host `lib/vifast-fs/tests/kt75_persist_host.rs` (ba ca) chạy trong cổng.

## 2. Số đo

```text scope=kt75-persist-run.log
# persist: phases=7 ok=7 boots=10 neg=1 host_checks=10
# reboot: boots=10 pids=10 phase_reboots=7 (7 pha; mỗi pha ghi ở một tiến trình, đọc lại ở tiến trình KHÁC)
# persist.sh: PASS
```

```text scope=kt75-qemu-b07.log
# persist: phase=7 action=write wrote=73728 recorded=1 fsync=1 durable=1 flush_honored=1 flushes_done=296 flushes_ignored=0 device_flushes=293 pending_blocks=0 pushed=1879 overflow=0 detail=197568495791 (=single(46)<<32|double(175)) sha_prefix=9cf09c3a errors=0
```

```text scope=kt75-qemu-b08.log
# persist: phases=7 ok=7 writes_ok=7 verifies_ok=7 records=14 const:phases_want=7 const:records_want=14
```

```text scope=kt75-host-b07.log
# host: phase=7 records=13 files=3 fsck=1 fail=0 img_sha256=96496275d2d42f0946514308cb040be43d5c54b07807f48939422ee15c622301 fsutil_mutated=0
```

## 3. Đỏ trước + tự phá (D2-113F: chạy trên **bản copy**, không sửa cây thật)

```text scope-red=kt75-red-before-qemu.log
FAIL log b01.log: dòng plan xuất hiện 0 lần
not ok - b01: log QEMU không khớp mô hình (kind=write phase=1)
```

```text scope-red=kt75-red-selftest.log
# red-selftest: PASS (QEMU + host đều phát hiện payload bị đổi trên đĩa)
```

## 4. Cổng

```text scope=kt75-gate-summary.log
TESTS=729 / SUITES=100
```

```text scope=kt75-ktest-matrix.log
# KTEST: 21/21 boot sạch (smp=matrix, NBOOT=1, ảnh/boot=ktest ktest-guard ktest-user ktest-proc ktest-vblk ktest-con ktest-exec)
```

```text scope=kt75-conformance.log
ok - trace-compare: 61/61 dòng khớp, 5/5 ca (22:16 23:19 24:7 25:7 26:12), build=7304a40491224327
```

```text scope=kt75-soak.log
# soak: 0/1000 lỗi (PASS)
```

```text scope=kt75-product-sha.log
BUILD_EXIT=0
# bytes=188552
digest build=ef566cfa583e34bb text=49874
```

## 5. Vá tối thiểu theo review của lead (hai mục)

- **(a)** `persist.sh`: `phase_reboots` trong dòng tổng kết **tính từ PID thật** của cặp boot
  ghi/đọc lại (chỉ đếm cặp mà cả hai boot `OK`), không còn là hằng (D2-102 — dòng tổng kết cũng
  là evidence). Các dòng `# phase_reboot:` vẫn là giá trị tính như trước.
- **(b)** `docs/k6-persist.md` §3.1: bảng nhãn **“quan sát — CHƯA chứng minh”** cho các bộ đếm chỉ
  thấy một giá trị (`overflow`, `size_bad`, `absent_bad`, `replayed`, `errors`, `id_mismatch`,
  `timeouts`, `recorded`/`fsync`/`durable`); thêm **nợ ca âm** ở §9 và ở `STATE.md`. Không mở rộng
  phạm vi: **không** dựng ca âm mới trong đợt này.

- **D2-106b** — `ktest_persist.rs`: `bytes_lost` in kèm công thức `(=pending(<n>)*512)`; `detail` của pha bảy in
  cả công thức `(=single(<i>)<<32|double(<j>))` + hai thành phần (D2-106b) — host `check` **kiểm đẳng thức**,
  không chỉ đọc một số.
- **D2-102** — `persist.sh`: dòng tổng kết **suy** `phases`/`ok` từ log b08 (đã qua `check_log`) và `neg` từ kết quả
  thật của hai boot ca âm; `pids` đếm **unique** bằng `sort -u | wc -l`; `verify` siết miễn trừ số `x.y` (đòi
  tiền tố chữ), `selfbreak` thêm ca **số kiểu phân cách nghìn** phải đỏ.

## 7. Nợ

Ảnh sản phẩm **thiếu** U-mode/shell (D2-113, KT-76); ca âm `flush` là **mô hình cache**; matrix
**không** chứa ảnh persist (một pha là **hai** boot); chưa có SMP/ngắt trên ảnh persist; chưa có
`fsync` từ U-mode; chưa có ca torn-write (KT-71 giữ bằng chứng riêng); chưa có ca `NoSpace`.
