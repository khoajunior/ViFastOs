#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-75 — kiểm **số đo trong tài liệu** theo **D2-109C + D2-110A + D2-112**.
#
# Vì sao có script này: mọi con số nêu trong `docs/k6-persist.md`, `docs/k3b-core.md` §23,
# `STATE.md` và `JOURNAL/kt75-*.md` phải **xuất hiện nguyên văn trong một log ĐÃ NẰM TRONG REPO**,
# và mỗi khẳng định phải có **phạm vi log riêng** (D2-112a) — không “haystack chung”.
#
# Bốn điều kiện D2-112 (thiếu một là **không đạt**):
#   (a) mỗi khối ```text scope=<tệp-log>``` được kiểm với **đúng** tệp log đó, **từng dòng**;
#   (b) **không** bảng ngoại lệ chứa chữ số đo; chỉ miễn **định danh** (hex, mã task, ngày, số hiệu
#       tiết `§N`, `file:line`, số hiệu `x.y`), mỗi mục có lý do trong `ALLOW`;
#   (c) log **mutation / ca âm / đỏ trước** KHÔNG được là nguồn (`FORBIDDEN`); chúng chỉ được
#       trích trong khối `scope-red=` (vẫn kiểm nguyên văn, không tính là số đo của tính năng);
#   (d) `STATE.md`: quét **toàn bộ** khối trạng thái (fence đầu, mọi dòng) **và** toàn bộ mục
#       `## KT-75` (mọi fence), không chỉ fence đầu.
#
# Dùng:
#   bash v2/agent/JOURNAL/kt75-verify-numbers.sh                      # kiểm cây thật
#   KT75_DOCS_ROOT=<dir> KT75_LOG_ROOT=<dir> bash …                   # kiểm **bản copy** (self-break)
# Exit: 0 = mọi số khớp; 1 = có số không có nguồn (in `fail=N` + từng dòng).
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V2_ROOT="$(cd "$HERE/../.." && pwd)"
DOCS_ROOT="${KT75_DOCS_ROOT:-$V2_ROOT}"
LOG_ROOT="${KT75_LOG_ROOT:-$HERE}"

PY=""
for c in python3.12 python3.11 python3; do
    if command -v "$c" >/dev/null 2>&1; then
        PY="$c"
        break
    fi
done
[ -n "$PY" ] || {
    echo "kt75-verify-numbers: không có python3" >&2
    exit 2
}

"$PY" - "$DOCS_ROOT" "$LOG_ROOT" <<'PY'
import os
import re
import sys

docs_root, log_root = sys.argv[1], sys.argv[2]

# (b) Miễn trừ **định danh** — KHÔNG phải số đo. Không mục nào miễn một con số đo được.
ALLOW = [
    (re.compile(r"^0x[0-9a-fA-F_]+$"), "hex literal (địa chỉ/giá trị nhị phân)"),
    (re.compile(r"^KT-\d+[a-z]?$"), "mã task"),
    (re.compile(r"^D2-\d+[A-Za-z]?$"), "mã quyết định"),
    (re.compile(r"^D-\d+$"), "mã quyết định v1"),
    (re.compile(r"^K[0-9]+(\.[0-9]+)?$"), "mã phase (K6)"),
    (re.compile(r"^v[0-9]+$"), "nhãn phiên bản (v1)"),
    (re.compile(r"^\d{4}-\d{2}-\d{2}$"), "ngày"),
    (re.compile(r"^\d{2}:\d{2}(:\d{2})?$"), "giờ trong log"),
    (re.compile(r"^§\d+(\.\d+)*$"), "tham chiếu tiết thiết kế"),
    # D2-112b (sửa theo reviewer): **cấm** miễn trừ số dạng `x.y` trần (`262.144` lọt trước đây).
    # Chỉ miễn **định danh** có **tiền tố chữ** rồi tới số hiệu, vd `Tcb.2`, `ktest.1` — chữ cái
    # đứng trước chứng minh đó là tên, không phải số đo viết theo kiểu phân cách nghìn.
    (re.compile(r"^[A-Za-z][A-Za-z0-9_/-]*\.\d+(\.\d+)*$"), "định danh có tiền tố chữ + số hiệu"),
    (re.compile(r"^[A-Za-z_][A-Za-z0-9_/.-]*\.(rs|md|sh|toml|elf|log|txt|py):\d+(-\d+)?$"),
     "tham chiếu `file:line`"),
    (re.compile(r"^R\d+$"), "luật cấu trúc (R1..R5)"),
    (re.compile(r"^T-INFRA$"), "nhãn loại lỗi"),
    (re.compile(r"^(DONE|READY|BLOCKED|REVIEW|PASS|FAIL)$"), "trạng thái"),
]

# (c) Log **không** được làm nguồn: mutation/ca âm/đỏ trước cố tình chứa giá trị sai.
FORBIDDEN = re.compile(r"(mutation|break|selfbreak|neg|ca-am|red-)", re.IGNORECASE)

FENCE = re.compile(r"^```text scope(-red)?=(\S+)\s*$")
NUMERIC = re.compile(r"^[0-9][0-9.,/:%-]*$")


def strip_blocks(text):
    """Trả (prose_lines, blocks) với blocks = [(scope, [dòng], is_red)]."""
    prose, blocks = [], []
    cur = None
    for line in text.splitlines():
        if cur is None:
            m = FENCE.match(line.strip())
            if m:
                cur = (m.group(2), [], bool(m.group(1)))
                continue
            prose.append(line)
        else:
            if line.strip() == "```":
                blocks.append(cur)
                cur = None
            else:
                cur[1].append(line.rstrip())
    if cur is not None:
        raise SystemExit("FAIL: khối scope chưa đóng")
    return prose, blocks


def numeric_tokens(line):
    out = []
    for raw in line.split():
        tok = raw.strip("`*_()[]{}<>\"'.,;:!?|")
        if tok and NUMERIC.fullmatch(tok):
            out.append(tok)
    return out


def allowed(tok):
    return any(rx.fullmatch(tok) for rx, _why in ALLOW)


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def state_top_block(text):
    """(d) khối trạng thái đầu `STATE.md` — fence đầu tiên, **mọi** dòng."""
    start = text.find("\n```\n")
    if start < 0:
        return ""
    start += 1
    end = text.find("\n```", start + 4)
    return text[start:] if end < 0 else text[start:end + 4]


def state_kt75(text):
    start = text.find("## KT-75")
    if start < 0:
        return ""
    nxt = text.find("\n## ", start + len("## KT-75"))
    return text[start:] if nxt < 0 else text[start:nxt]


targets = [
    ("docs/k6-persist.md", os.path.join(docs_root, "docs/k6-persist.md"), None),
    ("docs/k3b-core.md#23", os.path.join(docs_root, "docs/k3b-core.md"), "## 23."),
    ("agent/STATE.md#top+KT-75", os.path.join(docs_root, "agent/STATE.md"), "STATE"),
]
journal_dir = os.path.join(docs_root, "agent", "JOURNAL")
if os.path.isdir(os.path.join(docs_root, "agent", "JOURNAL")):
    for name in sorted(os.listdir(journal_dir)):
        if name.startswith("kt75-") and name.endswith(".md"):
            targets.append((f"agent/JOURNAL/{name}", os.path.join(journal_dir, name), None))

fails = 0
checked_blocks = 0
checked_lines = 0
checked_tokens = 0


def check_text(rel, text):
    """Kiểm blocks + prose của một vùng văn bản; trả (fails, blocks, lines, tokens)."""
    global checked_blocks, checked_lines, checked_tokens
    local_fails = 0
    prose, blocks = strip_blocks(text)
    for scope, lines, is_red in blocks:
        checked_blocks += 1
        log_path = os.path.join(log_root, scope)
        if not os.path.isfile(log_path):
            print(f"FAIL {rel}: không thấy log {scope}")
            local_fails += 1
            continue
        if not is_red and FORBIDDEN.search(scope):
            print(f"FAIL {rel}: scope={scope} là log ca âm/mutation — D2-112c cấm làm nguồn")
            local_fails += 1
            continue
        log_text = read(log_path)
        for line in lines:
            if not line.strip():
                continue
            checked_lines += 1
            if line not in log_text:
                kind = "ca âm" if is_red else "log"
                print(f"FAIL {rel}: dòng không có trong {kind} {scope}: {line[:100]}")
                local_fails += 1
    for line in prose:
        is_heading = line.lstrip().startswith("#")
        for tok in numeric_tokens(line):
            checked_tokens += 1
            if allowed(tok) or (is_heading and re.fullmatch(r"\d+(\.\d+)*\.?", tok)):
                continue
            print(f"FAIL {rel}: số '{tok}' ngoài khối scope (phải là số đo trong log) — dòng: {line.strip()[:110]}")
            local_fails += 1
    return local_fails


for rel, path, section in targets:
    if not os.path.isfile(path):
        print(f"FAIL {rel}: không thấy tệp")
        fails += 1
        continue
    text = read(path)
    if section == "STATE":
        # (d) khối trạng thái đầu + toàn bộ mục KT-75; prose ngoài hai vùng **không** quét
        top = state_top_block(text)
        if not top:
            print(f"FAIL {rel}: không thấy khối trạng thái đầu")
            fails += 1
        else:
            fails += check_text(rel + "#top", top)
        kt75 = state_kt75(text)
        if not kt75:
            print(f"FAIL {rel}: không thấy tiêu đề [## KT-75]")
            fails += 1
        else:
            fails += check_text(rel + "#KT-75", kt75)
        continue
    if section:
        start = text.find(section)
        if start < 0:
            print(f"FAIL {rel}: không thấy tiêu đề [{section}]")
            fails += 1
            continue
        nxt = text.find("\n## ", start + len(section))
        text = text[start:] if nxt < 0 else text[start:nxt]
    fails += check_text(rel, text)

print(
    f"# kt75-verify-numbers: docs={len(targets)} blocks={checked_blocks} lines={checked_lines} "
    f"tokens={checked_tokens} fail={fails}"
)
sys.exit(0 if fails == 0 else 1)
PY
