#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-80 / OD-K6-5 luật 3 — in dòng evidence `counter=<tên>` bằng cách **đọc giá trị thật** từ
# log QEMU của KT-78 (không viết tay): bộ đếm phải có cả đường xanh (=0) và ca âm (≠0).
# Fail-closed: thiếu log/không đọc được giá trị ⇒ exit 1, không in gì.
#
# Dùng: bash v2/test/rules/counter-evidence.sh [--root R] >v2/agent/JOURNAL/kt80-*.log
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
if [ "${1:-}" = "--root" ] && [ -n "${2:-}" ]; then
    ROOT="$2"
fi
J="$ROOT/v2/agent/JOURNAL"
POS_LOG="$J/kt78-ktest-matrix.log"
NEG_LOG="$J/kt78-guard/selftest-20261117T015708Z.log"

[ -f "$POS_LOG" ] || { echo "counter-evidence: thiếu $POS_LOG" >&2; exit 1; }
[ -f "$NEG_LOG" ] || { echo "counter-evidence: thiếu $NEG_LOG" >&2; exit 1; }

value() { # <tệp> <tên bộ đếm> <0|khac0>
    local file="$1" name="$2" want="$3" vals
    vals="$(grep -o "${name}=[0-9][0-9]*" "$file" 2>/dev/null | cut -d= -f2 | sort -u)"
    if [ "$want" = "0" ]; then
        printf '%s\n' "$vals" | grep -qx '0' || return 1
        echo 0
    else
        printf '%s\n' "$vals" | grep -vx '0' | head -1
    fi
}

emit() { # <tên> <tệp xanh> <tệp âm>
    local name="$1" pos neg
    pos="$(value "$POS_LOG" "$name" 0)" || { echo "counter-evidence: $name không có giá trị 0 trong $POS_LOG" >&2; exit 1; }
    neg="$(value "$NEG_LOG" "$name" khac0)" || true
    [ -n "$neg" ] || { echo "counter-evidence: $name không có ca âm ≠0 trong $NEG_LOG" >&2; exit 1; }
    echo "counter=$name positive=$pos negative=$neg positive_log=kt78-ktest-matrix.log negative_log=kt78-guard/selftest-20261117T015708Z.log"
}

emit qemu_ppid1
emit isolation_bad
echo "# counter-evidence: PASS (giá trị đọc từ log KT-78; registry v2/test/rules/counters.toml)"
