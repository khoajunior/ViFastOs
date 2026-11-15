# KT-62 — nhật ký (K3.5 tầng 1: 16 syscall → `Machine` thật + `init` U-mode dùng core)

- **Ngày:** 2026-11-12 (phiên Agent 2 dòng OS, HEAD vào `ebae897e` = D2-87)
- **Task:** KT-62 (QUEUE: READY), ngân sách 3 h — kết thúc **trong ngân sách**.
- **Nguồn đã đọc:** `review/agent0/K3-PLAN.md`, D2-79/D2-83/D2-84/D2-85/D2-86/D2-87,
  `reports/K3B-DOD.md` §11, `docs/k3b-core.md` §8/§9/§10, `spec/abi.toml` + `lib/vifast-abi`,
  `kbin/src/{main,ktest_user,ktest_core}.rs`, `plat-riscv64/src/{user,core_bridge}.rs`,
  `core/src/{machine,core_ipc,core_tcb,core_sched,ipc,core_sync}.rs`, `lib/vifast-traceformat`.
- **Không chạm:** `viscript/`, `spec/`, QUEUE/DECISIONS/AGENTS, `.scn`; `unsafe` chỉ thêm
  trong `kernel/vifast-plat-riscv64` (D2-17); `vifast-core` vẫn `#![forbid(unsafe_code)]`.

## 0. Baseline

- `git rev-list --count HEAD..origin/main` = **0** (trước khi làm).
- `gate --fast` baseline: **PASS**, **598 test / 78 suite**, L1 1.000 seed — `kt62-baseline-gate.log`.
- Boot baseline 3 ảnh: **53 + 3 + 7 TAP**, 3/3 sạch — `kt62-baseline-ktest.log`; log ảnh
  `ktest-user` cũ giữ làm nguồn **đỏ trước**: `kt62-red-before-old-image.log`.

## 1. Đỏ trước (D2-05)

| Đỏ | Cách tạo | Kết quả |
|---|---|---|
| TAP mới chưa có đường syscall | chạy runner **mới** (`expected.txt` 17 + manifest 17 + evidence) bằng `--check-log` trên log ảnh `ktest-user` **HEAD cũ** | **ĐỎ**: thiếu TAP 5..17 + evidence (`kt62-red-before-new-tap.log`) — đúng "tính năng chưa có", không phải test giả |

## 2. Code (chỉ `v2/**`)

| Tệp | Thay đổi |
|---|---|
| `kernel/vifast-core/src/syscall.rs` | **mới** — bảng 16 syscall → `Machine` + `SyscallEnv` + `SyscallOutcome`/`SyscallReturn`; ngoài dải ⇒ 19 (D2-79); cap index ngoài bảng ⇒ 1 |
| `kernel/vifast-core/src/{core_sync,core_tcb}.rs` | `wait_addr_obj` (ObjId) + `tcb_remaining` (budget cho Yield) — **chỉ thêm hàm**, không thêm trường ⇒ `size_of::<Machine>()` không đổi |
| `kernel/vifast-core/src/{lib.rs,spec.rs}` | `pub mod syscall`, tái xuất `MIN_COMPAT` (sinh từ spec) |
| `tools/vifast-gen/gen.py` | sinh `MIN_COMPAT` từ `[abi].min_compat` (D2-11) — chạy `vifast-gen gen`, gen-check xanh |
| `lib/vifast-abi/src/{generated.rs,lib.rs}` | `MIN_COMPAT = 1` + test |
| `kernel/vifast-plat-common/src/user.rs` | façade `dispatch(m, tcb, hart, nr, args, env, task)` — **xoá** bảng 3 syscall; chỉ ghi sổ `UserTask` + đổi `SyscallOutcome` → `SyscallAction` |
| `kernel/vifast-plat-common/{Cargo.toml,tests/user.rs}` | dependency `vifast-core`; 9 host test mới cho façade |
| `kernel/vifast-plat-riscv64/src/user.rs` | `SyscallEnv` thật (`put_str`/`resolve_addr`/`now_ns`/`features`); `CORE_TCB` + `bind_core_tcb`; `record_sys`/`sys_calls`/`sys_err`/`sys_val`; trap `ecall` ⇒ core; `# sys:` evidence; `Switched` ⇒ PANIC có chẩn đoán |
| `kernel/vifast-kbin-riscv64/init/init_user.S` | 10 `ecall`: put_str, 5 ngoài dải (SBI/99/16/17/31), **DebugGetInfo 15**, **Yield 8**, put_str, Exit 42 |
| `kernel/vifast-kbin-riscv64/src/ktest_user.rs` | core init + rig (CNode 32 + TCB/EP/SC/SC2/Notify/Reply) + `init` TCB `Running{0}`; TAP 1..17 (5/6 U-mode, 7..13 kernel-level dispatch, 14 Exit không Reply) |
| `sim/vifast-sim/tests/kt62_syscall.rs` | **mới** — 8 host test cho bảng dispatch (16 số hiệu, 19, cap 1/2/18, Exit không Reply, Yield đổi TCB, IPC vòng, NB, WaitAddr/Clock/Put/Halt) |
| `test/boot/{ktest.sh,expected.txt}` | manifest 17 TAP + evidence `# sys:`; `ktest-user 17 17` |
| `docs/k3b-core.md` | §11 (bảng 16 syscall, biên hook, TAP, hai ngưỡng, nợ) |
| `agent/{JOURNAL/kt62-*,STATE.md}` | nhật ký + log + STATE |

## 3. Kết quả đo

| Hạng mục | Kết quả | Log |
|---|---|---|
| Host test mới | **9** (sim kt62, có ca Notification payload) + **9** (plat-common façade) | `kt62-host-tests.log` |
| `gate --fast` | **PASS 607 test / 79 suite** (598 + 9 sim test mới; 9 plat-common test chạy ngoài workspace v2), L1 1.000 seed | `kt62-gate.log` |
| TAP QEMU (matrix smp 1/2/4) | **9/9 boot sạch**: `ktest 53/53`, `ktest-guard 3/3`, `ktest-user 17/17` | `kt62-ktest-matrix.log` |
| `size_of::<Machine>()` | **180.168 B** ≤ pin **196.608 B** | `# core:`/`# user core:` |
| Ảnh `.data+.bss` | **191.416 / 197.568 / 2.680 / 192.304** ≤ **262.144**; file 185.736 / 484.704 / 53.808 / 323.296 (gồm `.symtab`/`.strtab`, không nạp) | `kt62-budget.log` |
| Clippy kernel | workspace + `kbin` features `ktest,user` + `plat-riscv64 --features user`: **0 warning/0 error** với `-D warnings` | `kt62-clippy-kernel.log` |
| Soak | **0/1000** (lần 2, sau vá review), wall 226 s, BUILD_ID `6129a5025e4382ef`, sha256 `46d3fc26…`, `git=7b449ea2` | `kt62-soak.log` |
| Conformance | không đổi (ảnh `ktest` 53 TAP như cũ; `# trace:` chỉ do `.scn`) | — |

## 4. Soak + ghi chú phạm vi

`bash v2/test/boot/soak.sh 1000 3` — **0 lỗi / 1.000** boot, wall **226 s**, 3 luồng, không
icount, timeout 20 s; `BUILD_ID=6129a5025e4382ef` (FNV-1a `.text`; **đã đổi** vì `core_bridge::init` thêm đường
`smp::hart_count()` — KT-62 bản đầu `34dc594e0f651ed3`), `sha256=46d3fc26ed1dd04d951fb40840e5ac09a64b33c7f0e58fb83c1cf05a626f8d95`,
`git=7b449ea2` (trên D2-88) — pha review đổi đường syscall/hart nên **soak lại** (không retry)
(`kt62-soak.log`). Ảnh sản phẩm vẫn chạy `boot_smoke` (một vòng đời TCB) mỗi boot; không PANIC.

## 4b. Vá sau review `reviewer-kt62` (DRIFT trung bình) + D2-88

| Mục | Vá |
|---|---|
| **F1** (ABI chức năng) | `Recv`(3)/`NBRecv`(4)/`NBSendWait`(7) trên cap **Notification** trả **`a1 = word`, `mr[0] = word`, `mr[1] = badge`** (trước: `value = length`/`0` và `mr` stale từ `ipc_area` Endpoint). Đỏ trước: `kt62-red-before-notification.log` (kỳ vọng 153, nhận 1); host test `kt62_payload_notification` + TAP 10 QEMU `notif=0x99/0xab/0xcd ok=1` |
| **F2** | tạo **`kt62-host-tests.log`** thật (9 sim + 9 façade); sửa `9 → 8` (nay 9 vì thêm ca F1) và `606/78 → 607/79` |
| **F3** | comment `ktest_user.rs` hết lệch: "mã 5/a0=5" → **19**; `MARKER_BYTES` 51 (26+25, không phải 50) |
| **D2-88(a)** | `DebugGetInfo.num_harts` = `smp::hart_count()` (probe SBI HSM trước bring-up, sau dùng bitmask present) — **không** phải `Machine::hart_count`; `core_bridge::init` truyền số thật vào `Machine::init_zeroed`; `# user core: harts=`/`# user getinfo: num_harts=`/`tap=7 nr=15 … harts=` pin **1/2/4** trên matrix (`kt62-ktest-matrix.log`) |
| **D2-88(b)** | docs/comment: ns = **`rdtime`** (`timer::now_ticks`), SBI TIME chỉ cho ngắt timer |
| **D2-88(c)** | `vifast-gen` sinh **16 hằng `SYS_*`** từ `spec/abi.toml` `[syscall]` (`SYS_CALL`…`SYS_DEBUG_GET_INFO`); core tái xuất, bỏ hardcode; `spec/` **không** đổi |
| **F7** | `resolve_addr`: `aobj::switch_root` trả `false` ⇒ **PANIC + halt** (`PANIC: u-mode resolve_addr khong tra duoc satp user`), không trả lỗi mềm khi `satp` còn root kernel |
| **F4** | ghi rõ **phủ QEMU**: chỉ 12/5-lần-ngoài-dải/15/8/14 chạy thật trên QEMU; `resolve_addr`(9/10), `now_ns`(11), `Switched` mới **biên dịch + host rig/TAP `TestEnv`** — nợ phủ, không gọi "SyscallEnv thật" (`k3b-core.md` §11.2/§11.5) |

**Phạm vi KT-62:** syscall **đi vào core** (tầng 1). **Không** làm: chuyển cap (KT-63), finalise
IPC khi Revoke (KT-63), `TlbFlush`/`SlotPool`/`Bind`/`resume_with_regs` (KT-64), procsrv/shell
(KT-65). Nợ ghi ở `docs/k3b-core.md` §11.5. `insufficientRights` **không** sinh được từ IPC (core
chỉ kiểm quyền ở Retype/Copy/Mint) — TAP nhóm "sai cap" dùng `InvalidCapability`/
`WrongObjectType`/`NotSupported` đúng D2-53 Q11, ghi rõ thay vì giả lập mã không có đường.
