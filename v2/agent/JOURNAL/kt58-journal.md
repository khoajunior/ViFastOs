# KT-58 — K3b tầng 1: port core `no_std` (Untyped + CNode + MDB + capability)

- **Ngày:** 2026-11-15 (Asia/Tokyo). **Agent 2 dòng OS**, phiên mới. HEAD lúc mở: `95021ca4`
  (`[KT-00] K3 DONE (D2-82)… mở K3b KT-58`).
- **Phạm vi:** tạo `v2/kernel/vifast-core` (`no_std`, `forbid(unsafe_code)`, không `alloc`),
  port **nguyên ngữ nghĩa** `Untyped`/`CNode`/MDB/cap từ `v2/sim/vifast-sim/src/kobj.rs`;
  parity host với sim; **không** nối core vào syscall/kernel (KT-59/60/61).
- **Không đụng:** `spec/`, `viscript/`, QUEUE/DECISIONS/AGENTS, sim `src/` (chỉ dev-dep +
  test mới), `.scn`/`expected.txt`/manifest/ktest. Chỉ stage `v2/`.

## 1. Baseline (trước thay đổi)

`JOURNAL/kt58-baseline-gate.log` — `./v2/bin/v2 gate --fast` PASS, **574 test/75 suite**
(khớp D2-82/KT-57). **T-INFRA:** L1 baseline **86.297 ms** đo khi worktree đang được
sửa/recompile lại (máy bận/cold), không phải số chuẩn; gate cuối **833 ms** / 1.000 seed —
**cùng seed, không nới ngưỡng** (D2-07 không retry; khác điều kiện đo).

## 2. ABI từ spec (D2-11, không hardcode)

- `v2/tools/vifast-gen/gen.py`: sinh thêm từ `[object_size]` (`SLOT_BYTES=48`,
  `CNODE_BASE_BYTES=64`, `PAGE_BYTES=4096`, `UNTYPED_MIN_BYTES=4096`,
  `DEFAULT_ALIGN_BYTES=16`, `OBJ_*_BYTES` cho 6 object lõi cố định). `generated.rs` sinh lại;
  `gen-check` PASS; `docs/abi.md` **không đổi** (chỉ code Rust).
- `v2/lib/vifast-abi/src/lib.rs`: `#![cfg_attr(not(test), no_std)]` + test
  `kich_thuoc_object_dung_spec_khong_hardcode` (gate +1 test).
- `spec/` không sửa. 4 bit quyền (READ/WRITE/GRANT/GRANT_REPLY) là hằng thiết kế §4.3/QD
  §3.12 (spec không có bảng quyền) — ghi trong `vifast-core/src/spec.rs`.

## 3. Core (v2/kernel/vifast-core)

7 tệp, 1.919 dòng (`wc -l`): `lib.rs` 57, `spec.rs` 35, `error.rs` 281, `arena.rs` 89,
`slotpool.rs` 162, `machine.rs` 1.060, `check.rs` 235. Không `unsafe`; bảng tĩnh
`MAX_OBJECTS=256`, `MAX_CAPS=512`, `MAX_UNTYPEDS=16`, `MAX_SLOT_WORDS=4096`; arena Untyped là
`Region { base, capacity, cursor }` (watermark = `cursor`; `base` = **nhãn PA** — D2-83).

Bản đồ sim → core + khác biệt/nợ: `v2/docs/k3b-core.md` §3/§6.

## 4. Review độc lập (reviewer-kt58) + vá 7 mục

Verdict **DRIFT thấp** (không UNSAFE); tự chạy parity 4 passed, core riscv64 build + clippy,
gate 578/76, matrix 9/9, product 11.248. Các lệch **parity vẫn xanh** đã vá:

1. **`Revoke` trên CNode đã finalise** trả `InvalidCapability` như sim (`cnode_slots_of` →
   body `Dead`), không đọc metadata trả `Ok(0)`/`SlotEmpty`; ca `dead_cnode_errors`.
2. **`put_cap` vào CNode đã finalise** trả `InvalidCapability` (sim `cnode_set_slot` →
   body_mut `Dead`), không `WrongObjectType`; `cnode_set_slot` phân biệt `!alive` ⇒
   `InvalidCapability`, sống sai kiểu ⇒ `WrongObjectType`.
3. **D2-83:** `untyped_arena` của core là **nhãn PA**, không "y hệt sim"; `k3b-core.md`
   §2/§3/§6.7 đã sửa, digest **không** so `base` (chỉ capacity + thống kê byte).
4. **D2-83 biên giới VM:** core **không** Retype `Frame`/`PageTable`/`VSpace` — **thiết kế**,
   không phải thiếu sót; thêm test pin `core_d2_83_vm_reserved_not_supported`
   (`NotSupported`, không đốt byte/cap) + ghi §6.4.
5. **Parity nhánh còn thiếu:** `delete` cap gốc (con thành gốc), `Mint(Some(7))` trên Tcb ⇒
   `InvalidArgument` còn `Some(0)` hợp lệ, Retype Untyped `len` 8192 > `ARENA_ALIGN` ⇒
   `Alignment`, ca `InvalidCapability` (CNode chết) + `IllegalOperation` (cap đã có slot) —
   thêm `script_delete_root_cap`, `script_badge_and_align`, `script_dead_cnode_errors`.
6. **M1 độc lập:** M1 cũ trùng red-before ⇒ đổi M1 = `Mint(Some(0))` giữ badge nguồn; M2 =
   delete bỏ reparent; M3 = Revoke bỏ reset arena; ba mutation độc lập, log riêng.
7. **L1 baseline 86.297 ms vs gate 833 ms:** khác điều kiện (máy bận/cold) — ghi ở §1, không
   nới ngưỡng.

## 5. Đỏ trước + mutation (D2-05)

- Lần chạy parity **đầu tiên** đỏ thật: core trả `object_type` cho 256 ô `Meta` (mảng tĩnh)
  thay vì `n_objects` ⇒ digest `bytes#0`/`diff-0#0` lệch. Vá guard `n_objects`, sau đó xanh.
  Log: `JOURNAL/kt58-red-before.log`.
- **M1** (`Mint(Some(0))` giữ badge nguồn) ⇒ đỏ digest `rights#8` —
  `JOURNAL/kt58-mutation-m1.log`.
- **M2** (`delete` bỏ reparent) ⇒ `check_caps` đỏ — `JOURNAL/kt58-mutation-m2.log`.
- **M3** (`Revoke` Untyped bỏ reset arena) ⇒ `CheckUntyped used` đỏ —
  `JOURNAL/kt58-mutation-m3.log`.
- M1..M3 độc lập nhau + độc lập red-before; mỗi lần sửa tạm `machine.rs`, chạy test, khôi
  phục sha256 `acde4970…` (đã xác nhận trước/sau), không retry lấy xanh.

## 6. Parity xanh

`JOURNAL/kt58-parity.log` — `cargo test --manifest-path v2/Cargo.toml -p vifast-sim
--test kt58_core_parity -- --nocapture`:

```text
# core: size_of::<Machine>() = 68264 B
# parity differential: Stats { steps: 2016, ok: 877, err: 1139, retype_ok: 57,
#   copy_ok: 71, mint_ok: 13, delete_ok: 37, revoke_ok: 219, revoke_deleted: 79,
#   caps_closed: 1138 }
test result: ok. 4 passed; 0 failed
```

14 kịch bản tường minh (KT-09/10/11 + D2-83), differential 32 seed × 48 op, 2 test core-only
(pin D2-83 + hằng ABI/dấu chân); so kết quả (tên+số mã lỗi), digest object/cap/slot/byte,
`check_caps`/`check_all_bytes` của **cả hai**.

## 7. Cổng sau thay đổi

- `JOURNAL/kt58-gate-run1-clippy-fail.log`: gate đầu **đỏ clippy** (`type_complexity` trong
  test) — sửa bằng alias `ObjFp/CapRow/CnRow/UnRow`, không nới `-D warnings`.
- `JOURNAL/kt58-gate.log`: gate cuối — **579 test/76 suite, 0 failed**, L1 1.000 seed
  **830 ms**, wall **4 m 41,3 s** — lần chạy sạch trên cây đóng băng). Baseline 574 → +4 parity + 1 ABI = **579**.
- `JOURNAL/kt58-ktest-matrix.log`: `bash v2/test/boot/ktest.sh 1 matrix` — **9/9**, TAP
  **43/43 + 3/3 + 7/7** (expected.txt không đổi; kernel không đổi), wall 43,6 s.
- `JOURNAL/kt58-product-run.log`: product `run.sh 1 1` PASS, file 90.696, **data+bss 11.248**
  (trần 262.144), 0 chuỗi `ktest`.
- `JOURNAL/kt58-core-riscv.log`: core build `riscv64gc-unknown-none-elf` + clippy `-D warnings`
  xanh.
- **Budget:** ảnh kernel **không đổi** (core chưa link): product 11.248 / ktest 17.344 /
  guard 2.680 / user 11.752 B. Dấu chân core tĩnh `size_of::<Machine>() = 68.264 B [ĐO]` là
  nợ KT-61 khi link.

## 8. Bất biến giữ nguyên

- Không sửa sim `src/`: sim vẫn oracle; chỉ **thêm** dev-dependency `vifast-core` +
  `tests/kt58_core_parity.rs`.
- Không sửa `spec/`, không đổi ABI, không mở `spec/kernel.toml`.
- `viscript/` không chạm; commit chỉ `v2/`.

## 9. Bàn giao

- Commit `[KT-58]` (crate + gen + test) và `[KT-58b]` (docs/journal/STATE), push `main`.
- Đề xuất KT-59: TCB/transition/sched trên nền `vifast-core`; KT-60 IPC/Reply/Notification;
  KT-61 nối core + đo lại `.data+.bss` (thay `SlotPool` bằng slot trong arena/`PhysMem`; VM
  qua object layer KT-52 theo D2-83).
