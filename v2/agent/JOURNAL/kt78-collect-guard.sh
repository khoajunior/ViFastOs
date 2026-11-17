#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-78 — thu **bằng chứng process group** từ log của harness vào repo (D2-106/D2-109).
#
# Vì sao có tệp này: log guard của `soak.sh`/`persist.sh` nằm ở `LOGDIR` (mặc định `/tmp`) — số đo
# phải nằm trong **log đã ở repo**, và dòng tổng hợp phải do **script** sinh (không viết tay). Tệp
# này là script sinh đó: nó (a) lọc log của **lần chạy hiện tại**, (b) in dòng `# guard_summary …`
# bằng `qg_guard_summary` (awk trong `qemu_guard.sh`, **công thức in kèm**), (c) chép nguyên văn các
# dòng `# boot_start`/`# boot_end`.
#
# Dùng (KHÔNG mở QEMU — chỉ đọc log):
#   bash v2/agent/JOURNAL/kt78-collect-guard.sh --src /tmp/k3-soak/qemu-guard.log \
#        --dst v2/agent/JOURNAL/kt78-soak-qemu-guard.log --last-run
# Exit: 0 = ghi xong; 2 = thiếu tệp nguồn.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
GUARD="$ROOT/v2/test/boot/qemu_guard.sh"
SRC=""
DST=""
LAST_RUN=0

while [ $# -gt 0 ]; do
    case "$1" in
        --src) SRC="${2:-}"; shift 2 ;;
        --dst) DST="${2:-}"; shift 2 ;;
        --last-run) LAST_RUN=1; shift ;;
        -h | --help)
            sed -n '2,20p' "$0"
            exit 0
            ;;
        *) echo "kt78-collect-guard: tham số lạ $1" >&2; exit 2 ;;
    esac
done
[ -n "$SRC" ] && [ -n "$DST" ] || {
    echo "kt78-collect-guard: cần --src và --dst" >&2
    exit 2
}
[ -f "$SRC" ] || {
    echo "kt78-collect-guard: không thấy $SRC" >&2
    exit 2
}
[ -f "$GUARD" ] || {
    echo "kt78-collect-guard: không thấy $GUARD" >&2
    exit 2
}
# Chỉ cần hàm tổng hợp; KHÔNG gọi `qg_init` (không mở QEMU, không trap).
. "$GUARD"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/kt78-collect.XXXXXX")" || exit 2
trap 'rm -rf "$WORK"' EXIT
FILTERED="$WORK/guard.log"
if [ "$LAST_RUN" = 1 ]; then
    # Mỗi lần chạy script ghi một header `# qemu_guard: pid=…`; lấy từ header **cuối** (lần hiện tại).
    awk '/^# qemu_guard: pid=/ { start = NR } { a[NR] = $0 } END { for (i = start; i <= NR; i++) print a[i] }' \
        "$SRC" >"$FILTERED"
else
    cat "$SRC" >"$FILTERED"
fi

{
    echo "# KT-78 — bằng chứng process group, do 'kt78-collect-guard.sh' sinh (không viết tay)."
    echo "# nguồn: $SRC"
    printf '# lệnh: bash v2/agent/JOURNAL/kt78-collect-guard.sh --src %s --dst %s%s\n' \
        "$SRC" "$DST" "$([ "$LAST_RUN" = 1 ] && printf ' --last-run')"
    printf '# lọc: %s\n' "$([ "$LAST_RUN" = 1 ] && printf 'chỉ lần chạy cuối (header `# qemu_guard: pid=` cuối cùng)' || printf 'nguyên tệp')"
    QG_SUMMARY_SRC="harness:$SRC (ban sao loc: $FILTERED)" qg_guard_summary "$FILTERED"
    echo "# --- các dòng '# boot_start'/'# boot_end' (nguyên văn) ---"
    grep '^# boot_' "$FILTERED" || true
} >"$DST" 2>&1 || exit 2

echo "# kt78-collect-guard: dst=$DST lines=$(wc -l <"$DST" | tr -d ' ')" >&2
SUMMARY="$(sed -n 's/^# guard_summary .*/&/p' "$DST" | tail -1)"
[ -n "$SUMMARY" ] && echo "# kt78-collect-guard: $SUMMARY" >&2
exit 0
