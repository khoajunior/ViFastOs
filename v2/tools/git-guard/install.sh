#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# v2/tools/git-guard/install.sh — cài/kiểm/gỡ hook chặn commit bằng máy (KT-85 + KT-85b).
#
# Cài **bốn** hook vào `<git-common-dir>/hooks/` — thư mục hook **chung** của repo, nên hook
# áp cho **mọi** worktree (agent làm trong `../wt-<mã>` vẫn bị chặn; đó là điều D2-123 mục (4)
# muốn). Bản gốc nằm trong repo, cài là **copy** (không symlink) để xoá worktree không làm
# gãy hook.
#
#   pre-commit           (KT-85)   chặn `git commit` của vai không phải lead
#   pre-push             (KT-85)   chặn `git push` của vai không phải lead
#   commit-msg           (KT-85b)  bắt buộc nhãn [KT-##]/[ST-##] ở đầu subject
#   reference-transaction (KT-85b) chặn MỌI cập nhật nhánh bảo vệ của vai không phải lead
#                                  (phủ cherry-pick, merge, reset, rebase — vá A1-183)
#
# `reference-transaction` chỉ có từ git 2.28. Trên máy có git cũ hơn, script **không cài**
# hook đó (cài cũng không bao giờ chạy), in `[SKIP]` kèm lý do và **báo rõ** để lead biết
# lỗ hổng cherry-pick vẫn còn trên máy đó. Ba hook còn lại cài bình thường.
#
# Bất biến phải giữ (D2-123 mục (4), D2-125.2, đề bài KT-85/KT-85b):
#   1. `core.hooksPath` **không** được trỏ chỗ khác — nếu trỏ chỗ khác thì hook của ta
#      không bao giờ chạy mà không ai thấy; script **từ chối** cài và in cách sửa.
#   2. Idempotent: chạy lại lần hai không đổi gì (so sha256 nội dung).
#   3. `--check` chỉ đọc: kiểm hook đang cài **đúng** (tồn tại, có quyền chạy, nội dung
#      trùng bản gốc) và in trạng thái; mã thoát 0 = đúng, 1 = lệch.
#   4. `--uninstall` gỡ **cả bốn** hook (kể cả reference-transaction khi git đã hạ cấp).
#
# Dùng:
#   v2/tools/git-guard/install.sh                 # cài (mặc định)
#   v2/tools/git-guard/install.sh --check         # kiểm, không ghi
#   v2/tools/git-guard/install.sh --uninstall     # gỡ (chỉ gỡ hook của ta)
#   v2/tools/git-guard/install.sh --repo <dir>    # áp cho repo khác (dùng cho ca tự kiểm)
#   v2/tools/git-guard/install.sh --print-support # in phiên bản git + hook nào cài được
#   v2/tools/git-guard/install.sh --help
#
# Lưu ý quyền: hook **không** đặt VIFAST_ROLE và không ghi biến đó vào đâu. Chỉ Agent 1
# (lead) đặt `VIFAST_ROLE=lead` trong môi trường của lệnh commit/push. Người thật giữ
# quyền `--no-verify` (xem v2/docs/k6-git-guard.md §4).

set -u

HOOK_NAMES=(pre-commit pre-push commit-msg reference-transaction)
REF_HOOK='reference-transaction'
REF_HOOK_MIN_MAJOR=2
REF_HOOK_MIN_MINOR=28
MARKER='vifast-guard'

GUARD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    awk 'NR > 1 { if ($0 ~ /^#/) { sub(/^# ?/, ""); print } else { exit } }' "${BASH_SOURCE[0]}"
}

die_usage() {
    echo "install.sh: $1" >&2
    echo "install.sh: dùng --help để xem cách dùng" >&2
    exit 2
}

sha_of() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        return 1
    fi
}

has_marker() {
    # Hook của ta: có nhãn `vifast-guard` trong 30 dòng đầu (không phải hook lạ của người).
    [ -f "$1" ] || return 1
    head -n 30 "$1" 2>/dev/null | grep -q "$MARKER"
}

abs_common_dir() {
    local d
    d="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
    if [ -z "$d" ]; then
        d="$(git -C "$1" rev-parse --git-common-dir 2>/dev/null || true)"
        case "$d" in
            /*) ;;
            '') ;;
            *) d="$(cd "$1" && cd "$d" 2>/dev/null && pwd)" ;;
        esac
    fi
    printf '%s\n' "$d"
}

# ------------------------------------------------------------------ phiên bản git
# `git version` → "2.54.0" (có thể kèm "(Apple Git-157)"). Không đọc được ⇒ coi như cũ
# (fail-closed: thà báo thiếu hook còn hơn cài hook không bao giờ chạy).
git_version_num() {
    git --version 2>/dev/null |
        sed -n 's/^git version \([0-9][0-9]*\)\.\([0-9][0-9]*\).*/\1 \2/p' |
        head -n 1
}

# ref_tx_supported → 0 nếu git >= 2.28 (có hook reference-transaction).
ref_tx_supported() {
    local v major minor
    v="$(git_version_num)"
    if [ -z "$v" ]; then
        return 1
    fi
    major="${v% *}"
    minor="${v#* }"
    if [ "$major" -gt "$REF_HOOK_MIN_MAJOR" ]; then
        return 0
    fi
    if [ "$major" -eq "$REF_HOOK_MIN_MAJOR" ] && [ "$minor" -ge "$REF_HOOK_MIN_MINOR" ]; then
        return 0
    fi
    return 1
}

# hook_skipped <tên> → 0 nếu hook này KHÔNG cài được trên git hiện tại (in lý do qua biến).
hook_skipped() {
    if [ "$1" = "$REF_HOOK" ] && [ "$REF_SUPPORTED" -eq 0 ]; then
        return 0
    fi
    return 1
}

# Danh sách hook **cài được** trên git hiện tại (mỗi tên một dòng).
supported_hook_names() {
    local h
    for h in "${HOOK_NAMES[@]}"; do
        if hook_skipped "$h"; then
            continue
        fi
        printf '%s\n' "$h"
    done
}

# ------------------------------------------------------------------ tham số
action='install'
repo_override=''

while [ $# -gt 0 ]; do
    case "$1" in
        --check) action='check' ;;
        --install) action='install' ;;
        --uninstall) action='uninstall' ;;
        --print-support) action='support' ;;
        --repo)
            [ $# -ge 2 ] || die_usage "--repo cần một thư mục"
            repo_override="$2"
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *) die_usage "tham số lạ [$1]" ;;
    esac
    shift
done

# ------------------------------------------------------------------ xác định repo
if [ -n "$repo_override" ]; then
    REPO="$(cd "$repo_override" 2>/dev/null && pwd)" || {
        echo "install.sh: không có thư mục [$repo_override]" >&2
        exit 2
    }
else
    REPO="$(git -C "$GUARD_DIR" rev-parse --show-toplevel 2>/dev/null)" || {
        echo "install.sh: không tìm thấy gốc repo từ [$GUARD_DIR]" >&2
        exit 2
    }
fi

if ! git -C "$REPO" rev-parse --git-common-dir >/dev/null 2>&1; then
    echo "install.sh: [$REPO] không phải repo git" >&2
    exit 2
fi

COMMON_DIR="$(abs_common_dir "$REPO")"
HOOKS_DIR="$COMMON_DIR/hooks"
HOOKS_PATH="$(git -C "$REPO" config --get core.hooksPath 2>/dev/null || true)"

# ------------------------------------------------------------------ git có hook reference-transaction?
GIT_VER_RAW="$(git --version 2>/dev/null || echo '(không chạy được git version)')"
GIT_VER_NUM="$(git_version_num)"
REF_SUPPORTED=0
if ref_tx_supported; then
    REF_SUPPORTED=1
fi

if [ "$action" = "support" ]; then
    echo "install.sh: $GIT_VER_RAW"
    echo "  git version (số)          : ${GIT_VER_NUM:-(không đọc được)}"
    echo "  cần tối thiểu cho hook    : ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR} (reference-transaction)"
    if [ "$REF_SUPPORTED" -eq 1 ]; then
        echo "  reference-transaction     : CÀI ĐƯỢC (git đủ mới)"
    else
        echo "  reference-transaction     : KHÔNG cài được — git cũ hơn ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR}"
        echo "                              ⇒ cherry-pick/merge/reset vào nhánh chính KHÔNG bị hook chặn"
        echo "                              ⇒ phải nâng git HOẶC dựa vào bảo vệ nhánh GitHub (§6 tài liệu)"
    fi
    echo "  hook sẽ cài               : $(supported_hook_names | tr '\n' ' ')"
    exit 0
fi

echo "install.sh: KT-85 + KT-85b / D2-123 mục (4), D2-125.2 — hook chặn commit bằng máy"
echo "  repo            : $REPO"
echo "  git-common-dir  : $COMMON_DIR"
echo "  hooks dir       : $HOOKS_DIR   (chung cho mọi worktree)"
echo "  core.hooksPath  : ${HOOKS_PATH:-(chưa đặt — đúng, hook lấy từ git-common-dir/hooks)}"
echo "  bản gốc         : $GUARD_DIR"
echo "  VIFAST_ROLE     : ${VIFAST_ROLE:-(chưa đặt trong môi trường hiện tại)}"
echo "  git             : $GIT_VER_RAW"
if [ "$REF_SUPPORTED" -eq 1 ]; then
    echo "  reference-transaction: cài được (git >= ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR})"
else
    echo "  reference-transaction: [SKIP] git < ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR} — KHÔNG có hook này,"
    echo "                         không cài (cài cũng không chạy). Lỗ hổng A1-183 vẫn còn trên máy này:"
    echo "                         cherry-pick/merge/reset vào nhánh chính KHÔNG bị chặn."
    echo "                         ⇒ NÂNG GIT lên >= ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR} rồi chạy lại, hoặc dựa vào"
    echo "                         bảo vệ nhánh GitHub (v2/docs/k6-git-guard.md §6)."
    echo "                         Ba hook còn lại vẫn cài bình thường."
fi
echo "  chế độ          : $action"
echo

# ------------------------------------------------------------------ kiểm bất biến hooksPath
fails=0
hooks_path_ok=1

if [ -n "$HOOKS_PATH" ]; then
    # chấp nhận nếu trỏ đúng thư mục hook chuẩn của repo (không đổi hành vi)
    resolved="$HOOKS_PATH"
    case "$resolved" in
        /*) ;;
        *) resolved="$REPO/$resolved" ;;
    esac
    if [ "$(cd "$resolved" 2>/dev/null && pwd)" = "$(cd "$HOOKS_DIR" && pwd)" ]; then
        echo "[ OK ] core.hooksPath trỏ đúng thư mục hook chuẩn ($HOOKS_DIR)"
    else
        hooks_path_ok=0
        echo "[FAIL] core.hooksPath = $HOOKS_PATH — KHÔNG phải $HOOKS_DIR"
        echo "       ⇒ hook của repo bị vô hiệu âm thầm. Sửa bằng:"
        echo "           git -C \"$REPO\" config --unset core.hooksPath"
        echo "       rồi chạy lại install.sh. Không cài đè khi bất biến này sai (fail-closed)."
        fails=$((fails + 1))
    fi
else
    echo "[ OK ] core.hooksPath chưa đặt ⇒ git dùng $HOOKS_DIR"
fi

# ------------------------------------------------------------------ hành động
install_one() {
    local name="$1" src="$GUARD_DIR/$1" dst="$HOOKS_DIR/$1"
    local src_sha dst_sha bak

    if [ ! -f "$src" ]; then
        echo "[FAIL] thiếu bản gốc $src"
        fails=$((fails + 1))
        return 1
    fi
    src_sha="$(sha_of "$src")" || {
        echo "[FAIL] không tính được sha256 của $src"
        fails=$((fails + 1))
        return 1
    }

    if [ ! -d "$HOOKS_DIR" ]; then
        mkdir -p "$HOOKS_DIR" || {
            echo "[FAIL] không tạo được $HOOKS_DIR"
            fails=$((fails + 1))
            return 1
        }
    fi

    if [ -f "$dst" ]; then
        dst_sha="$(sha_of "$dst" || true)"
        if [ "$dst_sha" = "$src_sha" ] && [ -x "$dst" ]; then
            echo "[ OK ] $name đã cài đúng, không đổi (sha256 trùng bản gốc)"
            return 0
        fi
        if ! has_marker "$dst"; then
            bak="$dst.bak.$(date +%Y%m%d%H%M%S)"
            mv -f "$dst" "$bak" || {
                echo "[FAIL] $dst là hook lạ và không sao lưu được"
                fails=$((fails + 1))
                return 1
            }
            echo "[  ! ] $dst không phải hook vifast-guard ⇒ đã sao lưu sang $bak"
        fi
    fi

    cp -f "$src" "$dst" || {
        echo "[FAIL] không copy được $src → $dst"
        fails=$((fails + 1))
        return 1
    }
    chmod 0755 "$dst" || {
        echo "[FAIL] không đặt được quyền chạy cho $dst"
        fails=$((fails + 1))
        return 1
    }

    dst_sha="$(sha_of "$dst" || true)"
    if [ "$dst_sha" = "$src_sha" ] && [ -x "$dst" ]; then
        echo "[CÀI ] $name → $dst (sha256 khớp bản gốc, có quyền chạy)"
    else
        echo "[FAIL] $name cài xong nhưng kiểm lại không khớp (sha hoặc quyền)"
        fails=$((fails + 1))
        return 1
    fi
}

check_one() {
    local name="$1" src="$GUARD_DIR/$1" dst="$HOOKS_DIR/$1"
    local src_sha dst_sha

    if [ ! -f "$src" ]; then
        echo "[FAIL] thiếu bản gốc $src"
        fails=$((fails + 1))
        return 1
    fi
    src_sha="$(sha_of "$src")" || {
        echo "[FAIL] không tính được sha256 của $src"
        fails=$((fails + 1))
        return 1
    }
    if [ ! -f "$dst" ]; then
        echo "[FAIL] $name CHƯA cài (thiếu $dst)"
        fails=$((fails + 1))
        return 1
    fi
    if [ ! -x "$dst" ]; then
        echo "[FAIL] $name thiếu quyền chạy ($dst) — git sẽ bỏ qua hook"
        fails=$((fails + 1))
        return 1
    fi
    if ! has_marker "$dst"; then
        echo "[FAIL] $name ở $dst không phải hook vifast-guard"
        fails=$((fails + 1))
        return 1
    fi
    dst_sha="$(sha_of "$dst" || true)"
    if [ "$dst_sha" = "$src_sha" ]; then
        echo "[ OK ] $name đúng (sha256 trùng bản gốc)"
    else
        echo "[FAIL] $name lệch bản gốc (sha256 khác) — chạy lại install.sh"
        fails=$((fails + 1))
        return 1
    fi
}

uninstall_one() {
    local name="$1" dst="$HOOKS_DIR/$1"
    if [ ! -f "$dst" ]; then
        echo "[ OK ] $name không có ở $dst (không có gì để gỡ)"
        return 0
    fi
    if ! has_marker "$dst"; then
        echo "[  ! ] $dst không phải hook vifast-guard ⇒ KHÔNG gỡ"
        return 0
    fi
    rm -f "$dst" || {
        echo "[FAIL] không gỡ được $dst"
        fails=$((fails + 1))
        return 1
    }
    echo "[ GỠ ] $name (đã xoá $dst)"
}

check_one_or_skip() {
    if hook_skipped "$1"; then
        echo "[SKIP] $1 — git < ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR} không có hook này; đây KHÔNG phải lỗi cài đặt"
        echo "       ⇒ lỗ hổng A1-183 (cherry-pick/merge/reset vào nhánh chính) vẫn còn: NÂNG GIT hoặc bật"
        echo "         bảo vệ nhánh trên GitHub (v2/docs/k6-git-guard.md §6)."
        if [ -f "$HOOKS_DIR/$1" ]; then
            echo "       (có tệp $HOOKS_DIR/$1 — git cũ sẽ bỏ qua; gỡ bằng --uninstall nếu không dùng)"
        fi
        return 0
    fi
    check_one "$1"
}

install_one_or_skip() {
    if hook_skipped "$1"; then
        echo "[SKIP] $1 — KHÔNG cài: git < ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR}, hook không tồn tại nên cài cũng không chạy"
        return 0
    fi
    install_one "$1"
}

case "$action" in
    check)
        for h in "${HOOK_NAMES[@]}"; do check_one_or_skip "$h"; done
        ;;
    install)
        for h in "${HOOK_NAMES[@]}"; do install_one_or_skip "$h"; done
        ;;
    uninstall)
        for h in "${HOOK_NAMES[@]}"; do uninstall_one "$h"; done
        ;;
esac

# ------------------------------------------------------------------ trạng thái sau khi cài
echo
if [ "$action" != "check" ] && [ "$hooks_path_ok" -eq 1 ]; then
    n_wt="$(git -C "$REPO" worktree list 2>/dev/null | wc -l | tr -d ' ')"
    echo "  Áp cho         : $n_wt worktree đang đăng ký (hook ở git-common-dir)"
    echo "  Ca âm đầy đủ   : v2/tools/git-guard/neg-test.sh      (KT-85: commit/push)"
    echo "                   v2/tools/git-guard/neg-test-kt85b.sh (KT-85b: cherry-pick/merge/reset/commit-msg)"
    echo "  Kiểm lại       : v2/tools/git-guard/install.sh --check"
fi
echo

if [ "$fails" -ne 0 ]; then
    echo "install.sh: FAIL ($fails mục) — $action KHÔNG đạt"
    exit 1
fi
echo "install.sh: PASS — $action đạt ($(supported_hook_names | tr '\n' ' '))"
if [ "$REF_SUPPORTED" -eq 0 ]; then
    echo "install.sh: CẢNH BÁO — git < ${REF_HOOK_MIN_MAJOR}.${REF_HOOK_MIN_MINOR}: KHÔNG có reference-transaction,"
    echo "            lỗ hổng A1-183 (cherry-pick/merge/reset vào nhánh chính) CHƯA được vá trên máy này."
fi
exit 0
