# KT-79 · Build tái lập byte (OD-K6-3)

**Ngày**: 2026-11-17 · **Task**: KT-79 · **Quyết định**: OD-K6-3 (D2-119), D2-120 (2 máy)
**Trạng thái**: **đạt một phần** — 2 đường dẫn trên **1 máy** cho cùng sha; **máy thứ hai** (psvn,
Windows) build được nhưng **chưa** cùng sha (xem §6–§7). Không được gọi là "đạt OD-K6-3".

Mọi số trong tài liệu này đều có nguyên văn trong log đã commit ở `v2/agent/JOURNAL/kt79-*.log`
(kiểm bằng `v2/agent/JOURNAL/kt79-verify-numbers.sh`).

## 1. P20 — cùng mã nguồn, ba sha

`D2-118(C)` ghi ba sha từ cùng nguồn: `fd202276…`/188552 B (artifact cây làm việc),
`8f3b02…`/188544 B (clean build của reviewer), `29d7e8…`/188552 B (clean build của agent).
Hai artifact còn tồn tại trong repo/đo lại được:

| Nguồn | Đường dẫn tuyệt đối | sha256 | Bao nhiêu B |
|---|---|---|---|
| artifact cây làm việc (KT-75/76) | `/Users/mps/dvchd/ViFastOs` | `fd202276…` | 188552 |
| dựng lại, đường **ngắn** | `/tmp/kt79/A` | `809c6d25…` | 188552 |
| dựng lại, đường **dài** (+40 ký tự) | `/tmp/kt79/B-BBBB-…` | `255fb99b…` | 188544 |

Chênh **8 B** đúng bằng chênh **độ dài tiền tố đường dẫn** — nhưng đó chỉ là *triệu chứng*; §3 mới
là nguyên nhân (log: `JOURNAL/kt79-machines.log`, `JOURNAL/kt79-baseline-diff.log`).

## 2. Máy và toolchain (ghim)

| Máy | OS | `rustc -V` |
|---|---|---|
| ps | Darwin arm64 | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| psvn | Windows NT 10.0.26300 (Windows 11) | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |

Trước KT-79 `rust-toolchain.toml` để `channel = "stable"`; đo được **psvn có stable = 1.100.0**
còn ps có stable = 1.99.1 ⇒ cùng nhãn, khác mã máy. Nay ghim `channel = "1.99.1"` +
`components = ["rustfmt", "clippy"]` (`rust-toolchain.toml:8`). `llvm-tools` phải có cho toolchain
ghim (budget/.text digest dùng `llvm-size`): cài bằng
`rustup component add llvm-tools --toolchain 1.99.1`.

## 3. Nguyên nhân thật (bằng chứng diff byte)

Dựng lại 2 lần ở 2 đường dẫn, **cùng** target dir riêng, rồi diff ELF
(`JOURNAL/kt79-baseline-diff.log`):

1. `.text` **giống cỡ** (49874 B) nhưng khác **781 B**; `.rodata` khác cỡ **14424 vs 14464**
   (+40 = chênh độ dài tiền tố `/private/tmp/kt79/A` vs `/private/tmp/kt79/B-…`).
2. Trong `.rodata` chỉ có **một** chuỗi đường dẫn tuyệt đối:
   `/private/tmp/kt79/A/v2/lib/vifast-alloc/src/lib.rs` (bản B dài hơn 40 ký tự). Đây là
   **panic location** của crate `v2/lib/vifast-alloc` — crate **nằm ngoài** workspace
   `v2/kernel` nên cargo truyền đường dẫn **tuyệt đối** cho rustc (mọi crate *trong* workspace
   được truyền đường dẫn **tương đối**, ví dụ `vifast-core/src/sched.rs`).
3. `.rodata` đổi cỡ ⇒ vị trí mọi chuỗi sau nó đổi ⇒ mọi cặp `auipc/addi` trỏ tới `.rodata`
   trong `.text` đổi immediate ⇒ `.text` khác. Đây là **lan truyền**, không phải mã khác.
4. `.symtab`/`.strtab` cũng khác: crate ngoài workspace có **metadata hash** do cargo băm từ
   đường dẫn tuyệt đối (`libvifast_alloc-<hash>` khác nhau mỗi chỗ checkout) ⇒ tên symbol mang
   hash đường dẫn.

Hệ quả: **không phải** "do debuginfo" và **cũng không chỉ** "do độ dài đường dẫn" — độ dài chỉ
quyết định **cỡ**; nội dung/tên symbol quyết định **byte**.

Ghi chú kỹ thuật đã đo: `--remap-path-prefix` **không** nhận tiền tố tương đối (`../..`, `..` đều
không khớp), vì rustc so khớp **nguyên văn**; phải truyền tiền tố **tuyệt đối đã canonical**
(`pwd -P`, vì trên macOS `/tmp` là symlink tới `/private/tmp`).

## 4. Biện pháp (đã cài)

`v2/tools/repro-build.sh` (POSIX) và `v2/tools/repro-build-windows.ps1` (Windows) bọc
`cargo build` của ảnh sản phẩm bằng:

1. `--remap-path-prefix=<gốc cây>=/vifast` — che đường dẫn tuyệt đối. Trên Windows (rustc nhúng
   `\`) thì remap **từng thư mục** chứa `.rs` sang dạng `/` chuẩn.
2. `-C strip=symbols` — bỏ `.symtab`/`.strtab` (nguồn metadata hash đường dẫn). **Không** mất khả
   năng đọc `.text` digest: `v2/test/conformance/image_digest.py` vẫn chạy (đo trên bản tái lập:
   `build=2cbe9bbe339d7c02 text=49874`), `budget.sh` vẫn đo (`data+bss=200696`).
3. `CARGO_INCREMENTAL=0`, `SOURCE_DATE_EPOCH=0`, `LC_ALL=C` + toolchain ghim.
4. `RUSTC_WRAPPER=v2/tools/rustc-canon.rs` (POSIX) — chuẩn hoá `\`→`/` và thay `-C metadata=…`
   bằng hằng số. **Bắt buộc**: đo được bản **không** wrapper cho `1924266c…` ở đường ngắn và
   `c1072d72…` ở đường dài (crate khác sở hữu một monomorphization dùng chung, layout `.text`
   đổi); có wrapper thì hai đường dẫn cho **cùng** `c1072d72…`.

Ca âm: `--break`/`--negative` tắt remap (và wrapper) ⇒ sha phải **khác**.

## 5. Kiểm trong cổng

```bash
./v2/bin/v2 check-repro            # 2 đường dẫn tuyệt đối khác nhau => phải cùng sha256
./v2/bin/v2 check-repro --negative # ca âm: phải ĐỎ (exit 1)
```

`check-repro` chép **hai bản sao y hệt** của đầu vào build (`v2/kernel`, `v2/lib`, `v2/tools`,
`v2/test/conformance`, `v2/Cargo.toml`, `v2/Cargo.lock`, `rust-toolchain.toml`, `.cargo/`) vào
một gốc **ngắn** và một gốc **dài**, so **từng byte** hai bản sao (fail-closed nếu khác), rồi
build lạnh từng bản và so sha256. Kết quả máy ps: **PASS**, cùng `sha256=c1072d72…`, `size=82552`
(`JOURNAL/kt79-repro-check.log`). Thời gian 2 build lạnh ~2 phút ⇒ **không** đưa vào `gate --fast`
(chỉ ~60 s); chạy như lệnh riêng/nightly.

## 5.1. A1-175 — ảnh phát hành không bao giờ bị build repro ghi đè

`release/` của repo (`v2/kernel/target/riscv64gc-unknown-none-elf/release/vifast-kbin-riscv64`) là
nơi KT-75/76/78 soak/đo; một build canonical ghi vào đó sẽ làm reviewer không tái lập được byte
(D2-108A tái diễn). Luật trong `v2/tools/repro-*`:

1. **Mọi** build repro dùng `CARGO_TARGET_DIR` **riêng** (mặc định `mktemp -d`); trỏ vào trong cây
   nguồn ⇒ **fail-closed, không build**.
2. `repro-build.sh` ghi sha ảnh phát hành **trước** build và so **sau**; đổi ⇒ dừng, không dùng kết quả.
3. `v2 check-repro` kiểm ở **đầu và cuối**; ca âm: cố tình trỏ `--target-dir` vào repo ⇒ phải bị
   từ chối (in `ca âm A1-175 OK`).
4. Mọi so sánh byte chỉ dùng target dir tạm.

## 6. Máy thứ hai (ps, Linux x86_64) — KẾT QUẢ ĐẠT

Máy đúng theo D2-120.4/A1-173 là **`ps@ps`** (Linux x86_64, 12 core): distro rust 1.93.1 **không
rustup** ⇒ cài rustup **trong `$HOME`** (không sudo, không sửa gói hệ thống; gỡ bằng
`rm -rf ~/.rustup ~/.cargo`) + toolchain 1.99.1 + `rustup target add riscv64gc-unknown-none-elf` +
`llvm-tools`. Log đầy đủ: `JOURNAL/kt79b-ps-setup.log`. Chuyển mã nguồn: bundle HEAD
(`52ed115e…`, sha hai đầu khớp) + overlay 8 tệp delta KT-79 (`kt79b-overlay.tar`,
`JOURNAL/kt79b-ps-setup.log`); sau đó 2 tệp ghim được chuyển vào `v2/` đúng vị trí mới.

Kết quả build canonical (`JOURNAL/kt79b-ps-builds.log`):

| Máy | Đường dẫn tuyệt đối | sha256 | B |
|---|---|---|---|
| Linux ps | `/home/ps/kt79b/repo` | `c1072d72…` | 82552 |
| Linux ps | `/home/ps/kt79/b-very-long-directory-name-for-absolute-path-embedding-0123456789` | `c1072d72…` | 82552 |
| macOS ps | `<tmp>/repro-a` | `c1072d72…` | 82552 |
| macOS ps | `<tmp>/repro-a-very-long-directory-name-for-absolute-path-embedding-0123456789` | `c1072d72…` | 82552 |

⇒ **2 máy × 2 đường dẫn = cùng sha256 `c1072d72…`, cùng 82552 B, cùng `.text` digest
`build=2cbe9bbe339d7c02 text=49874`** ⇒ **đạt đủ OD-K6-3** cho artifact canonical.

**Ghi chú psvn (Windows 11)** — dữ kiện **phụ, đã bị thay thế** (A1-173: giao nhầm máy): psvn build
được với cùng rustc 1.99.1 nhưng thứ tự monomorphization khác (`core::fmt::Display::fmt` do crate
khác sở hữu) ⇒ sha khác; đó là **hạn chế của host Windows**, ghi vào nợ, **không** dùng làm kết
luận OD-K6-3.

## 7. Giới hạn và cách kiểm khi có máy thứ ba

- Đã kiểm: **2 máy × 2 đường dẫn** (macOS + Linux) ⇒ **cùng** sha `c1072d72…`; ca âm đỏ.
- Đã kiểm **không** đạt: Windows host (psvn) cho sha khác — nợ, không dùng kết luận.
- Khi có máy thứ ba: chạy `./v2/bin/v2 check-repro`, so sha với `c1072d72…`.
- Khi có máy thứ ba (cùng OS với ps): chạy `./v2/bin/v2 check-repro` trên máy đó, so dòng `PASS`
  (sha256) với log này; nếu khác, chạy `--keep` và diff `.rodata`/`.text` như §3 trước khi kết luận.
- Ảnh sản phẩm **không đổi** so KT-76: build trơn (không canonical) vẫn ra `fd202276…`/188552
  (`JOURNAL/kt79-plain-sha.log`) ⇒ matrix 21/21 + soak 0/1000 của KT-76 vẫn áp cho đúng artifact
  đó; không cần soak lại. Bản **canonical** (82552) là artifact *khác* (strip) — nếu sau này dùng
  nó làm ảnh phát hành thì phải chạy lại matrix/soak trên chính sha đó (nợ).
