# KT-86 — nhúng `git rev` + cờ dirty vào ảnh, harness từ chối đo khi lệch (D2-125.3)

**Agent**: 2 dòng OS · **Ngày**: 2026-11-17 · **Ngân sách**: 2 h · **Không tự commit** (KT-85 chặn).
**HEAD lúc bắt đầu**: `712178bce8f077b88570b0ec2963b22d1186828c` (`712178bce8f0`), `main`.
**HEAD lúc kết thúc**: `137e9867cebf…` — lead commit `a6e216ec` (KT-85b) + `137e9867` (A1-185)
trong lúc KT-86 đang chạy. Bằng chứng build ở `712178bc` vì thế **cũng** là một lần chứng minh
`rev-mismatch` thật (§11).
**Trạng thái**: bắt buộc **XONG**; A1-184 (remap cho `$RUSTC` lồng nhau) = **NỢ**, không chặn.

Log bằng chứng: `v2/agent/JOURNAL/kt86-logs/` (chép nguyên từ `/tmp/kt86-logs/`).
Tài liệu: [`v2/docs/k6-build-rev.md`](../docs/k6-build-rev.md).

---

## 0. Tóm tắt một màn hình

| Hạng mục đề bài | Kết quả | Bằng chứng |
|---|---|---|
| 1. Nhúng `rev` (12 hex) + `dirty` (0/1) **tại thời điểm build**, in lúc boot từ `const` (D2-102) | **XONG** | `# build: rev=985a605ec5be dirty=0` (log `boot-fix-clean.log` dòng 68) |
| 2. Harness đọc dòng đó, **TỪ CHỐI ĐO** khi `rev != HEAD` / `dirty=1` / **không có dòng** | **XONG** | `REV-GATE: REFUSE …`, `exit 3`, không một `ok` nào |
| 3. Ba ca âm (a) dirty (b) rev khác HEAD (c) không có dòng | **XONG** | `ca-a-gate.log`, `ca-b-gate.log`, `ca-c-gate.log` |
| 4. Cùng commit ⇒ cùng sha trên **2 `CARGO_TARGET_DIR`** | **XONG** | `same-commit-2-targets.log`: `75751aae…` × 2, 82552 B |
| 5. (tuỳ chọn) `build.rs` truyền `--remap-path-prefix` cho `$RUSTC` lồng nhau (A1-184) | **KHÔNG LÀM — NỢ** (§7) | — |

Ngoài yêu cầu, đã làm thêm **hai** bất biến để cổng không thể "nhận nhầm là sạch":
`tree-dirty` (harness **tự** tính lại dirty) và `image-mismatch` (byte ảnh phải chứa đúng danh
tính của log) — §4.

---

## 1. Định nghĩa (nguồn sự thật duy nhất: `v2/test/boot/build-rev.sh`)

- `rev` = **12 hex đầu** của `git rev-parse HEAD`, đọc **tại thời điểm build**.
- `dirty=1` ⇔ `git status --porcelain --untracked-files=normal` có ≥ 1 dòng cho đường dẫn trong
  **`v2/kernel`** hoặc **`v2/lib`**. Có tính tệp **chưa track** (`proc_fs.rs` của KT-77c là ví dụ
  thật: nó chưa commit nhưng **là** mã của ảnh). Không tính `v2/agent/JOURNAL`, `v2/docs`,
  `v2/test`, `spec/`, `viscript/` — nếu tính thì mỗi lần ghi log sẽ làm cổng **tự khoá**.
- Không có git / không phải repo / `git` lỗi ⇒ `rev=unknown`, `dirty=1` (**fail-closed**).

Ba nơi dùng **cùng** một định nghĩa (không chép lại): `v2/test/boot/build-rev.sh` (harness),
`v2/tools/repro-build.sh` (source tệp trên), `build.rs` (đọc env do `repro-build.sh` đặt; khi
`cargo build` trần thì tự gọi `git` với **cùng** pathspec).

## 2. Cơ chế

```
git rev-parse HEAD (12 hex) ─┐
git status  v2/kernel v2/lib ─┤→ build.rs → cargo:rustc-env=VIFAST_BUILD_REV/_DIRTY
                              │              → const BUILD_LINE (main.rs) → boot: "# build: rev=… dirty=…"
                              └→ repro-build.sh: export VIFAST_BUILD_* + in dòng REPRO rev=… dirty=…
harness: đọc log boot → parse CHẶT → so rev với HEAD → ảnh dirty? → cây dirty? → byte ảnh chứa danh tính?
         (bất kỳ bước nào sai ⇒ REV-GATE: REFUSE reason=…, exit 3, KHÔNG đo)
```

`build.rs` phát thêm `cargo:rerun-if-env-changed=VIFAST_BUILD_REV/_DIRTY`,
`cargo:rerun-if-changed=<git-dir>/HEAD`, `<git-dir>/index`, và từng thư mục crate trong phạm vi
dirty + `v2/lib` (không watch cả `v2/kernel` vì `target/` nằm trong đó ⇒ build script chạy lại mọi
lần). Kể cả khi giá trị **cũ** vì cargo dùng lại build script, cổng vẫn **tự tính lại** rev/dirty
⇒ giá trị cũ chỉ có thể gây **từ chối oan**, không bao giờ gây "nhận nhầm là sạch".

## 3. Bằng chứng — cổng CHẤP NHẬN (cây sạch, rev khớp HEAD)

Cây dùng để đo: **bản sao** `/tmp/kt86/fix` (`git clone --shared` từ repo, **không** phải repo làm
việc — commit ở đó **không** chạm `refs/heads/main` của repo; kiểm sau khi xong: `main` vẫn
`712178bce8f0`). Bản sao + 8 tệp của KT-86 ⇒ commit fixture `R1 = 985a605ec5beff54857e90cd011f2f5ef143e57c`.

```
$ bash v2/tools/repro-build.sh --target-dir /tmp/kt86/fix-t1
REPRO … sha256=75751aaeb850536f603a1f0ca5f5699c7e348de9615993597e0375e9a6aa7617 size=82552
        wall_s=8 rev=985a605ec5be dirty=0 build=5bed09c031a901fd text=49904 …
$ boot ảnh đó
# build: rev=985a605ec5be dirty=0            (dòng 68 của log)
$ bash v2/test/boot/ktest.sh --check-rev boot-fix-clean.log <ảnh>
REV-GATE: ACCEPT rev=985a605ec5be head=985a605ec5beff54857e90cd011f2f5ef143e57c log=…
EXIT=0
```

## 4. Ba ca âm (+ hai ca củng cố) — **log nguyên văn**

### (a) sửa **một dòng** mã kernel, **không** commit ⇒ `reason=dirty`

Sửa đúng một dòng trong `v2/kernel/vifast-kbin-riscv64/src/main.rs` của bản sao (thêm chú thích
trên dòng `static PLATFORM`), không commit; `git diff --stat` = `1 file changed, 1 insertion(+), 1 deletion(-)`
(`ca-a-git-diffstat.log`). Build lại (`fix-t3`, `ca-a-build.log`): `dirty=1` trong cả dòng `REPRO`
lẫn dòng boot.

```
# build: rev=985a605ec5be dirty=1                     ← rev VẪN khớp HEAD, chỉ cờ dirty bật
REV-GATE: REFUSE reason=dirty image_dirty=1 rev=985a605ec5be log=…/boot-fix-dirty.log
EXIT=3
```

⇒ Từ chối **đúng lý do** `dirty=1`, không phải vì rev.

### (b) build ở commit A, HEAD là commit B ⇒ `reason=rev-mismatch`

Commit fixture `R2 = 339b2a390733ed9ea02e27db688852a58c14ed24` (một khoảng trắng trong `main.rs`),
**không** build lại; cổng chạy trên log của ảnh build ở `R1`:

```
REV-GATE: REFUSE reason=rev-mismatch image_rev=985a605ec5be head_rev=339b2a390733 log=…/boot-fix-clean.log
EXIT=3
```

Đối chứng: build lại ở `R2` (cây sạch) ⇒ `46b1155f…`, boot in `# build: rev=339b2a390733 dirty=0`,
cổng **ACCEPT** (`tree-dirty-before.log`). Tức cổng chỉ đỏ vì rev lệch.

### (c) ảnh **không có** dòng `# build:` ⇒ `reason=no-build-line`

Log thật của ảnh build **trước** KT-86 (`/tmp/k3-product-boot-1.log`, ảnh KT-77b):

```
REV-GATE: REFUSE reason=no-build-line log=…/k3-product-boot-1.log
EXIT=3
```

### (c2) dòng có nhưng **méo** ⇒ `reason=malformed`

Cùng log, sửa `dirty=0` → `dirty=2`:
`REV-GATE: REFUSE reason=malformed line=[# build: rev=985a605ec5be dirty=2]` — **exit 3**.
(cùng nhóm với: `rev` sai độ dài, thiếu khoá, có rác thêm.)

### (d) ảnh khai `dirty=0`, **cây đang bẩn** ⇒ `reason=tree-dirty` (bắt lớp lỗi KT-76)

Build ở `R2` **lúc cây sạch** (ảnh nói `rev=339b2a390733 dirty=0`, hoàn toàn "trung thực"), **sau đó**
làm bẩn cây (một dòng, không build lại) ⇒ ảnh không còn đại diện mã hiện tại:

```
REV-GATE: REFUSE reason=tree-dirty root=/private/tmp/kt86/fix rev=339b2a390733
            first=[v2/kernel/vifast-kbin-riscv64/src/main.rs]
EXIT=3
```

Đây là bất biến **quan trọng nhất**: rev của ảnh **khớp** HEAD, cờ của ảnh = 0, nhưng harness
**tự** tính lại và bắt được ⇒ không có đường "ảnh cũ + cây mới" nào lọt.

### (e) log của **ảnh khác** ⇒ `reason=image-mismatch`

Cổng kiểm byte của chính ảnh (`grep -a -F "rev=<rev> dirty=<cờ>"`): log hợp lệ nhưng ảnh truyền
vào không chứa chuỗi đó ⇒ từ chối (không cho mượn log của ảnh khác làm bằng chứng).

## 5. Cổng trong bốn runner — chạy thật, không chỉ đọc mã

`ktest.sh` (matrix): cổng đặt **sau** build+budget, **trước** vòng boot; `soak.sh`: sau build+budget+
BUILD_ID, trước lane; `persist.sh` (chế độ `run`): sau build, trước pha đầu; `run.sh`: trên log boot
**đầu tiên**, trước mọi `ok`. Cổng đỏ ⇒ **`exit 3`** (mã riêng: "TỪ CHỐI ĐO", khác `1` = "đo ra đỏ",
`2` = "lỗi cấu hình").

**`run.sh` (chạy thật, cây bẩn, `run-sh-gate.log`):**

```
# boot: n=1 smp=1 timeout=20s
REV-GATE: REFUSE reason=dirty image_dirty=1 rev=339b2a390733 log=/tmp/k3-product-boot-1.log
# run.sh: TỪ CHỐI ĐO (KT-86/D2-125.3) — không boot nào được tính
EXIT=3
```

⇒ **không** có dòng `ok 1 - boot 1` nào: cổng chặn **trước** phép đo, đúng yêu cầu.

`ktest.sh 1 1` (chạy thật, cây bẩn): xem `ktest-gate.log` — kết quả `REV-GATE: REFUSE …` + exit 3
trước khi vào vòng `(ảnh × smp × boot)`.

## 6. Cùng commit ⇒ cùng sha (2 `CARGO_TARGET_DIR`)

Cùng cây `R1`, hai target dir khác nhau, hai lần build độc lập (`same-commit-2-targets.log`):

| Lần | `CARGO_TARGET_DIR` | sha256 | size | rev | dirty |
|---|---|---|---|---|---|
| 1 | `/tmp/kt86/fix-t1` | `75751aaeb850536f603a1f0ca5f5699c7e348de9615993597e0375e9a6aa7617` | 82552 | `985a605ec5be` | 0 |
| 2 | `/tmp/kt86/fix-t2` | `75751aaeb850536f603a1f0ca5f5699c7e348de9615993597e0375e9a6aa7617` | 82552 | `985a605ec5be` | 0 |

Cùng `build=5bed09c031a901fd text=49904` (D2-80). Từ KT-86, `repro-build.sh` **dừng** nếu byte ảnh
không chứa đúng danh tính vừa tính — chặn ca "cargo dùng lại build script ⇒ ảnh mang rev cũ".

**Baseline đổi (báo trước cho lead/KT-77c)**: so với baseline KT-79/KT-77c `c1072d72…`/82552 B/
`build=2cbe9bbe…`/`text=49874` (KT-77c đo **trước** KT-86), ảnh sản phẩm sau KT-86 là
`build=5bed09c031a901fd`, `text=49904` (**+30 B** `.text`), size **không đổi** 82552 (padding hấp
thụ), sha đổi vì `.rodata` có thêm dòng danh tính. `c1072d72…` từ nay chỉ là mốc **lịch sử**.

Chênh sha giữa hai cây (dirty `6474e228…` ở repo thật vs sạch `75751aae…` ở bản sao) **đúng bằng**
chuỗi danh tính khác nhau (`rev=712178bce8f0 dirty=1` vs `rev=985a605ec5be dirty=0`, cùng độ dài)
— mọi byte khác giữa hai cây không vào ảnh không feature.

## 7. NỢ A1-184 (không chặn) — đã xác định cách sửa, chưa làm

`build.rs` gọi `$RUSTC` trực tiếp cho các ELF U-mode (`build_user_lib`, `build_rlib`,
`build_user_elf`/`build_user_elf_with`); **rustc không đọc `RUSTFLAGS`** (chỉ cargo đọc) ⇒ các lời
gọi lồng nhau **không** nhận `--remap-path-prefix` ⇒ ảnh **có feature `user`** vẫn nhúng đường dẫn
tuyệt đối ⇒ cùng commit, hai đường dẫn nguồn, sha khác.

**Cách sửa** (đã xác định): trong `build.rs`, lấy token `--remap-path-prefix=…` từ
`CARGO_ENCODED_RUSTFLAGS` (cargo cấp cho build script, phân tách bằng `\x1f`) và/hoặc `RUSTFLAGS`,
rồi thêm vào **ba** chỗ gọi `$RUSTC` lồng nhau. Ca kiểm: build `vifast-kbin-ktest-user`
(`--features ktest,user`) ở hai gốc có độ dài khác nhau bằng `repro-build.sh`, đòi cùng sha.

**Vì sao chưa làm**: `build.rs` là tệp **tranh chấp** trong cùng khung giờ — KT-77c thêm
`build_user_elf_with` + `--cfg shell_fs` lúc **13:47**, KT-86 thêm khối danh tính lúc **13:56**.
Sửa **thân** ba hàm đó trong lúc hai task cùng ghi là rủi ro làm mất việc của nhau; đề bài xếp
hạng mục này **tuỳ chọn** ("không làm được ⇒ ghi nợ, **không** chặn"). Đã báo KT-77c (họ xác nhận
**trả** `build.rs`/`main.rs` cho KT-86 lúc 13:58) nên **người làm tiếp có thể sửa ngay**.

## 8. Hệ quả vận hành (phải biết trước khi chạy matrix/soak/persist)

1. **Cây bẩn ⇒ mọi phép đo bị từ chối.** Hiện `v2/kernel` + `v2/lib` đang bẩn (WIP của KT-77c +
   KT-86), nên `matrix`/`soak`/`persist`/`run.sh` **sẽ trả `exit 3`** cho tới khi lead **commit**.
   Đây là **mục đích** của D2-125.3 (chặn bằng máy, không bằng lời dặn — D2-108A), không phải lỗi.
2. **Ảnh cũ (trước KT-86) không đo được nữa** — đúng ý đồ (không chứng minh được nguồn gốc).
3. Baseline sha ảnh sản phẩm **đổi một lần** (§6) — mốc mới phải lấy từ log của lần build trên
   HEAD sạch, **không** chép tay.
4. `persist.sh --red-selftest` **không** qua cổng (đó là ca kiểm của **bộ kiểm**, không phải phép
   đo ngưỡng) — ghi rõ trong tệp.

## 9. Tệp đã chạm (đúng vùng ghi)

| Tệp | Loại |
|---|---|
| `v2/test/boot/build-rev.sh` | **mới** — nguồn sự thật duy nhất (rev/dirty/parse/gate) |
| `v2/docs/k6-build-rev.md` | **mới** — tài liệu (không chứa số đo, xem ghi chú đầu tài liệu) |
| `v2/agent/JOURNAL/kt86-journal.md`, `kt86-boot.sh`, `kt86-logs/**` | **mới** — nhật ký + log bằng chứng |
| `v2/kernel/vifast-kbin-riscv64/build.rs` | **thêm** `build_identity` + hàm phụ cuối tệp; **không** sửa thân hàm nào |
| `v2/kernel/vifast-kbin-riscv64/src/main.rs` | **thêm** `const BUILD_LINE` + **một** dòng `put_str(BUILD_LINE)` |
| `v2/tools/repro-build.sh` | **thêm** export danh tính + in `REPRO rev=… dirty=…` + dừng nếu ảnh không mang danh tính |
| `v2/test/boot/ktest.sh` | **thêm** `kt86_rev_gate`, gọi trước vòng đo, `--check-rev`, `exit 3` |
| `v2/test/boot/soak.sh` | **thêm** cổng (build+boot ảnh sản phẩm một lần) trước lane, `exit 3` |
| `v2/test/boot/persist.sh` | **thêm** `rev_gate` trước pha đầu (chế độ `run`), `exit 3` |
| `v2/test/boot/run.sh` | **thêm** cổng trên log boot đầu tiên, `exit 3` |

**Không** chạm: `v2/tools/git-guard/**`, `.git/hooks/**`, `v2/tools/verify-numbers.py`,
`v2/test/rules/**`, `spec/`, `v2/agent/{STATE,BLOCKED}.md`, `crates/`, `viscript/`,
`proc_rt.rs`/`proc_fs.rs`/`proc_abi.rs` (của KT-77c). Không `#[ignore]`, không đổi ngưỡng, không
nới điều kiện harness.

## 10. Chưa xong / giao lại

- **A1-184** (§7): nợ có chủ đích, cách sửa + ca kiểm đã ghi.
- **Số đo vào tài liệu**: `v2/docs/k6-build-rev.md` **chưa** có khối `scope=` vì log KT-86 **chưa
  commit** (KT-85 chặn); sau khi lead commit, thêm khối `scope=v2/agent/JOURNAL/kt86-logs/…` cho
  các con số (sha/size/build/text).
- **Lead cần**: commit phần KT-86 (4 tệp sửa + 3 tệp mới) **cùng** WIP của KT-77c (hoặc commit
  KT-86 trước, khi đó ảnh sản phẩm vẫn có `proc_fs` chưa track ⇒ `dirty=1` ⇒ **cổng đỏ**, không
  đo được — nên commit đủ cả hai), rồi mới chạy matrix/soak/persist.
- **`v2 check`**: xem `kt86-logs/v2-check.log` (kết quả ở báo cáo cho lead).

---

## 11. Bổ sung: bốn runner, sự cố thật, và HEAD dịch giữa task

### 11.1. Cả bốn runner đều chặn (chạy thật, trên bản sao `/tmp/kt86/fix` đang bẩn)

| Runner | Dòng chứng cứ | Exit |
|---|---|---|
| `ktest.sh 1 1` (matrix) | `REV-STATE: … rev=339b2a390733 dirty=1` → `REV-GATE: REFUSE reason=dirty` → `# ktest.sh: TỪ CHỐI ĐO` | **3** |
| `soak.sh --smoke 1 1` | `REV-GATE: REFUSE reason=dirty` → `# soak.sh: TỪ CHỐI ĐO` | **3** |
| `persist.sh` | `REV-GATE: REFUSE reason=dirty` → `# persist.sh: TỪ CHỐI ĐO` | **3** |
| `run.sh 1 1` | `REV-GATE: REFUSE reason=dirty` → `# run.sh: TỪ CHỐI ĐO`, **không** dòng `ok 1 - boot 1` | **3** |

`ktest.sh` matrix đã build **đủ 7 ảnh test** rồi mới chặn (cổng đặt sau bước neo ảnh, trước vòng
đo) — đúng thứ tự yêu cầu "trước khi chạy matrix".

### 11.2. Sự cố tìm được **nhờ** chạy thật (đã sửa)

Bản vá đầu tiên của tôi đặt `PRODUCT_IMAGE="…/$TARGET/…"` **trước** dòng `TARGET=…` trong
`soak.sh` ⇒ `line 49: TARGET: unbound variable` (vì `set -u`). Chạy `soak.sh --smoke 1 1` mới lộ;
đọc mã không thấy. Đã sửa (đặt sau `TARGET`) và chạy lại ⇒ `SOAK_EXIT=3` với lý do `dirty`.
Đây là lý do vì sao mỗi runner phải được **chạy**, không chỉ được "viết đúng".

### 11.3. `rev-mismatch` trên **lịch sử thật** của repo (không phải fixture)

Ảnh build lúc HEAD `712178bce8f0`; lead commit thêm hai lần trong lúc task chạy ⇒ HEAD
`137e9867cebf`. Cổng trên log của ảnh cũ:

```
REV-GATE: REFUSE reason=rev-mismatch image_rev=712178bce8f0 head_rev=137e9867cebf
EXIT=3
```

⇒ Đây là đúng lớp lỗi KT-79/KT-77b (artifact cũ, mã mới) bị chặn **bằng máy**, không cần ai nhớ.

### 11.4. `./v2/bin/v2 check` — PASS

`v2 check` ở HEAD `137e9867`: `EXIT=0`, **0** dòng `not ok`; toàn bộ 17 bước OK, gồm
`luật máy K6 (KT-80)` (luật 1 quét cả `v2/docs/k6-build-rev.md` mới — **0** lỗi) và `luật cấu
trúc R1..R5` (tệp `v2/test/boot/build-rev.sh` mới có SPDX trong 5 dòng đầu).
Log: `v2/agent/JOURNAL/kt86-logs/v2-check-full.log`.

**Ghi chú minh bạch**: một lần chạy `v2 check` giữa task **đỏ** ở bước luật máy với 8 lỗi
`E1_NUMBER_*` — tất cả thuộc `v2/agent/reports/TRA-LOI-A0-D2-125.md`, tệp của **agent khác** đang
được ghi/xoá cùng lúc (sau đó lead xoá nó ở `f02d46e9`: "moi so do phai vao khoi scope=").
**Không** lỗi nào thuộc tệp của KT-86; lần chạy sau (cùng HEAD) PASS.

### 11.5. Ghi chú vận hành thêm

- Ảnh sản phẩm trong `v2/kernel/target/…/release/` đã bị **build lại** trong lúc chạy thử
  (`run.sh`/`soak.sh`/`persist.sh` tự build vào đó — hành vi sẵn có của runner, không phải việc
  riêng của KT-86). Ảnh mới nhất ở đó: `rev=137e9867cebf dirty=1`, 188680 B.
- Máy có QEMU của agent khác (`qemu_foreign=1` trong `run-sh-gate.log`); chỉ `kill` PID do chính
  script của tôi tạo (qemu_guard), không `pkill` (AGENTS §9).
- Không commit gì vào repo: hai commit fixture (`R1`, `R2`) chỉ nằm trong **bản sao** `/tmp/kt86/fix`
  (`.git` riêng), `refs/heads/main` của repo **không đổi** (kiểm: `git rev-parse main` sau khi xong
  = đúng HEAD do lead đặt, không phải `985a605e`/`339b2a39`).
