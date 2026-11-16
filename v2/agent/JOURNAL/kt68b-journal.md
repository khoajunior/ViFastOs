# KT-68b — vá DRIFT reviewer sau `[KT-68]` (arity chính xác + bằng chứng clippy + 3 nit)

Ngày: 2026-11-16 (Agent 2 dòng OS). Commit gốc: `76b6e22e` `[KT-68]`. Trigger: `reviewer-kt68`
trả **DRIFT nhẹ** sau commit (2 mục chặn + 3 nit); lõi KT-68 đạt (method id sinh từ spec,
`cap_high` high-water, teardown không yếu test cũ, literal đã vá, `run.sh` đúng 1 dòng,
disassembly 0 lệnh đặc quyền). D2-104 đã được lead ghi trong `DECISIONS.md` **trước** khi vá.

## 1. (chặn) Arity invocation là **CHÍNH XÁC** — D2-104

**Lỗi**: `cnode()` chỉ kiểm `len <` tối thiểu; `dispatch` chỉ chặn `len > 4`, nên `Copy len=4`,
`Move len=3/4`, `Delete len=2..4`, `Revoke len=2..4` **vẫn thành công** — trái câu “thiếu/**thừa**
MR ⇒ 5” trong `k6-capalloc.md` và TAP23 (tài liệu/TAP nói sai sự thật).

**Vá** (`kernel/vifast-core/src/invoke.rs`, doc ~346, khối arity ~373–388): kiểm arity **trước**
khi đổi CPTR, theo `spec/[method_signature]` (optional chỉ ở cuối):

| Method | `len` hợp lệ | Thừa ⇒ |
|---|---|---|
| `Copy` | 2 hoặc 3 | 4 ⇒ 5 |
| `Mint` | 2, 3 hoặc 4 | 5 ⇒ 5 (dispatch chặn `>4`) |
| `Move` | 2 | 3/4 ⇒ 5 |
| `Delete` | 1 | 2/3/4 ⇒ 5 |
| `Revoke` | 0 hoặc 1 | 2/3/4 ⇒ 5 |

Mọi ca sai arity: `InvalidArgument` (**5**) + `bump(&DENIED)` (có đếm), **không** chạm máy trạng
thái. Bỏ các check `len <` cũ trong từng nhánh (dead sau khối arity). `Mutate`/`Rotate` vẫn 18
(không phải lỗi arity).

**Test**:
- Host `capalloc_tests.rs` (D2-104 block ~642): 10 ca thừa (`Copy 4; Mint 5; Move 3/4;
  Delete 2/3/4; Revoke 2/3/4`) đều `Err(5)`, kèm chứng minh **không side effect** (EP slot 1
  sống, đích 4 vẫn rỗng). `cargo test -p vifast-core --lib` = **42 passed** (`kt68b-core-tests.log`).
- QEMU TAP23 (`ktest_capa.rs` ~905, `arity_hits` ~934): cùng 10 ca, evidence
  `# capa: am … arity=10 …`, `ok 23` (`kt68b-ktest-matrix.log`). Regex `ktest.sh:538` thêm
  `arity=10` (fail-closed).
- Docs: `k6-capalloc.md` §3 (bảng arity + “kiểm trước CPTR”), §6 TAP23, §7 đỏ-trước;
  `k3b-core.md` §16.2 thêm luật D2-104.

## 2. (chặn) Bằng chứng clippy — chạy lại 4 tổ hợp, log **mã thoát từng tổ hợp**

`JOURNAL/kt68b-clippy-kernel.log` ghi **4/4 `CLIPPY_EXIT=0`** (workspace host; kbin `ktest`;
kbin `ktest,user`; plat-riscv64 `user`) — kể cả khi 0 vẫn ghi mã.

**Trung thực về log cũ**: lần chạy đầu của KT-68, combo `ktest,user` từng **EXIT=101** (3 lint
`collapsible_if`/`let_and_return`/`needless_range_loop` trong `ktest_capa.rs`) — nhưng log thô
**đã bị ghi đè** bởi lần chạy lại cuối nên **không trích được** (chỉ còn câu ghi trong
`kt68-journal.md` §6 và tin nhắn báo lead). **Không** dựng lại log 101 bằng cách tự tạo lỗi
(theo đúng chỉ đạo). Từ nay log clippy giữ **mã thoát từng tổ hợp** ngay trong tệp.

## 3. Nit reviewer (làm luôn, rẻ)

1. **Hai ô debug riêng** (`ktest_capa.rs:61–62, 180, 188, 277–284, 302–303`):
   `LOAD_SLOT_S = 0`, `LOAD_SLOT_C = 3` (trước cùng `3` ⇒ chung `RANGE_SLOTS[3]`); `Task` mang
   `slot` của chính nó, `run_pair` `set_active_slot(task.slot)`. Không đổi TAP (24/24).
2. **`field u is never read`** (`capalloc_tests.rs:71`): bỏ `u: ObjId` khỏi `SelfRig` + literal
   (`CrossRig.u` vẫn dùng cho `check_bytes`). Clippy host sạch.
3. **Trailing whitespace** `kt68-do-truoc-qemu.log`: tệp gốc là **CRLF** (git coi `\r` là
   trailing whitespace) ⇒ chuẩn hoá **LF**, `git diff --check` sạch.

## 4. Số đo sau vá (chép từ log — D2-80/D2-100)

| Hạng mục | Kết quả | Log |
|---|---|---|
| Matrix `ktest.sh 1 matrix` | **12/12 boot sạch** (ktest 65/65, guard 3/3, user **24/24** × smp 1/2/4) | `kt68b-ktest-matrix.log` |
| Gate `--fast` | **PASS 661/87**, L1 wall **925 ms** (cây ấm) | `kt68b-gate.log` |
| Clippy kernel 4 tổ hợp `-D warnings` | **4/4 `CLIPPY_EXIT=0`** | `kt68b-clippy-kernel.log` |
| Core host | **42 passed** | `kt68b-core-tests.log` |
| Ảnh **sản phẩm** | sha256 **`f6318b5d…` không đổi** (arity chỉ ảnh hưởng nhánh `user`/test) ⇒ **soak 0/1000 của KT-68 vẫn áp đúng artifact**, không soak lại | `kt68b-product-sha.log` |

## 5. Delta so `76b6e22e` (file:dòng để confirm)

| # | Tệp | Vị trí | Nội dung |
|---|---|---|---|
| 1 | `kernel/vifast-core/src/invoke.rs` | doc ~346; khối **373–388** | arity chính xác + đếm, bỏ check `len <` từng nhánh |
| 2 | `kernel/vifast-core/src/capalloc_tests.rs` | `SelfRig` ~71–77, ~103–108; block **~642** | bỏ field `u`; 10 ca thừa arity + chứng minh không side effect |
| 3 | `kernel/vifast-kbin-riscv64/src/ktest_capa.rs` | **61–62, 180, 188, 277–284, 302–303**; **905–934**; evidence am | tách `LOAD_SLOT_S/C`, `Task.slot`; 10 ca thừa arity TAP23; `arity={}` |
| 4 | `test/boot/ktest.sh` | **538** | regex `am` thêm `arity=10` |
| 5 | `docs/k6-capalloc.md` | **62–65, 120, 131** | bảng arity chính xác; TAP23; đỏ-trước |
| 6 | `docs/k3b-core.md` | §16.2 (~sau dòng lỗi) | luật arity D2-104 |
| 7 | `agent/JOURNAL/kt68-do-truoc-qemu.log` | toàn tệp | CRLF → LF |
| 8 | `agent/STATE.md` | cuối mục KT-68 | đoạn KT-68b |
| 9 | `agent/JOURNAL/kt68b-*.{md,log}` | mới | journal + log bằng chứng |

## 6. Nợ — không đổi

`CNode.Mutate`/`Rotate` chưa cài (18); không `GetFreeSlot` (quét tuyến tính); `n_caps`
high-water (ID không tái dùng); capalloc 1 hart; `capa_server` là chương trình test (KT-69 IDL
thay); literal `# mem:`/`# stack:` cũ KT-52/67 ngoài phạm vi.
