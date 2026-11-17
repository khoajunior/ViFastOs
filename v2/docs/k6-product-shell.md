# KT-77b · Ảnh **sản phẩm** có U-mode + shell (OD-K6-2)

**Ngày**: 2026-11-17 · **Task**: KT-77b · **Quyết định**: OD-K6-1, OD-K6-2, D2-122 (ảnh canonical),
D2-08 (không harness trong ảnh sản phẩm), D2-119.
**Trạng thái**: **đạt tiêu chí một, ba, năm, sáu** của OD-K6-2; **tiêu chí hai CHƯA ĐẠT** (mới chạy được phần `active`
của `spec/v1-parity.toml`, bốn lệnh) — phần "liệt kê thư mục / đọc tệp / ghi tệp / đọc lại sau
reboot / chạy chương trình" đang là `deferred-K7` **trong spec đã đóng băng**, nên **không** được
tính là đạt trọn tiêu chí hai; **tiêu chí bốn** (ma trận + soak) thuộc bước đo của lead, tài liệu
này chỉ cung cấp log boot một/hai/bốn hart.

Mọi số đo dưới đây nằm trong khối ```` ```text scope=… ```` trích **nguyên văn** từ log đã commit
(OD-K6-5 luật một); chữ trong văn xuôi dùng số đếm bằng lời để không có số ngoài khối.

## 1. Kết luận ngắn

- Ảnh **sản phẩm canonical** build bằng `v2/tools/repro-build.sh` với feature `product-shell`:
  boot tới `shell>` trên QEMU riscv64 ở **một, hai, bốn hart**; dòng `# shell: up` do **shell
  U-mode** in (không phải kernel).
- Kịch bản **bốn lệnh `active`** (`help`, `echo`, `ps`, `kill`) chạy qua **serial ngoài** trên
  ảnh sản phẩm: **bốn trên bốn** ở mỗi cấu hình hart, `KET_LUAN=ok`.
- **Ca âm shell tắt**: build canonical **không** feature ⇒ runner báo `KET_LUAN=khong_co_shell`
  (`exit` năm) — đây là ca âm bắt buộc để tiêu chí một được tính.
- **Ca âm detector**: ảnh sản phẩm **không** có marker harness; ảnh test (`ktest-proc`) bị bắt.
- **Ca âm thứ ba**: gửi byte nhưng `expect` sai ⇒ byte **đã vào shell** (`# shell: ?`) và kịch bản
  đỏ (`exit` ba) — chứng minh kịch bản không phải "xanh sẵn".
- Ngân sách: ảnh canonical **dưới trần**; `size_of::<Machine>()` **không đổi**.
- **Đường U-mode trước KT-77b phụ thuộc feature `ktest`** của crate arch (xem §9): đã tách bằng
  feature `umode` (kernel) + `product-shell` (ảnh sản phẩm); ảnh sản phẩm **không** link module
  harness nào. Đây **không** phải "nhét harness vào ảnh sản phẩm" — hướng ngược lại.

## 2. Ảnh canonical và artifact đo

Ảnh đo được **copy vào `v2/artifacts/<sha256>.bin`** (quyền chỉ đọc, không ghi) rồi boot **từ tệp
đó**; mọi dòng log parity đều in `image_sha256` của tệp đang boot (đón trước D2-122.6b; việc đóng
băng artifact thuộc KT-84).

Hai lần build **độc lập** (hai `CARGO_TARGET_DIR` tạm khác nhau) cho **cùng sha** — đây là bằng
chứng tái lập của lần build này, không thay cho KT-79 (KT-79 mới là task chứng minh **hai** máy):

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-build-shell-canonical.log
REPRO machine=Darwin-arm64 root=/Users/mps/dvchd/ViFastOs sha256=39ca3cc62fcbb7527130592d8001c8b699412f0afd916d2baa30798a3adf4f57 size=176768 wall_s=13 build=6e90c73c43f9b4aa text=106764 image=/private/var/folders/jp/05hgrzxj65bgh355l2zs2zz80000gn/T/vifast-repro-target.2P3GZI/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-build-canonical.log
REPRO machine=Darwin-arm64 root=/Users/mps/dvchd/ViFastOs sha256=39ca3cc62fcbb7527130592d8001c8b699412f0afd916d2baa30798a3adf4f57 size=176768 wall_s=13 build=6e90c73c43f9b4aa text=106764 image=/private/var/folders/jp/05hgrzxj65bgh355l2zs2zz80000gn/T/vifast-repro-target.cwNsB3/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

Ảnh **không shell** của cùng lần build (ca âm tiêu chí ba) vẫn **đúng byte** như baseline KT-79
(`A1-175`: ảnh phát hành trong repo không đổi):

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-build-canonical.log
REPRO machine=Darwin-arm64 root=/Users/mps/dvchd/ViFastOs sha256=c1072d72d9a50809a2d5f236088b12569b09dc03b0a87d13c113040e9dc4b998 size=82552 wall_s=7 build=2cbe9bbe339d7c02 text=49874 image=/private/var/folders/jp/05hgrzxj65bgh355l2zs2zz80000gn/T/vifast-repro-target.POEdTa/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

**Chưa có** ELF đủ symbol cho ảnh có shell: `repro-build.sh` hiện chỉ xuất ảnh đã strip (D2-122
yêu cầu hai tệp cùng lần build) — phần đó thuộc **KT-84**, ghi ở đây để không ai đọc nhầm.

## 3. Đường boot: kernel giao máy cho runtime U-mode

`src/main.rs` giữ **nguyên** các pha K3 (MMU, object layer, core, PLIC/UART IRQ, timer, SMP) rồi:

- in dòng TAP `ok … k3 boot` (dòng bằng chứng cũ, không đổi);
- nếu feature `product-shell` **bật**: in dòng `# k3 shell: handoff …` rồi gọi
  `proc_rt::run(...)` — **không** in `# k3 halt`, vì shell phải sống để nhận kịch bản serial;
- nếu feature **tắt**: giữ **nguyên** `# k3 halt` + `halt(0)` (ca âm; ảnh đúng sha baseline).

`src/proc_rt.rs` (ảnh sản phẩm) dựng **cùng bố cục cspace** với ảnh test nhờ module hằng dùng
chung `src/proc_abi.rs`; ba chương trình U-mode hardcode số endpoint trong ELF của chính chúng
(`init/procsrv_user.rs:29`, `init/shell_user.rs:45`) nên bố cục kernel phải khớp từng slot:

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77a-serial-1-help-serial-custom-smp4.log
# product shell: harts=4 procsrv=0xc shell=0xe victim=0x10 kern=0x12 const:ep_proc=2 const:ep_ps=5 const:ep_events=8 const:slot_cspace=20 input=serial no-inject=1
```

Runtime **không** có script nội bộ, **không** TAP, **không** `captured_*`. Nó **có** phần policy
mà `ps`/`kill` cần (`PS_REQ` → đọc `Machine` → `PS_REPLY`; `EV_KILL` → `Tcb.Suspend(Abort)`;
fault `Exit` → chuyển tiếp sang procsrv) — trước KT-77b phần này **chỉ** nằm trong
`proc_demo.rs` (ảnh test), đúng dạng "user mode phụ thuộc harness" mà OD-K6-1 cảnh báo. Cách xử
lý: tách **policy** sang module **không** phụ thuộc test (ba tệp `proc_abi.rs`, `proc_rt.rs` và
phần service dùng chung) — **không** `#[cfg(feature = "ktest")]` cho phần sản phẩm.

## 4. Byte của người dùng: serial → ngắt RX → ring → shell U-mode

Byte đi **từ ngoài** (stdin QEMU) → PLIC ngắt RX của UART → handler → ring KT-69 → shell U-mode.
Runtime mở **cửa sổ RX ngắn khi đang ở S-mode** rồi đóng trước khi hạ U-mode. Lý do đo được:
trap từ U-mode **không** đổi `satp` về root kernel, mà handler đọc ring qua **physmap** — physmap
(root index trên hai trăm) **không** map trong VSpace user (`copy_kernel_low_root` chỉ chép nửa
thấp), nên ngắt đến đúng lúc đang chạy U-mode sẽ page-fault fatal. Đây là **nợ K7** (§10), không
phải phụ thuộc harness; byte đến khi cửa sổ đóng nằm ở FIFO của UART rồi được vét cạn.

Bằng chứng một boot sạch có `exit` (halt hẳn, không bị timeout): shell exit, procsrv nhận
`Exit`, ring rỗng và hợp lệ:

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-run-exit-smp1.log
# u-mode exit: code=0 faultkind=Exit{0} tcb=14 state=BlockedOnSend
# proc: recv exit pid=14 code=0
# product halt: shell_exit=0 fault_seen=1 events=6 ps_replies=1 ps_n=3 kill_suspend=1 proc_up=1 rx_windows=296 rx_irqs=0 rx_bytes=32 rx_late=32 ring_used=0 ring_head=2 ring_tail=2 ring_ok=1
```

**Đọc đúng `fault_seen`**: đây là **một** fault `FaultKind::Exit{0}` của shell — **không** phải page
fault, không phải W^X. Shell gọi `Exit(0)` (`init/shell_user.rs`, nhánh `exit`), core ghi
`Tcb.last_fault` khi procsrv `NBRecv` fault đã xếp hàng (`v2/kernel/vifast-core/src/core_ipc.rs:319`;
nhánh procsrv đang chờ là `v2/kernel/vifast-core/src/core_ipc.rs:802`), procsrv báo `EV_FAULT` và runtime đếm ở
`v2/kernel/vifast-kbin-riscv64/src/proc_rt.rs:185`. Ca W^X chủ đích **không** có trên ảnh sản
phẩm: nó chỉ nằm trong ảnh test (`v2/kernel/vifast-kbin-riscv64/src/ktest_proc.rs`, TAP cuối) và
kết thúc bằng `PANIC: u-mode fault …` — log ảnh test ở §10.

## 5. Prompt `shell>`

Shell U-mode in `shell>` **thành dòng riêng** sau `# shell: up` và sau mỗi dòng lệnh. Vì sao
không in tiền tố cùng dòng như v1: `expect` của `spec/v1-parity.toml` neo theo dòng
(dạng `^# shell: …$`), nên nếu prompt dính cùng dòng thì kịch bản sẽ trượt oan.

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77a-serial-1-help-serial-custom-smp4.log
# shell: up cmds=help/echo/ps/wait/kill/exit
shell>
# shell: help cmds=6
```

## 6. Kịch bản parity trên ảnh sản phẩm (một/hai/bốn hart)

Runner `v2/test/parity/run.sh --mode serial --image v2/artifacts/<sha>.bin --smp N` (dùng
**nguyên** guard KT-78). Mỗi cấu hình hart: **bốn trên bốn** mục `active`, không còn QEMU sót sau khi script dọn.
Số `khop` lớn hơn `min_count` vì `v2/test/parity/stdin_wrap.sh:32-43` **cố ý** bơm payload lặp
trong cửa sổ ngắn (thiết kế của KT-77a, không phải lỗi của runtime).

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-parity-smp124.log
===== SMP=1 =====
ok 1 - help (khop=16 >= 2, waited_ms=0)
ok 2 - echo (khop=8 >= 1, waited_ms=0)
ok 3 - ps (khop=8 >= 2, waited_ms=0)
ok 4 - kill (khop=8 >= 1, waited_ms=0)
# parity: KET_LUAN=ok (exit 0)
===== SMP=2 =====
ok 1 - help (khop=16 >= 2, waited_ms=0)
ok 2 - echo (khop=8 >= 1, waited_ms=0)
ok 3 - ps (khop=8 >= 2, waited_ms=0)
ok 4 - kill (khop=8 >= 1, waited_ms=0)
# parity: KET_LUAN=ok (exit 0)
===== SMP=4 =====
ok 1 - help (khop=16 >= 2, waited_ms=0)
ok 2 - echo (khop=8 >= 1, waited_ms=0)
ok 3 - ps (khop=8 >= 2, waited_ms=0)
ok 4 - kill (khop=8 >= 1, waited_ms=0)
# parity: KET_LUAN=ok (exit 0)
```

Số hart **thật** trong log boot (không phải hằng trong tài liệu) — khối dưới là dòng của cấu hình
bốn hart; ba hart thứ cấp được `hart_start`:

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-run-exit-smp4.log
# smp: boot=1 harts=4 present=0xf online=0xf started=0xd absent=0xf0 timeout=0 dtb=0x87e00000 dtb_magic=1 base=0x80400000 stride=0x1d000
```

## 7. Ba ca âm

**(a) Shell bị tắt** — đúng ảnh sản phẩm canonical **không** feature (sha baseline KT-79). Mọi
mục `active` đỏ ở bước chờ shell, kết luận "không có shell", `exit` năm. Đây là ca âm bắt buộc
của tiêu chí một.

```text scope-red=v2/agent/JOURNAL/kt77b-logs/kt77b-neg-shell-off.log
not ok 1 - help: khong thay shell trong log (qemu_rc=0)
# parity: KET_LUAN=khong_co_shell (exit 5)
```

**(b) Detector harness** — ảnh sản phẩm **không** có marker harness; ca âm dùng ảnh `ktest-proc`
**canonical, đã strip** (cùng đường remap/strip với ảnh sản phẩm, không phải "bắt nhờ symtab"):

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-harness-scan.log
# scan product: hits=0
ok 1 - anh san pham khong chua ma harness (hits=0)
# scan negative: hits=22
ok 2 - ca am: anh co harness bi bat (hits=22 > 0)
```

**(c) Byte vào shell nhưng `expect` sai** — chứng minh kịch bản **không** xanh sẵn: shell in
`# shell: ?` cho dòng sai, runner phân loại "byte đã vào shell" và đỏ (`exit` ba):

```text scope-red=v2/agent/JOURNAL/kt77b-logs/kt77b-neg-wrong-expect.log
not ok 1 - help: byte da vao shell (co 9 dong 'shell: ?') nhung expect khong khop sau 2000 ms (^# shell: echo KHONG-BAO-GIO$)
# parity: KET_LUAN=not_ok (exit 3)
```

## 8. Ngân sách (giữ nguyên ngưỡng)

Ảnh sản phẩm có shell **dưới trần ảnh**; `.data+.bss` dưới trần QD §3.5; `size_of::<Machine>()`
**không đổi** vì KT-77b không thêm trường vào core (chỉ dùng lại API sẵn có):

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-budget.log
# image=product-shell path=artifacts/39ca3cc62fcbb7527130592d8001c8b699412f0afd916d2baa30798a3adf4f57.bin file=176768 data+bss=203232 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
```

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-run-exit-smp1.log
# core: product addr=0x8022a080 bytes=189384 pin=196608 image_cap=262144 untyped=65536 retype=1 configure=1 suspend=1 resume=1 state_ready=1 objects=3 caps=1 used=0 check_caps=1 check_bytes=1 check_all_bytes=1 check_sched=1 footprint_ok=1
```

## 9. Tách phụ thuộc harness (`ktest` → `umode` + `product-shell`)

**Phát hiện** (đúng loại OD-K6-1 cảnh báo, nhưng ở mức **cổng feature** chứ không phải logic):
trước KT-77b, `v2/kernel/vifast-plat-riscv64/Cargo.toml` khai `user = ["ktest"]`, và bốn hàm
**kernel** mà đường U-mode bắt buộc dùng — `copy_kernel_low_root` (`src/obj.rs:201`),
`switch_root` (`src/obj.rs:231`), `restore_kernel_root` (`src/obj.rs:251`), `set_sum`
(`src/obj.rs:262`) — nằm sau `#[cfg(feature = "ktest")]`. Hệ quả: bật U-mode cho ảnh sản phẩm
sẽ kéo luôn `probe` (mã harness, `src/lib.rs:91`) vào ảnh.

**Cách xử lý** (không nhét harness vào ảnh sản phẩm — ngược lại):
- thêm feature **`umode`** ở crate arch = phần **kernel** mà U-mode cần; `user = ["umode"]`;
  bốn hàm trên đổi sang `#[cfg(any(feature = "ktest", feature = "umode"))]`;
- ảnh sản phẩm bật **`product-shell`** (crate kbin) ⇒ `user` ⇒ `umode`, **không** `ktest`;
- kết quả kiểm bằng máy: ảnh sản phẩm **không** có marker harness (§7b).

**Vì sao không ghi `BLOCKED-CHO-A0`**: phụ thuộc tìm được là **gán nhãn feature sai** cho hàm
kernel (không phải U-mode gọi mã test), sửa được **trong** vùng ghi của task mà **không** thêm
một dòng harness nào vào ảnh sản phẩm. Nếu reviewer/Agent 0 đọc khác, bằng chứng nằm ở hiệu ứng
diff: `obj.rs` chỉ đổi **điều kiện cfg**, `Cargo.toml` chỉ đổi **dây feature**, không đổi ngữ
nghĩa ảnh test (kiểm bằng boot lại `ktest-proc` — §10).

## 10. Ảnh test không hồi quy + phần chưa xong

Ảnh `ktest-proc` build lại **sau** thay đổi (tách hằng số sang `proc_abi`, thêm prompt) vẫn
**mười trên mười** TAP, kết thúc đúng `PANIC` chủ đích của ca W^X:

```text scope=v2/agent/JOURNAL/kt77b-logs/kt77b-regress-ktest-proc.log
ok 3 - shell help echo
ok 7 - shell exit va procsrv finalise
PANIC: u-mode fault scause=0xc stval=0x40002000 sepc=0x40002000 sstatus=0x8000000200006000 from_u=1
```

**Chưa xong / nợ (ghi để lead không đọc nhầm là đã đủ OD-K6-2):**

- **Tiêu chí hai mới đạt phần `active`**: `ls`, `cat` (ghi/đọc tệp), `exec`, và đặc biệt
   **đọc lại tệp sau reboot** cần ảnh đĩa persist + FS **trong ảnh sản phẩm** — hiện `vblk`/FS
   chỉ có trong ảnh test, và `spec/v1-parity.toml` đánh các mục đó `deferred-K7`. Kịch bản
   `active` **không** sửa được (spec đóng băng cho KT-77b).
- **Tiêu chí bốn** (ma trận + soak nghìn lần) chưa chạy trong task này; log ở §6 chỉ là
   một/hai/bốn hart cho ảnh sản phẩm — lead sắp bước đo riêng.
- **Ngắt RX thường trực chưa an toàn khi ở U-mode** (trap từ U-mode không đổi `satp`): nợ K7,
   sẽ đóng bằng cách cho trap từ U-mode khôi phục root kernel **trước** khi dispatch ngắt, rồi bỏ
   cửa sổ S-mode.
- **ELF đủ symbol** của ảnh có shell: chờ **KT-84** (D2-122 yêu cầu hai tệp cùng lần build).
- **Ảnh test đổi `.text`** vì `shell.elf` thêm prompt và `proc_demo` đổi nguồn hằng số; ma trận
   và các log `ktest-*` cũ phải chạy lại (TAP kỳ vọng **không** đổi — §10), việc đó thuộc bước đo
   của lead (vùng `v2/test/boot/**` là của KT-78, task này **không** chạm).
- **`user` không còn kéo `ktest`**: một số mục `pub(crate)` của crate arch (ví dụ
   `user::ranges_save`) chỉ được `exec` dùng, nên khi build `product-shell` trình biên dịch cảnh
   báo "never used". Không phải lỗi; dọn bằng `#[cfg(feature = "exec")]` ở lần chạm tệp sau.

## 11. Tái lập (lệnh có tham số dạng chữ, để tài liệu không chứa số đo viết tay)

```bash
# ảnh sản phẩm CÓ shell (canonical, stripped) — in dòng REPRO sha256/size/build/text
VIFAST_REPRO_CARGO_EXTRA="--features product-shell" bash v2/tools/repro-build.sh
# ca âm shell tắt (ảnh đúng baseline KT-79)
bash v2/tools/repro-build.sh
# copy ảnh đo vào artifacts/<sha256>.bin (chỉ đọc, không ghi) rồi boot từ tệp đó
bash v2/test/parity/run.sh --mode serial --image v2/artifacts/<sha256>.bin --smp N
# ca âm shell tắt / sai expect
bash v2/test/parity/run.sh --mode serial --image v2/artifacts/<sha-cua-anh-khong-shell>.bin --smp N
bash v2/test/parity/run.sh --mode serial --image v2/artifacts/<sha256>.bin --smp N --negative wrong-expect
# detector harness (bắt buộc kèm ca âm)
bash v2/agent/JOURNAL/kt77b-harness-scan.sh \
  --image v2/artifacts/<sha256>.bin --negative v2/artifacts/<sha-anh-test>.bin
```
