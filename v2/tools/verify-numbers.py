#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""KT-80 — luật máy OD-K6-5 (luật 1, 2, 3, 7, 8) cho dòng OS ViFastOs v2.

Tổng quát hoá bốn verifier cũ (`JOURNAL/kt73-verify-numbers.sh`,
`kt74/kt75/kt76/kt78-verify-numbers.sh` + `kt76_verify_numbers.py`) thành **một** công cụ;
KHÔNG copy-paste bốn bản. Nguồn luật:

* **Luật 1** (OD-K6-5 #1, D2-109C + D2-110A + D2-112): mọi số trong tài liệu đích
  (`v2/agent/reports/*.md`, `v2/docs/k6-*.md`) phải nằm trong khối ```` ```text scope=<log> ````
  và **trích nguyên văn từng dòng** từ đúng tệp log đó. Log phải **nằm trong repo** và
  **đã có trong git** (index) — `/tmp` hoặc đường dẫn ngoài repo không được làm nguồn.
  Ngoài khối chỉ được có **định danh** theo lớp cú pháp (mỗi lớp một lý do trong `ALLOW`);
  **không** có bảng miễn trừ chứa chữ số (D2-112b).
* **Luật 2** (OD-K6-5 #2): commit chứa log phải là **tổ tiên** của commit chứa báo cáo
  (hoặc cùng commit). Dùng `git log -1 --format=%H -- <path>` + `git merge-base --is-ancestor`;
  chạy được trên cây bẩn vì chỉ đọc lịch sử commit (HEAD), không đọc nội dung cây.
* **Luật 3** (OD-K6-5 #3, D2-108B): mỗi dòng evidence khai `counter=<tên>` phải có khai báo
  trong `v2/test/rules/counters.toml`; `proven=false` ⇒ **không** được nằm trong khối `scope=`
  (khối được coi là `ok`), chỉ được nằm trong `scope-pending=`; `proven=true` ⇒ phải có **log
  ca âm ở trong repo** cho thấy bộ đếm **khác 0**. Fail-closed.
* **Luật 7** (OD-K6-5 #7, D2-118B): bản trích tiêu chí (`criteria.txt`, `K*-DOD.md`) phải khai
  nguồn trong `v2/test/rules/criteria.toml`; **số cột trích = số cột gốc** trừ cột bị bỏ có
  ghi tên + lý do ngay trong tệp trích (`<!-- cot-bo: … — ly-do: … -->`). Tệp `criteria.txt`
  hoặc `*-DOD.md` không khai báo ⇒ ĐỎ.
* **Luật 8** (OD-K6-5 #8, D2-113F): `--self-test` chạy **mọi** đường phá đã từng lọt trong
  `v2/test/rules/verify-script-regressions.txt` trên **bản copy** (thư mục tạm) và đòi **ĐỎ
  từng ca, đúng mã lỗi**; nếu ai sửa verifier làm một ca cũ xanh ⇒ self-test ĐỎ.

Ngoại lệ lịch sử: tệp có trong `[[legacy]]` của `v2/test/rules/numbers-policy.toml` được miễn
luật 1/3/7 (`sha256` ghim nội dung — sửa tệp ⇒ ĐỎ, phải xử lý lại). Luật 2 luôn chạy.

Dùng:
    python3.12 v2/tools/verify-numbers.py                 # cả 5 luật (dùng trong cổng)
    python3.12 v2/tools/verify-numbers.py --law 1         # một luật
    python3.12 v2/tools/verify-numbers.py --self-test     # = luật 8
    python3.12 v2/tools/verify-numbers.py --root <repo>   # kiểm bản copy (không bao giờ ghi)
Exit 0 = mọi luật được yêu cầu XANH; 1 = có luật ĐỎ; 2 = lỗi cấu hình/registry.
"""

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

RULES_DIR_REL = os.path.join("v2", "test", "rules")
JOURNAL_REL = os.path.join("v2", "agent", "JOURNAL")
POLICY_NAME = "numbers-policy.toml"
COUNTERS_NAME = "counters.toml"
CRITERIA_NAME = "criteria.toml"
REGRESSIONS_NAME = "verify-script-regressions.txt"

DEFAULT_TARGET_GLOBS = ("v2/agent/reports/*.md", "v2/docs/k6-*.md")

# ------------------------------------------------------------------ định danh (không phải số đo)
# D2-112(b): **không** bảng miễn trừ chứa chữ số. Mỗi lớp dưới đây là một **định danh** cú pháp,
# KHÔNG phải một số đo; mọi lớp đều có lý do. Muốn thêm lớp mới: thêm lý do + ca phá ở luật 8
# (một lớp quá rộng sẽ bị chính `--self-test` bắt nếu nó che một ca phá đã đăng ký).
ALLOW = [
    (re.compile(r"^0x[0-9a-fA-F_]+$"), "hex literal (địa chỉ/giá trị nhị phân)"),
    (re.compile(r"^KT-\d+[a-z]?$"), "mã task"),
    (re.compile(r"^D2-\d+[A-Za-z]?$"), "mã quyết định v2"),
    (re.compile(r"^D-\d+$"), "mã quyết định v1"),
    (re.compile(r"^K[0-9]+(\.[0-9]+)?$"), "mã mốc (K6, K3.5)"),
    (re.compile(r"^S\d+$"), "mã dòng phát hiện (S1)"),
    (re.compile(r"^v[0-9]+$"), "nhãn phiên bản (v1/v2)"),
    (re.compile(r"^\d{4}-\d{2}-\d{2}$"), "ngày"),
    (re.compile(r"^\d{2}:\d{2}(:\d{2})?$"), "giờ"),
    (re.compile(r"^§\d+(\.\d+)*$"), "tham chiếu tiết"),
    (re.compile(r"^[A-Za-z_][A-Za-z0-9_/.-]*\.(rs|md|sh|toml|elf|log|txt|py|json|vidl|out|png|csv|yml):\d+(-\d+)?$"),
     "tham chiếu `file:line`"),
    (re.compile(r"^[A-Za-z][A-Za-z0-9_/-]*\.\d+(\.\d+)*$"), "định danh có tiền tố chữ + số hiệu"),
    (re.compile(r"^R\d+$"), "luật cấu trúc (R1..R5)"),
    (re.compile(r"^L[0-9IVX]+$"), "mức test (L0/L1) hoặc mã luật (LI, LVIII)"),
    (re.compile(r"^T-INFRA$"), "nhãn loại lỗi"),
    (re.compile(r"^(DONE|READY|BLOCKED|REVIEW|PASS|FAIL|OK|RED|GREEN|DRIFT)$"), "trạng thái"),
    (re.compile(r"^T\d+[a-z]?$"), "mã lỗi v1 (T26/T31/T33a)"),
    (re.compile(r"^P\d-\d+$"), "mã audit v1 (P1-5)"),
    (re.compile(r"^Q\d-[A-Z]$"), "mã câu hỏi v1 (Q1-A)"),
    (re.compile(r"^(x86_64|aarch64|riscv64|arm64|i386)$"), "tên kiến trúc"),
    (re.compile(r"^[A-Za-z]\d{1,2}$"), "mã một chữ + số ≤ 2 (C1, N7, D0, S1)"),
]

# Tên log ca âm/mutation: **không** được làm nguồn cho `scope=` (D2-112c).
NEGATIVE_NAME = re.compile(r"(mutation|break|selfbreak|neg|ca-am|red-)", re.IGNORECASE)
FENCE = re.compile(r"^```text[ \t]+scope(-red|-pending)?=(\S+)[ \t]*$")
NUMERIC = re.compile(r"^[0-9][0-9.,/:%-]*$")
QUOTED = re.compile(r"“([^”]+)”")
HEADING_SECTION = re.compile(r"^#{1,6}\s+(\d{1,2}(?:\.\d{1,2})*)\.?(?:\s|$)")
COUNTER_TOKEN = re.compile(r"(?<![A-Za-z0-9_])counter=([a-z][a-z0-9_]*)")
# Reviewer KT-80 (chặn): `drops=5` / `TESTS=729` trong prose từng lọt vì `numeric_tokens` chỉ lấy
# token tách bằng khoảng trắng. Bắt **giá trị số dính sau `=`** — trừ định danh: hex/0x, sha/build/
# digest/hash (giá trị băm), ngày, giờ. `wall_ms=9845`, `fail=0`, `seeds_total=400000` ⇒ ĐỎ.
ASSIGN = re.compile(r"(?<![A-Za-z0-9_])[A-Za-z_][A-Za-z0-9_.-]*=([0-9][0-9A-Za-z_+.,:/%-]*)")
IDENT_KEYS = {"sha", "sha256", "sha512", "build", "digest", "hash"}
ASSIGN_IDENT_VALUE = [
    re.compile(r"^0x[0-9a-fA-F_]+$"),           # hex
    re.compile(r"^[0-9a-fA-F]{8,}$"),           # băm/định danh hex dài
    re.compile(r"^\d{4}-\d{2}-\d{2}$"),        # ngày
    re.compile(r"^\d{2}:\d{2}(:\d{2})?$"),      # giờ
]


class ConfigError(Exception):
    """Registry sai/không đọc được — luôn fail-closed."""


# ------------------------------------------------------------------ TOML (có fallback 3.9)
def load_toml(path):
    """Đọc TOML; dùng tomllib (>=3.11) hoặc fallback tối thiểu cho python 3.9.

    Fallback chỉ hỗ trợ đúng tập con registry dùng: `[[table]]`, `key = "x"`, `key = true/false`,
    `key = 123`, mảng chuỗi một dòng. Gặp cú pháp khác ⇒ ConfigError (fail-closed, không đoán).
    """
    if not os.path.isfile(path):
        raise ConfigError(f"không có tệp registry {path}")
    raw = open(path, encoding="utf-8").read()
    try:
        import tomllib  # noqa: PLC0415 — chỉ có trên python >= 3.11

        return tomllib.loads(raw)
    except ImportError:
        pass
    data = {}
    cur = data
    for lineno, line in enumerate(raw.splitlines(), 1):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        m = re.match(r"^\[\[([A-Za-z0-9_.-]+)\]\]$", s)
        if m:
            key = m.group(1)
            arr = data.setdefault(key, [])
            if not isinstance(arr, list):
                raise ConfigError(f"{path}:{lineno}: '{key}' vừa bảng vừa mảng")
            cur = {}
            arr.append(cur)
            continue
        m = re.match(r"^\[([A-Za-z0-9_.-]+)\]$", s)
        if m:
            cur = data.setdefault(m.group(1), {})
            continue
        m = re.match(r'^([A-Za-z0-9_.-]+)\s*=\s*(.+)$', s)
        if not m:
            raise ConfigError(f"{path}:{lineno}: không hiểu dòng (fallback TOML)")
        key, val = m.group(1), m.group(2).strip()
        if val in ("true", "false"):
            cur[key] = val == "true"
        elif re.fullmatch(r"-?\d+", val):
            cur[key] = int(val)
        elif re.fullmatch(r'"[^"]*"', val):
            cur[key] = val[1:-1]
        elif re.fullmatch(r"\[[^\[\]]*\]", val):
            items = [x.strip() for x in val[1:-1].split(",") if x.strip()]
            out = []
            for it in items:
                if not re.fullmatch(r'"[^"]*"', it):
                    raise ConfigError(f"{path}:{lineno}: mảng chỉ nhận chuỗi (fallback TOML)")
                out.append(it[1:-1])
            cur[key] = out
        else:
            raise ConfigError(f"{path}:{lineno}: giá trị không hỗ trợ (fallback TOML)")
    return data


# ------------------------------------------------------------------ tiện ích
def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def read_committed_or_worktree(ctx, rel):
    """Bản **đã commit** (HEAD) nếu tệp có commit; ngược lại đọc working tree.

    Luật 2 là thuộc tính của **artifact đã commit**; bản nháp đang sửa trong working tree (của
    task song song) không được làm đỏ cổng của task này. Khi bản nháp được commit, nó thành HEAD
    và lần cổng kế tiếp kiểm nó. Cùng triết lý D2-105(b): xác minh bằng HEAD.
    """
    if ctx.get("is_git"):
        proc = subprocess.run(["git", "-C", ctx["root"], "show", f"HEAD:{rel}"],
                              capture_output=True, check=False)
        if proc.returncode == 0:
            return proc.stdout.decode("utf-8", errors="replace")
    return open(os.path.join(ctx["root"], rel), encoding="utf-8").read()


def sha256_committed(ctx, rel):
    """sha256 của bản **đã commit** (HEAD) — ngoại lệ ghim nội dung đã được duyệt.

    Sửa tệp trong working tree (bản nháp của task khác) **không** làm đỏ oan; nhưng khi bản sửa
    được **commit** thì sha HEAD đổi ⇒ ĐỎ cho tới khi ngoại lệ được xử lý lại. Bản copy không có
    git (tự phá) dùng nội dung working tree.
    """
    if not ctx.get("is_git"):
        return sha256_file(os.path.join(ctx["root"], rel))
    proc = subprocess.run(["git", "-C", ctx["root"], "show", f"HEAD:{rel}"],
                          capture_output=True, check=False)
    if proc.returncode != 0:
        return ""
    return hashlib.sha256(proc.stdout).hexdigest()


def git(root, *args):
    proc = subprocess.run(
        ["git", "-C", root, *args], capture_output=True, text=True, check=False
    )
    return proc.returncode, proc.stdout.strip(), proc.stderr.strip()


def is_git(root):
    rc, out, _ = git(root, "rev-parse", "--git-dir")
    return rc == 0 and bool(out)


def tracked(ctx, rel):
    """Tệp có trong **index** (đã `git add`, kể cả `-N`) — không đòi đã commit."""
    if ctx.get("tracked_set") is None:
        return True
    return rel in ctx["tracked_set"]


def last_commit(ctx, rel):
    cache = ctx.setdefault("commit_cache", {})
    if rel in cache:
        return cache[rel]
    root = ctx["root"]
    if not ctx.get("is_git"):
        cache[rel] = None
        return None
    rc, out, _ = git(root, "log", "-1", "--format=%H", "--", rel)
    cache[rel] = out or None if rc == 0 else None
    return cache[rel]


def is_ancestor(root, older, newer):
    if not older or not newer:
        return False
    rc, _, _ = git(root, "merge-base", "--is-ancestor", older, newer)
    return rc == 0


def ancestry_cache_path(root):
    """Cache (a,b) → quan hệ tổ tiên. Quan hệ commit là **bất biến** nên cache không cũ đi.

    Không bao giờ ghi vào repo: mặc định ở `TMPDIR`; `KT80_ANCESTRY_CACHE` để đổi chỗ.
    66 cặp × ~64 ms/lần gọi git trên macOS là ~4 s mỗi cổng ⇒ cache chỉ để **giảm wall**,
    không tham gia quyết định đúng/sai (mỗi khoá vẫn được kiểm bằng `git` ở lần đầu).
    """
    env = os.environ.get("KT80_ANCESTRY_CACHE")
    if env:
        return env
    tag = hashlib.sha256(os.path.realpath(root).encode()).hexdigest()[:12]
    return os.path.join(tempfile.gettempdir(), f"kt80-ancestry-{tag}.json")


def relpath(root, path):
    return os.path.relpath(os.path.abspath(path), os.path.abspath(root)).replace(os.sep, "/")


def inside(root, path):
    root_real = os.path.realpath(root)
    path_real = os.path.realpath(path)
    return path_real == root_real or path_real.startswith(root_real + os.sep)


def target_docs(root, globs):
    import glob as globmod

    docs = []
    for pat in globs:
        for p in globmod.glob(os.path.join(root, pat)):
            if os.path.isfile(p) and p not in docs:
                docs.append(p)
    return sorted(docs)


# ------------------------------------------------------------------ cấu trúc tài liệu
def parse_doc(text):
    """Tách (prose, blocks). Mỗi block: kind, scope, start, lines=[(lineno, text)]."""
    prose = []
    blocks = []
    cur = None
    for lineno, line in enumerate(text.splitlines(), 1):
        if cur is None:
            m = FENCE.match(line.strip())
            if m:
                cur = {
                    "kind": (m.group(1) or "scope")[1:] if m.group(1) else "scope",
                    "scope": m.group(2),
                    "start": lineno,
                    "lines": [],
                }
                # m.group(1) là "-red"/"-pending" (None cho "scope")
                cur["kind"] = {None: "scope", "-red": "red", "-pending": "pending"}[m.group(1)]
                continue
            prose.append((lineno, line))
            continue
        if line.strip() == "```":
            blocks.append(cur)
            cur = None
            continue
        cur["lines"].append((lineno, line.rstrip("\r")))
    if cur is not None:
        raise ConfigError(f"khối scope mở ở dòng {cur['start']} nhưng chưa đóng")
    return prose, blocks


def numeric_tokens(line):
    """Token số **đứng riêng** (bỏ dấu markdown hai đầu) — định nghĩa của kt76, giữ tương thích.

    Giới hạn đã biết (ghi ở `v2/docs/k6-machine-rules.md`): token dính sau `key=` (ví dụ
    `writes=0`) được coi là **định danh trường evidence** như kt76; các ca phá đã đăng ký ở
    luật 8 không phụ thuộc giới hạn này.
    """
    out = []
    for raw in line.split():
        tok = raw.strip("`*_()[]{}<>\"'.,;:!?|")
        if tok and NUMERIC.fullmatch(tok):
            out.append(tok)
    return out


def allowed_identifier(tok):
    return any(rx.fullmatch(tok) for rx, _why in ALLOW)


def resolve_log(root, scope):
    """Trả (path, rel, lỗi). Log phải nằm trong repo; không nhận đường dẫn tuyệt đối/escape."""
    if os.path.isabs(scope):
        return None, None, ("E1_SCOPE_OUTSIDE_REPO", f"scope tuyệt đối [{scope}] — log phải ở trong repo")
    if ".." in scope.split("/"):
        return None, None, ("E1_SCOPE_OUTSIDE_REPO", f"scope thoát repo [{scope}]")
    cands = [os.path.join(root, scope), os.path.join(root, JOURNAL_REL, scope)]
    for cand in cands:
        if os.path.isfile(cand):
            if not inside(root, cand):
                return None, None, ("E1_SCOPE_OUTSIDE_REPO", f"scope ngoài repo [{scope}]")
            return cand, relpath(root, cand), None
    return None, None, ("E1_SCOPE_MISSING", f"không thấy log [{scope}] trong repo")


def log_lines(ctx, path):
    cache = ctx.setdefault("log_cache", {})
    if path not in cache:
        cache[path] = set(
            ln.rstrip("\r")
            for ln in open(path, encoding="utf-8", errors="replace").read().splitlines()
        )
    return cache[path]


# ------------------------------------------------------------------ luật 1
def context_excluded(ctx, rel):
    for e in ctx["policy"].get("excluded", []):
        if e.get("path") == rel:
            return e
    return {}


def law1(root, ctx):
    """Số do script sinh từ log đã commit + trích nguyên văn. Trả list lỗi."""
    fails = []
    policy = ctx["policy"]
    legacy = {e["path"]: e for e in policy.get("legacy", [])}
    negative_reg = {e["name"]: e for e in policy.get("negative_log", [])}
    generated = ctx["generated_criteria"]

    # (0) ngoại lệ lịch sử: ghim sha256 — sửa nội dung ⇒ ĐỎ (không thể "thêm số rồi miễn").
    for rel, entry in sorted(legacy.items()):
        path = os.path.join(root, rel)
        if not os.path.isfile(path):
            fails.append((rel, 0, "E1_LEGACY_STALE", "ngoại lệ trỏ tệp không tồn tại"))
            continue
        want = entry.get("sha256", "")
        if not re.fullmatch(r"[0-9a-f]{64}", want or ""):
            fails.append((rel, 0, "E1_LEGACY_SHA", "ngoại lệ thiếu sha256 (64 hex)"))
            continue
        got = sha256_committed(ctx, rel)
        if got != want:
            fails.append(
                (rel, 0, "E1_LEGACY_SHA",
                 "nội dung ngoại lệ đã đổi (sha256 khác) — phải đưa số vào khối scope= hoặc "
                 "cập nhật ngoại lệ có tên+lý do+ngày")
            )

    excluded = ctx["excluded"]
    if ctx.get("is_git"):
        # chỉ kiểm "mục mồ côi" trên repo thật; bản copy tự phá không chép tệp excluded
        for rel in sorted(excluded):
            if not os.path.isfile(os.path.join(root, rel)):
                fails.append((rel, 0, "E1_EXCLUDED_STALE", "mục [[excluded]] trỏ tệp không tồn tại"))
    pre = [rel for rel, e in legacy.items() if e.get("kind") == "pre_existing"]
    if pre:
        for rel in pre:
            entry = legacy[rel]
            print(f"# note - luat 1: ngoại lệ 'có trước luật' [{rel}] chủ {entry.get('owner', '?')}: "
                  f"{entry.get('debt', 'không ghi nợ')}")
    ctx["law1_pre_existing"] = len(pre)
    docs = ctx["docs"]
    checked = 0
    ctx["law1_excluded"] = len(excluded)
    for rel in sorted(excluded):
        entry = context_excluded(ctx, rel)
        print(f"# note - luat 1: loại khỏi tài liệu đích [{rel}] chủ {entry.get('owner', '?')}: "
              f"{entry.get('reason', '')}")
    for rel in docs:
        if rel in legacy or rel in excluded:
            continue
        checked += 1
        path = os.path.join(root, rel)
        text = open(path, encoding="utf-8").read()
        try:
            prose, blocks = parse_doc(text)
        except ConfigError as exc:
            fails.append((rel, 0, "E1_BLOCK_UNCLOSED", str(exc)))
            continue
        # (1) khối scope: log hợp lệ + dòng trích nguyên văn.
        for block in blocks:
            scope = block["scope"]
            logpath, logrel, err = resolve_log(root, scope)
            if err:
                code, why = err
                fails.append((rel, block["start"], code, f"`{scope}`: {why}"))
                continue
            if not tracked(ctx, logrel):
                fails.append((rel, block["start"], "E1_SCOPE_UNTRACKED",
                              f"log `{logrel}` chưa có trong git (index) — phải commit log trước"))
            base = os.path.basename(logrel)
            entry_neg = negative_reg.get(base)
            listed_neg = entry_neg is not None and not entry_neg.get("mixed", False)
            if block["kind"] == "scope":
                if NEGATIVE_NAME.search(base) or listed_neg:
                    fails.append((rel, block["start"], "E1_SCOPE_FORBIDDEN",
                                  f"`scope={base}` là log ca âm/mutation — D2-112c cấm làm nguồn; "
                                  "dùng `scope-red=`"))
                    continue
            elif block["kind"] == "red":
                if not (NEGATIVE_NAME.search(base) or entry_neg is not None):
                    fails.append((rel, block["start"], "E1_SCOPE_RED_NOT_NEGATIVE",
                                  f"`scope-red={base}` không phải tên log ca âm/mutation và không có "
                                  "trong [[negative_log]]"))
                    continue
            lines = log_lines(ctx, logpath)
            for lineno, line in block["lines"]:
                if not line.strip():
                    continue
                if line not in lines:
                    fails.append((rel, lineno, "E1_NOT_VERBATIM",
                                  f"dòng không có nguyên văn trong `{logrel}`: {line[:120]}"))
            # tệp trích tiêu chí sinh từ máy: khối phải bằng **toàn bộ** tệp (không trích lẻ né số)
            if logrel in generated:
                body = "\n".join(ln for _n, ln in block["lines"]).rstrip("\n")
                whole = open(logpath, encoding="utf-8").read().rstrip("\n")
                if body != whole:
                    fails.append((rel, block["start"], "E1_CRITERIA_PARTIAL",
                                  f"khối `scope={base}` không bằng toàn bộ tệp tiêu chí sinh từ máy"))
        # (2) prose: chỉ định danh; số đo ⇒ ĐỎ.
        heading = None
        for lineno, line in prose:
            stripped = line.strip()
            hs = HEADING_SECTION.match(stripped) if stripped.startswith("#") else None
            scan = line
            # Đoạn trích dẫn nguyên văn của tệp tiêu chí được **bỏ khỏi** dòng trước khi bắt số
            # (không thể "mượn" một trích dẫn hợp lệ để hợp thức hoá số khác cùng dòng).
            for quote in [q for q in QUOTED.findall(line) if len(q.replace(" ", "")) >= 4]:
                for gen_path in generated.values():
                    if quote in gen_path:
                        scan = scan.replace(quote, " ")
                        break
            scan = re.sub(r"Agent[ ]+\d+", "Agent", scan)
            for tok in numeric_tokens(scan):
                if allowed_identifier(tok):
                    continue
                if hs and tok.rstrip(".") == hs.group(1):
                    continue
                fails.append((rel, lineno, "E1_NUMBER_OUTSIDE",
                              f"số '{tok}' ngoài khối `scope=` (phải là số đo trích từ log)"))
            # (reviewer KT-80) số đo dạng `key=value` ở prose — dạng bằng chứng repo dùng nhiều nhất
            for m in ASSIGN.finditer(scan):
                key = scan[max(0, m.start()):m.start() + m.group(0).index("=")]
                value = m.group(1).rstrip(".,;:)%")
                if not value or key.lower() in IDENT_KEYS:
                    continue
                if any(rx.fullmatch(value) for rx in ASSIGN_IDENT_VALUE):
                    continue
                fails.append((rel, lineno, "E1_NUMBER_ASSIGNED",
                              f"số '{key}={value}' ngoài khối `scope=` (số đo gán trong prose)"))
    ctx["law1_checked"] = checked
    return fails


# ------------------------------------------------------------------ luật 2
def batch_latest_commits(root, paths):
    """Commit **mới nhất** chạm từng đường dẫn — MỘT lệnh `git log` cho cả tập.

    `git log --format=@@%H --name-only --no-renames -- <paths>` liệt kê mọi commit chạm một
    trong các đường dẫn, mới nhất trước; lần xuất hiện **đầu tiên** của một đường là commit
    mới nhất của nó. Một lệnh ~120 ms thay cho ~66 lệnh `git log -1 -- <path>` (~3,3 s).
    """
    paths = sorted(set(paths))
    if not paths:
        return {}
    rc, out, err = git(root, "log", "--format=@@%H", "--name-only", "--no-renames", "--", *paths)
    if rc != 0:
        raise ConfigError(f"git log lỗi: {err[:200]}")
    result = {}
    cur = None
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        if line.startswith("@@"):
            cur = line[2:]
        elif cur and line in paths and line not in result:
            result[line] = cur
    return result


def law2(root, ctx):
    """Luật 2 — commit chứa log phải là **tổ tiên** của commit chứa báo cáo (hoặc cùng commit).

    Dùng đúng chuỗi lệnh được giao: `git log --format=%H -- <path>` (lấy commit chứa tệp, gộp
    một lệnh cho cả tập) + `git merge-base --is-ancestor` (so cặp commit). Chạy được trên cây
    bẩn: chỉ đọc lịch sử commit (HEAD), không dựa vào nội dung working tree. Quan hệ commit là
    bất biến ⇒ kết quả `--is-ancestor` được cache ở `TMPDIR` theo cặp (a,b) — cache chỉ giảm
    wall, không thay đổi kết luận.
    """
    fails = []
    per_doc = []
    all_paths = set(ctx["docs"])
    for rel in ctx["docs"]:
        path = os.path.join(root, rel)
        try:
            text = read_committed_or_worktree(ctx, rel)
            _prose, blocks = parse_doc(text)
        except ConfigError as exc:
            fails.append((rel, 0, "E2_BLOCK_UNCLOSED", str(exc)))
            continue
        entries = []
        for block in blocks:
            _logpath, logrel, err = resolve_log(root, block["scope"])
            if err:
                continue  # luật 1 báo lỗi nguồn; ở đây không nhân đôi
            entries.append((block["start"], logrel))
            all_paths.add(logrel)
        per_doc.append((rel, entries))
    if not ctx.get("is_git"):
        return fails
    try:
        latest = batch_latest_commits(root, all_paths)
    except ConfigError as exc:
        return fails + [("", 0, "E2_BAD_REGISTRY", str(exc))]
    cache_path = ancestry_cache_path(root)
    memo = {}
    ctx["ancestor_cache"] = memo
    ctx["ancestor_dirty"] = False
    try:
        import json as jsonmod

        with open(cache_path, encoding="utf-8") as fh:
            memo.update(jsonmod.load(fh))
    except (OSError, ValueError):
        pass
    pairs = 0
    pending = 0
    for rel, entries in per_doc:
        report_commit = latest.get(rel)
        seen_logs = set()
        for start, logrel in entries:
            if logrel in seen_logs:
                continue
            seen_logs.add(logrel)
            log_commit = latest.get(logrel)
            if log_commit is None:
                if report_commit is None:
                    pending += 1
                    continue
                fails.append((rel, start, "E2_LOG_NOT_COMMITTED",
                              f"báo cáo đã commit nhưng log `{logrel}` chưa commit"))
                continue
            if report_commit is None:
                pending += 1
                continue
            pairs += 1
            skey = f"{log_commit}:{report_commit}"
            if log_commit == report_commit:
                memo[skey] = True
            elif skey not in memo:
                memo[skey] = is_ancestor(root, log_commit, report_commit)
                ctx["ancestor_dirty"] = True
            if not memo[skey]:
                fails.append((rel, start, "E2_ORDER",
                              f"log `{logrel}` commit {log_commit[:8]} KHÔNG là tổ tiên của "
                              f"báo cáo {report_commit[:8]} — phải commit log trước"))
    if ctx.get("ancestor_dirty"):
        try:
            import json as jsonmod

            with open(cache_path, "w", encoding="utf-8") as fh:
                jsonmod.dump(memo, fh)
        except OSError:
            pass  # cache chỉ là tối ưu wall
    ctx["law2_pairs"] = pairs
    ctx["law2_pending"] = pending
    return fails


# ------------------------------------------------------------------ luật 3
def law3(root, ctx):
    """Bộ đếm báo 0 phải có ca âm; proven=false thì không được nằm trong khối ok."""
    fails = []
    reg = ctx["counters"]
    by_name = {}
    for entry in reg.get("counter", []):
        name = entry.get("name", "")
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name):
            fails.append((COUNTERS_NAME, 0, "E3_BAD_REGISTRY", f"tên bộ đếm không hợp lệ [{name}]"))
            continue
        if name in by_name:
            fails.append((COUNTERS_NAME, 0, "E3_BAD_REGISTRY", f"khai báo trùng bộ đếm [{name}]"))
        entry.setdefault("proven", False)
        entry.setdefault("negative_pattern", re.escape(name) + r"=[1-9][0-9]*")
        by_name[name] = entry
        if not entry.get("reason"):
            fails.append((COUNTERS_NAME, 0, "E3_BAD_REGISTRY", f"[{name}] thiếu lý do"))
        if entry["proven"]:
            neglog = entry.get("negative_log", "")
            logpath, logrel, err = resolve_log(root, neglog)
            if err:
                fails.append((COUNTERS_NAME, 0, "E3_NO_NEGATIVE_LOG",
                              f"[{name}] thiếu log ca âm trong repo [{neglog}]"))
                continue
            if not tracked(ctx, logrel):
                fails.append((COUNTERS_NAME, 0, "E3_NO_NEGATIVE_LOG",
                              f"[{name}] log ca âm `{logrel}` chưa có trong git"))
                continue
            body = open(logpath, encoding="utf-8", errors="replace").read()
            try:
                rx = re.compile(entry["negative_pattern"], re.MULTILINE)
            except re.error as exc:
                fails.append((COUNTERS_NAME, 0, "E3_BAD_REGISTRY",
                              f"[{name}] negative_pattern sai: {exc}"))
                continue
            if not rx.search(body):
                fails.append((COUNTERS_NAME, 0, "E3_NEGATIVE_ZERO",
                              f"[{name}] log ca âm `{logrel}` không khớp "
                              f"`{entry['negative_pattern']}` ⇒ chưa chứng minh bộ đếm khác 0"))
    # quét tài liệu (mọi tài liệu đích, kể cả legacy: dòng counter= không được lọt vào ngoại lệ)
    for rel in ctx["docs"]:
        path = os.path.join(root, rel)
        try:
            prose, blocks = parse_doc(open(path, encoding="utf-8").read())
        except ConfigError:
            continue
        where = []
        for lineno, line in prose:
            for m in COUNTER_TOKEN.finditer(line):
                where.append((lineno, m.group(1), "prose"))
        for block in blocks:
            for lineno, line in block["lines"]:
                for m in COUNTER_TOKEN.finditer(line):
                    where.append((lineno, m.group(1), block["kind"]))
        for lineno, name, kind in where:
            entry = by_name.get(name)
            if entry is None:
                fails.append((rel, lineno, "E3_COUNTER_UNDECLARED",
                              f"bộ đếm `{name}` chưa khai trong {COUNTERS_NAME}"))
                continue
            if kind == "prose":
                fails.append((rel, lineno, "E3_COUNTER_OUTSIDE_BLOCK",
                              f"`counter={name}` phải nằm trong khối scope= có log chứng minh"))
            elif kind == "scope" and not entry["proven"]:
                fails.append((rel, lineno, "E3_UNPROVEN_IN_OK",
                              f"`counter={name}` có proven=false nhưng nằm trong khối scope= "
                              "(khối được coi là ok) — chuyển sang `scope-pending=`"))
    used = set()
    for rel in ctx["docs"]:
        try:
            text = open(os.path.join(root, rel), encoding="utf-8").read()
        except OSError:
            continue
        used.update(m.group(1) for m in COUNTER_TOKEN.finditer(text))
    ctx["law3_unused"] = sorted(set(by_name) - used)
    ctx["law3_used"] = sorted(used)
    return fails


# ------------------------------------------------------------------ luật 7
def cells(line):
    body = line.strip()
    if body.startswith("|"):
        body = body[1:]
    if body.endswith("|"):
        body = body[:-1]
    return [c.strip() for c in body.split("|")]


def parse_omissions(text):
    """Đọc `<!-- cot-bo: Tên cột — ly-do: … -->` (nhiều dòng được)."""
    out = []
    for m in re.finditer(r"<!--\s*cot-bo\s*:(.*?)-->", text, re.DOTALL):
        body = " ".join(m.group(1).split())
        parts = re.split(r"—|--|ly-do:", body, maxsplit=1)
        names = [x.strip() for x in re.split(r"[,;]", parts[0]) if x.strip()]
        reason = parts[1].strip() if len(parts) > 1 else ""
        for name in names:
            out.append((name, reason))
    return out


def source_heading_text(path, heading):
    lines = open(path, encoding="utf-8").read().splitlines()
    start = [i for i, ln in enumerate(lines) if ln.startswith(heading)]
    if not start:
        raise ConfigError(f"không thấy tiêu đề [{heading}] trong {path}")
    i = start[0]
    end = [j for j, ln in enumerate(lines[i + 1:], i + 1) if ln.startswith("## ")]
    return lines[i:end[0] if end else len(lines)]


def law7(root, ctx):
    """Bản trích tiêu chí giữ đủ cột so với bảng gốc (hoặc ghi rõ cột bỏ + lý do)."""
    fails = []
    reg = ctx["criteria"]
    declared = set()
    for ex in reg.get("extraction", []):
        target_rel = ex.get("file", "")
        source_rel = ex.get("source", "")
        declared.add(target_rel)
        target = os.path.join(root, target_rel)
        source = os.path.join(root, source_rel)
        for p, what in ((target, "tệp trích"), (source, "bảng gốc")):
            if not os.path.isfile(p):
                fails.append((target_rel, 0, "E7_SOURCE_MISSING", f"không thấy {what} {relpath(root, p)}"))
                break
        else:
            text = open(target, encoding="utf-8").read()
            omissions = parse_omissions(text)
            for name, reason in omissions:
                if len(reason) < 10:
                    fails.append((target_rel, 0, "E7_OMIT_UNDECLARED",
                                  f"cột bỏ [{name}] thiếu lý do (≥10 ký tự) trong `<!-- cot-bo: … -->`"))
            kind = ex.get("kind", "")
            if kind == "table-rows":
                lines = open(source, encoding="utf-8").read().splitlines()
                header = [ln for ln in lines if ln.startswith(ex.get("header", "| # |"))]
                if len(header) != 1:
                    fails.append((target_rel, 0, "E7_SOURCE_MISSING",
                                  f"bảng gốc phải có đúng một dòng tiêu đề [{ex.get('header')}]"))
                    continue
                header_cells = cells(header[0])
                ncols = len(header_cells)
                if ex.get("columns") != ncols:
                    fails.append((source_rel, 0, "E7_COLUMN_MISMATCH",
                                  f"registry khai {ex.get('columns')} cột nhưng bảng gốc có {ncols}"))
                for name, _reason in omissions:
                    if name not in header_cells:
                        fails.append((target_rel, 0, "E7_OMIT_UNDECLARED",
                                      f"cột bỏ [{name}] không có trong bảng gốc"))
                expected = ncols - len(omissions)
                rows = [ln for ln in text.splitlines() if re.match(r"^N\d+[ \t]*\|", ln)]
                if len(rows) != ex.get("rows"):
                    fails.append((target_rel, 0, "E7_COLUMN_MISMATCH",
                                  f"trích {len(rows)} dòng, registry đòi {ex.get('rows')}"))
                for row in rows:
                    got = len(cells(row))
                    if got != expected:
                        fails.append((target_rel, 0, "E7_COLUMN_MISMATCH",
                                      f"dòng [{row[:24]}…] có {got} cột, cần {expected} "
                                      f"({ncols} gốc − {len(omissions)} bỏ)"))
            elif kind == "design-row":
                part = source_heading_text(source, ex.get("heading", ""))
                rows = [ln for ln in part if ln.startswith(ex.get("row_prefix", "| K6 |"))]
                if len(rows) != 1:
                    fails.append((source_rel, 0, "E7_SOURCE_MISSING",
                                  f"bảng gốc phải có đúng một dòng [{ex.get('row_prefix')}] ở "
                                  f"[{ex.get('heading')}]"))
                    continue
                src_cells = cells(rows[0])
                if ex.get("columns") != len(src_cells):
                    fails.append((source_rel, 0, "E7_COLUMN_MISMATCH",
                                  f"registry khai {ex.get('columns')} cột nhưng dòng gốc có "
                                  f"{len(src_cells)}"))
                tgt = [ln for ln in text.splitlines() if ln.startswith(ex.get("row_prefix", "| K6 |"))]
                if len(tgt) != 1:
                    fails.append((target_rel, 0, "E7_COLUMN_MISMATCH",
                                  f"tệp trích phải có đúng một dòng [{ex.get('row_prefix')}]"))
                    continue
                got = len(cells(tgt[0]))
                expected = len(src_cells) - len(omissions)
                if got != expected:
                    fails.append((target_rel, 0, "E7_COLUMN_MISMATCH",
                                  f"dòng trích có {got} cột, cần {expected} "
                                  f"({len(src_cells)} gốc − {len(omissions)} bỏ)"))
            elif kind == "dod-block":
                _prose, blocks = parse_doc(text)
                want = os.path.basename(source_rel)
                hits = [b for b in blocks if os.path.basename(b["scope"]) == want]
                if len(hits) != 1:
                    fails.append((target_rel, 0, "E7_SOURCE_MISSING",
                                  f"DOD phải có đúng một khối `scope={want}` (thấy {len(hits)})"))
                    continue
                body = [ln for _n, ln in hits[0]["lines"] if ln.startswith("|")]
                k6 = [ln for ln in body if ln.startswith("| K6 |")]
                ns = [ln for ln in body if re.match(r"^N\d+[ \t]*\|", ln)]
                if len(k6) != 1 or len(cells(k6[0])) != 3:
                    fails.append((target_rel, hits[0]["start"], "E7_COLUMN_MISMATCH",
                                  "khối DOD phải có dòng K6 đủ 3 cột"))
                ncols = ex.get("columns")
                for ln in ns:
                    if len(cells(ln)) != ncols:
                        fails.append((target_rel, hits[0]["start"], "E7_COLUMN_MISMATCH",
                                      f"dòng DOD [{ln[:20]}…] có {len(cells(ln))} cột, cần {ncols}"))
            else:
                fails.append((target_rel, 0, "E7_NOT_REGISTERED", f"kind [{kind}] không hỗ trợ"))
    # tệp trích/DOD **không khai báo** ⇒ ĐỎ (fail-closed cho tệp mới), trừ ngoại lệ lịch sử.
    legacy = {e["path"] for e in ctx["policy"].get("legacy", [])}
    for rel in ctx["docs"]:
        base = os.path.basename(rel)
        if base.endswith("-DOD.md") and rel not in declared and rel not in legacy:
            fails.append((rel, 0, "E7_NOT_REGISTERED",
                          "tệp DOD chưa khai nguồn tiêu chí trong criteria.toml"))
    import glob as globmod

    found = set()
    for sub in ("v2/agent", "v2/test", "v2/docs", "v2/tools"):
        found.update(globmod.glob(os.path.join(root, sub, "**", "criteria.txt"), recursive=True))
    if ctx.get("tracked_set"):
        found.update(os.path.join(root, f) for f in ctx["tracked_set"] if f.endswith("criteria.txt"))
    for path in sorted(found):
        if not os.path.isfile(path):
            continue
        rel = relpath(root, path)
        if rel not in declared and rel not in legacy:
            fails.append((rel, 0, "E7_NOT_REGISTERED",
                          "tệp criteria.txt chưa khai nguồn trong criteria.toml"))
    return fails


# ------------------------------------------------------------------ luật 8: self-test
def _git(root):
    env = {
        "GIT_AUTHOR_NAME": "kt80",
        "GIT_AUTHOR_EMAIL": "kt80@example.invalid",
        "GIT_COMMITTER_NAME": "kt80",
        "GIT_COMMITTER_EMAIL": "kt80@example.invalid",
    }

    def g(*args):
        proc = subprocess.run(["git", "-C", root, *args], capture_output=True, text=True,
                              check=False, env={**os.environ, **env})
        if proc.returncode != 0:
            raise ConfigError(f"self-test: git {' '.join(args)} lỗi: {proc.stderr[:200]}")
        return proc.stdout

    return g


def _fixture_git(root):
    """Khởi tạo repo fixture: commit ĐẦU chứa toàn bộ log + báo cáo (log trước, cùng commit)."""
    g = _git(root)
    g("init", "-q")
    g("config", "user.email", "kt80@example.invalid")
    g("config", "user.name", "kt80")
    g("add", "-A")
    g("commit", "-q", "-m", "fixture: log trước")
    return g


def build_fixture(base, verifier_dir):
    """Dựng repo mini XANH (log commit trước báo cáo) — nền cho mọi ca phá."""
    root = os.path.join(base, "fixture")
    if os.path.isdir(root):
        shutil.rmtree(root)
    os.makedirs(os.path.join(root, "v2", "agent", "JOURNAL"))
    os.makedirs(os.path.join(root, "v2", "agent", "reports"))
    os.makedirs(os.path.join(root, "v2", "docs"))
    os.makedirs(os.path.join(root, "v2", "test", "rules"))
    os.makedirs(os.path.join(root, "docs", "v1"))
    os.makedirs(os.path.join(root, "docs", "v2"))
    open(os.path.join(root, "v2", "agent", "JOURNAL", "good.log"), "w").write(
        "# good: blocks=2 lines=6 tokens=9 fail=0\n# good: wall_ms=1200\n"
    )
    open(os.path.join(root, "v2", "agent", "JOURNAL", "good-red.log"), "w").write(
        "# red: drops=2\n"
    )
    open(os.path.join(root, "v2", "agent", "JOURNAL", "counter.log"), "w").write(
        "# kt80 counter: counter=qemu_orphan value=0\n"
    )
    open(os.path.join(root, "v2", "agent", "JOURNAL", "counter-negative.log"), "w").write(
        "# infra orphan: qemu_orphan=1\n"
    )
    open(os.path.join(root, "docs", "v2", "design.md"), "w").write(
        "### 10.1 Dòng lõi\n\n| Mốc | Nội dung | Tiêu chí xong |\n|---|---|---|\n"
        "| K6 | memsrv | tiêu chí |\n\n### 10.2 Sau\n"
    )
    open(os.path.join(root, "docs", "v1", "KNOWN-ISSUES.md"), "w").write(
        "| # | Lỗi v1 | Mô tả (1–2 dòng) | Tiêu chí đóng gốc (dossier) | Ca kiểm tương ứng trong v2 | Trạng thái |\n"
        "|---|---|---|---|---|---|\n"
        "| 1 | T33a | mô tả | tiêu chí | ca kiểm | mở |\n"
    )
    open(os.path.join(root, "v2", "agent", "JOURNAL", "T-criteria.txt"), "w").write(
        "# Nguồn 1: docs/v2/design.md §10.1 — dòng K6 (nguyên văn, đọc-only)\n"
        "| K6 | memsrv | tiêu chí |\n"
        "# Nguồn 2: docs/v1/KNOWN-ISSUES.md — bảy dòng bảng (nguyên văn, đọc-only)\n"
        "N1 | T33a | mô tả | tiêu chí | ca kiểm | mở |\n"
    )
    open(os.path.join(root, "v2", "agent", "reports", "T.md"), "w").write(
        "# Báo cáo thử (fixture)\n\n"
        "Định danh: KT-80, D2-112, v2, §2.1, 2026-11-17, kt80-verify.log:12.\n\n"
        "```text scope=good.log\n"
        "# good: blocks=2 lines=6 tokens=9 fail=0\n"
        "```\n\n"
        "```text scope=counter.log\n"
        "# kt80 counter: counter=qemu_orphan value=0\n"
        "```\n\n"
        "```text scope-red=good-red.log\n"
        "# red: drops=2\n"
        "```\n\n"
        "```text scope=T-criteria.txt\n"
        "# Nguồn 1: docs/v2/design.md §10.1 — dòng K6 (nguyên văn, đọc-only)\n"
        "| K6 | memsrv | tiêu chí |\n"
        "# Nguồn 2: docs/v1/KNOWN-ISSUES.md — bảy dòng bảng (nguyên văn, đọc-only)\n"
        "N1 | T33a | mô tả | tiêu chí | ca kiểm | mở |\n"
        "```\n"
    )
    open(os.path.join(root, "v2", "docs", "k6-thu.md"), "w").write(
        "# Tài liệu thử (fixture)\n\nĐịnh danh: K6, OD-K6-5.\n\n"
        "```text scope=good.log\n# good: wall_ms=1200\n```\n"
    )
    open(os.path.join(root, "v2", "test", "rules", "numbers-policy.toml"), "w").write(
        "[targets]\n"
        'globs = ["v2/agent/reports/*.md", "v2/docs/k6-*.md"]\n\n'
        "[[negative_log]]\n"
        'name = "good-red.log"\n'
        'reason = "log ca âm của fixture"\n'
        'date = "2026-11-17"\n'
    )
    open(os.path.join(root, "v2", "test", "rules", "counters.toml"), "w").write(
        "[[counter]]\n"
        'name = "qemu_orphan"\n'
        "proven = true\n"
        'negative_log = "v2/agent/JOURNAL/counter-negative.log"\n'
        'negative_pattern = "qemu_orphan=[1-9]"\n'
        'reason = "bộ đếm fixture có ca âm"\n'
        'date = "2026-11-17"\n'
    )
    open(os.path.join(root, "v2", "test", "rules", "criteria.toml"), "w").write(
        "[[extraction]]\n"
        'id = "fixture-design"\n'
        'file = "v2/agent/JOURNAL/T-criteria.txt"\n'
        'source = "docs/v2/design.md"\n'
        'kind = "design-row"\n'
        'heading = "### 10.1"\n'
        'row_prefix = "| K6 |"\n'
        "columns = 3\n\n"
        "[[extraction]]\n"
        'id = "fixture-known-issues"\n'
        'file = "v2/agent/JOURNAL/T-criteria.txt"\n'
        'source = "docs/v1/KNOWN-ISSUES.md"\n'
        'kind = "table-rows"\n'
        'header = "| # |"\n'
        "rows = 1\n"
        "columns = 6\n\n"
        "[[extraction]]\n"
        'id = "fixture-dod"\n'
        'file = "v2/agent/reports/T.md"\n'
        'source = "v2/agent/JOURNAL/T-criteria.txt"\n'
        'kind = "dod-block"\n'
        "columns = 6\n"
    )
    copy_regressions(os.path.join(verifier_dir, REGRESSIONS_NAME),
                     os.path.join(root, "v2", "test", "rules", REGRESSIONS_NAME))
    _fixture_git(root)
    return root


def copy_regressions(src, dst):
    if os.path.isfile(src):
        shutil.copyfile(src, dst)


# Mỗi ca phá: (id ngắn trong registry) -> hàm(root) áp sabotage **trên bản copy**.
def sabotage_union_haystack(root):
    # dòng lấy số từ log **khác**: chỉ có trong good.log, nhưng khai scope=counter.log
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    text = open(p, encoding="utf-8").read().replace(
        "```text scope=counter.log\n# kt80 counter: counter=qemu_orphan value=0\n```",
        "```text scope=counter.log\n# good: wall_ms=1200\n```",
    )
    open(p, "w").write(text)


def sabotage_heading_number(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\n## 98765\n")


def sabotage_decimal(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\nNgưỡng 262.144 byte.\n")


def sabotage_fence_text_no_scope(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\n```text\nkết quả: 729 test\n```\n")


def sabotage_bare_small_number(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\nSố ca đỏ: 3.\n")


def sabotage_heading_not_reset(root):
    # cờ heading không reset (lỗi kt75): số **sau** tiêu đề đầu bị miễn oan.
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\n## 3. Mục\n\nSố test 729 ở prose.\n")


def sabotage_scope_tmp(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\n```text scope=/tmp/ngoai-repo.log\n# kt80: x=1\n```\n")


def sabotage_scope_negative_source(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\n```text scope=good-red.log\n# red: drops=2\n```\n")


def sabotage_counter_unproven(root):
    p = os.path.join(root, "v2", "test", "rules", "counters.toml")
    open(p, "w").write(
        "[[counter]]\nname = \"qemu_orphan\"\nproven = false\n"
        "reason = \"ca phá: chưa chứng minh\"\ndate = \"2026-11-17\"\n"
    )


def sabotage_assign_key_value(root):
    # phản biện KT-80 (chặn): số đo dạng `key=value` trong prose (dạng bằng chứng repo dùng nhiều nhất)
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\nĐo được drops=5 sau khi chạy.\n")


def sabotage_counter_proven_no_negative(root):
    # chiều thứ hai của luật 3: proven=true nhưng log ca âm KHÔNG cho thấy bộ đếm khác 0
    p = os.path.join(root, "v2", "test", "rules", "counters.toml")
    lines = []
    for line in open(p, encoding="utf-8").read().splitlines():
        if line.startswith("negative_log = "):
            line = 'negative_log = "v2/agent/JOURNAL/good.log"'
        lines.append(line)
    open(p, "w", encoding="utf-8").write("\n".join(lines) + "\n")


def sabotage_counter_undeclared(root):
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    open(p, "a").write("\n```text scope=good.log\n# kt80: counter=khong_khai value=0\n```\n")


def sabotage_criteria_drop_column(root):
    p = os.path.join(root, "v2", "agent", "JOURNAL", "T-criteria.txt")
    lines = open(p, encoding="utf-8").read().splitlines()
    out = []
    for ln in lines:
        if ln.startswith("N1 |"):
            cells_ = ln.split("|")
            ln = "|".join(cells_[:-2] + [cells_[-1]])  # bỏ một cột
        out.append(ln)
    open(p, "w").write("\n".join(out) + "\n")


def sabotage_log_after_report(root):
    # commit báo cáo TRƯỚC, log sau ⇒ luật 2 phải ĐỎ
    g = _git(root)
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    # báo cáo trích `late.log` trong commit **trước** khi log tồn tại
    open(p, "a").write("\n```text scope=late.log\n# late: x=1\n```\n")
    g("add", "-A")
    g("commit", "-q", "-m", "fixture: báo cáo trước log")
    newlog = os.path.join(root, "v2", "agent", "JOURNAL", "late.log")
    open(newlog, "w").write("# late: x=1\n")
    g("add", "-A")
    g("commit", "-q", "-m", "fixture: log sau báo cáo")


def sabotage_legacy_changed(root):
    # ngoại lệ lịch sử ghim sha256 của bản ĐÃ COMMIT: sửa rồi commit ⇒ ĐỎ (ngoại lệ hết hạn).
    p = os.path.join(root, "v2", "agent", "reports", "T.md")
    pinned = sha256_file(p)  # nội dung đang là HEAD (fixture commit đầu)
    pol = os.path.join(root, "v2", "test", "rules", "numbers-policy.toml")
    with open(pol, "a") as fh:
        fh.write(
            "\n[[legacy]]\n"
            'path = "v2/agent/reports/T.md"\n'
            f'sha256 = "{pinned}"\n'
            'reason = "ca phá luật 8: ngoại lệ rồi sửa"\n'
            'date = "2026-11-17"\n'
        )
    open(p, "a").write("\nSố đo thêm 424242.\n")
    g = _git(root)
    g("add", "-A")
    g("commit", "-q", "-m", "fixture: sửa tệp đang có ngoại lệ")


SABOTAGES = {
    "union-haystack": sabotage_union_haystack,
    "heading-number": sabotage_heading_number,
    "decimal-x-y": sabotage_decimal,
    "fence-text-no-scope": sabotage_fence_text_no_scope,
    "bare-small-number": sabotage_bare_small_number,
    "heading-not-reset": sabotage_heading_not_reset,
    "scope-tmp": sabotage_scope_tmp,
    "scope-negative-source": sabotage_scope_negative_source,
    "counter-unproven": sabotage_counter_unproven,
    "counter-undeclared": sabotage_counter_undeclared,
    "assign-key-value": sabotage_assign_key_value,
    "counter-proven-no-negative": sabotage_counter_proven_no_negative,
    "criteria-drop-column": sabotage_criteria_drop_column,
    "log-after-report": sabotage_log_after_report,
    "legacy-changed": sabotage_legacy_changed,
}


def read_regressions(path):
    cases = []
    if not os.path.isfile(path):
        raise ConfigError(f"không có {path} — luật 8 cần registry ca phá")
    for lineno, line in enumerate(open(path, encoding="utf-8").read().splitlines(), 1):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        parts = [p.strip() for p in s.split("|")]
        if len(parts) != 5:
            raise ConfigError(f"{path}:{lineno}: cần 5 cột (id | luật | mã lỗi | cách phá | mô tả)")
        cases.append({"id": parts[0], "law": int(parts[1]), "code": parts[2],
                      "sabotage": parts[3], "desc": parts[4], "line": lineno})
    return cases


def exit_code_of(args):
    proc = subprocess.run([sys.executable, os.path.abspath(__file__)] + args,
                          capture_output=True, text=True, check=False)
    return proc.returncode, proc.stdout + proc.stderr


def law8(root, ctx, quiet=False):
    """Chạy mọi ca phá đã đăng ký trên **bản copy**; ca nào xanh ⇒ ĐỎ."""
    fails = []
    reg_path = os.path.join(root, RULES_DIR_REL, REGRESSIONS_NAME)
    try:
        cases = read_regressions(reg_path)
    except ConfigError as exc:
        return [(REGRESSIONS_NAME, 0, "E8_BAD_REGISTRY", str(exc))]
    if not cases:
        return [(REGRESSIONS_NAME, 0, "E8_BAD_REGISTRY", "registry ca phá rỗng")]
    base = tempfile.mkdtemp(prefix="kt80-selftest-")
    try:
        fixture = build_fixture(base, os.path.dirname(os.path.abspath(__file__)))
        # (a) đối chứng dương: fixture XANH cho cả 4 luật nội dung.
        for law in (1, 2, 3, 7):
            rc, out = exit_code_of(["--root", fixture, "--law", str(law)])
            if rc != 0:
                fails.append((REGRESSIONS_NAME, 0, "E8_BASELINE_RED",
                              f"fixture nền ĐỎ ở luật {law}: {out.strip().splitlines()[-1][:160]}"))
        # (b) từng ca phá: copy fixture, áp sabotage, đòi ĐỎ **đúng mã**.
        for case in cases:
            if case["sabotage"] not in SABOTAGES:
                fails.append((REGRESSIONS_NAME, case["line"], "E8_UNKNOWN_SABOTAGE",
                              f"cách phá [{case['sabotage']}] không có trong verifier"))
                continue
            work = os.path.join(base, "case-" + case["id"])
            try:
                shutil.copytree(fixture, work)
                SABOTAGES[case["sabotage"]](work)
                rc, out = exit_code_of(["--root", work, "--law", str(case["law"])])
                ok = rc != 0 and case["code"] in out
                if not quiet:
                    status = "ok" if ok else "not ok"
                    print(f"{status} - self-test {case['id']}: "
                          f"{'ĐỎ đúng mã' if ok else 'KHÔNG ĐỎ đúng mã'} {case['code']}")
                if not ok:
                    fails.append((REGRESSIONS_NAME, case["line"], "E8_CASE_GREEN",
                                  f"ca [{case['id']}] {'XANH' if rc == 0 else 'ĐỎ sai mã'}: "
                                  f"cần ĐỎ [{case['code']}] — {case['desc']}"))
            finally:
                shutil.rmtree(work, ignore_errors=True)
    finally:
        shutil.rmtree(base, ignore_errors=True)
    ctx["law8_cases"] = len(cases)
    return fails


# ------------------------------------------------------------------ điều phối
def load_context(root):
    rules = os.path.join(root, RULES_DIR_REL)
    ctx = {"root": root}
    ctx["is_git"] = is_git(root)
    if ctx["is_git"]:
        _rc, out, _e = git(root, "ls-files", "-z")
        ctx["tracked_set"] = set(x for x in out.split("\0") if x)
    else:
        ctx["tracked_set"] = None
    try:
        ctx["policy"] = load_toml(os.path.join(rules, POLICY_NAME))
        ctx["counters"] = load_toml(os.path.join(rules, COUNTERS_NAME))
        ctx["criteria"] = load_toml(os.path.join(rules, CRITERIA_NAME))
    except ConfigError:
        raise
    globs = ctx["policy"].get("targets", {}).get("globs", list(DEFAULT_TARGET_GLOBS))
    docs = []
    for path in target_docs(root, globs):
        rel = relpath(root, path)
        # Tài liệu đích = tệp **đã có trong repo** (index). Tệp chưa `git add` là bản nháp của
        # task khác: chưa phải báo cáo, và D2-105(a) cấm KT-80 làm đỏ cổng vì bản nháp song song.
        # Commit xong tệp sẽ vào index/HEAD và lần cổng kế tiếp kiểm nó (không có đường lách:
        # commit là điều kiện để thành "báo cáo").
        if ctx["tracked_set"] is not None and rel not in ctx["tracked_set"]:
            continue
        docs.append(rel)
    ctx["docs"] = docs
    generated = {}
    for ex in ctx["criteria"].get("extraction", []):
        if ex.get("generated", False):
            generated[ex.get("file", "")] = open(
                os.path.join(root, ex["file"]), encoding="utf-8"
            ).read() if os.path.isfile(os.path.join(root, ex["file"])) else ""
        # tệp trích `criteria.txt` luôn là "sinh từ máy" ⇒ khối trích phải bằng toàn bộ tệp
        f = ex.get("file", "")
        if f.endswith("criteria.txt") and f not in generated:
            path = os.path.join(root, f)
            generated[f] = open(path, encoding="utf-8").read() if os.path.isfile(path) else ""
    ctx["generated_criteria"] = generated
    ctx["excluded"] = {e["path"] for e in ctx["policy"].get("excluded", [])}
    ctx["docs"] = [d for d in ctx["docs"] if d not in ctx["excluded"]]
    return ctx


LAW_NAMES = {1: "so trong bao cao do script sinh tu log da commit",
             2: "log commit truoc bao cao (thu tu commit)",
             3: "bo dem bao 0 phai co ca am",
             7: "trich tieu chi giu du cot",
             8: "ca pha cu la ca hoi quy (self-test)"}


def main(argv=None):
    t0 = time.monotonic()
    ap = argparse.ArgumentParser(description="KT-80 — 5 luật máy OD-K6-5")
    ap.add_argument("--root", default=os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.abspath(__file__)))), help="gốc repo (mặc định: repo chứa script)")
    ap.add_argument("--law", action="append", default=[],
                    help="luật cần chạy: 1,2,3,7,8 (lặp lại hoặc '1,2')")
    ap.add_argument("--self-test", action="store_true", help="= --law 8")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args(argv)
    root = os.path.abspath(args.root)
    laws = []
    for item in args.law:
        for part in item.split(","):
            part = part.strip()
            if part:
                laws.append(int(part))
    if args.self_test:
        laws.append(8)
    if not laws:
        laws = [1, 2, 3, 7, 8]
    try:
        ctx = load_context(root)
    except ConfigError as exc:
        print(f"not ok - luat 0: E0_CONFIG {exc}")
        return 2
    print(f"# verify-numbers: root={root} docs={len(ctx['docs'])} "
          f"legacy={len(ctx['policy'].get('legacy', []))} laws={','.join(map(str, laws))}")
    ok = 0
    fail = 0
    walls = {}
    for law in laws:
        law_t0 = time.monotonic()
        if law == 1:
            fails = law1(root, ctx)
            extra = (f"docs_checked={ctx.get('law1_checked', 0)} "
                     f"pre_existing={ctx.get('law1_pre_existing', 0)} "
                     f"excluded={ctx.get('law1_excluded', 0)}")
        elif law == 2:
            fails = law2(root, ctx)
            extra = (f"pairs={ctx.get('law2_pairs', 0)} pending={ctx.get('law2_pending', 0)}")
        elif law == 3:
            fails = law3(root, ctx)
            extra = (f"counters={len(ctx['counters'].get('counter', []))} "
                     f"used={len(ctx.get('law3_used', []))}")
        elif law == 7:
            fails = law7(root, ctx)
            extra = f"extractions={len(ctx['criteria'].get('extraction', []))}"
        elif law == 8:
            fails = law8(root, ctx, quiet=args.quiet)
            extra = f"cases={ctx.get('law8_cases', 0)}"
        else:
            print(f"not ok - luat {law}: luật không tồn tại (nhận 1,2,3,7,8)")
            fail += 1
            continue
        law_ms = int((time.monotonic() - law_t0) * 1000)
        walls[law] = law_ms
        if fails:
            fail += 1
            for rel, lineno, code, why in fails:
                loc = f"{rel}:{lineno}" if lineno else rel
                print(f"not ok - luat {law}: {loc}: {code} {why}")
            print(f"not ok - luat {law}: {len(fails)} lỗi ({extra})")
        else:
            ok += 1
            print(f"ok - luat {law}: {extra} ({law_ms} ms)")
        if law == 3 and ctx.get("law3_unused"):
            print(f"# note - luat 3: bộ đếm khai mà chưa dùng: {', '.join(ctx['law3_unused'])}")
    wall = int((time.monotonic() - t0) * 1000)
    print(f"# verify-numbers: ok={ok} fail={fail} wall_ms={wall} "
          f"law_wall_ms={{{','.join(f'{k}:{v}' for k, v in walls.items())}}}")
    return 0 if fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
