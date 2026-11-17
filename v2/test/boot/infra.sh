#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-78 — OD-K6-6: chụp trạng thái hạ tầng quanh một phép đo **không** do script boot sinh ra.
#
# Vì sao tệp riêng: `ktest.sh`/`soak.sh`/`persist.sh`/`run.sh` tự gọi `qg_infra` trước và sau phép
# đo của chúng. `gate`/`L1` do `v2/bin/v2` chạy (tệp đó **không** mở QEMU — `grep -c qemu-system
# v2/bin/v2` = 0), nên không sửa `v2/bin/v2`; thay vào đó chụp hạ tầng quanh lệnh gate bằng tệp này
# và dán hai dòng kết quả vào log bằng chứng của task:
#
#   bash v2/test/boot/infra.sh before-gate v2/agent/JOURNAL/kt78-gate-infra.log
#   ./v2/bin/v2 gate --fast
#   bash v2/test/boot/infra.sh after-gate  v2/agent/JOURNAL/kt78-gate-infra.log
#
# Dòng in ra: `# infra <tag>: qemu_total=… qemu_ppid1=… qemu_own=… qemu_foreign=… loadavg_1m=…
# loadavg_5m=… loadavg_15m=… ncpu=… boot_done=… verdict=OK|KHONG_HOP_LE [ly_do=…]`
#
# Exit: 0 = phép đo HỢP LỆ; 1 = KHÔNG HỢP LỆ (QEMU mồ côi/lạ hoặc load 1 phút > ncpu); 2 = lỗi môi trường.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/qemu_guard.sh"
QG_DIR="${TMPDIR:-/tmp}"
QG_LOG="${KT78_INFRA_LOG:-}"
qg_init || exit 2
qg_infra "${1:-?}" "${2:-}" || exit 1
exit 0
