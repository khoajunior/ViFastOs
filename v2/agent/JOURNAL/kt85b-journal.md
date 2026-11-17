# KT-85b — hook `reference-transaction` + `commit-msg` (D2-125.2), vá A1-183

**Ngày**: 2026-11-17 · **Agent**: Agent 2 dòng OS (phiên `agent2-kt85b`) · **Ngân sách**: 1 h
· **Task**: KT-85b = **D2-125.2** của Agent 0 · **Trạng thái**: xong phần cài + ca âm + tài liệu;
**chờ lead commit** (agent không tự commit — chính luật của task này).

**Vùng ghi đã dùng**: `.git/hooks/{commit-msg,reference-transaction}` (mới), `v2/tools/git-guard/**`
(thêm `commit-msg`, `reference-transaction`, `neg-test-kt85b.sh`; sửa `install.sh`),
`v2/docs/k6-git-guard.md` (sửa), `v2/agent/JOURNAL/kt85b-*`.
**Không chạm**: `v2/kernel/**`, `v2/tools/{verify-numbers.py,repro-*}`, `v2/test/**`, `spec/`,
`v2/agent/{STATE,BLOCKED,QUEUE,DECISIONS}.md`, `crates/`, `viscript/`.

## 1. Đã làm

| Tệp | Việc |
|---|---|
| `v2/tools/git-guard/reference-transaction` | **mới** — chặn mọi cập nhật nhánh bảo vệ khi `VIFAST_ROLE != lead` |
| `v2/tools/git-guard/commit-msg` | **mới** — bắt buộc nhãn `[KT-##]`/`[ST-##]` ở đầu subject; miễn `Merge …`/`Revert "…"` |
| `v2/tools/git-guard/install.sh` | sửa — 4 hook, idempotent, `--check` cả 4, `--uninstall` cả 4, `--print-support`, cổng git-2.28 |
| `v2/tools/git-guard/neg-test-kt85b.sh` | **mới** — sinh bằng chứng ca âm cho hai hook mới (clone cục bộ tạm, tự dọn) |
| `v2/docs/k6-git-guard.md` | sửa — 4 hook, mục 1.1/1.2 (vì sao), §3.1 (cổng phiên bản), §5 (bằng chứng), **§6 hướng dẫn GitHub**, §7 giới hạn + D2-125.1 |
| `.git/hooks/{commit-msg,reference-transaction}` | **cài** (copy từ bản gốc, sha256 trùng — xem `kt85b-install-check.log`) |

Hook cài ở **`<git-common-dir>/hooks/`** = `/Users/mps/dvchd/ViFastOs/.git/hooks/` ⇒ áp cho **mọi**
worktree (29 worktree đang đăng ký).

## 2. Hành vi hook (như đã cài)

- **`reference-transaction`**: đọc stdin `<old> <new> <ref>`; ref ∈ `refs/heads/main`,
  `refs/heads/master` (+ `VIFAST_PROTECTED_REFS`, **chỉ thêm** không bớt) **và**
  `VIFAST_ROLE != "lead"` ⇒ exit 1 (git huỷ cả transaction). Chỉ xét trạng thái
  **`preparing`/`prepared`**; `committed`/`aborted` cho qua.
  → Phủ `commit`, `cherry-pick`, `merge`, `reset`, `rebase`, `branch -f`, `update-ref`.
- **`commit-msg`**: subject khớp `^\[(KT|ST)-[0-9]+[a-z]?\]`; sai ⇒ exit 1 kèm ví dụ đúng.
  Miễn trừ `Merge …`, `Revert "…"`. Không kiểm vai (pre-commit lo) — nên chạy được cả khi
  `pre-commit` bị `--no-verify` (thực tế git bỏ qua cả cụm).
- **Cài / kiểm / gỡ**: `install.sh` cài cả 4, chạy lại **không đổi** (sha256), `--check` kiểm cả 4
  + `core.hooksPath`, `--uninstall` gỡ cả 4 (đã thử trên repo nháp: gỡ xong không còn tệp nào).
- **Cổng phiên bản**: `git version` < 2.28 ⇒ **không cài** `reference-transaction` (kèm `[SKIP]` +
  CẢNH BÁO lỗ hổng A1-183 chưa vá), vẫn cài 3 hook kia; `--check` trên git cũ trả **thành công**
  (thiếu hook do git cũ **không** phải lỗi). Máy này: `git version 2.54.0 (Apple Git-157)` ⇒
  cài đủ 4 (`kt85b-git-version.log`).

## 3. Bốn ca âm bắt buộc — kết quả

Log đầy đủ: `v2/agent/JOURNAL/kt85b-hook-neg.log` (script `neg-test-kt85b.sh`, 7 ca, 29 kết luận
PASS, 0 FAIL, mã thoát 0). Tóm tắt:

- **(a) cherry-pick** `VIFAST_ROLE=agent2 git cherry-pick <sha>` trên nhánh `main` ⇒
  `RESULT=DENY hook=reference-transaction state=prepared`, rc=128, `refs/heads/main` **không đổi**.
- **(b) merge** `VIFAST_ROLE=agent2 git merge --no-ff task/…` ⇒ DENY, `main` không đổi
  (merge commit đã tạo object nhưng ref bị huỷ).
- **(c) reset** `VIFAST_ROLE=agent2 git reset --hard HEAD~1` ⇒ DENY, `main` không đổi.
- **(d) commit-msg**: `VIFAST_ROLE=agent2 commit -m "khong co nhan"` ⇒ DENY (pre-commit chặn
  trước — git chạy pre-commit **trước** commit-msg); `VIFAST_ROLE=lead commit -m "khong co nhan"`
  ⇒ DENY bởi **commit-msg** (chứng minh hook nhãn chạy độc lập); `VIFAST_ROLE=lead commit -m
  "[KT-00] thu"` ⇒ **qua**; lead `merge` (message `Merge …`) ⇒ **qua** (miễn trừ đúng).

Kèm: **hai ca dương** (lead cherry-pick / lead merge vào `main` ⇒ qua), **PROBE trên repo thật**
(`update-ref` nhánh nháp được bật bảo vệ qua `VIFAST_PROTECTED_REFS` bằng vai `agent2` ⇒ DENY,
nhánh **không** được tạo; cùng lệnh vai `lead` ⇒ qua, rồi xoá), **cổng phiên bản** (git giả 2.27
⇒ `[SKIP]`, không cài, 3 hook kia vẫn cài, `--check` trả thành công).

## 4. Bất biến (repo thật)

- `git rev-parse refs/heads/main`: **712178bce8f077b88570b0ec2963b22d1186828c** trước **và** sau
  toàn bộ ca âm — không đổi.
- `HEAD` trước/sau không đổi; **fingerprint index** (`git ls-files -s | sha256`) trước/sau không
  đổi (c0c8be14…453ebc) ⇒ không stage gì vào checkout chính.
- Số worktree 29 → 29; nhánh chỉ còn `main` + `wip/r224` (không nhánh tạm nào sót); clone/repo
  nháp/git giả trong `/tmp` đã xoá sạch.
- Ca không-phá trên repo thật (`kt85b-khong-pha.log`): tạo/xoá nhánh task bằng vai agent **vẫn
  được**; `git update-ref refs/heads/main` (kể cả về đúng giá trị đang có) **bị chặn**.

## 5. Phát hiện mới (ghi vào tài liệu)

1. **git đời mới có BỐN trạng thái** `reference-transaction`: `preparing`, `prepared`, `committed`,
   `aborted` (máy này thấy `preparing` ở `update-ref`, `prepared` ở cherry-pick/merge/reset).
   Tài liệu git: mã thoát **chỉ** có tác dụng ở `preparing`/`prepared`. ⇒ Chỉ kiểm `prepared` như
   đề bài viết là **thiếu**; hook của ta chặn cả hai, fail-closed với trạng thái lạ.
2. **`reset --hard` bị từ chối vẫn có thể làm bẩn cây làm việc**: git ghi index + cây **trước**
   khi cập nhật ref; ref (lịch sử) nguyên vẹn nhưng cây có thể đã ở bản commit đích. Ca C-b
   trong log ghi lại đúng hiện tượng (`D viscript/agent/JOURNAL/khoa-may-do-luong.md`) và cách
   phục hồi bằng `VIFAST_ROLE=lead git reset --hard <sha-main>`. Đã ghi vào docs §7.
3. `commit-msg` chạy **sau** `pre-commit` ⇒ với vai `agent2` luôn thấy thông điệp `pre-commit`;
   muốn chứng minh hook nhãn phải dùng vai `lead` + message sai (đã làm ở D2).

## 6. Cổng cuối

- `./v2/bin/v2 check`: **1 bước FAIL — KHÔNG do KT-85b**. Bước "luật máy K6" đỏ vì
  `v2/agent/reports/TRA-LOI-A0-D2-125.md` (commit `155af460`, **của lead**, ngoài vùng ghi của
  tôi): 49 lỗi `E1_NUMBER_*` (số ngoài khối `scope=`). Toàn bộ bước khác OK (fmt, clippy, luật
  cấu trúc, self-test luật, báo cáo, ABI reserved, gen-check, IDL, build L0). Tệp của KT-85b
  **không** bị luật 1 bắt (đã sửa tài liệu: bỏ số khỏi prose, ngưỡng viết `git-2.28`, số liệu
  trích qua log). Log: `kt85b-v2-check.log`.
  → **Việc của lead**: sửa/ miễn trừ đúng tệp `TRA-LOI-A0-D2-125.md` (đưa số vào khối `scope=`
  trỏ log đã commit) rồi `v2 check` sẽ xanh; tôi không được chạm tệp đó.
- `install.sh --check`: **PASS** cả 4 hook (`kt85b-trang-thai-cuoi.log`).
- QEMU: **0 do task này tạo** (2 tiến trình qemu đang chạy thuộc task khác — kt86/kt77c — không
  giết, không đụng).

## 7. Chưa xong / bàn giao

- **Lead commit** (tôi không commit): 3 tệp mới trong `v2/tools/git-guard/`, `install.sh` +
  `v2/docs/k6-git-guard.md` đã sửa, 6 log `v2/agent/JOURNAL/kt85b-*` + journal này.
  Lệnh gợi ý: `VIFAST_ROLE=lead git commit -m '[KT-85b] hook reference-transaction + commit-msg (D2-125.2), 4 ca am'`
  (hook `commit-msg` mới sẽ kiểm chính message đó — nhãn `[KT-85b]` hợp lệ).
- **Bảo vệ nhánh GitHub**: chưa thi hành (repo cục bộ, chưa có remote GitHub). Hướng dẫn đầy đủ ở
  `v2/docs/k6-git-guard.md` **§6** (require PR + review, status check, no force push, no
  deletions, **restrict who can push** = chỉ Agent 1, linear history, bảo vệ tag). Agent 1 gửi cho
  người dùng; ghi lại ai bật + ngày + ảnh chụp cấu hình.
- **Máy git < 2.28**: nếu có, `reference-transaction` không tồn tại ⇒ A1-183 **chưa** vá ở máy đó;
  `install.sh` báo CẢNH BÁO, phải nâng git hoặc dựa vào GitHub.
- Chưa làm (ngoài phạm vi đề bài): hook kiểm diff có nằm trong vùng ghi của task; hook gọi
  `v2 check --staged`; chặn `commit-tree`/`update-ref` từ **máy khác** (chỉ GitHub chặn được).

## 8. Freeze + delta (để lead soát)

```
 M v2/docs/k6-git-guard.md                              (sửa: 4 hook + §6 GitHub + §7 giới hạn)
 M v2/tools/git-guard/install.sh                        (sửa: 4 hook, cổng git-2.28, --print-support)
?? v2/tools/git-guard/reference-transaction              (mới, hook)
?? v2/tools/git-guard/commit-msg                         (mới, hook)
?? v2/tools/git-guard/neg-test-kt85b.sh                  (mới, ca âm)
?? v2/agent/JOURNAL/kt85b-journal.md                     (mới, tệp này)
?? v2/agent/JOURNAL/kt85b-hook-neg.log                   (mới, bằng chứng ca âm)
?? v2/agent/JOURNAL/kt85b-install-check.log              (mới, install/check/sha256)
?? v2/agent/JOURNAL/kt85b-v2-check.log                   (mới, cổng check)
?? v2/agent/JOURNAL/kt85b-git-version.log                (mới, ngưỡng + phiên bản git)
?? v2/agent/JOURNAL/kt85b-khong-pha.log                  (mới, ca không-phá)
?? v2/agent/JOURNAL/kt85b-uninstall.log                  (mới, gỡ cả 4 hook)
?? v2/agent/JOURNAL/kt85b-trang-thai-cuoi.log            (mới, trạng thái cuối + QEMU)
```

Không tệp nào ngoài danh sách trên bị tôi sửa; `refs/heads/main` không đổi; không có commit nào
được tạo bởi tôi.
