# Phạm vi kiểm của cổng — tệp **chưa commit** (KT-87)

**Ngày**: 2026-11-17 · **Task**: KT-87 · **Luật gốc**: OD-K6-5 luật I (D2-105, D2-109C, D2-112)
· **Sửa ở**: `v2/tools/verify-numbers.py`, `v2/test/rules/{numbers-policy.toml,verify-script-regressions.txt}`.

Cổng `./v2/bin/v2 check` (bước **luật máy K6**) phải nhìn thấy **đúng tập tệp mà lần commit tới sẽ
chứa**, không chỉ tập tệp đã commit. Tài liệu này ghi: cổng nhìn thấy gì, vì sao, cờ chẩn đoán khi
cần tách nhiễu, và **quy trình cho agent** trước khi báo freeze.

## 1. Lỗ đã xảy ra (vì sao phải sửa)

Trước KT-87, tài liệu đích của luật I = tệp **có trong index** (`git add` rồi), nên tệp **untracked**
(bản nháp chỉ nằm trên đĩa) **không bị quét**. Hệ quả đã xảy ra hai lần trong cùng một buổi:

- **KT-85**: agent viết tài liệu mới, cổng xanh; nếu lead commit ngay thì HEAD đỏ. Agent tự phát
  hiện và lách bằng cách **stage trong clone tạm** — tức là đã phải làm đúng việc mà cổng lẽ ra
  phải tự làm.
- **KT-86 + KT-77c**: hai tài liệu mới được commit khi cổng **xanh**; commit xong `v2 check` **ĐỎ**
  (hàng chục lỗi số ở prose). Phải sửa tay **sau khi** commit — HEAD đỏ một lúc, đúng thứ Agent 0
  cấm.

Gốc của lỗ: “cổng xanh” trước commit **không** nói gì về tệp chưa commit, nên nó không dự đoán được
kết quả sau commit. Cổng phải trả lời câu hỏi *“nếu commit đúng cây hiện tại thì cổng có xanh
không?”* — câu trả lời độc lập với việc tệp đã vào index hay chưa.

## 2. Cổng nhìn thấy gì

Ba tập tệp, phân biệt bằng git (một lệnh cho mỗi tập):

| Trạng thái | Định nghĩa | Nhãn in kèm lỗi |
|---|---|---|
| `tracked` | có trong HEAD (đã commit, đã vào lịch sử) | `[tracked]` |
| `staged` | có trong index nhưng **chưa** commit (`git add` rồi) | `[staged — tệp đã git add, chưa commit]` |
| `untracked` | chưa vào index (bản nháp trên đĩa), **không** bị ignore | `[untracked — tệp chưa commit — cổng vẫn phải xanh trước khi commit]` |

Tài liệu đích (khớp `[targets].globs` của `v2/test/rules/numbers-policy.toml`) được lấy bằng **hợp**
của:

```text
git ls-files                              # index: tracked + staged
git ls-files -co --exclude-standard       # index + untracked (KHÔNG lấy tệp bị ignore)
```

Tệp bị ignore **không** thuộc phạm vi — chúng không thể thành commit, nên không phải “bản nháp sẽ
vào lịch sử”. Gốc kiểm không phải repo git (bản copy của luật VIII) thì không lọc: mọi tệp khớp glob
đều được quét, nhãn là `[no-git]`, đúng như trước KT-87.

Mỗi dòng lỗi in **thêm** trạng thái tệp, ví dụ (đường dẫn + dòng + mã + nhãn + lý do):

```text
not ok - luat I: v2/docs/k6-thu.md:3: E1_NUMBER_OUTSIDE [untracked — tệp chưa commit — cổng vẫn phải xanh trước khi commit] số '<số đo>' ngoài khối `scope=`
```

Đường dẫn tệp trong thông điệp **luôn** tương đối với gốc repo, nên hai cây (thật và clone tạm) cho
cùng chuỗi — chỉ nhãn trạng thái được phép khác nhau.

**Điều cổng KHÔNG làm**: tài liệu đích vẫn xác định bằng glob. Tệp mới đặt ngoài glob (ví dụ
`v2/docs/ghi-chu.md` — không khớp `v2/docs/k6-*.md`) **không** bị luật I quét, dù đã commit hay
chưa. Đó là quyết định **phạm vi chính sách** (không phải “tệp chưa commit”): muốn tài liệu được
kiểm thì đặt tên theo glob đích, hoặc mở quyết định đổi `[targets]`.

## 3. Cờ chẩn đoán (cổng mặc định **bật** kiểm untracked)

Khi cần tách nhiễu (ví dụ đang có bản nháp của task khác trên cây), chạy lại ở chế độ **chỉ index**
— đúng hành vi trước KT-87:

```bash
./v2/bin/v2 check-rules --staged-only        # chỉ luật máy K6, tập index
V2_CHECK_UNTRACKED=false ./v2/bin/v2 check   # cả cổng, tập index
```

- `--staged-only` chỉ có nghĩa **chẩn đoán**; nó in thêm dòng `# note - phạm vi quét: CHỈ index` để
  không ai nhầm kết quả chẩn đoán với kết quả cổng.
- Biến môi trường nhận các giá trị tắt: `false`, `off`, `no` (và dạng số không). Mọi giá trị khác
  — kể cả không đặt — đều **bật**.
- **Cổng thật** (`v2 check`, `v2 gate --fast`) gọi `check-rules` **không** kèm cờ ⇒ luôn bật kiểm
  untracked. Không có đường “tắt cho xanh”: tắt là tự khai báo đang chẩn đoán.

## 4. Ngoại lệ với tệp chưa commit

Ba cơ chế ngoại lệ trong `numbers-policy.toml`:

- **`[[legacy]]` / `kind = "pre_existing"`** ghim `sha256` của bản **đã commit** (bản con người đã
  đọc và duyệt). Tệp **chưa commit** không có sha HEAD để khớp ⇒ cổng **ĐỎ rõ**:
  `E1_LEGACY_UNTRACKED` (untracked) hoặc `E1_LEGACY_STAGED` (mới `git add`, chưa commit). Thông
  điệp nói thẳng: *“không ngoại lệ cho tệp chưa commit”*. Không có đường “thêm `[[legacy]]` + sha
  của bản nháp để miễn luật I” — nếu có, ai cũng lách được bằng một dòng registry.
- **`[[excluded]]`** là quyết định **phạm vi** (nêu tên + chủ + lý do + ngày), **không** ghim sha,
  nên nó áp được cho cả tệp chưa commit; khi tệp chưa commit, dòng `# note` in thêm `(untracked)`
  để người đọc biết. Mục `[[excluded]]` trỏ tệp không tồn tại vẫn là `E1_EXCLUDED_STALE`.
- **`[[negative_log]]`** không liên quan trạng thái tệp; log ca âm vẫn phải ở trong repo.

Với luật I, **log** được trích trong khối `scope=` vẫn phải **có trong index** (D2-109C): bản nháp
log chưa `git add` ⇒ `E1_SCOPE_UNTRACKED`. Lý do: một số đo chỉ có nguồn khi log sẽ được commit
cùng — nếu không, commit xong con số mất nguồn.

## 5. Quy trình cho agent (bắt buộc trước khi báo freeze)

- **Trước khi báo freeze**: chạy `./v2/bin/v2 check` trên **cây có cả tệp chưa commit** — không
  `git add` trước để “cho cổng thấy”, không stage trong clone tạm rồi báo xanh. Cổng đọc thẳng cây.
- **Đọc nhãn trạng thái** trong từng dòng lỗi: `[untracked]` nghĩa là tệp **chưa vào lịch sử** —
  sửa ngay, đừng chờ commit rồi mới sửa (đó đúng là lỗ KT-85/KT-86).
- **Sau khi commit**: chạy lại `./v2/bin/v2 check`. Kết quả phải **giống** lần trước (chỉ nhãn
  `untracked`/`staged` đổi thành `tracked`). Nếu khác, nghĩa là cây lúc chạy khác cây lúc commit —
  điều tra, đừng commit tiếp.
- **Không để lại rác thử nghiệm**: tệp thử phải bị xoá; `git status --porcelain` không được còn
  dấu vết (đường dẫn của tệp thử). Ca kiểm của KT-87 làm đúng như vậy trên cây thật.
- **Không tự `git add` tệp thật để “thấy cổng”**: chỉ cần chạy cổng. Còn nếu cần stage để thử
  (như ca (b)), làm trong **clone tạm** — index dùng chung đã gây nhiễm chéo (A1-177).

## 6. Ca kiểm và ca hồi quy

**Năm ca kiểm bắt buộc của KT-87** (tệp thử untracked → staged → committed trong clone tạm → xoá →
so cây thật với clone) chạy tự động bằng:

```bash
bash v2/agent/JOURNAL/kt87-probe.sh          # TAP; in kết quả từng ca (chỉ số ca sai là lỗi)
```

Script chỉ `git add`/`git commit` trong **clone tạm** `${TMPDIR:-/tmp}/kt87-clone.*`, và `trap` xoá
tệp thử trên cây thật khi thoát (kể cả khi lỗi). Nó băm nội dung tài liệu đích trước/sau để chứng
minh **cây thật không bị đổi**.

**Ca hồi quy** (luật VIII): mỗi đường phá mới của KT-87 thành một ca trong
`v2/test/rules/verify-script-regressions.txt`, chạy bởi `--self-test` trên bản copy. Bốn dòng dưới
đây trích **nguyên văn** từ registry (không chép tay):

```text scope=v2/test/rules/verify-script-regressions.txt
untracked-doc-skipped | 1 | E1_NUMBER_OUTSIDE | untracked-doc-skipped | tài liệu MỚI chưa `git add` có số ở prose từng được bỏ qua (KT-85/KT-86/KT-77c: cổng xanh trước commit, đỏ sau — KT-87)
staged-doc-skipped | 1 | E1_NUMBER_OUTSIDE | staged-doc-skipped | tài liệu mới chỉ `git add` (chưa commit) vẫn phải bị quét y như bản sẽ commit
legacy-untracked | 1 | E1_LEGACY_UNTRACKED | legacy-untracked | ngoại lệ `[[legacy]]` trỏ tệp chưa commit — sha chỉ ghim bản đã commit nên phải ĐỎ rõ, không miễn
legacy-staged | 1 | E1_LEGACY_STAGED | legacy-staged | mục `[[legacy]]` trỏ tệp mới chỉ `git add` (chưa commit) — chưa có sha HEAD để ghim, phải ĐỎ rõ
```

## 7. Wall

`check-rules` vẫn dưới ngưỡng sáu mươi giây của KT-87 (đo trên cây thật, gồm cả `--self-test` với
bốn ca mới). Phần tăng so với trước KT-87 là ba lệnh `git` đọc tập tệp (mỗi lệnh cỡ vài chục ms) và
bốn ca hồi quy mới; `git ls-tree` (tập HEAD) chỉ chạy **khi có lỗi cần in nhãn**, nên đường xanh
không trả giá.
