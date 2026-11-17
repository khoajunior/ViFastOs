# Chặn commit bằng máy — hook `pre-commit` + `pre-push` (KT-85)

**Ngày**: 2026-11-17 · **Task**: KT-85 · **Thi hành**: D2-123 mục bốn (a/b/c/d/e) + D2-121
(uỷ quyền Agent 1) · **Căn cứ sự cố**: A1-177 (index dùng chung bị nhiễm chéo dòng), A1-180
(agent tự commit làm HEAD đỏ) · **Vùng ghi**: `.git/hooks/{pre-commit,pre-push}`,
`v2/tools/git-guard/`, tài liệu này, `v2/agent/JOURNAL/kt85-*`.

Hai lần agent tự `git add` rồi commit (A1-177, A1-180) không phải sơ suất lẻ: lần thứ hai ⇒
D2-123 mục bốn chốt **sửa bằng cấu trúc, không nhắc bằng lời**. Cấu trúc đó gồm hai phần và
tài liệu này mô tả cả hai:

- **hook fail-closed** trong git: chỉ vai `lead` được commit/push (mục một, bốn);
- **worktree riêng cho từng task** (mục hai) — phần này mới là chỗ vá gốc A1-177, vì mỗi worktree
  có **index riêng**, nên hai dòng/task không thể stage lẫn của nhau.

Tài liệu **không** chứa số đo: mọi số đo nằm trong log
`v2/agent/JOURNAL/kt85-hook-neg.log` (theo OD-K6-5: số phải trích nguyên văn từ log đã commit;
khi lead commit log xong, ai cần số thì mở khối `scope=` trích đúng dòng).

## 1. Hai hook — cài ở đâu, chặn gì

| Hook | Chặn | Chạy khi |
|---|---|---|
| `pre-commit` | `git commit` (kể cả `--amend`, commit từ merge không tính — xem mục bảy) | trước khi tạo commit |
| `pre-push` | `git push` (kể cả `--dry-run` — git vẫn gọi hook) | trước khi gửi ref lên remote |

- **Vị trí cài**: `<git-common-dir>/hooks/{pre-commit,pre-push}` — thư mục hook **chung**, nên
  hook áp cho **mọi** worktree của repo. Đây là điểm cố ý: agent làm trong `../wt-<mã>` vẫn bị
  chặn, không có đường "worktree riêng nên không bị kiểm".
- **Bản gốc trong repo**: `v2/tools/git-guard/pre-commit` và `v2/tools/git-guard/pre-push`;
  `v2/tools/git-guard/install.sh` **copy** (không symlink) vào common dir. Copy để xoá worktree
  không làm gãy hook, và để `install.sh --check` so được sha256 giữa bản cài và bản gốc.
- Hook **không** chạy cho `git status`, `git log`, `git diff`, `git add`: các lệnh đó không đi
  qua hook nên không thể hỏng (có ca kiểm riêng trong log ca âm).
- Bản cài là **của repo**, không phải của một checkout: người mới clone phải chạy
  `v2/tools/git-guard/install.sh` (hook không đi theo git — mục bảy).

## 2. Quy trình worktree cho **mọi** agent (bắt buộc)

Luật (D2-123 mục bốn (a)): **agent không làm trên nhánh chính**. Mỗi task một worktree, một
nhánh `task/<mã>`; xong thì **báo lead**, lead commit/merge — agent không tự `git add`,
không tự commit, không tự push.

```bash
# (a) Tạo worktree riêng cho task (đặt cạnh repo, không đè lên checkout chính)
git -C /Users/mps/dvchd/ViFastOs worktree add ../wt-<mã> -b task/<mã>
cd ../wt-<mã>

# (b) Làm toàn bộ việc trong worktree này: sửa tệp, chạy cổng, ghi log
./v2/bin/v2 check

# (c) Xong: KHÔNG commit. Gửi lead danh sách tệp + log + kết quả cổng
#    (ví dụ: "KT-85 sẵn sàng trong /Users/mps/dvchd/wt-kt85, nhánh task/kt85, bốn tệp mới, check PASS")

# (d) LEAD (Agent 1) commit trong đúng worktree đó rồi merge/cherry-pick sang nhánh chính:
VIFAST_ROLE=lead git -C ../wt-<mã> commit -m '[KT-<mã>] <tóm tắt>'

# (e) Dọn sau khi lead đã merge
git -C /Users/mps/dvchd/ViFastOs worktree remove ../wt-<mã>
git -C /Users/mps/dvchd/ViFastOs branch -D task/<mã>
```

Vì sao quy trình này chặn đúng lỗi cũ:

- mỗi worktree có **index riêng** ⇒ tệp của task này không thể bị `git add` từ dòng khác cuốn
  vào commit (A1-177 không tái diễn được, kể cả khi hai agent chạy song song);
- `git commit` bị hook chặn nếu không có `VIFAST_ROLE=lead` ⇒ agent không tạo được commit
  "tự phát" làm đỏ HEAD (A1-180);
- lead thấy diff đúng một nhánh task, không phải một cây bẩn lẫn nhiều dòng.

Điều **không** được làm: commit/push trên checkout chính; `git add` ở checkout chính (kể cả
`git add -N`); `git push --force`; viết lại lịch sử (ba commit cũ `795df046`/`ccabd069`/`9af541a0`
giữ nguyên — D2-123 mục bốn (e)).

## 3. Cài, kiểm, gỡ

```bash
v2/tools/git-guard/install.sh            # cài (idempotent: chạy lại không đổi gì)
v2/tools/git-guard/install.sh --check    # chỉ kiểm, không ghi; mã thoát không = đúng, khác không = lệch
v2/tools/git-guard/install.sh --uninstall# gỡ (chỉ gỡ hook của vifast-guard)
v2/tools/git-guard/install.sh --repo DIR # áp cho repo khác (dùng cho ca tự kiểm)
```

- **Idempotent**: lần hai in "đã cài đúng, không đổi" (so sha256), không ghi đè vô ích.
- **`--check` kiểm**: hook tồn tại, có quyền chạy, nội dung trùng bản gốc, và `core.hooksPath`
  không bị trỏ chỗ khác. Đây là điều kiện đầu vào của `neg-test.sh` (không cài đúng thì ca âm
  vô nghĩa vì hook không chạy).
- **Bất biến `core.hooksPath`**: nếu biến này trỏ chỗ khác thì hook của repo **không bao giờ
  chạy** mà không ai thấy. `install.sh` **từ chối** cài khi bất biến sai và in cách sửa:
  `git config --unset core.hooksPath`. Không có cờ "kệ đi" — fail-closed.
- Hook lạ đã có sẵn ở cùng đường dẫn: `install.sh` sao lưu thành `<tên>.bak.<thời điểm>` rồi
  mới cài, in rõ đường dẫn sao lưu. `--uninstall` chỉ gỡ hook có nhãn `vifast-guard`.
- Quyền chạy hook là điều kiện sống còn: file không có quyền chạy thì git **bỏ qua im lặng**;
  `--check` báo đỏ đúng ca đó.

## 4. Luật vai — ai được commit/push

- Nguồn vai **duy nhất**: biến môi trường `VIFAST_ROLE`. Hook **không** tự đặt biến, **không**
  đọc `.git/config`, **không** đọc tệp cấu hình nào — nếu đọc được từ tệp thì agent sửa tệp là
  xong, luật vô nghĩa.
- **Chỉ Agent 1 (lead)** đặt `VIFAST_ROLE=lead` trong môi trường của lệnh commit/push. Agent 2
  và reviewer **không** đặt biến; không có biến nào khác thay thế.

| `VIFAST_ROLE` | `git commit` | `git push` |
|---|---|---|
| chưa đặt (hoặc rỗng) | **từ chối** (fail-closed) | **từ chối** |
| `agent2`, `reviewer`, vai khác | **từ chối** | **từ chối** |
| `Lead`, `lead ` (sai hoa/thừa dấu cách) | **từ chối** | **từ chối** |
| `lead` | cho qua | cho qua |

- Fail-closed là chủ ý: biến **chưa đặt** bị coi là "không đủ tư cách", không mặc định cho qua.
  Nếu lead quên đặt biến, commit bị chặn (đúng luật) — đặt lại biến rồi chạy lại, không có
  đường tắt.
- Khi từ chối, hook in: **ai** đang commit/push (`user.name`/`user.email`), **ở đâu** (nhánh,
  toplevel, checkout chính hay worktree liên kết), **vì sao**, và **phải làm gì** (tạo worktree
  `../wt-<mã>` trên nhánh `task/<mã>`, báo lead; lead đặt biến rồi commit trong đúng worktree).
  Kèm một dòng máy đọc được `vifast-guard: RESULT=DENY hook=...` để log/CI grep được.
- **Quyền của người** (không phải agent): `git commit --no-verify` / `git push --no-verify`
  **bỏ qua** hook. Hook không chặn, không thể chặn, và không được phép chặn — người thật giữ
  quyền đó (đề bài KT-85 yêu cầu bốn, và log ca âm có ca chứng minh `--no-verify` vẫn đi qua).
  Agent dùng `--no-verify` là **vi phạm luật repo**, không phải "tính năng".

## 5. Bằng chứng ca âm

- Log: `v2/agent/JOURNAL/kt85-hook-neg.log` — sinh bằng
  `v2/tools/git-guard/neg-test.sh --dir /tmp/kt85neg`, chạy trong **worktree tạm** trên nhánh
  `task/kt85neg`, dọn sạch sau khi xong.
- Nội dung log, theo thứ tự: tiền kiểm `install.sh --check`; **ca âm chưa đặt biến** (commit bị
  từ chối); **ca âm `VIFAST_ROLE=agent2` commit** (bị từ chối); **ca dương lead commit** (đi
  qua); **ca âm `VIFAST_ROLE=agent2` push** (bị từ chối, bare remote tạm không nhận ref);
  **ca dương lead push**; **ca không-phá** (`git status`/`git log` vẫn chạy); **ca `--no-verify`**
  (vẫn đi qua — quyền của người); cuối cùng là **bất biến**: HEAD checkout chính không đổi,
  fingerprint index checkout chính không đổi, nhánh tạm đã xoá, số worktree trở về như trước.
- Log **không** trích số vào tài liệu này: tại thời điểm viết, log chưa được commit (agent
  không tự commit — chính luật của task này); theo OD-K6-5, log phải **commit trước** báo cáo
  thì số mới được trích. Lead commit log rồi mới mở khối `scope=` nếu cần số.
- Tái lập: chạy lại `neg-test.sh` (nó tự tạo/dọn worktree tạm; `--keep` giữ lại khi có ca hỏng,
  `--no-cleanup` giữ nguyên để soi).

## 6. Nếu dùng GitHub — bật bảo vệ nhánh chính

Repo hiện là **cục bộ**, chưa có remote GitHub, nên phần này **chưa thi hành** (D2-123 mục bốn (c)
chỉ yêu cầu ghi hướng dẫn). Khi đưa repo lên GitHub:

- **Settings → Branches → Add branch protection rule** (hoặc **Rulesets** mới) cho `main`:
  - bật **Require a pull request before merging**, đặt **ít nhất một approving review** và
    **Dismiss stale approvals** khi có commit mới;
  - bật **Require status checks to pass** và chọn check của cổng (`v2 check` / `v2 gate --fast`
    nếu CI đã nối), bật **Require branches to be up to date**;
  - bật **Do not allow force pushes** và **Do not allow deletions**;
  - **Restrict who can push** cho nhánh `main`: chỉ tài khoản/khoá của **Agent 1**;
  - **Require linear history** (chặn merge commit do máy tạo, dễ soi lịch sử) — tuỳ chọn;
  - với tag phát hành: bật bảo vệ tag, cùng lý do.
- **Vì sao vẫn cần**: hook là rào cản **cục bộ**; người/agent có quyền trên máy vẫn bỏ qua được
  (`--no-verify`, đổi `core.hooksPath`, `git commit-tree`, clone rồi push từ máy khác). Bảo vệ
  nhánh trên server là ranh giới **duy nhất** mà agent không tự vượt được.
- Ghi lại **ai** đã bật (Agent 1), **ngày**, và ảnh chụp/đường dẫn cấu hình vào nhật ký vòng —
  để lần review sau kiểm được, không chỉ tin lời.

## 7. Giới hạn đã biết (phần chưa đóng)

Trung thực về phạm vi — hook này là **rào cản cấu trúc + dấu vết**, không phải ranh giới bảo mật:

- `git commit --no-verify` / `git push --no-verify`: bỏ qua hook — **cố ý** (quyền của người).
- `git merge` (tạo merge commit), `git cherry-pick`, `git revert` (commit tự động), `git tag`:
  không đi qua `pre-commit`; `pre-push` vẫn chặn push từ vai khác. Đề nghị follow-up:
  `pre-merge-commit` (ngoài vùng ghi của KT-85, cần lead mở task) nếu agent bắt đầu dùng merge.
- `git commit-tree` / `git update-ref` / `git push` từ máy khác: hook cục bộ không thấy. Chỉ
  bảo vệ nhánh trên GitHub (mục sáu) mới chặn.
- Hook không kiểm **nội dung**: không kiểm nhãn `[KT-##]`, không kiểm cổng xanh, không kiểm
  tệp có nằm trong vùng ghi của task hay không. Việc đó vẫn là của lead (`git show --stat`) và
  `v2 check`. Muốn tự động thì đó là task khác (ví dụ hook gọi `v2 check --staged`).
- Hook chạy theo **môi trường của process gọi git**: terminal của lead đã `export VIFAST_ROLE=lead`
  thì mọi lệnh trong terminal đó đều qua — kể cả lệnh của agent nếu chung shell. Vì vậy luật
  "chỉ Agent 1 đặt biến" là luật **vận hành**, không phải thứ hook cưỡng chế được.
- Người mới clone / máy khác: phải chạy `install.sh` (hook không nằm trong git). CI cũng vậy nếu
  CI commit (push từ CI thì bảo vệ nhánh mới là chốt).
- Branch protection GitHub: **chưa bật** (repo cục bộ) — xem mục sáu.

## 8. Gỡ / hoàn nguyên

```bash
v2/tools/git-guard/install.sh --uninstall   # gỡ hook của vifast-guard (giữ hook lạ)
v2/tools/git-guard/install.sh --check       # xác nhận đã gỡ (báo CHƯA cài)
```

Muốn tạm thời tắt mà không gỡ: **không** đổi `core.hooksPath` (làm hook của người khác chết
theo); gỡ bằng `--uninstall` rồi cài lại sau. Nếu phải commit gấp với tư cách người thật, dùng
`--no-verify` — đúng quyền, và không để lại cấu hình lệch.
