# KT-74 — journal (K6: `Tcb.SetTls` + `exec` chain)

Quy ước (D2-112): **mọi số đo nằm trong khối trích nguyên văn** (````text scope=<tệp-log>````), mỗi
dòng đúng **một** tệp; ngoài khối chỉ có định danh (hex, số hiệu spec `x.y`, mã task, ngày,
`file:line`). Không có "haystack chung", không bảng ngoại lệ chứa số đo, log mutation **không** làm
nguồn. Script kiểm: `JOURNAL/kt74-verify-numbers.sh`; tự phá **trên bản copy**:
`JOURNAL/kt74-verify-selfbreak.sh` (luật **F** của D2-113).

## 1. Quyết định

- **`SetTls` = invocation, không thêm syscall**: `spec/abi.toml` `[object].Tcb` **đã có** `SetTls`
  ⇒ chỉ **cài đặt** (tiền lệ D2-97); số method **lấy từ** `vifast_abi::TCB_SETTLS` (sinh tự động)
  — core **không** chép số, **không** sửa `spec/` (D2-99 hết lượt). Bảng `[method_signature]`
  **chưa** có khối `Tcb.SetTls` ⇒ KT-74 dùng arity **một** và ghi **đề xuất** bổ sung chữ ký ở
  mục chữ ký đề xuất trong `docs/k6-exec.md` để chủ `spec/` quyết (không tự thêm gì vào `spec/`).
- **TLS ghi được ở mọi trạng thái sống**, hiệu lực ở lần vào U-mode **kế tiếp** (khác seL4
  `TCB_SetTLSBase` đòi `Inactive`) — vì chuỗi `exec → SetTls → resume` phải chạy được giữa hai bước.
- **`exec` là kernel-mediated**, không phải syscall/method: ABI không có `Tcb.Exec` và task cấm
  thêm syscall ⇒ cơ chế nạp ảnh ở kernel, *policy* chọn ảnh là của procsrv (tiền lệ D2-96). `path`
  là khoá trong **bảng ảnh nhúng** — nợ ABI khi có VFS thật (KT-75+).
- **Giữ `pid`/cspace**: nạp vào **cùng** ô tác vụ và **không** tạo TCB mới ⇒ CSpace/cap/`fault_ep`
  nguyên vẹn; reset `sepc`/`sp`/thanh ghi/TLS; giải phóng VSpace+Frame cũ; **nguyên tử** (mọi
  đường lỗi của loader đi qua `bail!` nên trả lại tài nguyên vừa cấp).
- **Tách tệp cho R1**: `user_tls.rs` (TLS + `exec_frame`), `user_run.rs` (`Stop`/`run_task`),
  `user_ranges.rs` (bảng dải VA) — `user` **tái xuất** tên cũ nên ảnh test không đổi đường dẫn;
  `user.rs` từ vượt trần 1.500 dòng về dưới trần.

## 2. Lỗi **thật** đã bắt (không phải giả thuyết)

- **`tp` = hart id của kernel** (phát hiện lớn nhất). `read_hart_id` đọc `tp`; `sret` xuống U-mode
   nạp **toàn bộ** GPR từ trapframe ⇒ trước KT-74 `tp` của user luôn **không** nên `hart_id()` trong
   handler U-mode đúng **nhờ may**. Khi `SetTls` đặt `tp` khác **không**, `current_hart()` trả rác
   (giá trị `hart` = `tls` bị cắt, xem khối `scope-red` §7.1) và `dispatch_on_private_stack` panic
   `PANIC: syscall stack chua cap` — boot **treo** ngay sau TAP một. Vá: `restore_kernel_tp()` nạp
   lại `tp` từ `USER_KERNEL_CTX` ở đầu handler; `sret` về U-mode tự khôi phục `x4` từ frame.
- **`with_machine` lồng lock = deadlock**: `exec` gọi `core_bridge::with_machine` trong khi caller
   đã giữ lock ⇒ boot treo **không PANIC**, không log. Sửa: `exec` nhận `&mut Machine` từ caller.
- **Loader rò tài nguyên khi lỗi**: trước KT-74, đường lỗi của `load_init_slot` bỏ rơi
   VSpace/Frame vừa cấp — phép đo "không rò" của `exec` sẽ **sai**. Sửa: `bail!` +
   `init_pages::release_with` (trả mọi thứ đã cấp).
- **`entry` của bốn ảnh giống nhau** (`init.ld` link tại `USER_BASE`) ⇒ khẳng định "reset sepc"
   **không** được dựa vào `entry` khác nhau. Sửa: chứng minh bằng VSpace đổi + ảnh mới chạy thật
   (mã thoát riêng) + ảnh khác byte (`img_differ`), và ghi rõ `const:elf_linked_at_base=1`.
- **Test host lỗi thời (đã công bố)**: `capalloc_tests::capalloc_cap_thu_hoi_cap_lai_cung_slot`
   khẳng định bộ đếm **toàn cục** theo trị tuyệt đối ⇒ phụ thuộc thứ tự chạy (đo được: **FAIL một mình**
   trên HEAD sạch, trước mọi thay đổi của KT-74). Đổi sang **delta** của chính ca — chặt hơn.
- **Ngưỡng pin thay đổi do thêm trường** (công bố): `TcbData` thêm **một** `u64` TLS × trần
   `MAX_TCBS` ⇒ `size_of::<Machine>()` tăng đúng **một** `u64` TLS × trần `MAX_TCBS`. Pin và trần **giữ nguyên**; ba
   mẫu evidence trong `test/boot/ktest.sh` cập nhật **cùng lúc** với số đo mới (đo lại, không nới).

## 3. Red-before

- **Host** (`kt74-red-host.log`): bỏ dòng ghi `tls` trong `tcb_set_tls` ⇒
   `set_tls_ghi_va_doc_lai` + `set_tls_khi_ready_van_ghi_duoc` **đỏ**; khôi phục ⇒ md5 tệp trùng.
- **QEMU** (`kt74-red-qemu.log`): bỏ `release_with` trong `exec` ⇒ `frames` **tăng gấp đôi** (khối `scope-red` §7.1),
   `vspaces` **tăng gấp đôi**, TAP `exec giu pid cspace`/`exec chain` **đỏ** — chứng minh
   phép đo "không rò" là **thật**, không phải bộ đếm luôn bằng nhau.

## 4. Kết quả đo (nguyên văn)

```text scope=kt74-qemu-ktest-exec.log
# exec rig: stvec=0x802000a0 untyped=0 core_untyped=1 images=5 cspace=2 tcb=3 tcb_cptr=1 tls_const=0x74310001
# tls: set=0x74310001 observed_first=0 observed=0x74310001 method=15 const:method_source=spec traps=6 stats=2 sys0_calls=2 sys0_err=0 exit=1 exit2=1 tcb=0x3 tcb_tls=0x74310001 ok=1
# tls neg: method_bad=18 arity0=5 arity2=5 cap_untyped=18 cap_ro=3 tls_giu=0x74310001 set_tls_unchanged=1 ok=1
# exec: step=1 pid=0 tcb=3 cspace_same=1 old_vspace=2 new_vspace=14 vspace_changed=1 old_entry=0x40000000 new_entry=0x40000000 const:elf_linked_at_base=1 img_differ=1 img_a=8520 img_b=8496 freed=11 new_pages=11 exit=2 objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 -> objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 same=1 check=1 ok=1
# exec chain: execs=3 pid=0 tcb=3 exits=3,4,1 recycled=33 freed=33 objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 -> objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 frames_same=1 objects_same=1 caps_same=1 total=4 ok=1
# exec atomic: path_err=NotFound elf_err=Elf vspace_same=1 entry_same=1 objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 -> objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 frames_same=1 objects_same=1 caps_same=1 execs_total=4 ok=1
# tls reset: after_exec=0x0 observed=0 exit1=4 set=0xb2 observed2=0xb2 exit2=4 tcb_tls=0xb2 execs_total=5 ok=1
# exec final: c0 objects=17 frames=11 caps=14 vspaces=1 page_tables=3 mappings=11 -> objects=2 frames=0 caps=2 vspaces=0 page_tables=0 mappings=0 releases_clean=1 fail=0 execs_total=5
PANIC: KT-74 tls va exec xong (6 TAP, 0 fail)
```

```text scope=kt74-ktest-matrix.log
# KTEST: 21/21 boot sạch (smp=matrix, NBOOT=1, ảnh/boot=ktest ktest-guard ktest-user ktest-proc ktest-vblk ktest-con ktest-exec)
```

```text scope=kt74-qemu-ktest.log
# core: link addr=0x80253800 bytes=189384 pin=196608 image_cap=262144 parts=29 sum=189384 scalars=295 padding=1 sum_ok=1 in_image=1
```

```text scope=kt74-gate-summary.log
TESTS=726 / SUITES=99
```

```text scope=kt74-l1-thuan.log
--- L1: OK (916 ms, đo bởi cổng)
```

```text scope=kt74-conformance.log
ok - trace-compare: 61/61 dòng khớp, 5/5 ca (22:16 23:19 24:7 25:7 26:12), build=7304a40491224327
```

```text scope=kt74-soak.log
# soak: 1000/1000 lỗi=0 — 0/N chỉ là trần (QD §5.3); không retry (D2-07)
# soak: 0/1000 lỗi (PASS)
```

```text scope=kt74-product-sha.log
fd20227658832a3718b390b0cd7aab563396e23e01ecbd04632f7a9c4393a6da  v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

## 5. Tệp chính

| Nhóm | Tệp |
|---|---|
| Core | `kernel/vifast-core/src/{core_tcb.rs,invoke.rs,tcb.rs,tcbdata.rs,tls_tests.rs}` |
| Arch | `kernel/vifast-plat-riscv64/src/{exec.rs,user.rs,user_tls.rs,user_run.rs,user_ranges.rs,init_pages.rs}` |
| Ảnh test | `kernel/vifast-kbin-riscv64/src/ktest_exec.rs`, `init/exec_{a,b,c,d}_user.rs`, `build.rs`, `Cargo.toml` |
| Runner | `test/boot/{ktest.sh,expected.txt}` |
| Docs | `docs/k6-exec.md` (mới), `docs/k3b-core.md` §22, `agent/{STATE,BLOCKED}.md` |

## 6. Nợ

- **Ảnh sản phẩm U-mode/shell: BLOCKED** (số + `file:line` ở `docs/k6-exec.md` §8, `BLOCKED.md`) —
  nợ D2-113 chuyển nguyên trạng cho KT-76; trần **không** phải lý do.
- ABI `Tcb.Exec(path)` (cần VFS thật); `exec` chưa có ca SMP thật; ca biên `MAX_INIT_PAGES` chưa có.
- **Đề xuất cho chủ `spec/`**: bổ sung khối `[method_signature."Tcb.SetTls"]` (`params = ["tcb","tls"]`,
  `returns = []`) vào `spec/abi.toml` — hiện chữ ký của KT-74 là **đề xuất** (số method thì **đã**
  có trong spec, không tự đặt).

## 7. Chuẩn bị cho reviewer (ba câu hỏi của lead)

### 7.1 Ca âm của `restore_kernel_tp`

Bỏ **đúng dòng** `crate::user_tls::restore_kernel_tp();` trong `on_u_exception` (không sửa gì
khác), build lại ảnh `ktest-exec` và boot: `hart_id()` đọc `tp` **của user** ⇒ panic ngay ở syscall
đầu. Bằng chứng là **giá trị** `hart=`: `tp` = `tls` = `0x7431_0001` bị cắt bởi
`min(MAX_HARTS-1)`; log không tới được bất kỳ `ok` nào. Sau khi khôi phục, md5 `user.rs` trùng
lại và boot lại **đạt** đủ TAP.

```text scope-red=kt74-red-tp.log
# exec rig: stvec=0x802000a0 untyped=0 core_untyped=1 images=5 cspace=2 tcb=3 tcb_cptr=1 tls_const=0x74310001
# stack: chua cap cho hart=7 (init_syscall_stacks)
PANIC: syscall stack chua cap
```

Kết luận: **đã chứng minh** (panic thật, không phải "đã sửa là xong"). Đây cũng là ca âm cho
đường `tp`/hart id nói chung, không chỉ cho `SetTls`.

### 7.2 `+512 B` khớp phép tính — **test**, không phải suy luận

`TcbData` (phần tử của `Machine` theo trần `MAX_TCBS`) thêm **đúng một** `u64` TLS ⇒ tăng
`một u64 × MAX_TCBS`. Test host `pin_machine_tang_dung_8_byte_moi_tcb` khẳng định **đúng** đẳng thức đó
và vẫn dưới pin; không có hằng số nào được nới.

```text scope=kt74-pin-calc.log
# tls pin: size_of_machine=189384 pin_cu=188872 delta=512 size_of_tcbdata=784 max_tcbs=64 8xmax=512 khop=1 duoi_pin=1
```

### 7.3 `capalloc_cap_thu_hoi_cap_lai_cung_slot` hỏng **có sẵn** (không phải do KT-74)

Trên **worktree sạch ở commit nền** (trước mọi thay đổi của KT-74), chạy **một mình** ca đó ⇒
**đỏ**: khẳng định phụ thuộc **bộ đếm toàn cục** (`invoke::stats()` cộng dồn từ mọi test chạy song
song) nên đúng/sai theo **lịch chạy**. Bản sửa của KT-74 đổi sang **delta của chính ca** (vẫn giữ
nguyên khẳng định `Copy/Revoke/Delete`, không xoá assertion nào).

```text scope-red=kt74-red-capalloc-head.log
assertion failed: s.copy >= 4 && s.revoke >= 2 && s.delete >= 2
```

## 8. Vá theo review (D1..D8) — đã chạy lại toàn bộ

- **D1 (verifier miễn số ở prose)**: bản trước đặt cờ `heading = True` ở tiêu đề đầu rồi **không
  reset** ⇒ mọi số nguyên trần sau đó được miễn, tức điều (b) "cấm ngoại lệ chữ số" của D2-112
  **chưa** được thực thi. Nay cờ chỉ đúng **trong dòng tiêu đề**; selfbreak thêm bước **chèn số trần
  ở prose** vào bản copy ⇒ verifier **ĐỎ** đúng thông điệp `số ... ngoài khối scope` (in ra giá trị vừa chèn), rồi khôi phục
  ⇒ xanh. (Bản vá này làm lộ **hàng chục** số prose trước đây bị miễn; đã chuyển hết vào khối hoặc
  viết bằng chữ.)
- **D2 (`hart`)**: số `hart` (= `tls` bị cắt bởi `min(MAX_HARTS-1)`) nay nằm trong khối
  `scope-red=kt74-red-tp.log` ở **cả** `k6-exec.md`, `k3b-core.md`, `STATE.md`, `BLOCKED.md` — không
  còn ở prose.
- **D3 (`exec` nguyên tử với bảng dải VA)**: `load_init_slot` gọi `loading_ranges().reset()`
  **trước** khi ảnh mới chắc nạp được ⇒ lỗi muộn để lại bảng dải rác. Sửa: `exec` **chụp**
  (`user::ranges_save`) trước khi gọi loader và **khôi phục** (`ranges_restore`) khi loader lỗi.
  **Chưa có ca kiểm** cho đường lỗi muộn: muốn vậy phải làm cạn tài nguyên đúng một bậc (VSpace
  xong, PageTable/Frame hỏng) mà `ObjKind::Untyped` **không** được object layer hỗ trợ
  (`v2/kernel/vifast-plat-common/src/obj/mod.rs:720` trả `NotSupported`) nên không dựng được untyped
  nhỏ để ép lỗi; đã thử và **gỡ** ca đó thay vì để TAP đỏ giả. Câu "nguyên tử" trong `k6-exec.md`
  §6 đã **thu hẹp** đúng bằng chứng + ghi nợ KT-76.
- **D5 (`method` không hard-code)**: ảnh A nay lấy method từ **hằng sinh tự động**
  (`vifast_abi::TCB_SETTLS`, build.rs truyền `vifast_abi` cho `exec_a.elf`); evidence đổi
  `const:method=15` → `const:method_source=spec` ⇒ `method` (kernel nhận từ wire) và hằng spec là
  **hai** đường độc lập, không còn so hai bản sao của một giá trị.
- **D6 (assert thay vì nói)**: `ktest_inv` gộp `delta.set_tls` bằng **không** vào `ok18` (in thêm
  `tls_delta`); `releases_clean` đổi `≤` → **`==` không** (sau `release()` không còn Frame/mapping).
- **D7 (câu sai)**: "mọi call-site `load_init_slot` nằm trong `src/ktest_*.rs`" là **sai** —
  `src/proc_demo.rs` cũng gọi (chỉ biên dịch khi bật feature của ảnh test); đã sửa. Kết luận BLOCKED
  của ảnh sản phẩm **không** đổi.
- **D8 (số suy ra không có nguồn trong repo)**: chép **toàn bộ** gate log vào
  `JOURNAL/kt74-gate.log`; `kt74-gate-summary.log` chỉ còn là dòng tổng kết suy ra từ tệp đó.
