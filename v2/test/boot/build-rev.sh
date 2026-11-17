#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-86 (D2-125.3) — **CỔNG DANH TÍNH BẢN DỰNG**: nguồn sự thật **duy nhất** cho định nghĩa
# `rev`/`dirty` và cho phép kiểm "ảnh có đúng mã nguồn đang đứng ở HEAD không".
#
# Vì sao có tệp này (lần thứ TƯ của cùng một lớp lỗi — D2-108A):
#   KT-71 `--features ktest,user` ghi đè `release/` ⇒ build id giả; KT-76 "build sạch" thực ra là
#   no-op (`Finished in 0.03s`), ba sha cho cùng nguồn; KT-79 sha phụ thuộc **đường dẫn**;
#   KT-77b artifact build từ `proc_rt.rs` **ngắn hơn** bản đã commit (lệch 23 B `.rodata`).
#   Ba lần trước chặn bằng **lời dặn**. Lần này chặn bằng **máy**: ảnh mang `rev`+`dirty` của lúc
#   build, harness **từ chối đo** khi ảnh lệch HEAD hoặc khi cây nguồn bẩn.
#
# Dùng (source, không chạy trực tiếp):
#   . "$HERE/build-rev.sh"
#   vbrev_gate <log-boot> <đường-dẫn-trong-repo> [<ảnh>]   # 0 = ĐO ĐƯỢC, 1 = TỪ CHỐI
#
# ĐỊNH NGHĨA `dirty` (chép vào `v2/docs/k6-build-rev.md` §2 — sửa một chỗ phải sửa cả hai):
#   dirty=1 ⇔ `git status --porcelain --untracked-files=normal` trả **ít nhất một** dòng cho
#   đường dẫn nằm trong `v2/kernel` **hoặc** `v2/lib`.
#   * **Có** tính tệp chưa track (untracked): một tệp nguồn mới trong `v2/kernel` cũng vào ảnh.
#   * **Không** tính `v2/agent/JOURNAL`, `v2/docs`, `v2/test`, `spec/`, `viscript/`… — nếu tính,
#     mỗi lần agent ghi log sẽ làm dirty=1 và cổng **tự khoá** (mọi phép đo bị từ chối).
#   * **Không** tính tệp bị `.gitignore` (`target/`, `artifacts/`, `/content/`) — git đã lọc.
#   * Không có `git`, không phải repo, hoặc `git` lỗi ⇒ dirty=1 (**fail-closed**: không chứng minh
#     được là sạch thì coi là bẩn — bất định chỉ được dẫn tới TỪ CHỐI, không bao giờ dẫn tới
#     "nhận nhầm là sạch").
#
# Định dạng dòng bằng chứng do **kernel** in lúc boot (một dòng, đúng thứ tự khoá):
#   # build: rev=<12 hex> dirty=<0|1>
# `rev` = 12 ký tự hex đầu của `git rev-parse HEAD`; `dirty` = cờ ở trên, cả hai lấy **tại thời
# điểm build** (build.rs ⇒ `cargo:rustc-env` ⇒ `const` trong ảnh, D2-102: in từ const, không
# literal). Xem `v2/docs/k6-build-rev.md`.
set -u

# Phạm vi tính `dirty` — KHÔNG thêm đường dẫn khác mà không sửa docs + ca kiểm.
VBREV_SCOPE="v2/kernel v2/lib"
# Độ dài `rev` trong dòng bằng chứng (12 hex đầu của SHA-1 40 ký tự).
VBREV_REV_LEN=12

# ---------------------------------------------------------------- đọc git
# vbrev_toplevel <đường-dẫn-bất-kỳ-trong-repo> -> gốc worktree (rỗng nếu không có git)
vbrev_toplevel() {
    [ -n "${1:-}" ] || return 0
    git -C "$1" --no-optional-locks rev-parse --show-toplevel 2>/dev/null | tr -d '\r'
}

# vbrev_head <gốc> -> SHA đầy đủ của HEAD (rỗng nếu không đọc được)
vbrev_head() {
    [ -n "${1:-}" ] || return 0
    git -C "$1" --no-optional-locks rev-parse --verify HEAD 2>/dev/null | tr -d '\r'
}

# vbrev_dirty_lines <gốc> -> các dòng `git status --porcelain` trong phạm vi (rỗng = sạch);
# exit != 0 khi **không đọc được** trạng thái (caller phải coi là bẩn).
vbrev_dirty_lines() {
    [ -n "${1:-}" ] || return 1
    # shellcheck disable=SC2086 # VBREV_SCOPE là danh sách pathspec, cố ý tách theo khoảng trắng
    git -C "$1" --no-optional-locks status --porcelain --untracked-files=normal \
        --ignore-submodules=all -- $VBREV_SCOPE 2>/dev/null
}

# vbrev_dirty <gốc> -> "0" | "1" (in ra stdout; không bao giờ lỗi — fail-closed về "1")
vbrev_dirty() {
    local root="${1:-}" lines=""
    if [ -z "$root" ] || [ ! -d "$root" ]; then
        printf '1'
        return 0
    fi
    if ! lines="$(vbrev_dirty_lines "$root")"; then
        printf '1'
        return 0
    fi
    case "$lines" in
        *[![:space:]]*) printf '1' ;;
        *) printf '0' ;;
    esac
}

# vbrev_dirty_files <gốc> -> danh sách tệp bẩn (một dòng một tệp, đã bỏ cột trạng thái) để log
vbrev_dirty_files() {
    local lines=""
    lines="$(vbrev_dirty_lines "${1:-}" 2>/dev/null)" || lines=""
    [ -n "$lines" ] || return 0
    printf '%s\n' "$lines" | sed -e 's/^...//' -e 's/^ *//'
}

# vbrev_rev12 <gốc> -> 12 hex đầu của HEAD (rỗng nếu không đọc được)
vbrev_rev12() {
    local head=""
    head="$(vbrev_head "${1:-}")"
    [ -n "$head" ] || return 0
    printf '%s' "$head" | cut -c1-"$VBREV_REV_LEN"
}

# --------------------------------------------------- đọc dòng bằng chứng từ log/ảnh
# vbrev_line <log> -> dòng `# build: …` ĐẦU TIÊN (rỗng nếu không có)
vbrev_line() {
    [ -f "${1:-/nonexistent}" ] || return 0
    # `\r` vì serial QEMU có thể chèn CR (log lưu qua `tr`/`tee` khác nhau giữa các runner).
    grep -a -m1 -E '^# build: ' "$1" 2>/dev/null | tr -d '\r'
}

# vbrev_parse <dòng> -> đặt VBREV_LINE_REV / VBREV_LINE_DIRTY; 0 = đúng định dạng, 1 = sai
# Định dạng **chặt**: `# build: rev=<12 hex> dirty=<0|1>` và **không có gì thêm** — dòng méo
# (thiếu khoá, rev 40 ký tự, dirty=2, có ký tự lạ) không được coi là bằng chứng.
vbrev_parse() {
    local line="${1:-}" rest=""
    VBREV_LINE_REV=""
    VBREV_LINE_DIRTY=""
    case "$line" in
        "# build: rev="*) ;;
        *) return 1 ;;
    esac
    rest="${line#\# build: rev=}"
    VBREV_LINE_REV="${rest%% *}"
    rest="${rest#"$VBREV_LINE_REV"}"
    case "$rest" in
        " dirty="*) ;;
        *) return 1 ;;
    esac
    VBREV_LINE_DIRTY="${rest# dirty=}"
    case "$VBREV_LINE_REV" in
        '' | *[!0-9a-f]*) return 1 ;;
    esac
    [ "${#VBREV_LINE_REV}" -eq "$VBREV_REV_LEN" ] || return 1
    case "$VBREV_LINE_DIRTY" in
        0 | 1) ;;
        *) return 1 ;;
    esac
    return 0
}

# vbrev_image_has <ảnh> <rev> <dirty> -> 0 nếu **chính tệp ảnh** chứa chuỗi danh tính đó.
# Kiểm này chặn ca "log của ảnh khác bị đem ra làm bằng chứng" (log không cần khớp ảnh đang đo).
vbrev_image_has() {
    local image="${1:-}" rev="${2:-}" dirty="${3:-}"
    [ -f "$image" ] || return 1
    [ -n "$rev" ] && [ -n "$dirty" ] || return 1
    LC_ALL=C grep -a -q -F "rev=$rev dirty=$dirty" "$image" 2>/dev/null
}

# ------------------------------------------------------------------- cổng
# vbrev_refuse <lý-do> [<chi-tiết>] — in dòng TỪ CHỐI (máy grep được) ra stdout
vbrev_refuse() {
    echo "REV-GATE: REFUSE reason=$1${2:+ $2}"
}

# vbrev_accept <rev> <head> <log> — in dòng CHẤP NHẬN ra stdout
vbrev_accept() {
    echo "REV-GATE: ACCEPT rev=$1 head=$2 log=$3"
}

# vbrev_gate <log-boot> <đường-dẫn-trong-repo> [<ảnh>]
#   0 = cho phép đo; 1 = TỪ CHỐI ĐO (caller phải exit != 0 trước khi chạy phép đo).
# Thứ tự kiểm: (1) có log; (2) có dòng `# build:`; (3) định dạng; (4) rev khớp HEAD;
# (5) dirty trong ảnh = 0; (6) cây nguồn hiện tại sạch; (7) ảnh (nếu truyền) chứa đúng danh tính.
vbrev_gate() {
    local log="${1:-}" where="${2:-}" image="${3:-}"
    local line="" root="" head="" rev="" idirty="" tree_dirty=""

    if [ ! -f "$log" ]; then
        vbrev_refuse no-log "log=$log"
        return 1
    fi
    line="$(vbrev_line "$log")"
    if [ -z "$line" ]; then
        # Fail-closed: ảnh không chứng minh được nó build từ đâu ⇒ không đo (D2-125.3).
        vbrev_refuse no-build-line "log=$log"
        return 1
    fi
    if ! vbrev_parse "$line"; then
        vbrev_refuse malformed "line=[$line] log=$log"
        return 1
    fi
    rev="$VBREV_LINE_REV"
    idirty="$VBREV_LINE_DIRTY"

    root="$(vbrev_toplevel "$where")"
    if [ -z "$root" ]; then
        vbrev_refuse no-git "where=$where rev=$rev"
        return 1
    fi
    head="$(vbrev_head "$root")"
    if [ -z "$head" ]; then
        vbrev_refuse no-git "root=$root rev=$rev"
        return 1
    fi
    # rev nhúng là **tiền tố** 12 hex của HEAD; so tiền tố chứ không so chuỗi con (D2-125.3).
    case "$head" in
        "$rev"*) ;;
        *)
            vbrev_refuse rev-mismatch "image_rev=$rev head_rev=$(printf '%s' "$head" | cut -c1-"$VBREV_REV_LEN") log=$log"
            return 1
            ;;
    esac
    if [ "$idirty" != "0" ]; then
        vbrev_refuse dirty "image_dirty=$idirty rev=$rev log=$log"
        return 1
    fi
    # (6) Harness **tự** tính lại dirty: ảnh nói sạch mà cây đang bẩn ⇒ ảnh không đại diện cho
    # mã nguồn hiện tại (đúng lớp lỗi KT-76/KT-77b) ⇒ từ chối.
    tree_dirty="$(vbrev_dirty "$root")"
    if [ "$tree_dirty" != "0" ]; then
        local first=""
        first="$(vbrev_dirty_files "$root" | head -1)"
        vbrev_refuse tree-dirty "root=$root rev=$rev first=[$first]"
        return 1
    fi
    # (7) Ảnh phải chứa đúng chuỗi danh tính của dòng log (log của ảnh khác ⇒ từ chối).
    if [ -n "$image" ]; then
        if ! vbrev_image_has "$image" "$rev" "$idirty"; then
            vbrev_refuse image-mismatch "image=$image rev=$rev dirty=$idirty log=$log"
            return 1
        fi
    fi
    vbrev_accept "$rev" "$head" "$log"
    return 0
}

# vbrev_summary <đường-dẫn-trong-repo> — in một dòng trạng thái để log (không phải cổng)
vbrev_summary() {
    local root="" rev="" dirty=""
    root="$(vbrev_toplevel "${1:-}")"
    if [ -z "$root" ]; then
        echo "REV-STATE: root=<none> rev=<none> dirty=1"
        return 0
    fi
    rev="$(vbrev_rev12 "$root")"
    dirty="$(vbrev_dirty "$root")"
    echo "REV-STATE: root=$root rev=${rev:-<none>} dirty=$dirty scope=[$VBREV_SCOPE]"
}
