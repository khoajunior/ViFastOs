# KT-87 — `v2 check` phải kiểm cả tệp **CHƯA** `git add`

**Ngày**: 2026-11-17 · **Task**: KT-87 · **Dòng**: OS · **HEAD lúc làm**: `2cf554dd`
· **Ngân sách**: 1 h · **Vùng ghi**: `v2/tools/verify-numbers.py`, `v2/bin/v2` (chỉ dòng gọi
`check-rules`), `v2/test/rules/**`, `v2/docs/k6-check-scope.md` (mới), `v2/agent/JOURNAL/kt87-*`.

## 1. Lỗ phải vá

Trước KT-87, tài liệu đích của luật I = tệp **có trong index**. Tệp mới **untracked** không bị
quét ⇒ cổng xanh trên bản nháp, commit xong cổng đỏ:

1. **KT-85**: agent viết tài liệu mới, cổng xanh; phải tự lách bằng cách **stage trong clone tạm**.
2. **KT-86 + KT-77c**: `v2/docs/k6-build-rev.md` + `v2/docs/k6-product-fs.md` commit khi cổng xanh
   (`19b36457` / `8b2a3e3a`) ⇒ commit xong `v2 check` ĐỎ (61 lỗi ở `k6-product-fs.md`), phải sửa
   tay **sau** commit. HEAD đỏ một lúc — đúng thứ Agent 0 cấm.
   (Trong lúc tôi làm, KT-77c đã tự sửa và commit `c6dc418c`: 61 lỗi → 0. **Tôi không chạm** tệp
   đó — ngoài vùng ghi.)

Gốc lỗi: “cổng xanh trước commit” **không** dự đoán được kết quả sau commit, vì cổng chỉ nhìn
index chứ không nhìn **cái sẽ được commit**.

## 2. Đã sửa gì

### `v2/tools/verify-numbers.py` (159 dòng đổi)

| Chỗ | Thay đổi |
|---|---|
| `load_context()` (`:1390`) | tập tệp đích = `git ls-files` (index: tracked+staged) **hợp** `git ls-files -co --exclude-standard` (index + untracked không bị ignore) → `ctx["visible_set"]`. `ctx["untracked_scan"]` quyết định dùng tập nào. |
| `untracked_scan_enabled()` (`:243`) | mặc định **bật**; tắt bằng `V2_CHECK_UNTRACKED` = `false`/`off`/`no`/dạng số không. |
| `head_set()` (`:249`) | tập HEAD (`git ls-tree -r --name-only`), **tính lười** — chỉ chạy khi có lỗi cần in nhãn (đường xanh không trả giá). |
| `file_status()` (`:260`) | `tracked` (trong HEAD) / `staged` (trong index, chưa commit) / `untracked` (ngoài index) / `no-git` (bản copy của luật VIII). |
| `status_tag()` (`:278`) | nhãn in kèm **mỗi** dòng lỗi; `untracked` mang đúng câu “tệp chưa commit — cổng vẫn phải xanh trước khi commit”. |
| `main()` (`:1458`, `:1483`) | cờ `--staged-only`; header in `docs=… tracked=… staged=… untracked=… scan=index+untracked|index-only`; dòng `# note` khi chẩn đoán; mỗi lỗi in `status_tag`. |
| `law1` (`:488`, `:493`) | mục `[[legacy]]`/`pre_existing` trỏ tệp **untracked** ⇒ `E1_LEGACY_UNTRACKED`; tệp **staged** ⇒ `E1_LEGACY_STAGED` (sha chỉ ghim được bản **đã commit**; trước đây báo nhầm `E1_LEGACY_SHA` “nội dung đã đổi”). |
| `law1` note `[[excluded]]` | in thêm `(untracked)`/`(staged)` khi tệp chưa commit — `[[excluded]]` là quyết định **phạm vi**, không ghim sha, nên vẫn áp được nhưng phải **hiện ra**. |

Luật II/III/VII không phải sửa: cả ba đọc `ctx["docs"]`, nên nhìn thấy tệp chưa commit ngay khi
`load_context` đổi. Luật II tự nhiên đúng: báo cáo chưa commit ⇒ `pending` (không có thứ tự để kiểm),
không đỏ oan.

### `v2/test/rules/verify-script-regressions.txt` (+4 ca ⇒ **21**)

```
untracked-doc-skipped | 1 | E1_NUMBER_OUTSIDE | untracked-doc-skipped | …
staged-doc-skipped    | 1 | E1_NUMBER_OUTSIDE | staged-doc-skipped    | …
legacy-untracked      | 1 | E1_LEGACY_UNTRACKED | legacy-untracked    | …
legacy-staged         | 1 | E1_LEGACY_STAGED  | legacy-staged         | …
```

4 hàm phá mới trong verifier: `sabotage_untracked_doc_skipped`, `sabotage_staged_doc_skipped`,
`sabotage_legacy_untracked`, `sabotage_legacy_staged` (`legacy-untracked` ghim **sha của chính bản
nháp** — nếu verifier dùng sha working tree thì ca này XANH, nên nó kiểm đúng “sha chỉ ghim bản đã
commit”).

### `v2/test/rules/numbers-policy.toml`

Comment `[targets]` + `[[legacy]]`: tệp đích = tệp **sẽ được commit**; cờ/biến chẩn đoán; tệp chưa
commit không ghim sha được. **Không** thêm/bớt mục ngoại lệ nào.

### `v2/bin/v2` — **KHÔNG sửa**

`check_k6_rules()` đã chuyển tiếp `"$@"` (`v2/bin/v2:237`), nên `--staged-only` đi qua nguyên vẹn.
Đã kiểm bằng chính wrapper:

```
./v2/bin/v2 check-rules --law 1 --staged-only      ⇒ docs=28 tracked=28 untracked=0 scan=index-only
V2_CHECK_UNTRACKED=false ./v2/bin/v2 check-rules   ⇒ như trên + dòng # note phạm vi
```

Sửa `v2/bin/v2` là **không cần thiết** — bớt một chỗ có thể sai.

### `v2/docs/k6-check-scope.md` (mới)

Cổng nhìn thấy gì (ba trạng thái + ba lệnh git), vì sao, cờ chẩn đoán, ngoại lệ với tệp chưa
commit, **quy trình cho agent**, ca kiểm/ca hồi quy, wall. Tài liệu **tự nó** là tệp untracked khi
viết và đã xanh dưới luật I — minh hoạ sống cho hành vi mới.

## 3. Năm ca kiểm (bắt buộc) — log `v2/agent/JOURNAL/kt87-probe.log`

Chạy: `bash v2/agent/JOURNAL/kt87-probe.sh` → **`ok=28 fail=0 total=28`**, exit 0.
Tệp thử: `v2/docs/k6-kt87-probe.md` (khớp glob `v2/docs/k6-*.md`; xem §6 về tên trong đề bài).
Mọi thao tác `git add`/`git commit` chỉ trong clone tạm `${TMPDIR:-/tmp}/kt87-clone.*`.

| Ca | Kỳ vọng | Kết quả (log) |
|---|---|---|
| (a) tệp thử **untracked** | luật I ĐỎ, nhãn `untracked`, câu “tệp chưa commit — cổng vẫn phải xanh trước khi commit” | `ok 4`–`ok 8`; header `docs=30 tracked=28 untracked=2`; dòng lỗi `k6-kt87-probe.md:3 E1_NUMBER_OUTSIDE [untracked — …]` |
| (b) `git add` trong clone | vẫn ĐỎ, nhãn `staged`, cùng tập lỗi | `ok 14`–`ok 16`; header `docs=30 tracked=28 staged=1 untracked=1` |
| (c) commit trong clone | vẫn ĐỎ, nhãn `tracked`, `HEAD^` clone == HEAD thật | `ok 17`–`ok 19`; header `docs=30 tracked=29` |
| (d) xoá tệp | XANH trở lại (không còn lỗi tệp thử), `git status` sạch, sha cây thật trước == sau | `ok 21`–`ok 28` |
| (e) **cây thật == clone tạm** | cùng tập lỗi `file:dòng:mã` giữa “untracked trên cây thật” và “đã commit trong clone” | `ok 20`; hai tập lỗi in cạnh nhau trong log, giống từng dòng |

Đối chứng thêm trong log: `--staged-only` trên **cùng cây** không thấy tệp thử (`ok 9`) — đúng lỗ cũ
mà KT-87 vá; và `ok 10`: tên `v2/docs/kt87-probe.md` **không** khớp glob đích nên không bị luật I
quét (xem §6).

## 4. Ca hồi quy (luật VIII)

`--self-test`: **17 → 21 ca**, tất cả ĐỎ đúng mã (`ok - luat 8: cases=21`).
Đường phá mới đều là “cổng không thấy tệp sẽ được commit” — đúng lỗ KT-85/KT-86.

## 5. Wall (cây hiện tại)

| Lần chạy | Wall `check-rules` | Ghi chú |
|---|---|---|
| trước KT-87 | ~10,5 s | 17 ca self-test |
| sau KT-87 | 12,6 s … 15,8 s (dao động theo tải máy) | 21 ca (+4); luật 1 0,7–1,0 s · luật 2 1,7–2,2 s · luật 8 9,8–12,7 s |
| `v2 check` toàn bộ (sau KT-87) | 41 s … 47 s | gồm fmt/clippy/R1..R5/ABI/trace/gen-check/build |
| `v2 check` lần cuối (tệp mới còn untracked) | 46,8 s — **PASS** | bước luật máy K6: 15,1 s |

Ba lệnh `git` đọc tập tệp tốn ~0,03/0,10/0,05 s; `git ls-tree` (HEAD) chỉ chạy khi có lỗi cần in
nhãn. Ngưỡng KT-87 là 60 s ⇒ còn dư.

### Cổng cuối trên cây thật (có tệp chưa commit)

Chạy `./v2/bin/v2 check` khi `v2/docs/k6-check-scope.md` **còn untracked** (HEAD `2cf554dd`):

```
# verify-numbers: docs=29 tracked=28 staged=0 untracked=1 scan=index+untracked legacy=22
ok - luat 1 · ok - luat 2 · ok - luat 3 · ok - luat 7 · ok - luat 8: cases=21
v2 check: PASS (L1 chạy trong gate --fast)          # 47 s toàn bộ
```

Đây là điều KT-87 cần: cổng đọc **cả** tệp chưa commit **và** vẫn xanh, vì tài liệu mới thật sự
đúng luật. Nếu tài liệu sai luật, lỗi hiện ra **ngay** với nhãn `[untracked]` (ca (a) của
`kt87-probe.sh`). Không còn cảnh “xanh trước commit, đỏ sau commit”.

## 6. Ghi nhận / quyết định phụ

- **Tên tệp thử trong đề bài**: đề ghi `v2/docs/kt87-probe.md`, nhưng tên đó **không khớp** glob
  tài liệu đích (`v2/agent/reports/*.md`, `v2/docs/k6-*.md`) ⇒ không phải tài liệu đích ⇒ luật I
  không có nghĩa vụ quét, không thể dùng làm ca kiểm ĐỎ. Tôi dùng `v2/docs/k6-kt87-probe.md` và
  **kiểm luôn** ca đối chứng: tên literal không bị quét (`ok 10`). Đây là giới hạn **phạm vi chính
  sách** (glob), không phải lỗi “tệp chưa commit”; đã ghi ở `k6-check-scope.md` §2.
- **`[[excluded]]` + untracked**: giữ là **note**, không phải lỗi. Lý do: `[[excluded]]` là quyết
  định phạm vi có tên + chủ + lý do + ngày trong registry đã commit; biến nó thành lỗi sẽ chặn oan
  việc loại một tài liệu sống khỏi luật I. Không có đường lách **mới**: trước KT-87 tệp untracked
  vốn không bị quét, nên “exclude tệp chưa commit” không thêm quyền gì.
- **Không chạm** `v2/docs/k6-product-fs.md` (KT-77c), `v2/kernel/**`, `spec/`, `v2/test/boot/**`,
  `v2/agent/{STATE,BLOCKED,DECISIONS}.md`, `crates/`, `viscript/`. `v2/agent/STATE.md` trong
  `git status` là thay đổi của **agent khác**, không phải của KT-87.
- **Không** `git add`/`git commit` trên cây thật: `git status` chỉ có tệp của tôi (3 sửa + 4 mới),
  không có gì staged.

## 7. Còn lại (bàn giao)

- `v2/docs/k6-machine-rules.md` §6 vẫn ghi registry có **“mười bảy”** đường phá — nay **hai mươi**.
  Tệp **ngoài vùng ghi** KT-87 (chủ: KT-80/lead) ⇒ cần một dòng cập nhật khi lead chạm tệp đó.
  (Tôi **không** sửa để không vượt vùng ghi.)
- Lead commit giúp: `v2/tools/verify-numbers.py`, `v2/test/rules/*`, `v2/docs/k6-check-scope.md`,
  `v2/agent/JOURNAL/kt87-*` (đúng thứ tự: sửa code → commit → tệp thử không còn tồn tại).
- Sau commit: chạy lại `./v2/bin/v2 check` — kỳ vọng **giống hệt** lần chạy trên cây chưa commit
  (chỉ nhãn `untracked`/`staged` đổi thành `tracked`). Đó chính là điều KT-87 hứa.
