#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-77a — wrapper stdin cho MỘT lần boot QEMU: cấp cho QEMU stdin là FIFO và bơm payload của
# kịch bản vào đó sau một độ trễ ngắn.
#
# Vì sao: `qemu_guard.sh` (KT-78) chạy QEMU ở nền nên stdin của QEMU là `/dev/null`; đo được là
# byte bơm vào stdin QEMU **sau ~40 ms** (khi chardev stdio đã sẵn sàng) thì tới được shell, còn
# bơm ở thời điểm 0 thì bị mất. Wrapper này **không sửa** `qemu_guard.sh`: guard vẫn là tiến trình
# cha của QEMU trong process group riêng; wrapper chỉ là tệp thực thi (exec) mà guard khởi động.
#
# Dùng: stdin_wrap.sh <tệp-payload> <lệnh> [tham số...]
#   KT77_SEND_DELAY   giây, mặc định 0.05
set -u

[ $# -ge 2 ] || {
    echo "stdin_wrap.sh: cần <payload> <lệnh> [args...]" >&2
    exit 2
}
PAY="$1"
shift
[ -f "$PAY" ] || {
    echo "stdin_wrap.sh: không thấy payload $PAY" >&2
    exit 2
}
DELAY="${KT77_SEND_DELAY:-0.05}"
FIFO="$(mktemp "${TMPDIR:-/tmp}/kt77a-stdin.XXXXXX")" || exit 2
rm -f "$FIFO"
mkfifo "$FIFO" || exit 2
# Giữ một fd RDWR trên FIFO để `open` của QEMU **không** chặn (QEMU phải chạy ngay, không chờ
# writer) — đo được: byte bơm vào lúc QEMU chưa vào vòng lặp chính thì bị mất.
exec 8<>"$FIFO" || exit 2
(
    sleep "$DELAY"
    cat "$PAY" >&8
    rm -f "$FIFO"
) &
exec "$@" <"$FIFO"
