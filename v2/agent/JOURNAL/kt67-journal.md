# KT-67 — memsrv U-mode + `vifast-alloc` (Frame cấp qua IPC, cầu nối D2-83)

Ngày: 2026-11-16. Dòng: OS (Agent 2, phiên mới). Task: **KT-67** — task đầu tiên của **K6**
(D2-95), trên HEAD `2ed9f9bd` (KT-66 DONE, K3.5 DONE).

Kết quả ngắn:

- **`vifast-alloc`** (crate mới, `no_std`, `forbid(unsafe_code)`, 0 dependency): allocator Frame
  trên Untyped (4 KiB/2 MiB/1 GiB), `alloc`/`free`/`check_accounting`/đếm byte; **bản duy nhất**
  — object layer (`vifast-plat-common::obj`) đã **bỏ** `pa_used`/`pa_holes` riêng và dùng
  `vifast_alloc::FrameArena` (một nguồn sự thật byte; `MAX_HOLES`/`UNTYPED_PA_WINDOW` hết khai lại).
- **Cầu nối D2-83 kernel-mediated** (`vifast-plat-riscv64::mem_bridge` + core
  `add_foreign_object`/`foreign_key`/`kill_foreign_object`): core giữ **cap**, object layer giữ
  **Frame/PA**; tra ngược `core obj → (obj cap, pa, size_bits)`; chuyển **cap Frame** cho client
  đi đúng **KT-63** (receive window, bỏ `RIGHT_GRANT`). **Tên cho đúng (D2-96)**: `memsrv` U-mode
  trong KT-67 **chưa cài** — nó là **policy façade**; đường cấp Frame là **kernel-mediated**.
- **TAP QEMU**: ảnh `ktest` 60..64 (`# mem:` evidence) — alloc/revoke/oom/bridge/deny.
- **BLOCKED/đã chốt**: memsrv U-mode (façade) — ABI hiện tại không chở cap trong message và
  U-mode không có invocation tạo Frame; **D2-96 chọn (A)** (kernel-mediated) và mở **KT-67a** cho
  invocation hẹp `Untyped.Retype` (D2 mới). Lý do + `file:line` ở `docs/k6-memsrv.md` §6.

## 1. Bước làm (theo quy trình)

1. Đọc D2-83/84/85/86..95, `k3b-core.md`/`k35-procsrv.md`, object layer KT-52, core KT-58..64.
2. Baseline: gate `--fast` KT-66 đã ghi **649 test / 84 suite** (D2-94); lần đo lại trong phiên
   bị nhiễu vì máy chạy 3 workspace test song song ⇒ **không dùng** làm số baseline (xem §7).
3. Viết `vifast-alloc` + test host (10 ca) → rút ruột PA của obj layer sang crate.
4. Core: object đại diện (`core_foreign.rs`, tách để R1 ≤ 1.500 dòng).
5. `mem_bridge` + TAP `ktest_mem.rs`; cập nhật `expected.txt` (65), `manifest_main`,
   `evidence_for` (7 dòng `# mem:`), plan `1..65`, đánh số lại TAP trap lồng 60→65.
6. TAP smp1 xanh → matrix 12/12 → gate → soak → báo lead spawn reviewer.
7. Đỏ-trước ghi ở §7.

## 2. Tệp thay đổi (chỉ `v2/`)

| Tệp | Nội dung |
|---|---|
| `v2/lib/vifast-alloc/{Cargo.toml,src/lib.rs,tests/frames.rs}` | **mới** — allocator Frame + 10 test host |
| `v2/Cargo.toml` | thêm member `lib/vifast-alloc` |
| `v2/kernel/vifast-plat-common/Cargo.toml` | path-dep `vifast-alloc` |
| `v2/kernel/vifast-plat-common/src/obj/arena.rs` | `Untyped` giữ `FrameArena`; bỏ `pa_base`/`pa_used`/`pa_holes`/`n_pa_holes`; `pa_*` uỷ quyền allocator + ánh xạ lỗi |
| `v2/kernel/vifast-plat-common/src/obj/mod.rs` | `UNTYPED_PA_WINDOW`/`MAX_HOLES` lấy từ crate; `FrameBudget`; reset PA qua allocator |
| `v2/kernel/vifast-plat-common/src/obj/check.rs` | `check_objmem` gọi `FrameArena::check_accounting`; `frame_budget()` view |
| `v2/kernel/vifast-plat-common/src/obj/vspace.rs` | `pa_base()` |
| `v2/kernel/vifast-core/src/core_foreign.rs` | **mới** — `add_foreign_object`/`foreign_key`/`kill_foreign_object` |
| `v2/kernel/vifast-core/src/machine.rs` | `push_meta` → `pub(crate)` (tách tệp, giữ R1) |
| `v2/kernel/vifast-core/src/lib.rs` | `mod core_foreign;` |
| `v2/kernel/vifast-plat-riscv64/src/mem_bridge.rs` | **mới** — `grant`/`release`/`resolve`/`budget` |
| `v2/kernel/vifast-plat-riscv64/src/lib.rs` | `pub mod mem_bridge;` |
| `v2/kernel/vifast-kbin-riscv64/src/ktest_mem.rs` | **mới** — TAP 60..64 + `# mem:` |
| `v2/kernel/vifast-kbin-riscv64/src/ktest.rs` | `mod ktest_mem`, gọi pha, plan `1..65`, trap lồng 60→65 |
| `v2/test/boot/{expected.txt,ktest.sh}` | `ktest 65 65`; +5 tên TAP; +7 evidence `# mem:` |
| `v2/docs/k6-memsrv.md` | **mới** — thiết kế, TAP, giới hạn, BLOCKED + 2 lựa chọn |
| `v2/agent/{STATE.md,BLOCKED.md}`, `v2/agent/JOURNAL/kt67-*` | sổ + log |

Không chạm `spec/`, không thêm syscall, không sửa sim/`crates/`, không đụng `viscript/`,
`review/`, `v2/agent/reports/kt64/` (của tab khác).

## 3. Bằng chứng TAP (chép từ log boot, D2-80)

Log: `JOURNAL/kt67-ktest-matrix.log` (matrix 12/12, mỗi log trong `/tmp/k3-ktest/`). Các dòng
`# mem:` của lần boot `ktest smp=1`:

```
# mem: init untyped=2 u0=0 u1=1 base0=0x82000000 base1=0x80000000 window=1073741824 cap0=1073741824 cap1=1073741824 physmap=1
# mem: alloc cap0=0 window=1073741824 base=0x82000000 used=2101248 free=1071640576 hole=0 live4=1 live2=1 live1g=0 pa4=0x82200000 pa2=0x82000000 s21=1 s12=1 real4=1 real2=1 stats=2101248 frames=2 allocs=2 check=1 obj=1
# mem: revoke used0=2101248 used1=0 holes=0 frames=0 free=1073741824 realloc4=0x82000000 same=1 stats=0 check=1 obj=1
# mem: oom window=1073741824 pa1g=0x80000000 second=NotEnoughMemory code=9 small=9 used=1073741824 used_after=1073741824 free=0 atomic=1 panic=0 check=1 obj=1
# mem: bridge token=64 core_obj=48 obj_cap=2 key=0x90002 pa=0x82000000 bits=12 slot=69 rights_ok=1 obj_ok=1 resolve=1 used_granted=4096 check_caps=1 check_ipc=1 check_objmem=1
# mem: release released=1 used_after=0 free=1073741824 cap_after=0 caps_check=1 obj=1
# mem: deny wrong=2 rights=3 invalid=1 used=0 allocs=6 atomic=1 obj=1
```

Đọc: `used=2101248` = 2 MiB + 4 KiB (cấp giảm dần); `live=2101248` là số **test tự biết** (2 MiB +
4 KiB) và `stats=2101248` là **Σ span Frame liệt kê từ bảng object** (`untyped_stats`, **không**
đọc watermark) — bằng `used` (watermark `FrameArena`); `check_objmem` kiểm `Σ span + Σ hole ==
used` từ danh sách span + hole ⇒ **không tautology** (khai thiếu/khai chồng ⇒ `Corrupt`, có test
host `kiem_ke_toan_bat_duoc_khai_gian`); `revoke` đưa `used=0` và cấp lại rơi **đúng** `pa=0x82000000`
(`same=1`); `oom` trả **mã 9** hai lần (1 GiB và 4 KiB) và watermark **không đổi**; `bridge`
`rights_ok=1` (cap nhận `0xb` = `RIGHTS_ALL & !RIGHT_GRANT`), `resolve=1`; `release` byte về 0;
`deny` mã **2/3/1** và `atomic=1`.

TAP: **65/65** (ảnh chính, +5 TAP so với 60), guard **3/3**, user **14/14**, proc **10/10**.
Matrix **12/12** (4 ảnh × smp 1/2/4), `KTEST: PASS`.

Host: `cargo test -p vifast-alloc` → **10/10** (`JOURNAL/kt67-alloc-host.log`);
`cargo test -p vifast-plat-common --test obj` → **15/15** (`JOURNAL/kt67-obj-host.log`) —
chứng minh rút PA sang crate **không** đổi ngữ nghĩa object layer.

`size_of::<Machine>()` **không đổi** = **188.872 B** ≤ pin 196.608 (`# core: link … bytes=188872`);
ảnh không đổi bất thường: product **200.184 B**, ktest **209.920**, guard **2.680**, user
**202.344**, proc **206.536** (trần 262.144) — `JOURNAL/kt67-budget.log`.

## 4. Gate `--fast`

`./v2/bin/v2 gate --fast` **PASS** (`JOURNAL/kt67-gate.log`): fmt, clippy, R1..R5, self-test luật,
ABI/scn/report/gen-check + fixture âm, `build (L0)`, **L0 661 test / 87 suite** (Σ dòng
`test result: ok. N passed`; baseline D2-94 **649 test** ⇒ **+12 test** từ `vifast-alloc`,
gồm test chống tautology + tách hole khi bảng đầy), **L1 1.000 seed 1.282 ms**.
Ghi chú: reviewer-kt67 đọc **659/87** trên cây trước khi thêm test tách-hole thứ 12 — số đo cuối
là **661/87** trong log đính kèm.

## 5. Soak

`bash v2/test/boot/soak.sh 1000 3` **PASS**: **0/1000** lỗi, wall **238 s** (4 boot/s),
`BUILD_ID=a6dff9f704e6cd88` `sha256=0495501e7962…` (`JOURNAL/kt67-soak.log` dòng cuối) — soak ảnh
**sản phẩm** vì KT-67 đổi đường byte của object layer (allocator Frame).

## 6. Vì sao chọn cấu trúc này

- **Một nguồn sự thật byte, nhưng kiểm bằng hai nguồn độc lập**: `FrameArena` là chỗ duy nhất
  giữ watermark/hole PA; object layer chỉ *giữ* nó và **liệt kê danh sách Frame sống từ bảng
  object** để `check_accounting` đối chiếu (không tautology: khai thiếu/khai chồng ⇒ `Corrupt`,
  có test host + ca âm). Bằng chứng TAP 60 in `live` (test biết), `stats` (bảng object) và `used`
  (watermark) — **cả ba** phải bằng nhau.
- **Cầu nối một chiều đúng D2-83**: core **không** Retype VM (giữ pin TAP 51), chỉ thêm object
  đại diện 0 byte arena; cap Frame đi qua KT-63 như mọi cap khác (rights/GRANT/MDB giữ nguyên
  ngữ nghĩa), nên memsrv (khi mở ABI) không cần cơ chế cap thứ hai.
- **Không thêm syscall/spec**: đúng ràng buộc; phần bị chặn ghi rõ + 2 lựa chọn thay vì tự mở.

## 7. Đo lường, đỏ-trước, nợ

- **Vá theo chuẩn bị review (lead, sau D2-96)**: (1) `stats.pa_used` đổi sang **suy từ bảng
  object** (đường độc lập với watermark); TAP 60 đòi `live == stats.pa_used == used`; (2)
  `FrameArena::check_accounting` nhận **danh sách span Frame** caller liệt kê (kiểm trong dải,
  rời nhau, không chồng hole, `Σ + Σ hole == used` ⇒ phủ kín) + test `kiem_ke_toan_bat_duoc_khai_gian`;
  (3) mọi chỗ gọi memsrv = **policy façade**, đường cấp = **kernel-mediated** (D2-96).
- **Đỏ-trước**: `JOURNAL/kt67-red-before.log` — lần boot đầu sau khi cập nhật hợp đồng TAP
  (`expected.txt` 65, manifest +5 tên, 7 evidence `# mem:`) **trước** khi sửa định dạng evidence:
  runner FAIL đúng `thiếu plan-1..65`, `ev91`, `ev92` (TAP logic đã xanh nhưng hợp đồng đỏ) — đây
  là đỏ của **hợp đồng bằng chứng**, không phải đỏ của logic allocator. Với allocator, "đỏ" được
  khoá bằng 10 test host viết cùng lúc với crate + TAP pin ngay từ đầu; không có log đỏ-trước
  riêng cho logic (ghi nợ quy trình: lần sau viết test trước khi rút ruột obj layer).
- **Cổng cuối (sau vá)**: gate `--fast` **PASS 661 test/87 suite**, L1 **1.282 ms**
  (`JOURNAL/kt67-gate.log`); matrix **12/12** (`JOURNAL/kt67-ktest-matrix.log`); soak
  **0/1000**, wall **238 s** (`JOURNAL/kt67-soak.log`); budget 5 ảnh ≤ 262.144
  (`JOURNAL/kt67-budget.log`); `size_of::<Machine>()` **188.872** không đổi.
- **Nợ**: memsrv U-mode = policy façade, **BLOCKED** → **KT-67a** (D2-96, invocation hẹp
  `Untyped.Retype`); client U-mode **chưa** map/dùng được Frame (chưa có invocation `VSpace.Map`);
  Frame 1 GiB chỉ nhãn (D2-66); `free` không phát hiện double-free cùng cỡ (chốt ở object layer,
  như KT-52); `object_id` quay vòng sau 2^32 cấp (resolve fail-closed, chưa có thế hệ/GIL);
  baseline 649 test/84 suite lấy từ D2-94 (không đo lại sạch trong phiên vì máy nhiễu) — số final
  đo được là 661/87, +12 test từ crate mới.
