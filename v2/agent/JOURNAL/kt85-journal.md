# KT-85 — chặn commit bằng máy: hook `pre-commit` + `pre-push`, worktree cho agent (nhật ký)

**Ngày**: 2026-11-17 · **Task**: KT-85 · **Người ghi**: Agent 2 dòng OS (uỷ quyền Agent 1,
D2-121) · **HEAD khi mở việc**: `9ca53472` (`[KT-77b]`) · **Quyết định**: D2-123 mục (4) (a/b/c/d/e),
D2-121 (uỷ quyền), A1-177 + A1-180 (căn cứ sự cố) · **Ngân sách**: 1 h · **Không tự commit**.

## 1. Vùng ghi và thứ tự làm

Vùng ghi: `.git/hooks/{pre-commit,pre-push}` (cài), `v2/tools/git-guard/`, `v2/docs/k6-git-guard.md`,
`v2/agent/JOURNAL/kt85-*`. **Không chạm**: `v2/kernel/**` (KT-77c chạy song song), `v2/test/rules/**`,
`v2/docs/k6-product-shell.md` (KT-77b), `spec/`, `v2/agent/{STATE,BLOCKED}.md` (để lead ghi),
`crates/`, `viscript/`.

Làm trong **worktree riêng** theo đúng D2-123 mục (4)(a) — tự thi hành luật mình đang viết:

```text
$ git worktree add ../wt-kt85 -b task/kt85 9ca53472
$ cd /Users/mps/dvchd/wt-kt85
```

Thứ tự: (1) đọc căn cứ + luật máy K6 (để tài liệu không làm đỏ cổng); (2) viết hook + `install.sh`;
(3) cài vào `git-common-dir/hooks`; (4) viết `neg-test.sh`, chạy 3 ca âm + 2 ca dương + 2 ca phụ,
ghi log; (5) viết tài liệu (không có số đo trong prose — xem §3); (6) `v2 check`; (7) báo lead.

## 2. Phát hiện khi làm

- **Hook của worktree nằm ở common dir, không ở `.git/worktrees/<tên>/hooks`.** `git rev-parse
  --git-common-dir` từ `/Users/mps/dvchd/wt-kt85` trả `/Users/mps/dvchd/ViFastOs/.git`. Vậy cài
  hook **một lần** ở common dir là **mọi** worktree bị chặn — đúng ý D2-123 mục (4)(a) (agent làm
  trong worktree riêng vẫn không commit được), và `install.sh` phải resolve theo common dir chứ
  không theo toplevel (nếu không, cài từ worktree sẽ ghi vào chỗ không ai dùng).
- **Mỗi worktree có index riêng.** Đây là cơ chế vá gốc A1-177 (nhiễm index chéo dòng), mạnh hơn
  mọi lời nhắc: `neg-test.sh` chứng minh `git add` trong worktree tạm **không** đổi fingerprint
  index của checkout chính (in ở `BẤT BIẾN` cuối `kt85-hook-neg.log`).
- **Luật 1 (OD-K6-5) chặn số trong prose** của `v2/docs/k6-*.md`: mọi token bắt đầu bằng chữ số
  ngoài khối `scope=` là `E1_NUMBER_OUTSIDE`; số dạng `key=value` cũng bị (`E1_NUMBER_ASSIGNED`).
  Vì agent **không được `git add`/commit**, log của task này **chưa** vào index ⇒ **không** được
  trích log bằng khối `scope=` (sẽ là `E1_SCOPE_UNTRACKED`). Cách xử lý: tài liệu viết **không có
  số đo** (dùng chữ: "ba ca", "mục bốn", "bước (a)"), số đo để trong log; lead commit log rồi ai
  cần số thì mở khối `scope=` trích đúng dòng (ghi rõ trong tài liệu §5).
- **`v2 check` không quét tệp chưa `git add`** (`verify-numbers.py`: "Tài liệu đích = tệp đã có
  trong repo (index)"; `v2/tools/git-guard/*` cũng là tệp mới nên R3 SPDX chỉ có tác dụng sau khi
  vào index). Hệ quả: phải **tự kiểm trước** trên bản sao có tệp đã stage, nếu không cổng sẽ đỏ
  đúng lúc lead stage — đúng kiểu A1-180. Đã làm: `git clone --shared` worktree ra `/tmp/kt85-scan`,
  copy 7 tệp mới, `git add -A`, chạy `v2 check-rules --law 1 --law 2 --law 3 --law 7` ⇒ **ok cả
  bốn luật** (`docs=26`, `docs_checked=4`, `ok - luat 1..7`). Lần quét đầu **ĐỎ** 18 lỗi
  `E1_NUMBER_OUTSIDE` (các chữ "mục (4)", "bước 1)", "0 = đúng") — đã sửa bằng chữ, không bằng
  ngoại lệ.
- **Fail-closed phải thật**: nếu hook "quên" biến rồi cho qua thì luật vô nghĩa; nếu hook chết vì
  lệnh phụ (`git config` lỗi, `rev-parse` lỗi) mà không in gì thì người dùng tưởng hook hỏng. Vì
  vậy mọi lệnh chẩn đoán trong hook có fallback (`|| true`, `set -u` nhưng không `-e`), và nhánh
  từ chối in mã máy đọc được `vifast-guard: RESULT=DENY hook=...`.
- **Bash 3.2 của macOS** (máy này) + `set -u`: `local a="$1" b="x$a"` nổ `unbound variable` vì
  `$a` chưa gán lúc lượng giá — đã tách câu lệnh trong `neg-test.sh` (lần chạy đầu chết ở đây,
  giữ lại đúng vết trong git reflog của nhánh tạm đã xoá; lần chạy sau sạch).
- **Test phải có thay đổi chờ commit**: ca `--no-verify` lần đầu đỏ oan vì "nothing to commit" —
  đã thêm một dòng vào tệp probe trước khi commit. (Ghi lại vì lần sau ai viết ca tương tự sẽ gặp.)
- **`--no-verify` của người**: hook **không** chặn; đã đưa thành ca chứng minh trong log (đề bài
  yêu cầu 4) — đồng thời ghi rõ trong tài liệu §4: agent dùng `--no-verify` là vi phạm, người
  dùng là quyền.

## 3. Đã cài gì

| Thành phần | Tệp | Vai trò |
|---|---|---|
| Hook chặn commit | `v2/tools/git-guard/pre-commit` (bản gốc) → `.git/hooks/pre-commit` (bản chạy) | fail-closed theo `VIFAST_ROLE`; in ai/ở đâu/phải làm gì |
| Hook chặn push | `v2/tools/git-guard/pre-push` → `.git/hooks/pre-push` | cùng luật; đọc stdin ref của git để in vào log từ chối |
| Cài/kiểm/gỡ | `v2/tools/git-guard/install.sh` | idempotent, `--check`, `--uninstall`, `--repo`; **từ chối** khi `core.hooksPath` trỏ chỗ khác |
| Ca âm tái lập | `v2/tools/git-guard/neg-test.sh` | 3 ca âm + 2 ca dương + 2 ca phụ; tự tạo/dọn worktree tạm; bất biến HEAD/index/nhánh |
| Tài liệu | `v2/docs/k6-git-guard.md` | ai được commit, quy trình worktree (§2), GitHub branch protection (§6), giới hạn (§7) |
| Log ca âm | `v2/agent/JOURNAL/kt85-hook-neg.log` | 5 ca chính + 2 ca phụ + bất biến, `neg-test.sh exit=0` |
| Log cài/kiểm | `v2/agent/JOURNAL/kt85-install-check.log` | `--check` lần đầu + cài lần hai (idempotent) |

Hook đã cài vào `/Users/mps/dvchd/ViFastOs/.git/hooks/` (sha256 trùng bản gốc, có quyền chạy).
`core.hooksPath` **chưa đặt** (đúng); `install.sh --check` ⇒ PASS.

## 4. Bằng chứng (đường dẫn, không chép số vào prose)

- `v2/agent/JOURNAL/kt85-hook-neg.log` — sinh bằng `v2/tools/git-guard/neg-test.sh --dir /tmp/kt85neg`.
  Trình tự ca: (1) **không đặt `VIFAST_ROLE`** ⇒ commit bị từ chối (fail-closed); (2)
  **`VIFAST_ROLE=agent2`** ⇒ commit bị từ chối; (3) `VIFAST_ROLE=lead` ⇒ commit **thành công**
  (ca dương); (4) **`VIFAST_ROLE=agent2` push** ⇒ bị từ chối, bare remote tạm **không** nhận ref;
  (5) `VIFAST_ROLE=lead` push ⇒ thành công (chỉ tới bare repo trong `/tmp`); (6) ca không-phá:
  `git status`/`git log` rc=0 ở cả checkout chính lẫn worktree; (7) ca `--no-verify` ⇒ đi qua
  (quyền của người). Cuối log: **bất biến** — HEAD checkout chính không đổi, fingerprint index
  checkout chính không đổi, nhánh `task/kt85neg` đã xoá, số worktree trở về như trước.
- `v2/agent/JOURNAL/kt85-install-check.log` — `install.sh --check` (lần đầu: CHƯA cài ⇒ đỏ đúng
  kỳ vọng; sau khi cài: PASS) + lần cài thứ hai in "đã cài đúng, không đổi" (idempotent).
- `v2 check` trong worktree task: chạy với `CARGO_TARGET_DIR=/tmp/kt85-target` (thư mục tạm, xoá
  sau khi kiểm) để không đụng cache build 5 GB của KT-77c đang chạy song song. Kết quả ở §5.
- **HEAD checkout chính đổi trong lúc làm — do lead, không phải task này**: mở việc ở `9ca53472`,
  lúc đóng việc là `12fbff3e` (hai commit `[KT-00]` A1-181/A1-182 của lead, tạo **trước** khi hook
  được cài lúc 13:18:33). Worktree task vẫn cắt từ `9ca53472`; lead merge/cherry-pick trên HEAD mới.
  Không có tệp `kt85*` nào rơi vào checkout chính (`git status` ở đó không có `kt85`).

## 5. Kết quả cổng

- `./v2/bin/v2 check` (trong `/Users/mps/dvchd/wt-kt85`, `CARGO_TARGET_DIR=/tmp/kt85-target`):
  **PASS**, `exit=0`, toàn bộ bước xanh — fmt, clippy, luật cấu trúc R1..R5, self-test luật, luật
  máy K6 (law 1/2/3/7/8: `docs_checked=3`, `pairs=90`, `counters=2`, `cases=17`), abi/scn/báo
  cáo/gen-check/idl, build L0. Log: `v2/agent/JOURNAL/kt85-v2-check.log` (log này sinh **sau** khi
  hook cài; nếu lead muốn trích số từ nó thì phải commit log trước — luật 2).
- `v2 check-rules --law 1..7` trên bản sao có tệp đã stage (`/tmp/kt85-scan`): **ok** cả bốn luật
  đang bật — chứng minh tệp mới **không** làm đỏ cổng khi lead stage chúng.

## 6. Việc chưa xong / đề xuất (cho lead quyết)

- **Chưa commit** (đúng luật task này): toàn bộ 7 tệp nằm trong worktree `/Users/mps/dvchd/wt-kt85`,
  nhánh `task/kt85`. Lead commit tại đó bằng `VIFAST_ROLE=lead git -C ../wt-kt85 commit ...` rồi
  merge/cherry-pick sang `main`; sau đó `git worktree remove ../wt-kt85 && git branch -D task/kt85`.
- **Hook chưa bắt `git merge`/`cherry-pick`**: `pre-commit` **không** chạy cho merge commit. Nếu
  cần đóng nốt, thêm `pre-merge-commit` — **ngoài vùng ghi KT-85**, cần lead mở task (ghi ở tài
  liệu §7).
- **GitHub branch protection: chưa bật** (repo cục bộ, D2-123 mục (4)(c) chỉ đòi hướng dẫn) — đã
  viết hướng dẫn §6. Đây mới là ranh giới thật khi có remote.
- **Hook không kiểm nội dung commit** (nhãn `[KT-##]`, cổng xanh, tệp có trong vùng ghi): đề xuất
  task sau nếu muốn `pre-commit` gọi `v2 check --staged`.
- **Lưu ý vận hành sau khi cài hook**: mọi `git commit`/`git push` từ nay **phải** có
  `VIFAST_ROLE=lead` trong môi trường, kể cả của agent khác đang chạy song song (KT-77c). Đây là
  chủ ý (D2-123 mục bốn); nếu ai bị chặn, thông điệp hook in đúng việc phải làm — không được dùng
  `--no-verify` để lách.
- **Chưa cài trên máy khác / CI**: hook không đi theo git; người mới clone phải chạy `install.sh`
  (tài liệu §7).

## 7. Ghi chú cho lead khi commit

Thứ tự đề nghị (luật 2 của OD-K6-5: log trước báo cáo — tài liệu này **không** có khối `scope=`
nên không bắt buộc, nhưng giữ thứ tự để sau này trích số được):

1. `v2/agent/JOURNAL/kt85-hook-neg.log`, `kt85-install-check.log`, `kt85-journal.md`,
   `kt85-v2-check.log`;
2. `v2/tools/git-guard/{pre-commit,pre-push,install.sh,neg-test.sh}`;
3. `v2/docs/k6-git-guard.md`.

Commit phải chạy **sau** khi hook đã cài (hook đang cài rồi) và **với** `VIFAST_ROLE=lead` —
nếu lead chạy `git commit` mà chưa đặt biến thì chính lead bị chặn (fail-closed; đó là hành vi
đúng, không phải lỗi).
