# KT-86 · Nhúng `git rev` + cờ dirty vào ảnh, harness từ chối đo khi lệch (D2-125.3)

**Ngày**: 2026-11-17 · **Task**: KT-86 · **Quyết định**: D2-125.3 (của Agent 0) · **Ngân sách**: 2 h
**Trạng thái**: **đạt** phần bắt buộc (nhúng danh tính + cổng từ chối + ba ca âm + cùng commit ⇒
cùng sha trên hai `CARGO_TARGET_DIR`); **nợ** A1-184 (remap cho `$RUSTC` lồng nhau) — ghi ở §7.

> Tài liệu này **không chứa số đo**: luật máy `v2/tools/verify-numbers.py` (luật 1) chỉ nhận số đo
> nằm trong khối `scope=<log đã commit>`; log của KT-86 đang ở `v2/agent/JOURNAL/kt86-*` (chưa
> commit vì KT-85 chặn commit của agent) nên **chưa** trích được. Toàn bộ số (sha, kích thước,
> mã thoát, số dòng) nằm trong `v2/agent/JOURNAL/kt86-journal.md` + `kt86-*.log`; khi lead commit,
> thêm khối `scope=` tương ứng vào đây.

## 1. Vì sao (lần thứ tư của cùng một lớp lỗi)

Bốn lần artifact lệch mã nguồn mà **không** cổng nào bắt được, đều phát hiện **sau** khi đo:

| Lần | Cách lệch | Vì sao cổng cũ không thấy |
|---|---|---|
| KT-71 | `--features ktest,user` ghi đè `release/` | build id đọc từ ảnh **cũ** còn trên đĩa |
| KT-76 | "build sạch" thực ra là no-op | `cargo` nói "Finished"; ba sha cho cùng nguồn |
| KT-79 | sha phụ thuộc **đường dẫn** | cổng chỉ so sha **giữa hai lần build**, không so với HEAD |
| KT-77b | artifact build từ `proc_rt.rs` **ngắn hơn** bản đã commit | har ven không biết ảnh build từ **mã nào** |

Điểm chung: **ảnh không tự khai nguồn gốc**, nên "đo trên artifact lệch nguồn" chỉ bị phát hiện
bằng mắt người. KT-86 đổi điều đó: ảnh **mang** `git rev` + cờ dirty của lúc build, và harness
**từ chối đo** khi ảnh không khớp HEAD hoặc khi cây nguồn bẩn.

## 2. Định nghĩa `dirty` (một nguồn sự thật)

Nguồn sự thật duy nhất: `v2/test/boot/build-rev.sh` (biến `VBREV_SCOPE`). `build.rs` và
`repro-build.sh` dùng **chính** định nghĩa đó (script `source` tệp này), không chép lại.

`dirty` bằng **một** khi và chỉ khi `git status --porcelain --untracked-files=normal` trả ít nhất
một dòng cho đường dẫn thuộc `v2/kernel` **hoặc** `v2/lib`. Diễn giải từng vế:

- **Có** tính tệp **chưa track**: một tệp nguồn mới trong `v2/kernel` cũng được biên dịch vào ảnh
  (KT-77c: `proc_fs.rs` chưa commit vẫn là mã của ảnh).
- **Không** tính `v2/agent/JOURNAL`, `v2/docs`, `v2/test`, `spec/`, `viscript/`: nếu tính, **mỗi
  lần agent ghi log** sẽ làm `dirty` bằng một ⇒ cổng từ chối **mọi** phép đo ⇒ tự khoá. Đây là lý
  do phạm vi hẹp, không phải nới điều kiện.
- **Không** tính tệp bị `.gitignore` (`target/`, `artifacts/`): git đã lọc, build không làm bẩn.
- Không có `git`, không phải repo, hoặc `git` lỗi ⇒ `dirty` bằng **một** (**fail-closed**).

`rev` = **mười hai** ký tự hex đầu của `git rev-parse HEAD` (đủ để người đọc đối chiếu, ngắn để
một dòng log không bị cắt). So khớp là **so tiền tố** của HEAD, không so chuỗi con.

## 3. Dòng bằng chứng do kernel in

In **ngay đầu boot** (trước mọi `PANIC` có thể xảy ra), từ **`const`** — không phải literal viết
tay (D2-102):

`# build: rev=<hex> dirty=<cờ>`

- Giá trị do `build.rs` đặt qua `cargo:rustc-env` (`VIFAST_BUILD_REV`, `VIFAST_BUILD_DIRTY`), đọc
  **tại thời điểm build**; `src/main.rs` khai `const BUILD_LINE: &str = concat!(…)` rồi
  `PLATFORM.put_str(BUILD_LINE)`.
- Vì là `const`, chuỗi danh tính **nằm trong ảnh** (`.rodata`), nên harness còn kiểm được
  "log này là của **chính** ảnh đang đo" (không tin log rời).
- Ảnh nào in dòng này: **ảnh sản phẩm** (`src/main.rs`). Các ảnh test (`src/ktest*.rs`) là bin
  riêng, **ngoài vùng ghi của KT-86**, nên cổng dùng một lần boot **ảnh sản phẩm** (cùng crate,
  cùng `build.rs`, cùng env danh tính trong cùng phiên build) làm chứng — xem §5.

## 4. Thứ tự kiểm của cổng `vbrev_gate`

`v2/test/boot/build-rev.sh` cài `vbrev_gate <log> <đường-dẫn-trong-repo> [<ảnh>]`. Kiểm **theo thứ
tự**, dừng ở lý do đầu tiên sai; mọi lý do đều in một dòng máy grep được
`REV-GATE: REFUSE reason=<mã> …`; cổng xanh in `REV-GATE: ACCEPT rev=… head=… log=…`.

| Mã lý do | Khi nào | Ý nghĩa |
|---|---|---|
| `no-log` | không thấy tệp log | không có bằng chứng nào |
| `no-build-line` | log **không** có dòng `# build:` | ảnh cũ/ảnh lạ ⇒ **không đo** (fail-closed) |
| `malformed` | dòng sai định dạng (thiếu khoá, `rev` sai độ dài, cờ lạ, có rác) | không nhận bằng chứng méo |
| `no-git` | không đọc được HEAD của cây | không chứng minh được "cùng nguồn" |
| `rev-mismatch` | `rev` nhúng **không** là tiền tố của HEAD | ảnh build từ commit khác (KT-79/KT-77b) |
| `dirty` | ảnh tự khai `dirty` khác không | build từ cây bẩn (KT-71/KT-77b) |
| `tree-dirty` | **harness tự** tính `dirty` khác không | ảnh "nói sạch" mà cây đang bẩn: ảnh không đại diện mã hiện tại |
| `image-mismatch` | byte của ảnh không chứa đúng chuỗi danh tính của log | log của **ảnh khác** bị đem làm bằng chứng |

**Bất biến quan trọng** (vì sao cổng không thể "nhận nhầm là sạch"): giá trị nhúng có thể **cũ**
(nếu cargo dùng lại build script), nhưng cổng **tự tính lại** `rev` và `dirty` từ cây. Một giá trị
cũ chỉ có thể dẫn tới **TỪ CHỐI oan** (an toàn), hoặc bị chính bước `rev-mismatch`/`tree-dirty`
bắt. Không có nhánh nào dẫn tới "chấp nhận ảnh lệch nguồn".

## 5. Cổng đặt ở đâu (bốn runner)

| Runner | Thời điểm cổng | Cổng đỏ |
|---|---|---|
| `ktest.sh` (`matrix`) | sau khi build đủ ảnh test + budget, **trước** vòng boot: build + boot **một** lần ảnh sản phẩm | `exit 3`, **không** slot nào chạy |
| `soak.sh` | sau build + budget + BUILD_ID, **trước** lane nào | `exit 3`, không boot nào được đếm |
| `persist.sh` (chế độ `run`) | sau build, **trước** pha đầu tiên | `exit 3`, không pha nào chạy |
| `run.sh` | trên log của boot **đầu tiên**, trước khi in `ok` nào | `exit 3`, không boot nào được tính |

`exit 3` là mã **mới**, cố ý **khác** `1` ("đo ra đỏ") và `2` ("lỗi cấu hình"): "TỪ CHỐI ĐO"
không được lẫn với "đo được nhưng đỏ". Không đổi ngưỡng, không `#[ignore]`, không nới điều kiện.

`persist.sh --red-selftest` **không** qua cổng: đó là ca kiểm của **bộ kiểm**, không phải phép đo
ngưỡng (ghi rõ trong tệp).

Có thêm chế độ một-log để dùng trong ca kiểm/CI, không build và không boot:

`bash v2/test/boot/ktest.sh --check-rev <log> [<ảnh>]` → `0` đo được, `3` từ chối.

## 6. Ba ca âm (bắt buộc)

Log đầy đủ: `v2/agent/JOURNAL/kt86-*.log`; tóm tắt + mã thoát ở `kt86-journal.md`.

- **(a) cây bẩn**: thêm **một** khoảng trắng vào một `const` của mã kernel, **không** commit,
  build lại ⇒ ảnh in cờ `dirty` khác không ⇒ cổng từ chối với `reason=dirty`.
- **(b) rev khác HEAD**: build ảnh ở commit A, rồi `git checkout`/commit sang B ⇒ ảnh in `rev` của
  A ⇒ cổng từ chối với `reason=rev-mismatch` (không cần build lại — chính điểm mạnh của việc nhúng
  rev vào ảnh).
- **(c) ảnh không có dòng `# build:`**: log của ảnh build **trước** KT-86 (và log bị cắt dòng) ⇒
  cổng từ chối với `reason=no-build-line` — fail-closed, không có nhánh "thiếu bằng chứng thì bỏ qua".

## 7. Cùng commit ⇒ cùng sha, và nợ A1-184

- **Hai `CARGO_TARGET_DIR` khác nhau, cùng cây nguồn** (điều kiện của `v2/tools/repro-build.sh`):
  build hai lần, so sha — log ở journal. Từ KT-86, dòng `REPRO` của `repro-build.sh` mang thêm
  `rev=`/`dirty=`, và script **dừng** nếu byte của ảnh không chứa đúng danh tính vừa tính (chặn ca
  "cargo dùng lại build script ⇒ ảnh mang rev cũ" — đúng lớp lỗi KT-76).
- **Nợ A1-184 (chưa sửa, không chặn)**: `build.rs` gọi `$RUSTC` trực tiếp cho các ELF U-mode nên
  **không** nhận `--remap-path-prefix` mà `repro-build.sh` đặt trong `RUSTFLAGS` (rustc không đọc
  `RUSTFLAGS`; chỉ `cargo` đọc). Hệ quả: ảnh **có** feature `user` vẫn phụ thuộc **đường dẫn**
  tuyệt đối của cây nguồn ⇒ cùng commit, hai đường dẫn, sha khác.
  - Cách sửa (đã xác định, chưa làm): trong `build.rs`, lấy các token `--remap-path-prefix=…` từ
    `CARGO_ENCODED_RUSTFLAGS` (cargo cấp, phân tách bằng ký tự đơn vị) và/hoặc `RUSTFLAGS`, rồi
    thêm vào **ba** chỗ gọi `$RUSTC` lồng nhau (`build_user_lib`, `build_rlib`,
    `build_user_elf`/`build_user_elf_with`).
  - Vì sao chưa làm: `build.rs` là tệp **tranh chấp** với KT-77c trong cùng khung giờ (KT-77c thêm
    `build_user_elf_with` + `--cfg shell_fs` lúc 13:47, KT-86 thêm khối danh tính lúc 13:56);
    sửa **thân** ba hàm đó trong lúc cả hai đang ghi là rủi ro mất việc của nhau, mà hạng mục này
    theo đề bài là **tuỳ chọn** và "không làm được ⇒ ghi nợ, **không** chặn". Ca kiểm bắt buộc của
    nó (cùng commit, hai đường dẫn nguồn, cùng sha) **chưa** có.
  - Ai làm tiếp: thêm tham số `remap: &[String]` cho ba hàm, hoặc biến chúng thành closure trong
    `main`; ca kiểm = build `vifast-kbin-ktest-user` (`--features ktest,user`) ở hai gốc có độ dài
    khác nhau bằng `repro-build.sh`, đòi cùng sha.

## 8. Hệ quả vận hành (đọc trước khi chạy matrix/soak/persist)

1. **Cây bẩn ⇒ không đo được.** Từ KT-86, mọi phép đo của bốn runner trên **từ chối** khi
   `v2/kernel`/`v2/lib` có thay đổi chưa commit. Đây là **mục đích** của task (chặn bằng máy, không
   bằng lời dặn — D2-108A): muốn đo thì **commit** (lead) rồi đo trên HEAD sạch.
2. **Baseline sha của ảnh sản phẩm đổi một lần** so với trước KT-86: thêm một dòng in ⇒ `.text` và
   `.rodata` đổi. Mốc `c1072d72…` của KT-79/KT-77c chỉ còn giá trị **lịch sử** (đo **trước** KT-86);
   các task sau phải lấy sha **mới** từ log của chính lần build trên HEAD sạch.
3. **Ảnh cũ (trước KT-86) không đo được nữa** — đúng ý đồ: nó không chứng minh được nguồn gốc.

## 9. Tệp đã chạm

| Tệp | Thay đổi |
|---|---|
| `v2/test/boot/build-rev.sh` | **mới** — nguồn sự thật duy nhất: định nghĩa `rev`/`dirty`, đọc/parse dòng bằng chứng, `vbrev_gate` |
| `v2/kernel/vifast-kbin-riscv64/build.rs` | **thêm** khối `build_identity` (env + `rustc-env` + `rerun-if-*`) và các hàm phụ ở cuối tệp; **không** sửa thân hàm nào |
| `v2/kernel/vifast-kbin-riscv64/src/main.rs` | **thêm** `const BUILD_LINE` + một dòng `put_str(BUILD_LINE)` đầu boot |
| `v2/tools/repro-build.sh` | **thêm** đọc/`export` danh tính, in ở header + dòng `REPRO`, **dừng** nếu ảnh không mang đúng danh tính |
| `v2/test/boot/{ktest,soak,persist,run}.sh` | **thêm** cổng danh tính + `exit 3`; thêm `--check-rev` cho `ktest.sh` |
| `v2/test/boot/build-rev.sh`, `v2/docs/k6-build-rev.md`, `v2/agent/JOURNAL/kt86-*` | mới |

Không chạm: `v2/tools/git-guard/**`, `.git/hooks/**`, `v2/tools/verify-numbers.py`, `v2/test/rules/**`,
`spec/`, `v2/agent/{STATE,BLOCKED}.md`, `crates/`, `viscript/`, `proc_rt.rs`, `proc_fs.rs`, `proc_abi.rs`.
