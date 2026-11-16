# K6 — IDL tối thiểu + ring buffer chia sẻ (KT-69)

Trạng thái: **KT-69** (K6, D2-95). Sản phẩm: định dạng **IDL tối thiểu** (`.vidl`) + generator
sinh stub `no_std` + **ring 1 producer/1 consumer** trên **một `Frame` chuyển qua cap** + hai
chương trình U-mode **riêng** (`ring_server`, `ring_client`) + TAP 25..29 trên QEMU + test host.
Không thêm syscall, **không sửa `spec/`**, không nới ngưỡng.

Số đo trong tài liệu này **không viết tay**: mọi con số lấy từ log boot do kernel in
(`# idl: …`, `# ring: …`) hoặc từ log cổng — bảng log ở `v2/agent/JOURNAL/kt69-journal.md`.

## 1. Phạm vi

| Tầng | Sở hữu | Trong KT-69 |
|---|---|---|
| `v2/lib/vifast-idl` | định dạng `.vidl` + stub sinh | **mới**: encode/decode `MessageInfo` + MR + reply; method id sinh từ IDL |
| `v2/tools/vifast-gen/gen_idl.py`, `v2/bin/vifast-idl` | generator + gen-check | sinh `generated.rs`; `--check` chạy trong `v2 check`; fixture âm |
| `v2/lib/vifast-ring` | ring SPSC trên Frame | **mới**: header/slot/stats; Acquire/Release; không `unsafe` |
| `vifast-plat-user` (shim U-mode) | truy cập bộ nhớ user | thêm 4 hàm atomic Acquire/Release/Relaxed trên VA user (không lệnh đặc quyền) |
| `ring_server` / `ring_client` U-mode | producer/consumer + giao thức IDL | **mới**: map Frame bằng cap (KT-67b + KT-68), handshake IDL, 40 message/5 vòng wrap |
| `ktest_ring.rs` | đo trên `Machine`/object layer | TAP 25..29 + evidence `# idl:`/`# ring:` |

**Quan hệ với KT-68**: `capa_server`/`capa_client` **giữ nguyên** làm hồi quy capalloc (TAP
20..24); pha ring **không** thay chúng — nó là giao thức IDL thật cho dịch vụ ring. Phần “thay
capa_server bằng IDL” của mô tả task vì vậy được hiểu là: **dịch vụ ring mới** dùng IDL; test
capalloc cũ vẫn tồn tại để chứng minh `CNode.*` không hồi quy (ghi rõ ở đây, không tô hồng).

## 2. Định dạng IDL tối thiểu (`v2/lib/vifast-idl/idl/ring.vidl`)

Cú pháp dòng (một interface/một tệp; `#` là chú thích):

```
interface <tên> version <u32>
method <tên>
  param <tên> <u32|u64>      # tối đa 4 (MR0..MR3)
  cap <tên> <frame|buffer>   # tối đa MỘT cap trên mỗi method
  returns <u32|u64|unit>
```

Interface hiện tại — `ringchan version 1`:

| id | method | tham số vô hướng | cap | trả | dùng ở đâu |
|---|---|---|---|---|---|
| 0 | `attach` | `slots: u32` | `buf: frame` | `u32` | server → client: trao **cap Frame** + hình học ring |
| 1 | `sync` | `phase: u32`, `count: u64` | — | `u32` | client → server: nhịp sau mỗi lô đã tiêu thụ |
| 2 | `finish` | `err: u32`, `sent: u64`, `got: u64` | — | `u32` | client → server: kết thúc + tổng |

### 2.1 Nguồn số method (ghi rõ, không giả vờ)

`spec/abi.toml` là nguồn sự thật của **ABI lõi** (16 syscall/mã lỗi/object/chữ ký method
kernel). Nó **không có** bảng method cho interface dịch vụ mới, và KT-69 **không mở `spec/`**
(D2-99). Vì vậy generator sinh method id **tuần tự từ tệp IDL** (0..N-1 theo thứ tự khai báo) và
ghi chú nguồn ngay trong tệp sinh. Đây là quyết định có chủ đích, không phải “số spec bị thiếu”.

### 2.2 Stub sinh ra

`./v2/bin/vifast-idl gen` sinh `v2/lib/vifast-idl/src/generated.rs`:

- hằng: `IFACE_NAME`, `IFACE_VERSION`, `METHOD_COUNT`, `METHOD_<TÊN>`, `REPLY_BIT` (bit 19 của
  `label` 20 bit), `INFO_CAPS_SHIFT`/`INFO_LENGTH_SHIFT` (đúng `spec/[message_info]`);
- `encode_<method>` (typed) → `(MessageInfo, [u64; 4])`, kèm `caps` count đúng method;
- `decode` **kiểm arity chính xác** (D2-104) và **tràn kiểu u32** ⇒ `None` (không cắt ngầm);
- `encode_<method>_reply`/`decode_<method>_reply` cho kiểu trả; reply dùng `label | REPLY_BIT`;
- **không `unsafe`**, không `ecall`, không crate phụ thuộc → test được trên host.

Gen-check: `./v2/bin/vifast-idl --check` nằm trong `v2 check`; fixture âm chứng minh sửa tay
stub hoặc sửa IDL mà không sinh lại đều bị bắt (`v2/test/gen/run_idl_fixtures.sh`).

## 3. Ring trên một `Frame` chia sẻ

### 3.1 Bố cục (4 KiB)

| Offset | Trường | Ai ghi | Ghi chú |
|---|---|---|---|
| `0x00` | magic `b"KT69RING"` | producer (`init`) | ghi **cuối** (Release) |
| `0x08` | version = 1 | producer | |
| `0x10` | size = 8 | producer | luỹ thừa 2 |
| `0x18` | mask = 7 | producer | |
| `0x20` | slot_bytes = 32 | producer | 4 word/message |
| `0x28` | head | producer | **Release** khi publish |
| `0x30` | tail | consumer | **Release** khi tiêu thụ |
| `0x38` | seq | producer | = head (giữ riêng theo yêu cầu header) |
| `0x80..0x188` | stats 33 trường | hai bên, mỗi bên sở hữu trường của mình | kernel đọc qua physmap |
| `0x200..0x300` | 8 slot × 32 B | producer ghi, consumer đọc | mỗi slot `[seq, seq+1, !seq, 0x69]` |

### 3.2 Bất biến

1. `send` chỉ ghi **một slot** (không copy cả buffer) rồi `store_release(head)`; `recv` đọc
   `head` bằng `load_acquire` **trước** khi đọc slot ⇒ consumer **không** đọc slot đang ghi.
2. `head`/`tail` là bộ đếm một chiều (không reset): slot = `(counter & mask)`; wrap an toàn.
3. `send` khi `head - tail == size` ⇒ **mã 9** (`NotEnoughMemory`, “hết chỗ”), **không** ghi đè,
   **không** chặn. `recv` khi `head == tail` ⇒ **mã 14** (`Busy`, chưa có dữ liệu), **không** chặn.
   Hai mã là mã ABI **có sẵn** — không thêm mã mới, không thêm syscall.
4. Không `unsafe` trong `vifast-ring`: mọi truy cập qua trait `Mem`; U-mode cài bằng 4 hàm atomic
   của shim (Acquire/Release/Relaxed trên VA user — D2-101 cho phép, không CSR/không physmap);
   kernel cài bằng `read_phys_u64`; host test cài bằng `AtomicU64` (kể cả **hai thread thật**).
5. Header `check()` là hàm thuần: magic/version/hình học/`head >= tail`/`used <= size`/
   `seq == head`/payload trong 4 KiB.

### 3.3 Cap + mapping hai chiều

`ring_server.elf` và `ring_client.elf` là **hai tiến trình thật** (2 CNode, 2 TCB, 2 VSpace,
mỗi bên cap ở slot riêng):

1. kernel cấp **một Frame** từ object layer (D2-83) + cap Frame trong CSpace server (slot 5) +
   cap VSpace từng bên (slot 3) + **hai Endpoint một chiều** (server→client, client→server);
2. server `Frame.Map` (method 11.0, số lấy từ `vifast_abi::FRAME_MAP` sinh từ spec) vào VSpace
   server tại `VA_RING = 0x4200_0000`, `Ring::init` 8×32 rồi gửi `attach` **mang cap Frame**
   (send caps header — KT-67b);
3. client nhận message, kernel cài cap vào **receive window** của client (ô 4), client kiểm
   `MessageInfo.caps = 1`, `Frame.Map` **cap nhận được** vào VSpace client tại cùng VA — hai VA
   khác VSpace nhưng **cùng PA** (kernel walk cả hai và đọc physmap);
4. policy chuyển cap là **copy** (KT-63): server vẫn giữ cap nguồn (`src_alive=1`).

### 3.4 Scheduler + “race có kiểm soát”

Kernel đóng vai scheduler tuần tự: mỗi lượt chạy một tác vụ tới `Yield` (trapframe giữ qua từng
lượt). Hai bên **xen kẽ theo bước**: server lấp đầy ring (8) ⇒ `send` thứ 9 trả mã 9; client
`recv` 4 rồi `sync`; server gửi lô 4; lặp tới 40 message. Đây là race **có kiểm soát** (không
song song thật — ghi rõ): consumer chỉ thấy slot đã publish; producer không ghi đè slot chưa
tiêu thụ; `peak_used = 8` chứng minh ring từng đầy khi cả hai còn sống.

`attach`/`sync`/`finish` là **request-response một chiều** (NBSend/NBRecv polling, không
`Call` chặn): server **linger 4 lượt** sau reply `finish` trước `Exit` vì `Exit` của sender
finalise IPC và có thể huỷ message còn trong hàng (D2-25) — nếu thoát ngay, client mất reply
(đã quan sát được ở lần chạy đầu, xem journal).

## 4. TAP 25..29 (QEMU, `ktest-user`)

| TAP | Điều kiện đo (kernel) |
|---|---|
| `ok 25 - idl stub sinh tu vidl` | server/client `Exit(0)`; `attach_msgs=1`, `sync_msgs=10`, `finish_msgs=1`, `sync_acks=10`, `labels_seen=0x7`, `attach_caps=1`; method id đối chiếu hằng `vifast_idl::METHOD_*`; **hình học ring lấy từ header do kernel đọc physmap** (`ring_check`, `header.size/slot_bytes`) — không dùng lời khai của chương trình (N1) |
| `ok 26 - ring day` | `full_err=9`, `full_code=9` (`NotEnoughMemory`), `full_head=8`, `sent>=8`; không chặn |
| `ok 27 - ring rong` | `empty_err=3`, `empty_code_first=14`, `empty_code_last=14` (`Busy`), `empty_tail=0`; **mã thật** lấy từ `Ring::recv` (D1), không in hằng kỳ vọng; không chặn |
| `ok 28 - ring thu tu va wrap` | `sent=recv=40`, `first=0`, `last=39`, `wrap=head/size=5` (công thức D2-106; `head=40`, `size=8` cùng dòng), `order=1`, `hdrerr=dups=lost=bad=0`, `prod_sum=cons_sum=820`, header `check=1` |
| `ok 29 - ring race co kiem soat` | `steps=s12/c12`, `peak=8`, hai VSpace **cùng PA** + cờ `R|W|U`, `installed=1 failed=0`, `caps/ipc/obj/bytes=1` |

Bằng chứng (kernel in từ biến đo — D2-102; hằng thiết kế mang `const:`):

```
# idl: const:iface=ringchan const:version=1 const:methods=3 attach=0 sync=1 finish=2 seen=0x7 attach_msgs=1 ...
# ring: frame=0x8202a000 const:va=0x42000000 size=8 slot=32 head=40 tail=40 seq=40 used=0 check=1 open=1 ...
# ring: sent=40 recv=40 full_err=9 full_code=9 full_name=NotEnoughMemory full_head=8 empty_err=3
       empty_code_first=14 empty_code_last=14 empty_name=Busy empty_tail=0 first=0 last=39
       wrap=head/size=5 const:size=8 head=40 order=1 hdrerr=0 dups=0 lost=0 bad=0 peak=8 const:total=40
# ring: prod_sum=820 cons_sum=820 sum_equal=1 slot0=32/33/18446744073709551583/105 const:sum=820
# ring: map server=1 client=1 same_pa=1 pa=0x8202a000 s_pa=0x8202a000 c_pa=0x8202a000 ...
# ring: caps b4=1 b4_ty=Frame b4_alive=1 frame_obj=44 installed=2->3 failed=1->1 caps=1 ipc=1 obj=1 bytes=1
       steps=s12/c12 exits=0/0 run_steps=28
```

(Log đầy đủ: `v2/agent/JOURNAL/kt69-ktest-matrix.log` + log từng ảnh; runner đòi **đủ** 7 dòng
evidence mới nên sửa stub/đổi số mà không cập nhật là đỏ.)

## 5. Ngưỡng + hồi quy

- `size_of::<Machine>()` **không đổi** (không thêm field core: chỉ dùng foreign object/IDL hằng)
  ⇒ vẫn **188.872 B** ≤ pin **196.608 B** (D2-84/85); ảnh test `ktest-user` `.data+.bss` =
  **202.792 B** ≤ trần **262.144 B** (runner `budget.sh` kiểm từng ảnh).
- Host: `vifast-ring` 2 test lib + 7 test integration (SPSC/đầy/rỗng/wrap/thứ tự/**2 thread thật**);
  `vifast-idl` 5 test (header không `unsafe`, hằng, roundtrip, arity, reply).
- QEMU: matrix **12/12** (4 ảnh × smp 1/2/4), `ktest-user` **29/29** TAP; gate `--fast` PASS;
  soak **0/1000**; clippy kernel 4 tổ hợp `-D warnings` `EXIT=0`; L1 1.000 seed ghi cả wall +
  chạy thuần (D2-100) — số ở journal.
- **D2-106**: `wrap` là giá trị **suy ra** nên evidence ghi công thức + đầu vào cùng dòng
  (`wrap=head/size=5 const:size=8 head=40`); chứng cứ độc lập thứ hai là `slot0=32` (slot 0 bị
  ghi đè lần thứ 5). TAP 28 đòi `head/size >= 5` từ **header đo được**, không dùng cờ tự khai.
- **D2-102 (sau review)**: `EmptyCode` lưu **mã thật** lần rỗng đầu/cuối (D1); delta cap in
  `pre->after` + `b4_ty/b4_alive` (D3); `torn` đổi tên **`hdrerr`** (N3) vì nó đếm lỗi
  header/`recv` bất thường; comment sở hữu stats sửa đúng (N2); const-assert hình học (N5).

## 6. Nợ mở (có chủ, không tô hồng)

1. **Method id của interface dịch vụ không nằm trong `spec/abi.toml`** — sinh từ IDL (mục 2.1).
   Muốn thành ABI lõi phải mở spec theo D2-22; KT-69 không làm.
2. **MR4..MR63 qua IPC buffer** vẫn chưa có (nợ từ KT-67b) — IDL hiện giới hạn 4 scalar.
3. **Chưa song song thật**: hai tiến trình chạy xen kẽ trên **một hart** theo scheduler tuần tự
   của kernel; Acquire/Release đã cài và có test hai thread host, nhưng QEMU chỉ chứng minh
   interleaving có kiểm soát (task ghi rõ “race có kiểm soát”).
4. **Cap move chưa có** (policy copy, KT-63): server vẫn giữ cap Frame nguồn; “trao hết” cần
   `CNode.Delete` ngoài danh sách trắng invocation (nợ KT-67b).
5. Ring capacity/slot_bytes là hằng của dịch vụ (`const:` trong bằng chứng), chưa cấu hình từ
   IDL; mở rộng thuộc KT-70 (pager nhiều client).

## 7. Chạy lại

```bash
cargo test --manifest-path v2/Cargo.toml -p vifast-ring -p vifast-idl   # host: ring + IDL
cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-core --lib    # core (không đổi)
bash v2/test/boot/ktest.sh 1 matrix                                     # 12 boot, TAP 25..29
./v2/bin/vifast-idl --check && bash v2/test/gen/run_idl_fixtures.sh     # gen-check IDL
./v2/bin/v2 gate --fast                                                 # cổng
```
