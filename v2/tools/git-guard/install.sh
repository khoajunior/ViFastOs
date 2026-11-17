#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# v2/tools/git-guard/install.sh — cài/kiểm/gỡ hook chặn commit bằng máy (KT-85).
#
# Cài `pre-commit` + `pre-push` vào `<git-common-dir>/hooks/` — thư mục hook **chung** của
# repo, nên hook áp cho **mọi** worktree (agent làm trong `../wt-<mã>` vẫn bị chặn; đó là
# điều D2-123 mục (4) muốn). Bản gốc nằm trong repo, cài là **copy** (không symlink) để
# xoá worktree không làm gãy hook.
#
# Bất biến phải giữ (D2-123 mục (4), đề bài KT-85):
#   1. `core.hooksPath` **không** được trỏ chỗ khác — nếu trỏ chỗ khác thì hook của ta
#      không bao giờ chạy mà không ai thấy; script **từ chối** cài và in cách sửa.
#   2. Idempotent: chạy lại lần hai không đổi gì (so sha256 nội dung).
#   3. `--check` chỉ đọc: kiểm hook đang cài **đúng** (tồn tại, có quyền chạy, nội dung
#      trùng bản gốc) và in trạng thái; mã thoát 0 = đúng, 1 = lệch.
#
# Dùng:
#   v2/tools/git-guard/install.sh                 # cài (mặc định)
#   v2/tools/git-guard/install.sh --check         # kiểm, không ghi
#   v2/tools/git-guard/install.sh --uninstall     # gỡ (chỉ gỡ hook của ta)
#   v2/tools/git-guard/install.sh --repo <dir>    # áp cho repo khác (dùng cho ca tự kiểm)
#   v2/tools/git-guard/install.sh --help
#
# Lưu ý quyền: hook **không** đặt VIFAST_ROLE và không ghi biến đó vào đâu. Chỉ Agent 1
# (lead) đặt `VIFAST_ROLE=lead` trong môi trường của lệnh commit/push. Người thật giữ
# quyền `--no-verify` (xem v2/docs/k6-git-guard.md §4).

set -u

HOOK_NAMES=(pre-commit pre-push)
MARKER='vifast-guard'

GUARD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
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

# ------------------------------------------------------------------ tham số
action='install'
repo_override=''

while [ $# -gt 0 ]; do
    case "$1" in
        --check) action='check' ;;
        --install) action='install' ;;
        --uninstall) action='uninstall' ;;
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

echo "install.sh: KT-85 / D2-123 mục (4) — hook chặn commit bằng máy"
echo "  repo            : $REPO"
echo "  git-common-dir  : $COMMON_DIR"
echo "  hooks dir       : $HOOKS_DIR   (chung cho mọi worktree)"
echo "  core.hooksPath  : ${HOOKS_PATH:-(chưa đặt — đúng, hook lấy từ git-common-dir/hooks)}"
echo "  bản gốc         : $GUARD_DIR"
echo "  VIFAST_ROLE     : ${VIFAST_ROLE:-(chưa đặt trong môi trường hiện tại)}"
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

case "$action" in
    check)
        for h in "${HOOK_NAMES[@]}"; do check_one "$h"; done
        ;;
    install)
        for h in "${HOOK_NAMES[@]}"; do install_one "$h"; done
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
    echo "  Ca âm đầy đủ   : v2/tools/git-guard/neg-test.sh (ba ca: agent2 / chưa đặt / push)"
    echo "  Kiểm lại       : v2/tools/git-guard/install.sh --check"
fi
echo

if [ "$fails" -ne 0 ]; then
    echo "install.sh: FAIL ($fails mục) — $action KHÔNG đạt"
    exit 1
fi
echo "install.sh: PASS — $action đạt (${HOOK_NAMES[*]})"
exit 0
