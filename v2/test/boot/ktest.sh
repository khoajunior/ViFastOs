#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-44..54 — runner boot các ảnh **TEST** riscv64 K3 (crate `vifast-kbin-riscv64`, feature `ktest`).
#
# D2-08: ảnh test KHÔNG nằm trong ảnh sản phẩm — cùng crate nhưng bin + feature riêng. Mỗi boot chỉ
# kết thúc được một lần bằng PANIC có chủ đích ⇒ **ba** ảnh test:
#   - `ktest`       : 38 TAP — KT-45 (image + trap) + KT-46 (mmu/walk/guard) + KT-48 (PLIC/UART IRQ)
#                     + KT-47 (timer SBI) + KT-49 (SMP 15..19) + KT-50 (IPI 20..24) + KT-51 (TLB/ASID
#                     25..29) + KT-52 (object layer 30..36) + KT-55 (conformance 5 `.scn` 37:
#                     `# trace:` chuẩn hoá) + trap lồng fatal (38).
#   - `ktest-guard` : 3 TAP — guard page fault **thật**: load `scause=13` (probe resume) rồi store
#                     `scause=15` fatal `PANIC: guard page fault`.
#   - `ktest-user`  : 5 TAP — U-mode: nạp `init` ELF (parse + map W^X), `sret`, 3 syscall (DebugPutStr
#                     12, DebugHalt 13, Exit 14), syscall lạ trả 19 `RangeError`, rồi U-mode đọc địa
#                     chỉ kernel ⇒ page fault `scause=13` + `PANIC: u-mode fault` (kết thúc có chủ đích).
#
# **KT-54 — nguồn sự thật duy nhất là `v2/test/boot/expected.txt`** (mỗi ảnh một dòng
# `<ảnh> <kế hoạch TAP> <số dòng ok>`): runner **đọc số từ tệp**, không hardcode; `expected.txt` lệch
# với số TAP mà runner biết tên ⇒ **đỏ** (không cho lệch âm thầm). Ngoài ra runner kiểm:
#   1. `TAP version 13` đúng 1 lần, dòng kế hoạch `1..N` đúng 1 lần, **thứ tự** version < kế hoạch < ok đầu;
#   2. dãy `ok` = **1..N liên tục, không trùng, không thiếu** (so cả dãy theo thứ tự) + số dòng ok khớp tệp;
#   3. không có `not ok` nào;
#   4. **đúng 1 dòng `PANIC`, đúng mẫu cho phép của ảnh** (ảnh chính: trap lồng; guard: guard page
#      fault; user: u-mode fault) — PANIC khác mẫu hoặc PANIC thứ hai ⇒ đỏ;
#   5. mỗi TAP N có **đúng một** dòng `ok N - <tên>` khớp tên trong manifest của runner;
#   6. mọi dòng `# evidence` bắt buộc của ảnh đều có (ảnh chính **65** mục, guard **5**, user **13**);
#   8. **KT-55**: mỗi ca conformance in `# conf: scn=N name=… steps=K` và đúng **K** dòng
#      `# trace: N …` đúng định dạng; ca 24 (2 hart) in `skipped=smp1` trên `-smp 1`;
#   7. `Platform HART Count` của OpenSBI khớp `-smp` (số hart là **máy ảo** nói, không chỉ kernel tự khai).
#
# D2-72 Q2/Q4/Q5/Q10: `-machine virt,aia=none,aclint=on -cpu rv64 -m 128M -smp N`, matrix **1/2/4**
# cho KT-49, budget riêng cho **mỗi** ảnh test (≤ 262.144 B), timeout 20 s/boot, **chỉ** `kill <pid>`
# do chính script tạo (AGENTS §9) — không `pkill -f qemu-system`.
# D2-74: `-icount shift=6` cố định **chỉ** trong runner này (đo định thời tất định); `run.sh`/soak
# KT-57 giữ dòng D2-72 Q2 không icount.
#
# Dùng:
#   bash v2/test/boot/ktest.sh [NBOOT] [SMP|matrix]     # mặc định 1 boot, 1 hart; `matrix` = 1 2 4
#     vd: bash v2/test/boot/ktest.sh 50 matrix          # 50 boot × 3 ảnh × smp 1/2/4 = 450 boot
#   bash v2/test/boot/ktest.sh --check-log <ảnh> <log> [SMP]   # kiểm MỘT log đã có (không build/boot)
#   bash v2/test/boot/ktest.sh --log-path <ảnh> <SMP> [i]      # in đường dẫn log của runner
#   bash v2/test/boot/ktest.sh --print-expected               # in bảng expected.txt đã đọc
#   bash v2/test/boot/ktest.sh --print-evidence <ảnh> [SMP]    # in ordinal + regex từng evidence bắt buộc
#   bash v2/test/boot/ktest.sh --help
# Biến môi trường: `KT54_LOGDIR` (mặc định `/tmp/k3-ktest`), `KT54_EXPECTED` (chỉ `--check-log`,
# dùng cho `selftest.sh` — vòng boot thật **luôn** đọc `v2/test/boot/expected.txt`).
#
# Exit: 0 = mọi boot sạch (hoặc `--check-log` xanh); 1 = có boot/log đỏ; 2 = lỗi cấu hình môi trường.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V2_ROOT="$(cd "$HERE/../.." && pwd)"
MANIFEST="$V2_ROOT/kernel/Cargo.toml"
BUDGET="$HERE/budget.sh"
EXPECTED_FILE="$HERE/expected.txt"
LOGDIR="${KT54_LOGDIR:-/tmp/k3-ktest}"
TARGET="riscv64gc-unknown-none-elf"
IMG_DIR="$V2_ROOT/kernel/target/$TARGET/release"
LABELS="ktest ktest-guard ktest-user" # thứ tự in bảng + thứ tự boot trong một vòng
TIMEOUT_DS=200                        # 20 s/boot (D2-72 Q10), đơn vị 0,1 s
QEMU_ICOUNT="-icount shift=6"         # D2-74 (shift CỐ ĐỊNH, không `auto`)
QEMU_MIN_MAJOR=9                      # sàn D2-72 Q2: QEMU >= 9.2
QEMU_MIN_MINOR=2

MODE=matrix
CHECK_LABEL=""
CHECK_LOG=""
CHECK_SMP=1
LOGP_LABEL=""
LOGP_SMP=1
LOGP_I=1
PRINT_LABEL=""
PRINT_SMP=1
N_BOOT=1
SMP_ARG=1
SMP_LIST="1"
EXPECT_ERR=""
MISSING="" # token "thiếu …" của lần validate_log gần nhất (toàn cục ⇒ không subshell)
SMP_COUNT=0 # số cấu hình SMP thật sự có trong SMP_LIST (chặn "0 boot mà PASS")
RUN_RESULT="" # PASS|FAIL|TIMEOUT của lần run_one gần nhất (để SKIP chỉ khi TIMEOUT)
ROWS=0     # số dòng bảng đã ghi = số "slot" (ảnh, smp, boot) đã xử lý (boot thật + SKIP)


export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

die() {
    echo "ktest.sh: $*" >&2
    exit 2
}

usage() {
    sed -n '3,40p' "${BASH_SOURCE[0]}" | sed -e 's/^# \{0,1\}//'
}

# ---------------------------------------------------------------- tiện ích

log_path() { printf '%s/%s-smp%s-%s.log' "$LOGDIR" "$1" "$2" "$3"; }

image_path() {
    case "$1" in
        ktest) printf '%s/vifast-kbin-ktest' "$IMG_DIR" ;;
        ktest-guard) printf '%s/vifast-kbin-ktest-guard' "$IMG_DIR" ;;
        ktest-user) printf '%s/vifast-kbin-ktest-user' "$IMG_DIR" ;;
        *) return 1 ;;
    esac
}

known_label() {
    case " $LABELS " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

# `expected_field <ảnh> <2|3>`: đọc đúng một trường từ expected.txt (nguồn sự thật duy nhất).
expected_field() {
    awk -v label="$1" -v field="$2" '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        $1 == label { print $field; exit }' "$EXPECTED_FILE"
}

# Đọc + kiểm **định dạng** expected.txt. Trả 1 và đặt `EXPECT_ERR` nếu sai (không `die` để
# `--check-log` còn in được `not ok` có chẩn đoán).
load_expected() {
    local out="" l=""
    EXPECT_ERR=""
    [ -f "$EXPECTED_FILE" ] || {
        EXPECT_ERR="không thấy $EXPECTED_FILE"
        return 1
    }
    if ! out="$(awk '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        {
            if (NF != 3) { printf "FMT dòng %d: cần đúng 3 trường [ảnh] [kế hoạch] [số ok], thấy %d trường\n", NR, NF; bad = 1; next }
            if ($2 !~ /^[0-9]+$/ || $3 !~ /^[0-9]+$/) { printf "FMT dòng %d: trường 2/3 phải là số nguyên\n", NR; bad = 1; next }
            if ($2 + 0 < 1 || $3 + 0 < 1) { printf "FMT dòng %d: kế hoạch và số ok phải >= 1\n", NR; bad = 1; next }
            if ($2 + 0 != $3 + 0) { printf "FMT dòng %d: kế hoạch %d khác số ok %d (runner không cho phép TAP đỏ)\n", NR, $2, $3; bad = 1; next }
            if (seen[$1]++) { printf "FMT dòng %d: nhãn %s bị lặp\n", NR, $1; bad = 1; next }
            printf "LINE %s %s %s\n", $1, $2, $3
        }
        END { exit bad }' "$EXPECTED_FILE")"; then
        EXPECT_ERR="$(printf '%s\n' "$out" | grep '^FMT' | head -3 | tr '\n' ';')"
        [ -n "$EXPECT_ERR" ] || EXPECT_ERR="expected.txt sai định dạng"
        return 1
    fi
    for l in $LABELS; do
        if [ -z "$(expected_field "$l" 2)" ]; then
            EXPECT_ERR="expected.txt thiếu dòng cho ảnh [$l] (runner biết: $LABELS)"
            return 1
        fi
    done
    for l in $(printf '%s\n' "$out" | awk '$1 == "LINE" { print $2 }'); do
        if ! known_label "$l"; then
            EXPECT_ERR="expected.txt có nhãn [$l] mà runner chưa biết (runner biết: $LABELS)"
            return 1
        fi
    done
    return 0
}

# Số TAP mà runner **biết tên** cho từng ảnh phải bằng kế hoạch trong expected.txt ⇒ thêm/bớt TAP
# mà chỉ sửa một bên là đỏ ngay (không lệch âm thầm).
check_manifests() {
    local smp="$1" l="" plan="" n=""
    EXPECT_ERR=""
    for l in $LABELS; do
        plan="$(expected_field "$l" 2)"
        n="$(manifest_for "$l" "$smp" | wc -l | tr -d ' ')"
        if [ "${n:-0}" -ne "$plan" ]; then
            EXPECT_ERR="expected.txt: ảnh [$l] kế hoạch $plan nhưng runner kiểm $n TAP — phải sửa cả hai cùng lúc"
            return 1
        fi
    done
    return 0
}

# D2-80: BUILD_ID ảnh = FNV-1a 64 trên section `.text` (cùng hàm băm với kernel).
image_build() {
    local img="$1" out=""
    out="$(python3.12 "$V2_ROOT/test/conformance/image_digest.py" "$img" 2>/dev/null)" || return 1
    printf '%s' "${out%% *}"
}

now_s() { date +%s; }

# --------------------------------------------------- manifest TAP (tên từng TAP)

ipi_name() {
    case "${1:-}" in '' | *[!0-9]*) return 1 ;; esac
    if [ "$1" -ge 2 ]; then printf '100 ipi delivered'; else printf '100 ipi traps (self)'; fi
}

manifest_main() {
    cat <<'EOF'
ktest image
trap breakpoint
trap ecall sbi
trap illegal inject
mmu on
map layout
walk khop
plic window
uart irq
plic no fake irq
timer 100 ticks monotonic
timer no lost tick
timer irq
timer deadline drift
harts alive __SMP__
per-hart stack/guard
per-hart timer
plic ctx per hart
barrier
__IPI20__
ipi drain khi cho ack
self-target khong gui
coalesce khong mat kind
ipi khong nham timer/external
100 vong shootdown Ok
stale mapping caught
hart ngoai active_on
rollover flush
timeout khong treo
retype vspace
untyped byte invariant
frame 3 sizes
wx violation x-then-w
wx violation device+exec
map/unmap 4 KiB
0 ro
__CONF37__
trap nested panic
EOF
}

manifest_guard() {
    cat <<'EOF'
guard image
guard load fault
guard store fault
EOF
}

manifest_user() {
    cat <<'EOF'
init u-mode
syscall debug_put_str
syscall exit
syscall la ma loi
u-mode kernel read fault
EOF
}

# Tên TAP 37 của KT-55 phụ thuộc smp: `-smp 1` không chạy ca 24 (cần 2 hart thật).
conf37_name() {
    if [ "${1:-1}" -ge 2 ]; then printf 'conformance 5/5'; else printf 'conformance 4/5 + 1 skipped'; fi
}

# `manifest_for <ảnh> <smp>`: danh sách tên TAP theo thứ tự 1..N (đã thay phần phụ thuộc cấu hình).
manifest_for() {
    case "$1" in
        ktest) manifest_main ;;
        ktest-guard) manifest_guard ;;
        ktest-user) manifest_user ;;
        *) return 1 ;;
    esac | sed -e "s|__SMP__|$2|g" -e "s|__IPI20__|$(ipi_name "$2")|g" \
        -e "s|__CONF37__|$(conf37_name "$2")|g"
}

# ------------------------------------------------------ bằng chứng `# …` bắt buộc

# `evidence_for <ảnh> <smp> [hart-boot]`: mỗi dòng là một regex `grep -E` **bắt buộc** phải khớp.
evidence_for() {
    local label="$1" smp="$2" hart="${3:-}" mask tlb_removed tlb_sent ipi_del ipi_drain ipi_coal tlb_mode tlb_peer
    mask="$(printf '%x' $(((1 << smp) - 1)))"
    if [ "$smp" -ge 2 ]; then
        evid24='^# trace: 24 [0-9]+ shootdown va=0x[0-9a-f]+ targets=0x[0-9a-f]+ acked=0x[0-9a-f]+ completed=1 => ok$'
    else
        evid24='^# conf: scn=24 name=vm-tlb steps=0 skipped=smp1$'
    fi
    if [ "$smp" -ge 2 ]; then
        ipi_mode=cross
        ipi_del="sent=100 delivered=100 traps=100 coalesced=0 spurious=0 kinds=34/33/33 timeout=0"
        ipi_drain="mode=cross mutual=1 boot_ok=1 peer_ok=1 traps_delta=0/0 wait_ticks=[0-9]+/[0-9]+ timeout_ended=1"
        ipi_coal="mode=cross posts=6 coalesced=4 sent_delta=2 pending=0x3 peer_bits=0x3 peer_delivered=2 ok=1"
        tlb_mode=cross
        tlb_sent=$((100 * (smp - 1)))
        tlb_peer="received=100 flushes=100 page=100 asid=0 all=0 implicit=0 prime_wrong=0 verify_wrong=0 service_timeout=0"
        tlb_removed=2
        [ "$smp" -eq 2 ] && tlb_removed=1
    else
        ipi_mode=self-ipi
        ipi_del="sent=100 delivered=0 traps=100 coalesced=0 spurious=0 kinds=0/0/0 timeout=0"
        ipi_drain="mode=self-wait mutual=0 boot_ok=0 peer_ok=0 traps_delta=0/0 wait_ticks=[0-9]+/[0-9]+ timeout_ended=1"
        ipi_coal="mode=mailbox posts=6 coalesced=4 sent_delta=0 pending=0x3 peer_bits=0x3 peer_delivered=2 ok=1"
        tlb_mode=self
        tlb_sent=0
        tlb_peer="received=0 flushes=0 page=0 asid=0 all=0 implicit=0 prime_wrong=0 verify_wrong=0 service_timeout=0"
        tlb_removed=1
    fi
    case "$label" in
        ktest) cat <<'EOF'
^# obj init: arena=0x[0-9a-f]+-0x[0-9a-f]+ bytes=[0-9]+ untyped=2 window=0x40000000 pa0=0x[0-9a-f]+ pa1=0x80000000 physmap=1$
^# retype: vspace=1 root=0x[0-9a-f]+ asid=[1-9][0-9]* pt=3 frame=1 cap_ok=1 walk_empty=1$
^# untyped bytes: cap=[0-9]+ used0=[0-9]+ after_retype=[0-9]+ after_revoke=[0-9]+ free0=[0-9]+ free1=[0-9]+ invariant=1 atomic=1 oom=1$
^# frame: s12=1 s21=1 s30=1 pa4=0x[0-9a-f]+ pa2=0x[0-9a-f]+ pa1g=0x80000000 aligned=1 real4=1 real2=1 window=1 bad13=InvalidArgument atomic=1$
^# wx: x_then_w=WxViolation device_exec=WxViolation atomic=1 maps=1 w=0 x=1$
^# map: va=0x40000000 level=0 pa=0x[0-9a-f]+ satp=1 asid=[1-9][0-9]* sum=1 write=1 read=1 protect=1 unmapped=1 flush=[1-9][0-9]* walk_after=0 kernel=1$
^# leak: cap=[0-9]+ used=0 free=[0-9]+ holes=0 frames=0 pts=0 vspaces=0 maps=0 orphans=0 check=1$
^# mmu: satp=0x[0-9a-f]+ mode=8 root=0x[0-9a-f]+ tables=[0-9]+ used=[0-9]+ arena=0x[0-9a-f]+-0x[0-9a-f]+ kernel_end=0x[0-9a-f]+$
^# mmu on: satp=0x[0-9a-f]+ mode=8 readback=0x[0-9a-f]+ root=0x[0-9a-f]+ arena=0x[0-9a-f]+ alias=1 uart=1 kernel_low=1 kernel_high=1$
^# mmu layout: guard=0x[0-9a-f]+-0x[0-9a-f]+ none_low=1 none_high=1 none_physmap=1 stack=0x[0-9a-f]+-0x[0-9a-f]+ rw_nx=1 trap=0x[0-9a-f]+-0x[0-9a-f]+ rw_nx=1 arena=0x[0-9a-f]+-0x[0-9a-f]+ arena_alias=1 plic_low_2m=1 plic_high_2m=1 physmap_2m_nx=1 uart_dev=1 plic_dev=1 kernel_high_2m=1 all_harts=1$
^# mmu layout: .*physmap_2m_nx=1 uart_dev=1 plic_dev=1 kernel_high_2m=1 all_harts=1$
^# walk khop: pre=post=1 software=8/8 hardware_a=1 hardware_b=1 alias=1 satp_root=0x[0-9a-f]+$
^# plic: base=0x[0-9a-f]+ ndev=[0-9]+ hart=[0-9]+ ctx=[0-9]+ plat_off=1 plat_on=1 threshold=0 priority_uart=1 enable_ctx=1 enable_mmode=0 claim_idle=0$
^# plic map: low=0x[0-9a-f]+-0x[0-9a-f]+ high=0x[0-9a-f]+-0x[0-9a-f]+ low_dev=1 high_dev=1 low_2m=1 high_2m=1 low_nx=1 high_nx=1$
^# uart irq: irqs=[1-9][0-9]* claims=[1-9][0-9]* completes=[1-9][0-9]* traps_delta=[1-9][0-9]* balanced=1 spurious=0 unexpected=0 last_irq=10 last_iir=0x2 ier_off=1 plic_enable=0$
^# plic quiet: claim_idle=0 quiet=1 stats_unchanged=1 external_traps=[1-9][0-9]* interrupts_delta=[1-9][0-9]* spurious=0 unexpected=0$
^# timer arm: now=[0-9]+ first_deadline=[0-9]+ tick_ticks=100000 tick_hz=100 timebase_hz=10000000 sie_stie=1 sstatus_sie=1$
^# timer ticks: ticks=[0-9]+ min_delta_ticks=[0-9]+ max_delta_ticks=[0-9]+ non_monotonic=0 late=0 missed=0 max_late_ticks=[0-9]+ first=[0-9]+ last=[0-9]+ deadline=[0-9]+$
^# timer drift: min_ns=[0-9]+ max_ns=[0-9]+ min_us=[0-9]+ max_us=[0-9]+ tick_ns=10000000 min_bound_ns=9000000 max_bound_ns=11000000 in_range=1$
^# timer irq: interrupts=[0-9]+ scause=0x8000000000000005 code=5 exceptions=[0-9]+ exceptions_before=[0-9]+ sie_stie=1$
^# timer sie: trap_spie_on=[0-9]+ handler_sie_off=[0-9]+ ticks=[0-9]+ sie_stie=1 sret_sie=1$
^# timer disable: sie_stie=0 sstatus_sie=0$
^# smp: boot=[0-9]+ harts=__SMP__ .* timeout=0 .* dtb_magic=1 .* stride=0x15000 area=0xa8000$
^# harts alive: n=__SMP__ mask=0x__MASK__ alive=0x__MASK__ tickets=1 arrived=0x__MASK__ timeout=0$
^# per-hart stack/guard: stack_ok=0x__MASK__ guard_ok=0x__MASK__ distinct=1 owner_ok=true$
^# per-hart timer: ok=0x__MASK__ total_ticks=[1-9][0-9]* arrived=0x__MASK__ timeout=0$
^# per-hart plic: ok=0x__MASK__ distinct=1 uart_irqs_boot=8 stats_clean=1 arrived=0x__MASK__ timeout=0$
^# barrier: round=5 arrived=0x[0-9a-f]+ missing=0x0 timeout=0 wait_ticks=[0-9]+ slow_hart=1 slow_delayed=[01]$
^# barrier timeout: round=6 expected_missing=0x[0-9a-f]+ arrived=0x[0-9a-f]+ missing=0x[0-9a-f]+ timeout=1 wait_ticks=[0-9]+ \(may khong treo\)$
^# ipi: harts=__SMP__ boot=[0-9]+ peer=-?[0-9]+ mode=__IPI_MODE__ target=100 sie_ok=0x__MASK__ setup_ok=1$
^# ipi delivered: mode=__IPI_MODE__ target=100 __IPI_DELIVERED__$
^# ipi drain: __IPI_DRAIN__ timeout_wait=[0-9]+ timeout_drains=[1-9][0-9]*$
^# ipi self: ok=0x__MASK__ local=[1-9][0-9]* sent_delta=0 traps_delta=0 kinds_tlb=[1-9][0-9]*$
^# ipi coalesce: __IPI_COALESCE__$
^# ipi distinct: ipi_traps=100 interrupts_delta=100 timer_delta=0 plic_delta=0 last_scause=0x8000000000000001 last_code=1 decode_ok=1 ok=1$
^# tlb init: asid=0 generation=0 rollover=0 satp=0x[0-9a-f]+ satp_asid=0 active_on=0x(1|2|4|8) free=255 issued=1$
^# tlb: harts=__SMP__ boot=[0-9]+ peer=-?[0-9]+ mode=__TLB_MODE__ rounds=100 asid_bits=8 asid_count=256$
^# tlb probe: va=0x80800000 pa_a=0x80800000 pa_b=0x80a00000 probe2_va=0x80a00000 pa_c=0x80c00000 pa_d=0x80e00000 leaf_a=1 leaf_b=1 nx=1$
^# shootdown rounds: mode=__TLB_MODE__ rounds=100 ok=100 acked_ok=100 sent=__TLB_SENT__ sent_expected=__TLB_SENT__ self_slot_wrong=0 local_verify_wrong=0 barrier_bad=0 targets=0x[0-9a-f]+ active_on=0x[0-9a-f]+ wait_ticks=[0-9]+$
^# shootdown all: bad=0 served=__SMPM1__ expected_served=__SMPM1__ online=0x__MASK__$
^# shootdown peer: mode=__TLB_MODE__ __TLB_PEER__$
^# stale: asid=0 va=0x80800000 forms=3 stale_seen=3 fresh_ok=3 wrong=0 note=stale-read-without-sfence-is-positive$
^# stale form: name=Page fresh_a=1 stale_seen=1 fresh_b=1 wrong=0$
^# stale form: name=Asid fresh_a=1 stale_seen=1 fresh_b=1 wrong=0$
^# stale form: name=All fresh_a=1 stale_seen=1 fresh_b=1 wrong=0$
^# outside: mode=__TLB_MODE__ removed=0x[0-9a-f]+ removed_count=__TLB_REMOVED__ narrowed=0x[0-9a-f]+ targets=0x[0-9a-f]+ acked=0x[0-9a-f]+ caller_in=[01] removed_acked_before=0x[0-9a-f]+ removed_acked_after=0x[0-9a-f]+ restored=1 restored_acked=0x[0-9a-f]+ outside_wrong=0 ok=1$
^# rollover: generation=1 exhaustions=1 rollover=1 flush=Ok asid=0 satp_asid=0 stale_seen=1 fresh_ok=1 free=255 issued=1 asid_ok=0x__MASK__ online=0x__MASK__ primed=0x4b51c000000000c2 wrong=0$
^# timeout: outcome=Timeout narrowed=0x[0-9a-f]+ targets=0x[0-9a-f]+ acked=0x[0-9a-f]+ missing=0x[0-9a-f]+ waited=[0-9]+ probe=1000000 timeouts=[1-9][0-9]* mailbox_empty=1 recovered=1 wrong=0$
^# tlb asid: ok=0x__MASK__ online=0x__MASK__ phase=0x[0-9a-f]+ arrived=0x__MASK__ timeout=0 satp_asid_boot=0 lease_asid=0 generation=1$
^# conf image: build=[0-9a-f]{16} text=[0-9]+$
^# conf: scn=22 name=vm-map steps=[0-9]+ ok=1$
^# conf: scn=23 name=vm-wx steps=[0-9]+ ok=1$
^# conf: scn=24 name=vm-tlb .*$
^# conf: scn=25 name=vm-fault steps=[0-9]+ ok=1$
^# conf: scn=26 name=vm-shared steps=[0-9]+ ok=1$
^# trace: 22 1 map va=0x[0-9a-f]+ frame=f4 rights=READ\|WRITE attrs=WB => ok$
^# trace: 22 [0-9]+ walk va=0x[0-9a-f]+ => NotMapped$
^# trace: 23 [0-9]+ map .* => WxViolation$
^# trace: 23 [0-9]+ check => ok$
__EVID24__
^# trace: 25 [0-9]+ fault va=0x[0-9a-f]+ access=Write reason=NotMapped => ok$
^# trace: 26 [0-9]+ fault va=0x[0-9a-f]+ access=Write reason=Permission => ok$
^# trace: 26 [0-9]+ check => ok$
^OpenSBI v[0-9]+\.[0-9]+\.[0-9]+$
^Platform HART Count *: *__SMP__$
EOF
            ;;
        ktest-guard) cat <<'EOF'
^# guard image: hart=[0-9]+ stvec=0x[0-9a-f]+ direct=1 satp=0x[0-9a-f]+ mode=8 root=0x[0-9a-f]+ tables=[0-9]+ used=[0-9]+ arena=0x[0-9a-f]+-0x[0-9a-f]+$
^# mmu map: high=0x[0-9a-f]+ physmap=0x[0-9a-f]+ guard=0x[0-9a-f]+-0x[0-9a-f]+ guard_none=1 alias=1$
^# guard load stats: faults\+[0-9]+ load\+[0-9]+ store\+[0-9]+ recovered\+[0-9]+ fatal\+[0-9]+ last_scause=0xd last_stval=0x[0-9a-f]+ armed=[0-9]+$
^OpenSBI v[0-9]+\.[0-9]+\.[0-9]+$
^Platform HART Count *: *__SMP__$
EOF
            ;;
        ktest-user) cat <<'EOF'
^# user image: hart=[0-9]+ stvec=0x[0-9a-f]+ direct=1 satp=0x[0-9a-f]+ mode=8 root=0x[0-9a-f]+ obj_arena=0x[0-9a-f]+-0x[0-9a-f]+ untyped=2$
^# init load: elf=1 entry=0x40000000 segs=3 text=1 ro=1 data=1 stack=8 image_end=0x[0-9a-f]+ guard=0x40007000 guard_none=1 wx=0 flags=1 high=1 root=0x[0-9a-f]+ asid=[1-9][0-9]* check=1$
^# init run: entry=0x40000000 u_mode=1 sret_spp=0 ecall_spp=0 entries=1 syscalls=5 exits=1 exit=42$
^# u sstatus: enter=0x[0-9a-f]+ trap=0x[0-9a-f]+ spp=0 spie=0 sie=0 sum=0 uxl=2$
^# user put_str: calls=2 bytes=51 sum_off=1$
^# user exit: code=42 state=Exited exits=1$
^# user badsys: nr=99 err=19 calls=2 panic=0$
^U: init hello from u-mode$
^U: bad syscall err=19 ok$
^# u-mode read: hart=[0-9]+ addr=0x80200000 expect=scause=0xd stval=0x80200000$
^# u-mode fault: scause=0xd stval=0x80200000 sepc=0x[0-9a-f]+ from_u=1 kernel_addr=1 fatal=1$
^OpenSBI v[0-9]+\.[0-9]+\.[0-9]+$
^Platform HART Count *: *__SMP__$
EOF
            ;;
        *) return 1 ;;
    esac |
        sed -e "s|__SMPM1__|$((smp - 1))|g" \
            -e "s|__SMP__|$smp|g" \
            -e "s|__MASK__|$mask|g" \
            -e "s|__IPI_MODE__|$ipi_mode|g" \
            -e "s|__IPI_DELIVERED__|$ipi_del|g" \
            -e "s|__IPI_DRAIN__|$ipi_drain|g" \
            -e "s|__IPI_COALESCE__|$ipi_coal|g" \
            -e "s|__TLB_MODE__|$tlb_mode|g" \
            -e "s|__TLB_SENT__|$tlb_sent|g" \
            -e "s|__TLB_PEER__|$tlb_peer|g" \
            -e "s|__TLB_REMOVED__|$tlb_removed|g" \
            -e "s|__BOOT_HART__|$hart|g" \
            -e "s|__EVID24__|$evid24|g"
}

# Mẫu PANIC **duy nhất** được phép của từng ảnh (đúng 1 dòng, không hơn).
panic_allow_re() {
    case "$1" in
        ktest) printf '%s' '^PANIC: nested trap \(KT-45: trap trong handler = fatal, khong resume\)$' ;;
        ktest-guard) printf '%s' '^PANIC: guard page fault scause=0xf stval=0x[0-9a-f]+ sepc=0x[0-9a-f]+ sstatus=0x[0-9a-f]+$' ;;
        ktest-user) printf '%s' '^PANIC: u-mode fault scause=0xd stval=0x80200000 sepc=0x[0-9a-f]+ sstatus=0x[0-9a-f]+ from_u=1$' ;;
        *) return 1 ;;
    esac
}

# ------------------------------------------------------------ bộ kiểm một log

# Khung TAP: version/kế hoạch/thứ tự, dãy ok 1..N liên tục không trùng không thiếu, không `not ok`,
# đúng 1 PANIC đúng mẫu.
check_tap_common() {
    local log="$1" label="$2" plan="" okwant="" vline="" pline="" firstok="" vcount="" pcount="" \
        okcount="" notok="" ptotal="" pbad="" allow="" wantseq="" gotseq=""
    plan="$(expected_field "$label" 2)"
    okwant="$(expected_field "$label" 3)"
    vline="$(grep -n -x -F 'TAP version 13' "$log" | head -1 | cut -d: -f1)"
    pline="$(grep -n -x -F "1..$plan" "$log" | head -1 | cut -d: -f1)"
    firstok="$(grep -nE '^ok [0-9]+' "$log" | head -1 | cut -d: -f1)"
    vcount="$(grep -c -x -F 'TAP version 13' "$log" || true)"
    pcount="$(grep -c -x -F "1..$plan" "$log" || true)"
    okcount="$(grep -cE '^ok [0-9]+' "$log" || true)"
    notok="$(grep -cE '^not ok' "$log" || true)"
    [ "${vcount:-0}" -eq 1 ] || MISSING="$MISSING tap-version=${vcount:-0}"
    [ "${pcount:-0}" -eq 1 ] || MISSING="$MISSING plan-1..$plan=${pcount:-0}"
    if [ -n "${vline:-}" ] && [ -n "${pline:-}" ] && [ -n "${firstok:-}" ]; then
        if [ "$vline" -ge "$pline" ] || [ "$pline" -ge "$firstok" ]; then
            MISSING="$MISSING tap-thu-tu"
        fi
    fi
    [ "${okcount:-0}" -eq "$okwant" ] || MISSING="$MISSING ok-count=${okcount:-0}/$okwant"
    [ "${notok:-0}" -eq 0 ] || MISSING="$MISSING not-ok=${notok:-0}"
    # Dãy số phải đúng 1..N **theo thứ tự**: thiếu (2..37), trùng (1 1 2...), đảo thứ tự đều đỏ.
    gotseq="$(grep -E '^ok [0-9]+' "$log" | sed -e 's/^ok //' -e 's/ .*//' | tr '\n' ' ')"
    wantseq="$(awk -v n="$plan" 'BEGIN { for (i = 1; i <= n; i++) printf "%d ", i }')"
    [ "$gotseq" = "$wantseq" ] || MISSING="$MISSING tap-day-1..$plan"
    # PANIC: tổng đúng 1, không dòng nào lệch mẫu cho phép của ảnh.
    allow="$(panic_allow_re "$label")"
    ptotal="$(grep -cE '^[[:space:]]*PANIC' "$log" || true)"
    pbad="$(grep -E '^[[:space:]]*PANIC' "$log" | grep -Evc -- "$allow" || true)"
    [ "${pbad:-0}" -eq 0 ] || MISSING="$MISSING panic-khac=${pbad:-0}"
    [ "${ptotal:-0}" -eq 1 ] || MISSING="$MISSING panic-tong=${ptotal:-0}/1"
}

# Dòng `# hart …`/`# stack …` per-hart + số dòng `# timer hart=`/`# plic hart=` (ảnh chính).
check_hart_lines() {
    local log="$1" smp="$2" h=""
    h=0
    while [ "$h" -lt "$smp" ]; do
        grep -Eq "^# hart $h alive ticket=[0-9]+ tp=$h " "$log" || MISSING="$MISSING hart$h-alive"
        grep -Eq "^# stack hart=$h .* owner=$h .* frame_trap=1 rw_nx=1 guard_none=1$" "$log" ||
            MISSING="$MISSING hart$h-stack"
        h=$((h + 1))
    done
    # Không chỉ **đếm**: tập hart phải đúng `0..N-1` và mỗi hart đúng một dòng `ok=1` — lặp dòng
    # của hart 0 thay cho hart 1 (cùng số dòng) phải đỏ (reviewer KT-54 mục D).
    local want_ids="" got_ids=""
    h=0
    while [ "$h" -lt "$smp" ]; do
        want_ids="$want_ids$h "
        h=$((h + 1))
    done
    got_ids="$(grep -E '^# timer hart=[0-9]+ .* ok=1$' "$log" |
        sed -n 's/^# timer hart=\([0-9][0-9]*\) .*/\1/p' | sort -n | tr '\n' ' ')"
    [ "$got_ids" = "$want_ids" ] || MISSING="$MISSING timer-hart-lines"
    got_ids="$(grep -E '^# plic hart=[0-9]+ .* ok=1$' "$log" |
        sed -n 's/^# plic hart=\([0-9][0-9]*\) .*/\1/p' | sort -n | tr '\n' ' ')"
    [ "$got_ids" = "$want_ids" ] || MISSING="$MISSING plic-hart-lines"
}

# KT-55: `# conf:` tự khai số bước trace của từng ca ⇒ số dòng `# trace: <scn> ` phải **bằng**
# đúng số đó (bắt mất/đội dòng mà không cần hardcode số ở runner), và mọi dòng phải đúng định dạng
# `# trace: <scn> <step> <op> [k=v …] => <kết quả>`.
check_trace() {
    local log="$1" smp="$2" scn="" steps="" n="" bad="" conf_count="" got="" want="" dup=""
    # Đúng 5 dòng `# conf:` của 5 ca (không thừa/thiếu/trùng).
    conf_count="$(grep -Ec '^# conf: scn=(22|23|24|25|26) name=' "$log" || true)"
    [ "${conf_count:-0}" -eq 5 ] || MISSING="$MISSING conf-count=${conf_count:-0}/5"
    for scn in 22 23 24 25 26; do
        steps="$(sed -n "s/^# conf: scn=$scn name=[^ ]* steps=\([0-9]*\) .*/\1/p" "$log" | head -1)"
        if [ -z "$steps" ]; then
            MISSING="$MISSING conf$scn"
            continue
        fi
        n="$(grep -c -E "^# trace: $scn " "$log" || true)"
        [ "${n:-0}" -eq "$steps" ] || MISSING="$MISSING trace$scn=${n:-0}/$steps"
        # Ca 24 được phép 0 bước **chỉ** khi smp=1 và conf ghi `skipped=smp1`; còn lại `steps>0`.
        if [ "$steps" -eq 0 ]; then
            if [ "$scn" = 24 ] && [ "$smp" -lt 2 ] &&
                grep -Eq '^# conf: scn=24 name=vm-tlb steps=0 skipped=smp1$' "$log"; then
                continue
            fi
            MISSING="$MISSING trace$scn-empty"
            continue
        fi
        # Tập `step` phải đúng **tập bước conformance** của tệp `.scn` (nguồn độc lập:
        # `check_scn.py --steps <scn>`), không trùng, không thiếu — không chỉ đếm dòng.
        got="$(sed -n "s/^# trace: $scn \([0-9][0-9]*\) .*/\1/p" "$log" | sort -n | tr '\n' ' ')"
        want="$(cd "$V2_ROOT/.." && python3.12 v2/test/scn/check_scn.py --steps "$scn" 2>/dev/null |
            tr ' ' '\n' | sed '/^$/d' | sort -n | tr '\n' ' ')"
        if [ -z "$want" ]; then
            MISSING="$MISSING trace$scn-steps-tool"
        else
            [ "$got" = "$want" ] || MISSING="$MISSING trace$scn-steps"
            [ "$(printf '%s' "$got" | wc -w | tr -d ' ')" -eq "$steps" ] ||
                MISSING="$MISSING trace$scn-count"
        fi
        dup="$(sed -n "s/^# trace: $scn \([0-9][0-9]*\).*/\1/p" "$log" | sort -n | uniq -d | tr '\n' ' ')"
        [ -z "$dup" ] || MISSING="$MISSING trace$scn-dup=$dup"
        bad="$(grep -E "^# trace: $scn " "$log" |
            grep -Evc '^# trace: [0-9]+ [0-9]+ [a-z_]+( [a-z_]+=[^ ]+)* => [^ ]+$' || true)"
        [ "${bad:-0}" -eq 0 ] || MISSING="$MISSING tracefmt$scn=$bad"
    done
    # D2-80: đúng 1 dòng `# conf image:` và BUILD_ID phải khớp ảnh **đang chạy** (tính lại từ ELF).
    local want_build="" got_build="" img=""
    img="$(image_path ktest)"
    want_build="$(image_build "$img")" || want_build=""
    got_build="$(sed -n 's/^# conf image: build=\([0-9a-f][0-9a-f]*\) text=[0-9][0-9]*$/\1/p' "$log" | head -1)"
    [ -n "$want_build" ] || MISSING="$MISSING image-build-tool"
    [ -n "$got_build" ] || MISSING="$MISSING image-build-missing"
    if [ -n "$want_build" ] && [ -n "$got_build" ] && [ "$got_build" != "${want_build#build=}" ]; then
        MISSING="$MISSING image-build-mismatch"
    fi
    [ "$(grep -c '^# conf image: ' "$log" || true)" -eq 1 ] || MISSING="$MISSING image-line-count"
}

# Mọi dòng `# evidence` bắt buộc của ảnh. In chẩn đoán ra stderr khi thiếu (để sửa nhanh).
check_evidence() {
    local log="$1" label="$2" smp="$3" hart="${4:-}" n=0 re=""
    while IFS= read -r re; do
        n=$((n + 1))
        if ! grep -Eq -- "$re" "$log"; then
            MISSING="$MISSING ev$n"
            echo "#   thiếu evidence#$n: $re" >&2
        fi
    done < <(evidence_for "$label" "$smp" "$hart")
}

# `validate_log <ảnh> <log> <smp>`: kiểm một log; trả 0 nếu sạch (MISSING rỗng), 1 nếu thiếu.
# Log QEMU có **CRLF ở dòng của firmware** (`OpenSBI v1.8.1\r`, `Platform HART Count … 2\r`) còn
# dòng kernel là LF ⇒ chuẩn hoá `\r` vào một bản tạm **trước khi** kiểm để mọi mẫu neo `$` đều đúng
# (không sửa log gốc của người đọc; bản tạm xoá ngay sau khi kiểm).
validate_log() {
    local label="$1" log="$2" smp="$3" norm="" rc=0
    MISSING=""
    [ -f "$log" ] || {
        MISSING="log-khong-thay"
        return 1
    }
    norm="$(mktemp "${TMPDIR:-/tmp}/kt54-norm.XXXXXX")" || {
        MISSING="khong-tao-duoc-tmp"
        return 1
    }
    if ! tr -d '\r' <"$log" >"$norm"; then
        rm -f "$norm"
        MISSING="doc-log-loi"
        return 1
    fi
    validate_norm "$label" "$norm" "$smp" || rc=1
    rm -f "$norm"
    return "$rc"
}

# `validate_norm <ảnh> <log-đã-chuẩn-hoá> <smp>`: thân kiểm (mọi `grep` neo `^…$` dùng tệp này).
validate_norm() {
    local label="$1" log="$2" smp="$3" plan="" n=0 name="" cnt="" vplan="" bh=""
    MISSING=""
    plan="$(expected_field "$label" 2)"
    vplan="$(manifest_for "$label" "$smp" | wc -l | tr -d ' ')"
    check_tap_common "$log" "$label" "$smp"
    # Mỗi TAP N: đúng **một** dòng `ok N - <tên>` (khớp manifest) ⇒ bắt cả trùng/thiếu/sai tên.
    n=0
    while IFS= read -r name; do
        n=$((n + 1))
        cnt="$(grep -c -x -F "ok $n - $name" "$log" || true)"
        if [ "${cnt:-0}" -ne 1 ]; then
            MISSING="$MISSING tap$n"
            echo "#   thiếu TAP #$n [$name] (thấy ${cnt:-0} dòng, cần đúng 1)" >&2
        fi
    done < <(manifest_for "$label" "$smp")
    [ "$n" -eq "$vplan" ] || MISSING="$MISSING manifest=$n/$vplan"
    case "$label" in
        ktest)
            check_hart_lines "$log" "$smp"
            check_trace "$log" "$smp"
            check_evidence "$log" "$label" "$smp"
            ;;
        ktest-guard)
            # Hart boot do OpenSBI chọn (có thể khác 0) ⇒ đọc từ log rồi đòi fault/load/store đúng hart đó.
            bh="$(sed -n 's/^# guard image: hart=\([0-9][0-9]*\) .*/\1/p' "$log" | head -1)"
            [ -n "$bh" ] || MISSING="$MISSING guard-image-hart"
            if [ -n "$bh" ]; then
                grep -Eq "^# guard load: hart=$bh cham 0x" "$log" || MISSING="$MISSING guard-load-line"
                grep -Eq "^# guard store: hart=$bh ghi 0x" "$log" || MISSING="$MISSING guard-store-line"
                grep -Eq "^# guard fault load scause=0xd stval=0x[0-9a-f]* sepc=0x[0-9a-f]* len=[24] recovered=1 hart=$bh$" "$log" ||
                    MISSING="$MISSING guard-load-real"
                awk -v h="$bh" '/^Platform HART Count *:/ { gsub(/\r/, ""); if ($NF > h) ok = 1 } END { exit !ok }' "$log" ||
                    MISSING="$MISSING guard-hart-out-of-range"
            fi
            check_evidence "$log" "$label" "$smp" "$bh"
            ;;
        ktest-user)
            check_evidence "$log" "$label" "$smp"
            ;;
    esac
    [ -z "$MISSING" ] || return 1
    return 0
}

# ------------------------------------------------------------- boot một ảnh

# `boot_image <ảnh> <log> <smp>`: 0 = QEMU tự thoát; 124 = quá 20 s (đã kill đúng PID của script).
boot_image() {
    local image="$1" out="$2" smp="$3" pid waited grace
    "$QEMU" $QEMU_ICOUNT -machine virt,aia=none,aclint=on -bios "$SBI" -kernel "$image" -nographic \
        -cpu rv64 -m 128M -smp "$smp" -no-reboot >"$out" 2>&1 &
    pid=$!
    waited=0
    while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt "$TIMEOUT_DS" ]; do
        sleep 0.1
        waited=$((waited + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null # chỉ kill ĐÚNG pid do script này tạo (AGENTS §9)
        grace=0
        while kill -0 "$pid" 2>/dev/null && [ "$grace" -lt 50 ]; do
            sleep 0.1
            grace=$((grace + 1))
        done
        kill -9 "$pid" 2>/dev/null # vẫn đúng PID đó; thoát hẳn để không treo `wait`
        wait "$pid" 2>/dev/null
        return 124
    fi
    wait "$pid" 2>/dev/null
    return 0
}

# Bảng tổng kết: SUMMARY = các dòng `ảnh|smp|boot|TAP|kết quả|log`.
SUMMARY=""
record_row() {
    SUMMARY="$SUMMARY$1|$2|$3|$4|$5|$6
"
    ROWS=$((ROWS + 1))
}

# `run_one <ảnh> <smp> <boot-i>`: boot + kiểm + ghi bảng; 0 = sạch.
run_one() {
    local label="$1" smp="$2" idx="$3" image="" log="" oks="" t0="" dt=""
    image="$(image_path "$label")"
    log="$(log_path "$label" "$smp" "$idx")"
    t0="$(now_s)"
    RUN_RESULT=""
    if ! boot_image "$image" "$log" "$smp"; then
        dt=$(($(now_s) - t0))
        total_boots=$((total_boots + 1))
        fails=$((fails + 1))
        RUN_RESULT=TIMEOUT
        record_row "$label" "$smp" "$idx" "-" TIMEOUT "$log"
        echo "not ok - $label smp=$smp boot $idx: quá $((TIMEOUT_DS / 10)) s (treo, đã kill pid), log $log"
        return 1
    fi
    total_boots=$((total_boots + 1))
    dt=$(($(now_s) - t0))
    oks="$(grep -cE '^ok [0-9]+' "$log" || true)"
    if validate_log "$label" "$log" "$smp"; then
        RUN_RESULT=PASS
        clean_boots=$((clean_boots + 1))
        record_row "$label" "$smp" "$idx" "${oks:-0}/$(expected_field "$label" 2)" PASS "$log"
        echo "ok - $label smp=$smp boot $idx (${oks:-0} TAP, ${dt}s) log $log"
        return 0
    fi
    fails=$((fails + 1))
    RUN_RESULT=FAIL
    record_row "$label" "$smp" "$idx" "${oks:-0}/$(expected_field "$label" 2)" FAIL "$log"
    echo "not ok - $label smp=$smp boot $idx: thiếu$MISSING, log $log"
    return 1
}

print_summary() {
    local l s
    echo "# ================= BẢNG TỔNG KẾT (image × smp × boot) ================="
    printf '# %-11s %-4s %-5s %-9s %-8s %s\n' "image" "smp" "boot" "TAP" "kết quả" "log"
    printf '%s' "$SUMMARY" | while IFS='|' read -r l s b t v p; do
        [ -n "${l:-}" ] || continue
        printf '# %-11s %-4s %-5s %-9s %-8s %s\n' "$l" "$s" "$b" "$t" "$v" "$p"
    done
    echo "# ------------------------- gộp theo (image, smp) -------------------------"
    for l in $LABELS; do
        for s in $SMP_LIST; do
            printf '%s' "$SUMMARY" | awk -F'|' -v l="$l" -v s="$s" '
                $1 == l && $2 == s { t++; if ($5 == "PASS") p++; else if ($5 == "TIMEOUT") to++; else if ($5 == "FAIL") f++ }
                END { if (t > 0) printf "# %-11s smp=%-2s %d/%d boot sạch (FAIL=%d TIMEOUT=%d)\n", l, s, p, t, f + 0, to + 0 }'
        done
    done
    echo "# ======================================================================="
}

# ------------------------------------------------------------------ chế độ

parse_args() {
    case "${1:-}" in
        '') return 0 ;;
        --check-log)
            MODE=check-log
            CHECK_LABEL="${2:-}"
            CHECK_LOG="${3:-}"
            CHECK_SMP="${4:-1}"
            ;;
        --log-path)
            MODE=log-path
            LOGP_LABEL="${2:-}"
            LOGP_SMP="${3:-1}"
            LOGP_I="${4:-1}"
            ;;
        --print-expected) MODE=print-expected ;;
        --print-evidence)
            MODE=print-evidence
            PRINT_LABEL="${2:-}"
            PRINT_SMP="${3:-1}"
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        -*) die "tham số '$1' không hiểu (xem --help)" ;;
        *)
            MODE=matrix
            N_BOOT="${1:-1}"
            SMP_ARG="${2:-1}"
            case "$SMP_ARG" in
                matrix) SMP_LIST="1 2 4" ;;
                *) SMP_LIST="$SMP_ARG" ;;
            esac
            ;;
    esac
}

check_smp_arg() {
    local s="" nsmp=0
    for s in $SMP_LIST; do
        nsmp=$((nsmp + 1))
        case "$s" in '' | *[!0-9]*) die "SMP [$s] không phải số" ;; esac
        [ "$s" -ge 1 ] && [ "$s" -le 8 ] || die "SMP=$s ngoài [1,8] (MAX_HARTS=8)"
    done
    # Danh sách rỗng (" " hoặc "") ⇒ `for` không chạy ⇒ **0 boot mà vẫn PASS** (reviewer KT-54 mục A):
    # fail-closed, không có cấu hình nào để boot thì không được coi là xanh.
    [ "$nsmp" -ge 1 ] || die "SMP_LIST rỗng (SMP='$SMP_ARG') — không có cấu hình nào để boot"
    SMP_COUNT="$nsmp"
}

do_print_expected() {
    local l
    echo "# BẢNG expected.txt ($EXPECTED_FILE) — mỗi ảnh một dòng <ảnh> <kế hoạch TAP> <số dòng ok>"
    printf '# %-11s %-7s %-6s %s\n' "image" "plan" "ok" "TAP runner kiểm tên"
    for l in $LABELS; do
        printf '# %-11s %-7s %-6s %s\n' "$l" "$(expected_field "$l" 2)" "$(expected_field "$l" 3)" \
            "$(manifest_for "$l" 1 | wc -l | tr -d ' ')"
    done
}

# `--print-evidence <ảnh> [SMP]`: in `<ordinal> <regex>` của danh sách evidence bắt buộc — để
# `selftest.sh`/tài liệu tra **ordinal** mà không hardcode (đổi danh sách thì test tự theo).
do_print_evidence() {
    local label="$1" smp="$2" n=0 re=""
    known_label "$label" || die "--print-evidence: nhãn [$label] không biết (dùng: $LABELS)"
    case "$smp" in '' | *[!0-9]*) die "--print-evidence: SMP [$smp] không phải số" ;; esac
    while IFS= read -r re; do
        n=$((n + 1))
        printf '%d\t%s\n' "$n" "$re"
    done < <(evidence_for "$label" "$smp")
}

do_log_path() {
    known_label "$LOGP_LABEL" || die "--log-path: nhãn [$LOGP_LABEL] không biết (dùng: $LABELS)"
    case "$LOGP_SMP" in '' | *[!0-9]*) die "--log-path: SMP [$LOGP_SMP] không phải số" ;; esac
    case "$LOGP_I" in '' | *[!0-9]*) die "--log-path: boot index [$LOGP_I] không phải số" ;; esac
    log_path "$LOGP_LABEL" "$LOGP_SMP" "$LOGP_I"
}

do_check_log() {
    local label="$1" log="$2" smp="$3" ev=""
    known_label "$label" || {
        echo "not ok - check-log: nhãn [$label] không biết (dùng: $LABELS)"
        return 1
    }
    case "$smp" in '' | *[!0-9]*) echo "not ok - check-log: SMP [$smp] không phải số"; return 1 ;; esac
    if ! load_expected; then
        echo "not ok - check-log $label smp=$smp: $EXPECT_ERR"
        return 1
    fi
    if ! check_manifests "$smp"; then
        echo "not ok - check-log $label smp=$smp: $EXPECT_ERR"
        return 1
    fi
    ev="$(evidence_for "$label" "$smp" "" | wc -l | tr -d ' ')"
    if validate_log "$label" "$log" "$smp"; then
        echo "ok - check-log $label smp=$smp (plan=$(expected_field "$label" 2) ok=$(expected_field "$label" 3), $ev evidence) log $log"
        return 0
    fi
    echo "not ok - check-log $label smp=$smp: thiếu$MISSING, log $log"
    return 1
}

# ------------------------------------------------------- vòng boot thật

run_matrix() {
    case "$N_BOOT" in '' | *[!0-9]*) die "NBOOT [$N_BOOT] không phải số" ;; esac
    [ "$N_BOOT" -ge 1 ] || die "NBOOT phải >= 1"
    check_smp_arg
    load_expected || die "$EXPECT_ERR"
    check_manifests "${SMP_LIST%% *}" || die "$EXPECT_ERR"
    mkdir -p "$LOGDIR" || die "không tạo được $LOGDIR"

    command -v cargo >/dev/null 2>&1 || die "không có cargo"
    QEMU="$(command -v qemu-system-riscv64 || true)"
    [ -n "$QEMU" ] || die "không có qemu-system-riscv64"
    [ -f "$EXPECTED_FILE" ] || die "không thấy $EXPECTED_FILE"

    # OpenSBI `fw_dynamic` nạp trực tiếp, không U-Boot/EDK2 (D2-72 Q3).
    SBI="${OPENSBI:-}"
    if [ -z "$SBI" ]; then
        for c in /opt/homebrew/share/qemu/opensbi-riscv64-generic-fw_dynamic.bin \
            /usr/share/qemu/opensbi-riscv64-generic-fw_dynamic.bin \
            /usr/local/share/qemu/opensbi-riscv64-generic-fw_dynamic.bin; do
            [ -f "$c" ] && {
                SBI="$c"
                break
            }
        done
    fi
    [ -f "${SBI:-/nonexistent}" ] || die "không tìm thấy OpenSBI fw_dynamic (đặt OPENSBI=...)"

    # QEMU version: thiếu hoặc dưới sàn major.minor ⇒ FAIL, không tự hạ (D2-72 Q2).
    local qv qmaj qrest qmin
    qv="$("$QEMU" --version 2>/dev/null | sed -n '1s/.*version \([0-9][^ ]*\).*/\1/p')"
    [ -n "$qv" ] || die "không đọc được phiên bản QEMU"
    case "$qv" in *.*) ;; *) die "phiên bản QEMU [$qv] không có dạng major.minor" ;; esac
    qmaj="${qv%%.*}"
    qrest="${qv#*.}"
    qmin="${qrest%%.*}"
    case "$qmaj" in '' | *[!0-9]*) die "không đọc được major của QEMU [$qv]" ;; esac
    case "$qmin" in '' | *[!0-9]*) die "không đọc được minor của QEMU [$qv]" ;; esac
    if [ "$qmaj" -lt "$QEMU_MIN_MAJOR" ] ||
        { [ "$qmaj" -eq "$QEMU_MIN_MAJOR" ] && [ "$qmin" -lt "$QEMU_MIN_MINOR" ]; }; then
        die "QEMU $qv < sàn $QEMU_MIN_MAJOR.$QEMU_MIN_MINOR (D2-72 Q2) — không tự hạ"
    fi

    echo "# runner KT-54: expected.txt = nguồn sự thật duy nhất; TAP 1..N liên tục; đúng 1 PANIC theo mẫu"
    echo "# qemu $qv (sàn $QEMU_MIN_MAJOR.$QEMU_MIN_MINOR, D2-72 Q2) — icount shift=6 (D2-74)"
    echo "# opensbi $SBI"
    local l
    for l in $LABELS; do
        echo "# expected[$l]: plan=$(expected_field "$l" 2) ok=$(expected_field "$l" 3) ten-TAP=$(manifest_for "$l" 1 | wc -l | tr -d ' ')"
    done
    echo "# smp-list: $SMP_LIST — NBOOT=$N_BOOT — timeout=$((TIMEOUT_DS / 10)) s/boot — logdir=$LOGDIR"

    echo "# build ảnh test chính+guard ($TARGET, release, --features ktest)"
    if ! cargo build --manifest-path "$MANIFEST" -p vifast-kbin-riscv64 \
        --bin vifast-kbin-ktest --bin vifast-kbin-ktest-guard \
        --features ktest --target "$TARGET" --release >/tmp/k3-ktest-build.log 2>&1; then
        tail -20 /tmp/k3-ktest-build.log >&2
        die "build ảnh test FAIL — CHƯA có ảnh test (xem /tmp/k3-ktest-build.log)"
    fi
    # KT-53: ảnh user bật thêm feature `user` (module U-mode chỉ có trong ảnh này).
    echo "# build ảnh test user ($TARGET, release, --features ktest,user)"
    if ! cargo build --manifest-path "$MANIFEST" -p vifast-kbin-riscv64 \
        --bin vifast-kbin-ktest-user --features ktest,user \
        --target "$TARGET" --release >/tmp/k3-ktest-build-user.log 2>&1; then
        tail -20 /tmp/k3-ktest-build-user.log >&2
        die "build ảnh test user FAIL — CHƯA có ảnh test (xem /tmp/k3-ktest-build-user.log)"
    fi
    for l in $LABELS; do
        [ -f "$(image_path "$l")" ] || die "không thấy ảnh test $(image_path "$l") — CHƯA có ảnh test"
    done

    # Budget: mỗi ảnh test cũng phải ≤ 262.144 B (D2-72 Q5, không tách ngân sách riêng).
    for l in $LABELS; do
        bash "$BUDGET" "$(image_path "$l")" "$l" || die "budget ảnh test [$l] VƯỢT trần"
    done

    # Ảnh test phải mang chuỗi `ktest` (D2-08: tách khỏi ảnh sản phẩm).
    local hits img
    for l in $LABELS; do
        img="$(image_path "$l")"
        if command -v strings >/dev/null 2>&1; then
            hits="$(strings "$img" 2>/dev/null | grep -c ktest || true)"
        else
            hits="$(LC_ALL=C grep -a -o ktest "$img" 2>/dev/null | wc -l | tr -d ' ' || true)"
        fi
        [ "${hits:-0}" -ge 1 ] || die "ảnh [$l] không chứa chuỗi 'ktest' — nhầm ảnh?"
        echo "# image=$l ktest-strings=$hits"
    done

    local smp="" i="" skip=""
    for smp in $SMP_LIST; do
        check_manifests "$smp" || die "$EXPECT_ERR"
        i=1
        while [ "$i" -le "$N_BOOT" ]; do
            skip=0
            for l in $LABELS; do
                if [ "$skip" -ne 0 ]; then
                    record_row "$l" "$smp" "$i" "-" SKIP "-"
                    continue
                fi
                # Chỉ **TIMEOUT** mới bỏ các ảnh còn lại của cùng (smp, boot) (máy có thể treo/chậm);
                # FAIL của một ảnh KHÔNG cần bỏ ảnh khác — boot độc lập, chạy tiếp để lấy thêm bằng chứng.
                if ! run_one "$l" "$smp" "$i"; then
                    [ "$RUN_RESULT" = TIMEOUT ] && skip=1
                fi
            done
            i=$((i + 1))
        done
    done

    # Không được phép "xanh vì không chạy gì": **mọi slot** (ảnh, smp, boot) phải có dòng trong
    # bảng — slot boot thật hoặc SKIP (chỉ sinh khi ảnh trước trong cùng vòng TIMEOUT).
    local want_rows=$((N_BOOT * SMP_COUNT * 3))
    [ "$ROWS" -eq "$want_rows" ] ||
        die "chạy THIẾU slot: $ROWS/$want_rows (NBOOT=$N_BOOT × smp=$SMP_COUNT × 3 ảnh) — không coi là PASS"
    [ "$total_boots" -ge 1 ] || die "không boot lần nào (0/$want_rows) — không coi là PASS"
    print_summary
    echo "# KTEST: $clean_boots/$total_boots boot sạch (smp=$SMP_ARG, NBOOT=$N_BOOT, ảnh/boot=$LABELS)"
    [ "$fails" -eq 0 ] || {
        echo "# KTEST: FAIL ($fails lỗi)"
        return 1
    }
    echo "# ktest.sh: PASS"
    return 0
}

# ------------------------------------------------------------------- main

parse_args "$@"
# `KT54_EXPECTED`: chỉ cho `--check-log` (selftest/mutation); vòng boot thật **luôn** đọc
# expected.txt trong repo để không có đường tắt đổi nguồn sự thật.
if [ -n "${KT54_EXPECTED:-}" ]; then
    [ "$MODE" = check-log ] || die "KT54_EXPECTED chỉ dùng cho --check-log (vòng boot thật đọc $HERE/expected.txt)"
    EXPECTED_FILE="$KT54_EXPECTED"
    echo "# KT54_EXPECTED=$EXPECTED_FILE (chỉ --check-log)" >&2
fi

total_boots=0
clean_boots=0
fails=0

case "$MODE" in
    print-expected)
        load_expected || die "$EXPECT_ERR"
        do_print_expected
        ;;
    log-path) do_log_path ;;
    print-evidence) do_print_evidence "$PRINT_LABEL" "$PRINT_SMP" ;;
    check-log)
        do_check_log "$CHECK_LABEL" "$CHECK_LOG" "$CHECK_SMP" || exit 1
        echo "# check-log: PASS"
        ;;
    matrix)
        run_matrix || exit 1
        ;;
    *) die "chế độ [$MODE] không biết" ;;
esac
exit 0
