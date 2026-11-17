# KT-77c · **FS + virtio-blk trong ảnh SẢN PHẨM** (D2-123 mục 2)

**Ngày**: 2026-11-17 · **Task**: KT-77c · **Quyết định**: D2-123(2)(3), D2-122(2)(6) (ảnh canonical +
`artifacts/<sha>.bin`), D2-121, D2-08 (không harness trong ảnh sản phẩm), A1-175 (target dir riêng),
QD §3.5 / D2-72 Q5 (trần `.data+.bss`), D2-102 (in hằng thiết kế), D2-116 (QEMU guard + đếm orphan).
**Trạng thái**: **đạt** tiêu chí **1, 3, 5, 6** của KT-77c; **đạt một phần** tiêu chí **2** (chỉ
`ls`/`cat` chuyển `active` — spec **không có** mục `ghi tệp`/`đọc lại sau reboot` riêng, xem §7);
tiêu chí **4** (ma trận + soak) **không** chạy ở đây — D2-123(3) quy định chạy **một lần** trên ảnh
cuối, sau KT-77c.

Mọi số đo dưới đây trích **nguyên văn** từ log trong `v2/agent/JOURNAL/kt77c-logs/` (OD-K6-5 luật
một); văn xuôi dùng số đếm bằng lời để không có số ngoài khối.

## 1. Kết luận ngắn

- Ảnh **sản phẩm** build bằng `v2/tools/repro-build.sh` với feature `product-fs` (= `product-shell`
  + nhãn `block` của crate arch, **không** `ktest`): boot tới `shell>`; **mount được FS** trên
  virtio-blk (`-drive`) **hoặc** trên volume RAM khi không có đĩa; `ls`, `cat`, `write` chạy; **ghi
  rồi reboot trên cùng tệp ảnh đĩa đọc lại đúng byte** ở một/hai/bốn hart.
- **Tái lập byte**: hai lần build ở **hai `CARGO_TARGET_DIR` khác nhau**, cùng một đường dẫn nguồn
  ⇒ **cùng sha**; ảnh **không** feature vẫn đúng baseline `c1072d72` (KT-79/D2-122).
- **Ngưỡng**: ảnh dưới trần phát hành; `.data+.bss` dưới trần; `size_of::<Machine>()` **không đổi**.
- **Ca âm**: (a) không đĩa / đĩa quá nhỏ ⇒ volume RAM + lý do in ra, **không treo**; (b) ghi rồi
  reboot ⇒ byte bền; (c) ảnh **có shell nhưng tắt FS** ⇒ parity `ls`/`cat` **đỏ**.
- **Bẫy feature** (giống KT-77b): `vblk = ["ktest"]` của crate arch ⇒ ảnh sản phẩm bật virtio-blk sẽ
  kéo `probe`/TAP vào. Nguyên nhân **chỉ là nhãn feature** (không tệp driver nào có
  `cfg(feature = "ktest")`) ⇒ sửa bằng `block = []` + `vblk = ["block", "ktest"]`, **không**
  `BLOCKED-CHO-A0`.

## 2. Ảnh canonical, artifact đo, tái lập

Ảnh đo được **copy** vào `artifacts/<sha256>.bin` (chỉ đọc) **trước** mọi phép đo (A1-175b) và mọi
boot dưới đây boot **từ tệp đó**.

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-build-fs-1.log
REPRO machine=Darwin-arm64 root=/Users/mps/dvchd/ViFastOs sha256=64a18ebff24af448a67ddf904cdeb9732091c9a02c8f00aa25cd9e16aa903cb0 size=205440 wall_s=14 build=1e84e92dc17e58ab text=130062 image=/private/tmp/kt77c-final1/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

Lần build **thứ hai**, `CARGO_TARGET_DIR` khác, cùng đường dẫn nguồn ⇒ **cùng sha** (bằng chứng tái
lập của lần build này):

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-build-fs-2.log
REPRO machine=Darwin-arm64 root=/Users/mps/dvchd/ViFastOs sha256=64a18ebff24af448a67ddf904cdeb9732091c9a02c8f00aa25cd9e16aa903cb0 size=205440 wall_s=15 build=1e84e92dc17e58ab text=130062 image=/private/tmp/kt77c-final2/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

**Ca âm tiêu chí 3** — build **không** feature (đường `# k3 halt`) vẫn đúng baseline `c1072d72`:

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-build-baseline.log
REPRO machine=Darwin-arm64 root=/Users/mps/dvchd/ViFastOs sha256=c1072d72d9a50809a2d5f236088b12569b09dc03b0a87d13c113040e9dc4b998 size=82552 wall_s=7 build=2cbe9bbe339d7c02 text=49874 image=/private/tmp/kt77c-base2/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64
```

> **Ghi chú về mốc `c1072d72` và rủi ro tranh chấp**: baseline trên đo **trước** thay đổi của **KT-86**
> (D2-125.3, thêm dòng `# build: rev=<12 hex> dirty=<0|1>` vào `src/main.rs`). KT-86 đã báo trước
> rằng dòng in đó **đổi sha ảnh không feature**; từ lúc đó **`c1072d72` không còn là mốc** cho ảnh
> sản phẩm — mốc mới do KT-86 ghi ở `v2/docs/k6-build-rev.md`. Ảnh `64a18ebf…` của tài liệu này
> cũng **sẽ đổi sha một lần** khi KT-86 land (thêm dòng in) ⇒ lead phải đo lại trên ảnh **cuối**.
>
> **Ghi chú về tái lập giữa hai đường dẫn nguồn**: hai lần build trên dùng **cùng** đường dẫn nguồn
> (đúng yêu cầu của KT-77c). Tái lập **giữa hai đường dẫn nguồn khác nhau** vẫn **hỏng ở phần U-mode**
> vì `build.rs` gọi `$RUSTC` trực tiếp nên ELF U-mode **không** nhận `--remap-path-prefix` của
> `repro-build.sh` (nợ **A1-184** do KT-77b phát hiện; KT-77c **không** sửa — ngoài ngân sách, đã báo
> lead).
>
> **Ghi chú về số dòng và byte-repro**: một dòng `//` thêm vào **giữa** một tệp nguồn bất kỳ làm lệch
> **số dòng** của mọi panic-location phía sau ⇒ `.rodata` đổi ⇒ baseline đổi sha. Đã đo: thêm ba dòng
> vào `src/main.rs` ⇒ `169ff6cb…` (`.rodata` lệch **5 byte**); thêm hai dòng vào `plat/src/lib.rs` ⇒
> vẫn lệch. Cách sửa: hai `#[cfg]` của `plat/src/lib.rs` sửa **tại chỗ** và mọi chú thích giải thích
> đặt ở **cuối tệp**; `mod proc_fs;` cũng khai ở **cuối** `src/main.rs`. Sau đó baseline trở lại
> `c1072d72`. Đây là quy tắc cần cho mọi task sau: **không chèn dòng vào giữa tệp đang link vào ảnh
> không feature**.

## 3. Ngân sách (tiêu chí 4)

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-budget.log
# image=product-fs path=/tmp/kt77c-f1/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64 file=205440 data+bss=223696 budget=262144 stack=65536 guard=4096 trap_stack=16384 syscall_stack=32768 hart_stacks=831488 noload_hart=950272
# data+bss-sections: .data=7832 .bss=215864 
```

`size_of::<Machine>()` **không đổi** (hằng `pin` = trần riêng của core):

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-write-smp1.log
# core: product addr=0x80236000 bytes=189384 pin=196608 image_cap=262144 untyped=65536 retype=1 configure=1 suspend=1 resume=1 state_ready=1 objects=3 caps=1 used=0 check_caps=1 check_bytes=1 check_all_bytes=1 check_sched=1 footprint_ok=1
```

`.bss` tăng so với KT-77b vì `Fs` (≈ 13 KiB) **nằm trong `.bss`**, không trên stack — xem §5. **Không**
nới `MAX_*`/`TOTAL_BLOCKS`/trần nào.

## 4. Hai phương tiện, một giao diện

`src/proc_fs.rs` là **dịch vụ FS phía kernel** của runtime shell sản phẩm (`src/proc_rt.rs`):

| Phương tiện | Khi nào | Bền? | Bằng chứng |
|---|---|---|---|
| `virtio-blk` | QEMU có `-drive … -device virtio-blk-pci` | **bền** qua reboot (cùng tệp đĩa) | §6 |
| `ram` | không có thiết bị, hoặc đĩa quá nhỏ/không mount được | **bay hơi** (`volatile=1`) | §7 |

Quyết định phương tiện **trước** khi mount (`prepare`), rồi format **một lần** nếu superblock không
hợp lệ, rồi mount **một** object `Fs` — mọi lỗi (PCI, thiết bị, format, mount) đều thành **một dòng
log** + reply lỗi cho shell; **không** nhánh nào panic/treo.

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-write-smp1.log
# product fs: medium=virtio-blk blocks=8192 used=2 total=8162 volatile=0 why=ok const:ram_pa=0x83000000 const:ram_blocks=96 const:reply_bytes=24 const:path_max=48
```

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-nodisk-smp1.log
# product fs: medium=ram blocks=96 used=2 total=66 volatile=1 why=khong-co-pci const:ram_pa=0x83000000 const:ram_blocks=96 const:reply_bytes=24 const:path_max=48
```

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-disk-too-small-smp1.log
# product fs: medium=ram blocks=96 used=2 total=66 volatile=1 why=dia-qua-nho const:ram_pa=0x83000000 const:ram_blocks=96 const:reply_bytes=24 const:path_max=48
```

Volume RAM ở PA `0x8300_0000` (ngoài mọi arena của boot) truy cập qua **physmap** bằng hai wrapper
**an toàn** `mmu::read_u64`/`mmu::write_u64` — `src/proc_fs.rs` **không** có `unsafe`.

## 5. `Fs` sống trong `.bss` (không trên stack) — lỗi đã đo và cách sửa

Bản đầu tiên trả `Fs` (≈ 13 KiB) qua bốn tầng (open → open_disk → mount → empty + sret slot) và
**tràn guard page 64 KiB** — đúng lỗi KT-75 đã ghi ở `ktest_persist.rs:915`:

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-crash-and-lsbug-transcript.log
PANIC: guard page fault scause=0xf stval=0x80400000 sepc=0x8021f864 sstatus=0x8000000200006100
```

Cách sửa: thêm **một** module nhỏ ở crate arch — `vifast-plat-riscv64/src/static_slot.rs`
(`StaticSlot<T>`, API **an toàn**, "một lần ghi", trả `&'static mut T`) — và `Fs` nằm trong ô
`static` của `proc_rt`; `proc_fs` chỉ chuyển **tham chiếu**. `unsafe` nằm gọn trong crate arch vì
luật R5 (`v2/bin/v2`) chỉ cho `unsafe` ở `kernel/vifast-plat-*`, `boot/`, `vifast-sync`,
`vifast-objmem` — `vifast-kbin-riscv64` **không** thuộc danh sách.

## 6. Đọc lại **sau reboot** trên cùng ảnh đĩa (tiêu chí 1)

Boot 1 ghi `/p.txt` = `hello77` rồi `fsync` (commit journal + `T_FLUSH`); QEMU thoát; boot 2 là **tiến
trình QEMU mới** trên **cùng tệp đĩa**, **không** ghi gì:

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-write-smp1.log
# product fs: medium=virtio-blk blocks=8192 used=2 total=8162 volatile=0 why=ok const:ram_pa=0x83000000 const:ram_blocks=96 const:reply_bytes=24 const:path_max=48
# shell: write /p.txt bytes=7 sync=ok
# shell: cat ok bytes=7 first=hello77
# shell: ls n=2 path=/
# product fs: medium=virtio-blk ls=34 cat=4 write=2 sync=2 errors=0 replies=56 dev_reads=94 dev_writes=99 dev_flushes=2
```

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-reboot-read-smp1.log
# product fs: medium=virtio-blk blocks=8192 used=3 total=8162 volatile=0 why=ok const:ram_pa=0x83000000 const:ram_blocks=96 const:reply_bytes=24 const:path_max=48
# shell: ls n=2 path=/
# shell: cat ok bytes=7 first=hello77
# shell: cat ok bytes=79 first=ViFastOs
# product fs: medium=virtio-blk ls=34 cat=14 write=0 sync=0 errors=0 replies=60 dev_reads=82 dev_writes=0 dev_flushes=0
```

`write=0`/`dev_writes=0` ở boot 2 là **điều kiện của phép đo**: nếu boot 2 có ghi thì "đọc lại" không
còn là đọc từ đĩa. Tên tệp trong `ls` là `p.txt` (không phải bản ghi lỗi của lần đo đầu — xem §10).

Cùng phép đo ở **hai** và **bốn** hart (hart phụ park, FS chỉ BSP dùng):

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-boot-reboot-read-smp4.log
# smp: boot=2 harts=4 present=0xf online=0xf started=0xb absent=0xf0 timeout=0 dtb=0x87e00000 dtb_magic=1 base=0x80400000 stride=0x1d000
# product fs: medium=virtio-blk blocks=8192 used=3 total=8162 volatile=0 why=ok const:ram_pa=0x83000000 const:ram_blocks=96 const:reply_bytes=24 const:path_max=48
# shell: cat ok bytes=7 first=hello77
```

**Ranh giới bền**: `fsync` = commit journal **rồi** `flush` thiết bị (`virtio_blk::flush` →
`T_FLUSH`); số `dev_flushes=2` ở boot 1 là **hai** lần `write` (mỗi lệnh `write` tự `fsync`), **0** ở
boot 2 (không ghi). Cơ chế "cắt điện" của KT-75 (`pending_blocks > 0` lúc thoát ⇒ cache bay hơi) là
của ảnh test `ktest-persist`; KT-77c **không** dựng lại nó (ngoài ngân sách) — cái được đo ở đây là
**byte trên đĩa host không đổi giữa hai boot** (sha ảnh đĩa trước/sau boot 2 **giống nhau**, in ở
`kt77c-evidence.log`).

## 7. Ca âm (tiêu chí 5) và ca âm harness (tiêu chí 6)

| Ca âm | Cách làm | Kết quả đo |
|---|---|---|
| (a) **không có đĩa** | boot **không** `-drive` | `medium=ram volatile=1 why=khong-co-pci`; `ls`/`cat`/`write` vẫn chạy; **không treo** (shell `exit` rc 0) — `kt77c-boot-nodisk-smp1.log` |
| (a') **đĩa hỏng** | `-drive` trỏ tệp 4 MiB **toàn rác** | superblock sai ⇒ format lại, `medium=virtio-blk`; **không treo** — `kt77c-boot-disk-broken-smp1.log` (`errors=2` là hai lần `cat /p.txt` trên volume vừa format — đúng đường lỗi, không phải sự cố) |
| (a'') **đĩa quá nhỏ** | `-drive` trỏ tệp 4 KiB (< `DATA_START+2` block) | `medium=ram why=dia-qua-nho`; **không treo** — `kt77c-boot-disk-too-small-smp1.log` |
| (b) **ghi rồi reboot** | boot 1 ghi + `fsync`, boot 2 đọc | byte đọc lại **đúng**; sha ảnh đĩa **không đổi** giữa hai boot (boot 2 chỉ đọc) — §6, `kt77c-evidence.log` |
| (c) **tắt feature FS** | build `--features product-shell` (không `product-fs`) | shell **có** nhưng **không** có lệnh FS: parity `ls`/`cat` **đỏ**, `help/echo/ps/kill` **xanh** — `kt77c-parity-neg-nofs.log` |
| (6) **marker harness** | `kt77b-harness-scan.sh` trên ảnh sản phẩm + ca âm `ktest-proc` | sản phẩm **0** hit; ảnh test **25** hit; probe-gate `ktest=0 umode=1` — `kt77c-harness-scan.log` |

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-parity-neg-nofs.log
ok 1 - help (khop=16 >= 2, waited_ms=0)
ok 2 - echo (khop=8 >= 1, waited_ms=0)
ok 3 - ps (khop=8 >= 2, waited_ms=0)
ok 4 - kill (khop=8 >= 1, waited_ms=0)
not ok 5 - ls: byte da vao shell (co 8 dong 'shell: ?') nhung expect khong khop sau 4000 ms (^# shell: ls n=[0-9]+ path=/)
not ok 6 - cat: byte da vao shell (co 16 dong 'shell: ?') nhung expect khong khop sau 4000 ms (^# shell: cat ok bytes=7 first=hello77$)
```

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-harness-scan.log
# kt77b-harness-scan: markers=15 (chuỗi do mã test phát ra; ảnh canonical đã strip)
# scan product: hits=0
ok 1 - anh san pham khong chua ma harness (hits=0)
# marker 'ktest_' hits=2
# marker 'proc_demo' hits=2
# marker '# ktest halt' hits=1
# marker 'not ok' hits=19
# marker 'probe_fault_entry' hits=1
# scan negative: hits=25
ok 2 - ca am: anh co harness bi bat (hits=25 > 0)
# feat product: ktest=0 umode=1
# feat harness: ktest=1
ok 3 - probe gate: cau hinh san pham KHONG bat 'ktest' (probe khong duoc bien dich), co 'umode'
ok 4 - ca am cua probe gate: cau hinh anh test BAT 'ktest' (ktest=1 > 0)
```

## 8. Parity `spec/v1-parity.toml` (tiêu chí 2)

Chỉ **hai** mục đổi `deferred-K7 → active`, `source` giữ nguyên, `send`/`expect` cập nhật theo shell
sản phẩm; **không** thêm/bớt mục nào (28 mục: **6** `active`, **22** `deferred-K7`).

- `ls`: `send='ls\n'`, `expect='^# shell: ls n=[0-9]+ path=/'`.
- `cat`: `send='write /p.txt hello77\ncat /p.txt\n'`, `expect='^# shell: cat ok bytes=7 first=hello77$'`
  — `send` gồm **ghi rồi đọc lại trong cùng boot** vì spec **không có mục riêng** cho "ghi tệp" và
  một mục chỉ một boot nên **không thể** đo "đọc lại sau reboot"; phép đó nằm ở §6 (hai boot).

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-parity-smp1.log
ok 5 - ls (khop=8 >= 1, waited_ms=0)
ok 6 - cat (khop=8 >= 1, waited_ms=0)
# parity: total=28 ok=6 deferred=22 active=6 (active_da_chay=6)
# parity: not_ok=0 timeout=0 khong_co_shell=0 qemu_rc=124
# parity: KET_LUAN=ok (exit 0)
```

`exec`/`runfs`/`run` **giữ** `deferred-K7` (D2-123(2) cho phép): `exec` cần nạp ELF từ FS + `exec`
cho U-mode, chưa có trên ảnh sản phẩm; lý do ghi trong mục tương ứng của spec (không đổi).

## 9. Không hồi quy ảnh test

`shell.elf` **chỉ** có thêm ba lệnh FS khi build với `--cfg shell_fs` (do `build.rs` đặt khi bật
`product-fs`); ảnh test build **không** `product-fs` ⇒ `help cmds=6` và dòng `# shell: up
cmds=help/echo/ps/wait/kill/exit` **giữ nguyên** ⇒ **không** phải sửa kỳ vọng nào của harness
(`proc_demo.rs`, `v2/test/boot/ktest.sh`).

```text scope=v2/agent/JOURNAL/kt77c-logs/kt77c-regress-ktest-proc.log
1..10
ok 1 - procsrv/shell/con load u-mode
ok 2 - procsrv recv fault exit
ok 3 - shell help echo
ok 4 - shell ps tu core
ok 5 - shell wait ma thoat
ok 6 - procsrv kill treo tcb
ok 7 - shell exit va procsrv finalise
ok 8 - u-mode kernel read fault
ok 9 - faultkind exit
ok 10 - faultkind wx from u
PANIC: u-mode fault scause=0xc stval=0x40002000 sepc=0x40002000 sstatus=0x8000000200006000 from_u=1
```

(PANIC cuối là **W^X chủ đích** của TAP 10, không phải sự cố.)

## 10. Lỗi đã gặp trong lúc làm (ghi để không tái diễn)

1. **Tràn guard page** khi `Fs` đi qua nhiều tầng trả-về-theo-giá-trị — §5.
2. **Hỏng tên trong `ls`**: reply đặt `ino` ở `words[1]` rồi OR byte tên vào cùng word ⇒ `p.txt`
   (ino = 3) in thành `s.txt` (`'p' | 3 = 's'`). Sửa: `ino` đi trong byte cao của **word 0**
   (`ino << 24`, ≤ 6 bit). Bằng chứng trước/sau: `kt77c-boot-disk-1.log` (tên sai) vs
   `kt77c-boot-reboot-read-smp1.log` (đúng).
3. **Byte-repro vỡ vì một dòng chú thích** — §2.
4. **Lệnh đầu tiên trong payload có thể mất** (nợ K7 của KT-77b: byte đến khi cửa sổ RX đóng nằm ở
   FIFO): kịch bản bằng chứng lặp mỗi lệnh **hai lần**; parity thì `stdin_wrap.sh` đã bơm tám lần.

## 11. Cái KHÔNG có trong ảnh sản phẩm (trung thực)

- **Không** `exec`/`runfs` trên FS (giữ `deferred-K7`).
- **Không** `mkdir`/`rm`/`sync`/`df`/`fsck` builtin ⇒ các mục đó vẫn `deferred-K7`.
- **Không** dịch vụ FS cho **nhiều** tiến trình: chỉ shell gọi, chỉ **một** `Fs` sống (đúng phạm vi
  "ảnh sản phẩm có shell"); procsrv không giữ FS.
- **Không** chạy ma trận 24 boot + soak 1.000 (D2-123(3): chạy **một lần** trên ảnh cuối, sau KT-77c).
- **Không** sửa nợ A1-184 (`build.rs` không truyền `--remap-path-prefix` cho ELF U-mode).
