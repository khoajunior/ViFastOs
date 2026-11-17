# Luật máy K6 (OD-K6-5) — cài trong cổng

**Ngày**: 2026-11-17 · **Task**: KT-80 · **Quyết định**: OD-K6-5 (D2-119, phụ lục A = D2-120)
· **Luật gốc**: D2-102, D2-106, D2-108, D2-109, D2-110, D2-112, D2-113F, D2-118, D2-105, D2-94.

Mục tiêu: mười vòng review vừa qua phát hiện lỗi **bằng tay**, lặp lại cùng dạng (số không có
nguồn, log ngoài repo, bộ đếm “0” không ca âm, bản trích cắt cột, verifier bị làm yếu). KT-80
biến **năm** trong tám luật của OD-K6-5 thành **kiểm tự động trong cổng** để lần sau cổng bắt
thay người. Hai luật còn lại (driver phải đàm phán feature; bảng tài nguyên một nguồn + kiểm
chồng lấn) thuộc **KT-81** — chưa mở.

## 1. Năm luật, một công cụ

| Luật | Nội dung | Cài ở | Mã lỗi |
|---|---|---|---|
| **I** | Số trong báo cáo do script sinh từ **log đã commit**; trích **nguyên văn** trong khối `scope=` | `v2/tools/verify-numbers.py` (`law1`) | `E1_*` |
| **II** | Log phải **commit trước** báo cáo (thứ tự commit, không phải nội dung cây) | cùng tệp (`law2`) | `E2_*` |
| **III** | Bộ đếm báo bằng không phải có **ca âm** làm nó khác không; `proven=false` không được nằm trong khối ok | cùng tệp (`law3`) + `v2/test/rules/counters.toml` | `E3_*` |
| **VII** | Bản trích tiêu chí giữ **mọi cột** (hoặc ghi tên cột bỏ + lý do trong tệp) | cùng tệp (`law7`) + `v2/test/rules/criteria.toml` | `E7_*` |
| **VIII** | Mỗi lần sửa script kiểm số phải kèm **ca phá của lần trước** làm ca hồi quy | cùng tệp (`--self-test`) + `v2/test/rules/verify-script-regressions.txt` | `E8_*` |

Một lệnh duy nhất: `./v2/bin/v2 check-rules` (luật `--law N`, tự chạy hồi quy bằng
`--self-test`). `v2 check` và `v2 gate --fast` **gọi lệnh này** (bước “luật máy K6 (KT-80)”);
không có đường tắt, không có ngoại lệ “tạm thời”: mọi ngoại lệ phải có tên + lý do + ngày trong
registry và bị **ghim sha256** (sửa nội dung là cổng đỏ).

## 2. Luật I — số phải nằm trong khối `scope=` và trích nguyên văn

Tài liệu đích (khai ở `[targets]` của `v2/test/rules/numbers-policy.toml`): `v2/agent/reports/*.md`
và `v2/docs/k6-*.md`. Khối evidence mở bằng dòng <code>```text scope=&lt;tệp-log&gt;</code>
(tệp log tính từ gốc repo hoặc từ `v2/agent/JOURNAL/`), các dòng sau là trích **nguyên văn**.

Quy tắc kiểm:

- **Mỗi khối chỉ so với đúng một tệp log** (D2-112a: không nối log thành “haystack chung”, nên
  không thể hoán số giữa hai ảnh); dòng trích phải trùng **cả dòng** với log, không phải xâu con.
- **Log phải ở trong repo** (đường dẫn tuyệt đối hoặc `..` ⇒ `E1_SCOPE_OUTSIDE_REPO`) và **đã có
  trong git** (index) ⇒ `/tmp` không làm nguồn được (D2-109C).
- **Ngoài khối chỉ được có định danh** (hex, mã task/`D2`/mốc/`R`/`L`, ngày, giờ, `§`, `file:dòng`,
  tên kiến trúc, mã hàng một chữ + số ≤ hai chữ số…). Không có bảng miễn trừ chứa chữ số: số đo
  đứng riêng ngoài khối ⇒ `E1_NUMBER_OUTSIDE` (D2-112b).
- **Log ca âm/mutation không được làm nguồn**: khối `scope=` trỏ vào log như vậy ⇒
  `E1_SCOPE_FORBIDDEN`; log ca âm chỉ được trích bằng `scope-red=` và tên tệp phải tự khai là ca
  âm hoặc nằm trong `[[negative_log]]` (D2-112c).
- Tệp tiêu chí **sinh từ máy** (`criteria.txt`) phải được chép **toàn bộ** trong một khối — không
  được trích lẻ để né số (`E1_CRITERIA_PARTIAL`), giữ nguyên tinh thần D2-110A của KT-76.
- **Ngoại lệ lịch sử** (mục `[[legacy]]`): chỉ áp cho tài liệu viết **trước** OD-K6-5 và nằm
  **ngoài vùng ghi** của KT-80; mỗi mục ghim `sha256` nội dung. Sửa nội dung tệp ngoại lệ ⇒
  `E1_LEGACY_SHA` (không thể “thêm số rồi miễn”); mục trỏ tệp không tồn tại ⇒ `E1_LEGACY_STALE`.
  Luật II **không** miễn cho tệp ngoại lệ.

## 3. Luật II — log commit trước báo cáo

Với mỗi cặp `(log, báo cáo)` mà báo cáo trích trong khối `scope=`/`scope-red=`:

- lấy commit chứa tệp bằng `git log --format=%H -- <path>` (gom **một** lệnh cho cả tập đường dẫn
  — cùng ngữ nghĩa, chỉ giảm wall), rồi kiểm bằng `git merge-base --is-ancestor`;
- commit của log phải là **tổ tiên** của commit của báo cáo, hoặc **cùng** commit;
- **cây bẩn không ảnh hưởng**: chỉ đọc lịch sử commit (HEAD), không đọc nội dung working tree;
- báo cáo **chưa commit** ⇒ cặp được đếm `pending` (báo cáo đang soạn chưa có thứ tự để kiểm);
  báo cáo **đã commit** mà log chưa commit ⇒ `E2_LOG_NOT_COMMITTED`; sai thứ tự ⇒ `E2_ORDER`.

Quan hệ commit là **bất biến** nên kết quả `--is-ancestor` được cache ở `TMPDIR` (khoá theo cặp
commit); cache chỉ giảm wall, không tham gia quyết định đúng/sai.

## 4. Luật III — bộ đếm báo bằng không phải có ca âm

Registry `v2/test/rules/counters.toml`. Dòng evidence khai `counter=<tên>` phải:

- có mục trong registry (thiếu ⇒ `E3_COUNTER_UNDECLARED`);
- nằm **trong** khối evidence (prose ⇒ `E3_COUNTER_OUTSIDE_BLOCK`);
- nếu `proven = true`: registry phải trỏ **log ca âm trong repo** khớp `negative_pattern` cho thấy
  bộ đếm **khác không** (thiếu log ⇒ `E3_NO_NEGATIVE_LOG`; log vẫn bằng không ⇒ `E3_NEGATIVE_ZERO`);
- nếu `proven = false`: dòng đó **không** được nằm trong khối `scope=` (khối được coi là ok) —
  chỉ được nằm trong `scope-pending=` (⇒ `E3_UNPROVEN_IN_OK` nếu vi phạm).

Đây là D2-108B (bộ đếm an ninh luôn bằng không mà không có ca âm thì **không chứng minh gì**) được
máy kiểm thay vì nhắc bằng lời. Giá trị dưới đây do `v2/test/rules/counter-evidence.sh` **đọc từ
log KT-78** rồi mới in (không viết tay):

```text scope=kt80-law3-counter-evidence.log
counter=qemu_ppid1 positive=0 negative=1 positive_log=kt78-ktest-matrix.log negative_log=kt78-guard/selftest-20261117T015708Z.log
counter=isolation_bad positive=0 negative=1 positive_log=kt78-ktest-matrix.log negative_log=kt78-guard/selftest-20261117T015708Z.log
```

## 5. Luật VII — bản trích tiêu chí giữ mọi cột

Registry `v2/test/rules/criteria.toml` khai từng bản trích và **bảng gốc** (đọc-only) của nó:
dòng K6 của thiết kế §10.1 (ba cột), bảy dòng sổ lỗi v1 (sáu cột), khối tiêu chí trong
`K6-DOD.md`. Máy kiểm: số cột trích **bằng** số cột gốc; muốn bỏ cột thì tệp trích phải ghi

```text
<!-- cot-bo: Tên cột — ly-do: … -->
```

và tên cột bị bỏ phải có thật trong bảng gốc (nếu không ⇒ `E7_OMIT_UNDECLARED`), lý do phải viết
(≥ mười ký tự). Tệp `criteria.txt` hoặc `*-DOD.md` **không khai báo** nguồn ⇒ `E7_NOT_REGISTERED`
(fail-closed cho tệp mới), trừ ngoại lệ lịch sử có ghim sha.

## 6. Luật VIII — ca phá cũ là ca hồi quy

`v2/test/rules/verify-script-regressions.txt` liệt kê **mười lăm** đường phá đã từng lọt qua các
verifier tiền nhiệm, mỗi dòng ghi: id · luật · mã lỗi kỳ vọng · cách phá · mô tả. Tài liệu này
**không chép tay** danh sách đó — nó trích nguyên văn từ registry:

```text scope=v2/test/rules/verify-script-regressions.txt
union-haystack | 1 | E1_NOT_VERBATIM | union-haystack | số của log khác được trích dưới scope= sai
heading-number | 1 | E1_NUMBER_OUTSIDE | heading-number | tiêu đề "## 98765" (số năm chữ số)
heading-not-reset | 1 | E1_NUMBER_OUTSIDE | heading-not-reset | cờ heading không reset (lỗi KT-75)
decimal-x-y | 1 | E1_NUMBER_OUTSIDE | decimal-x-y | số thập phân "262.144" trong prose (D2-117)
fence-text-no-scope | 1 | E1_NUMBER_OUTSIDE | fence-text-no-scope | fence ```text thiếu `scope=`
bare-small-number | 1 | E1_NUMBER_OUTSIDE | bare-small-number | số đo một chữ số trong prose
scope-tmp | 1 | E1_SCOPE_OUTSIDE_REPO | scope-tmp | `scope=/tmp/...` (log ngoài repo)
scope-negative-source | 1 | E1_SCOPE_FORBIDDEN | scope-negative-source | `scope=` trỏ log ca âm
counter-unproven | 3 | E3_UNPROVEN_IN_OK | counter-unproven | `proven=false` nhưng nằm trong khối ok
counter-undeclared | 3 | E3_COUNTER_UNDECLARED | counter-undeclared | `counter=` không khai registry
counter-proven-no-negative | 3 | E3_NEGATIVE_ZERO | counter-proven-no-negative | `proven=true` nhưng log ca âm không chứng minh bộ đếm khác 0
assign-key-value | 1 | E1_NUMBER_ASSIGNED | assign-key-value | số đo dạng `key=value` ở prose (phản biện KT-80: `drops=5` từng lọt)
criteria-drop-column | 7 | E7_COLUMN_MISMATCH | criteria-drop-column | bản trích bỏ một cột
log-after-report | 2 | E2_ORDER | log-after-report | commit báo cáo TRƯỚC commit log
legacy-changed | 1 | E1_LEGACY_SHA | legacy-changed | ngoại lệ lịch sử bị sửa nội dung
```

`verify-numbers.py --self-test` dựng một repo mini **XANH** trong thư mục tạm, rồi với **từng**
dòng của registry: copy repo mini ⇒ áp đúng cách phá ⇒ chạy đúng luật ⇒ **bắt buộc ĐỎ đúng mã**.
Ca nào xanh ⇒ self-test đỏ (`E8_CASE_GREEN`); nhờ vậy ai sửa verifier làm một ca cũ lọt lại thì
cổng đỏ ngay, không phụ thuộc trí nhớ reviewer.

Ngoài self-test, `v2/test/rules/selfbreak.sh` chứng minh **từng luật** ĐỎ được trên **bản copy**
của cây thật (D2-113F), có **đối chứng dương** trước khi phá, và in `shasum` cây thật trước/sau:

```text scope-red=kt80-selfbreak.log
# selfbreak: 5/5 ca phá ĐỎ đúng mã; cây thật không đổi
```

## 7. Cổng — thời gian và bằng chứng

Bước cổng gọi `check-rules` (nhãn bước trong log cổng là `luật máy K6 (KT-80)`). Lần chạy dưới đây là trên chính tài liệu này
(đã có trong danh sách đích), luật I kiểm luôn tài liệu này:

```text scope=kt80-check-rules.log
# verify-numbers: ok=5 fail=0 wall_ms=11259 law_wall_ms={1:911,2:1502,3:69,7:24,8:8630}
```

Ngân sách OD-K6-5 cho cả năm luật là **sáu mươi giây**; số đo thực tế nằm trong khối trên. Luật
VIII chiếm phần lớn wall vì nó chạy mười ba ca phá, mỗi ca một tiến trình riêng trên bản copy.
Wall này đo khi **không** có QEMU ngoài (`kt80-orphan.log` rỗng trước cổng).

Cổng đầy đủ (`v2 check` + `gate --fast`), clippy và ảnh sản phẩm:

```text scope=kt80-gate-dod.log
CHECK_EXIT=0
CHECK_WALL_MS=39923
GATE_EXIT=0
TESTS=729
SUITES=100
SHA_AFTER=fd20227658832a3718b390b0cd7aab563396e23e01ecbd04632f7a9c4393a6da
SIZE_AFTER=188552
```

```text scope=kt80-clippy.log
CLIPPY_EXIT=0
```

Ảnh sản phẩm **không đổi** trước/sau (`kt80-sha-before.log`, `kt80-sha-after.log`) — nhưng đây là
**ảnh trước canonical** (`fd202276…`), **KHÔNG** dùng làm bằng chứng v2.0 (D2-122: ảnh canonical của
KT-79 là `c1072d72…`/82552; ảnh phát hành chưa được tuyên bố tái lập byte). Lưu ý D2-116(c):
lúc kết thúc cổng có QEMU **của task khác** (một wrapper `stdin_wrap.sh` mồ côi) ⇒ **wall của cổng
không dùng để kết luận hiệu năng**; năm luật thì đo riêng ở khối trên khi đã kiểm **không còn** QEMU (`kt80-orphan.log` rỗng).

## 8. Ngoại lệ và giới hạn đã biết

- **Số ngoại lệ**: `legacy=` trong khối `scope=` của mục §7 = **mười chín** tệp **lịch sử** (viết
  trước ngày OD-K6-5 hiệu lực, `2026-11-17`), **cộng bốn** tệp **có trước luật** nhưng vi phạm
  **đường `key=value`** mới siết theo phản biện (`K6-DOD` chủ KT-76, `k6-exec` chủ KT-74,
  `k6-persist` chủ KT-75, `k6-parity` chủ KT-77a) — mỗi mục có `owner` + `debt` + `sha256` của bản
  HEAD, in `# note` mỗi lần chạy. Tất cả ghim `sha256` của bản **đã commit** (HEAD) nên bản nháp
  đang sửa của task khác không làm đỏ oan; commit bản sửa là ĐỎ cho tới khi xử lý lại. Phạm vi
  ngoại lệ: **chỉ** tệp có trước ngày luật hiệu lực; **không** có mục nào cho tài liệu mới.
  Ban đầu có thêm **một** tệp **có trước luật** (`v2/docs/k6-parity.md`, viết song song) với đúng
  cơ chế `kind="pre_existing"` + chủ + nợ; KT-77a đã **sửa tài liệu cho đúng luật** ở commit
  `de55b786` (số vào khối `scope=`, không ngoại lệ) nên mục đó đã được **xoá** — hiện `k6-parity.md`
  nằm trong danh sách **được kiểm** (`docs_checked` tăng lên), không còn ngoại lệ nào cho tài liệu
  mới.
- **Một tệp loại khỏi tài liệu đích** (`[[excluded]]`, `excluded=` trong khối §7):
  `v2/agent/reports/A1-decision-log.md` — **sổ quyết định sống** của Agent 1, ghi thêm sau mỗi quyết
  định (ví dụ A1-177); ghim sha sẽ đỏ oan mỗi lần ghi, mà KT-80 không được sửa tệp của vai khác.
  Quyết định phạm vi có tên + ngày + chủ + điều kiện quay lại; tệp mất ⇒ `E1_EXCLUDED_STALE`.
  Không có ngoại lệ/loại trừ nào cho tài liệu **mới** sau khi luật đã vào cổng.
- **Bộ tách số** (sau phản biện KT-80): bắt cả token số **đứng riêng** lẫn **giá trị số dính sau
  dấu `=`** ở prose — ví dụ các khoá `drops`, `TESTS`, `wall_ms`, `fail` đi kèm số (phản biện chỉ
  đúng: trước đây chỉ lấy token tách bằng khoảng trắng nên dạng này lọt). Trừ **định danh**: hex/
  `0x`, giá trị băm (`sha*`, `build`, `digest`, `hash`), ngày, giờ. Ca hồi quy `assign-key-value` ở
  luật VIII giữ đúng đường này (mỗi lần sửa verifier phải kèm ca phá của lần trước).
- **`v2/agent/STATE.md` không nằm trong danh sách đích** của luật I (D2-112d): số của STATE thuộc
  từng task và đã có verifier riêng của task đó; luật I chỉ áp cho `reports/*.md` + `k6-*.md`.
- Luật III chỉ kiểm dòng **khai** `counter=<tên>`; bộ đếm nằm trong văn xuôi (không khai) vẫn phải
  do reviewer bắt — đó là lý do luật III **không** thay thế review người cho task kernel/driver.

## 9. Chạy lại và mở rộng

```bash
./v2/bin/v2 check-rules                    # cả năm luật + self-test (được cổng gọi)
./v2/bin/v2 check-rules --law N            # một luật (N = I, II, III, VII, VIII)
./v2/bin/v2 check-rules --self-test        # chỉ luật VIII
./v2/bin/v2 check-rules --root <bản-copy>  # kiểm bản copy, không ghi
bash v2/test/rules/selfbreak.sh            # ca phá từng luật trên bản copy + shasum cây thật
bash v2/test/rules/counter-evidence.sh     # in counter= từ log thật (luật III)
```

Thêm luật mới (ví dụ luật V/VI của OD-K6-5 ở KT-81): thêm nhánh `lawN` + mã lỗi `EN_*`, thêm
dòng vào `verify-script-regressions.txt` với **ca phá của chính luật đó**, và mở rộng
`selfbreak.sh`. Không được thêm ngoại lệ không tên, không được nới ngưỡng để qua cổng.
