#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# KT-44/45/46/47/48/49 — boot ảnh **TEST** riscv64 K3 (crate `vifast-kbin-riscv64`, feature `ktest`).
#
# D2-08: ảnh test KHÔNG nằm trong ảnh sản phẩm — cùng crate nhưng bin + feature riêng.
# KT-49/50: **hai** ảnh test (mỗi boot chỉ kết thúc được một lần bằng PANIC có chủ đích):
#   - `vifast-kbin-ktest`       : 37 TAP — KT-45 (image + 3 trap) + KT-46 (mmu on, map layout,
#                                 walk khop) + KT-48 (3 TAP PLIC + UART IRQ thật) + KT-47 (4 TAP
#                                 timer SBI: 100 tick đơn điệu, 1.000 tick không mất, irq thật,
#                                 drift Δ ∈ [9,11] ms) + **KT-49 SMP** (5 TAP: N harts alive qua
#                                 SBI HSM, stack/guard riêng mỗi hart, timer PerHart, PLIC context
#                                 `2*h+1` per hart, barrier + timeout không treo) + **KT-50 IPI**
#                                 (5 TAP: 100 IPI thật qua SBI/MSIP, drain khi chờ ack, không
#                                 self-target, coalesce không mất kind, không nhầm timer/external)
#                                 + **KT-51 TLB/ASID** (5 TAP: 100 vòng shootdown Ok, stale mapping
#                                 dương tính, hart ngoài `active_on` không bị chờ, rollover flush,
#                                 timeout không treo) + **KT-52 object layer** (7 TAP: Retype
#                                 VSpace, bất biến byte Untyped, Frame 3 cỡ, W^X 2 ca, map/unmap
#                                 4 KiB phần cứng, 0 rò) + trap lồng fatal (TAP 37).
#   - `vifast-kbin-ktest-guard` : 3 TAP — guard page fault **thật**: load `scause=13` (probe resume)
#                                 rồi store `scause=15` fatal `PANIC: guard page fault`.
# Số TAP kỳ vọng đọc từ `expected.txt` (không hardcode hai nơi).
#
# D2-72 Q2/Q4/Q5/Q10: `-machine virt,aia=none,aclint=on -cpu rv64 -m 128M -smp N`, matrix
# **1/2/4** cho KT-49, budget riêng cho **mỗi** ảnh test (≤ 262.144 B), timeout 20 s/boot, chỉ
# `kill <pid>` do chính script tạo (AGENTS §9).
#
# Dùng: bash v2/test/boot/ktest.sh [NBOOT] [SMP|matrix]   (mặc định 1 boot, 1 hart)
#   vd: bash v2/test/boot/ktest.sh 50 matrix   # 50 boot **mỗi** N=1/2/4, log riêng từng N
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V2_ROOT="$(cd "$HERE/../.." && pwd)"
MANIFEST="$V2_ROOT/kernel/Cargo.toml"
BUDGET="$HERE/budget.sh"
EXPECTED_FILE="$HERE/expected.txt"
TARGET="riscv64gc-unknown-none-elf"
IMG_MAIN="$V2_ROOT/kernel/target/$TARGET/release/vifast-kbin-ktest"
IMG_GUARD="$V2_ROOT/kernel/target/$TARGET/release/vifast-kbin-ktest-guard"
N_BOOT="${1:-1}"
SMP_ARG="${2:-1}"
case "$SMP_ARG" in
    matrix) SMP_LIST="1 2 4" ;;
    *) SMP_LIST="$SMP_ARG" ;;
esac
# KT-47: `-icount` để QEMU phục vụ timer ảo (ACLINT/SBI TIME) **đúng hạn** — không icount, QEMU 11
# TCG trên macOS chỉ chạy virtual timer mỗi ~200 ms (đo được: Δ tick 0.3–2.9 s), nên không thể đo
# DoD "Δ tick ∈ [9,11] ms". Dòng lệnh QEMU của D2-72 Q2 (machine/cpu/ram/smp) **giữ nguyên**; chỉ
# thêm icount cho **ảnh test** (đo định thời tất định); `run.sh`/soak giữ nguyên dòng D2-72 Q2.
# **Shift CỐ ĐỊNH = 6** (64 ns/lệnh), KHÔNG dùng shift tự động (`auto`): `auto` tự đổi shift giữa chừng (bám thời
# gian thật) ⇒ thời gian ảo nhảy, Δ tick lệch 3–17 ms (TAP drift đỏ ngẫu nhiên); shift cố định giữ
# Δ = 10 ms ± quantization icount, vẫn trong [9,11] ms. Deviation ghi ở `v2/docs/k3-riscv64.md`
# §10.5 + JOURNAL/STATE + D2-74.
QEMU_ICOUNT="-icount shift=6"
TIMEOUT_DS=200   # 20 s/boot (D2-72 Q10), đơn vị 0,1 s
QEMU_MIN_MAJOR=9 # sàn D2-72 Q2: QEMU >= 9.2
QEMU_MIN_MINOR=2

export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

die() { echo "ktest.sh: $*" >&2; exit 1; }

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
        [ -f "$c" ] && { SBI="$c"; break; }
    done
fi
[ -f "${SBI:-/nonexistent}" ] || die "không tìm thấy OpenSBI fw_dynamic (đặt OPENSBI=...)"

# QEMU version: thiếu hoặc dưới sàn major.minor ⇒ FAIL, không tự hạ (D2-72 Q2).
QEMU_VERSION="$("$QEMU" --version 2>/dev/null | sed -n '1s/.*version \([0-9][^ ]*\).*/\1/p')"
[ -n "$QEMU_VERSION" ] || die "không đọc được phiên bản QEMU"
case "$QEMU_VERSION" in *.*) ;; *) die "phiên bản QEMU '$QEMU_VERSION' không có dạng major.minor" ;; esac
QEMU_MAJOR="${QEMU_VERSION%%.*}"
QEMU_REST="${QEMU_VERSION#*.}"
QEMU_MINOR="${QEMU_REST%%.*}"
case "$QEMU_MAJOR" in '' | *[!0-9]*) die "không đọc được major của QEMU '$QEMU_VERSION'" ;; esac
case "$QEMU_MINOR" in '' | *[!0-9]*) die "không đọc được minor của QEMU '$QEMU_VERSION'" ;; esac
if [ "$QEMU_MAJOR" -lt "$QEMU_MIN_MAJOR" ] ||
    { [ "$QEMU_MAJOR" -eq "$QEMU_MIN_MAJOR" ] && [ "$QEMU_MINOR" -lt "$QEMU_MIN_MINOR" ]; }; then
    die "QEMU $QEMU_VERSION < sàn $QEMU_MIN_MAJOR.$QEMU_MIN_MINOR (D2-72 Q2) — không tự hạ"
fi
echo "# qemu $QEMU_VERSION (sàn $QEMU_MIN_MAJOR.$QEMU_MIN_MINOR, D2-72 Q2)"
echo "# opensbi $SBI"

# Kế hoạch/số `ok` kỳ vọng cho từng ảnh — một nguồn duy nhất `expected.txt`.
expected_field() { awk -v label="$1" -v field="$2" '$1 == label { print $field; exit }' "$EXPECTED_FILE"; }
PLAN_MAIN="$(expected_field ktest 2)"
OK_MAIN="$(expected_field ktest 3)"
PLAN_GUARD="$(expected_field ktest-guard 2)"
OK_GUARD="$(expected_field ktest-guard 3)"
case "${PLAN_MAIN:-}" in '' | *[!0-9]*) die "expected.txt: thiếu kế hoạch TAP cho 'ktest'" ;; esac
case "${OK_MAIN:-}" in '' | *[!0-9]*) die "expected.txt: thiếu số ok cho 'ktest'" ;; esac
case "${PLAN_GUARD:-}" in '' | *[!0-9]*) die "expected.txt: thiếu kế hoạch TAP cho 'ktest-guard'" ;; esac
case "${OK_GUARD:-}" in '' | *[!0-9]*) die "expected.txt: thiếu số ok cho 'ktest-guard'" ;; esac
echo "# expected: ktest plan=$PLAN_MAIN ok=$OK_MAIN; ktest-guard plan=$PLAN_GUARD ok=$OK_GUARD"

echo "# build 2 ảnh test ($TARGET, release, --features ktest)"
if ! cargo build --manifest-path "$MANIFEST" -p vifast-kbin-riscv64 \
    --bin vifast-kbin-ktest --bin vifast-kbin-ktest-guard \
    --features ktest --target "$TARGET" --release >/tmp/k3-ktest-build.log 2>&1; then
    tail -20 /tmp/k3-ktest-build.log >&2
    die "build ảnh test FAIL — CHƯA có ảnh test (xem /tmp/k3-ktest-build.log)"
fi
for IMAGE in "$IMG_MAIN" "$IMG_GUARD"; do
    [ -f "$IMAGE" ] || die "không thấy ảnh test $IMAGE — CHƯA có ảnh test"
done

# Budget: mỗi ảnh test cũng phải ≤ 262.144 B (D2-72 Q5, không tách ngân sách riêng).
bash "$BUDGET" "$IMG_MAIN" ktest || die "budget ảnh test VƯỢT trần"
bash "$BUDGET" "$IMG_GUARD" ktest-guard || die "budget ảnh test guard VƯỢT trần"

# Ảnh test phải mang chuỗi `ktest` (D2-08: tách khỏi ảnh sản phẩm).
for PAIR in "ktest:$IMG_MAIN" "ktest-guard:$IMG_GUARD"; do
    LABEL="${PAIR%%:*}"
    IMAGE="${PAIR#*:}"
    if command -v strings >/dev/null 2>&1; then
        HITS="$(strings "$IMAGE" 2>/dev/null | grep -c ktest || true)"
    else
        HITS="$(LC_ALL=C grep -a -o ktest "$IMAGE" 2>/dev/null | wc -l | tr -d ' ' || true)"
    fi
    [ "${HITS:-0}" -ge 1 ] || die "ảnh $LABEL không chứa chuỗi 'ktest' — nhầm ảnh?"
    echo "# image=$LABEL ktest-strings=$HITS"
done

# boot_once <tag> <image> <smp> <index>: boot QEMU, in đường dẫn log qua stdout (chỉ đường dẫn).
boot_once() {
    local tag="$1" image="$2" smp="$3" index="$4"
    local out="/tmp/k3-ktest-$tag-smp$smp-$index.log"
    "$QEMU" $QEMU_ICOUNT -machine virt,aia=none,aclint=on -bios "$SBI" -kernel "$image" -nographic \
        -cpu rv64 -m 128M -smp "$smp" -no-reboot >"$out" 2>&1 &
    local pid=$!
    local waited=0
    while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt "$TIMEOUT_DS" ]; do
        sleep 0.1
        waited=$((waited + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null # chỉ kill ĐÚNG pid do script này tạo (AGENTS §9)
        wait "$pid" 2>/dev/null
        echo "$out"
        return 124
    fi
    wait "$pid" 2>/dev/null
    if grep -q '^OpenSBI v' "$out"; then
        echo "# opensbi $(sed -n 's/^OpenSBI \(v[0-9][0-9.]*\).*/\1/p' "$out" | head -1)" >&2
    fi
    echo "$out"
    return 0
}

# tap_numbers_ok <log> <plan>: đúng tập `ok 1..plan`, không thiếu/không lặp/không thừa.
tap_numbers_ok() {
    local out="$1" plan="$2"
    local got want
    got="$(grep -o '^ok [0-9][0-9]*' "$out" | awk '{print $2}' | sort -n | tr '\n' ' ')"
    want="$(seq 1 "$plan" | tr '\n' ' ')"
    [ "$got" = "$want" ]
}

# TAP chung cho cả hai ảnh: plan, đủ số `ok`, không `not ok`, không PANIC ngoài danh sách cho phép.
common_tap_ok() {
    local out="$1" plan="$2" ok_count="$3" allowed_panic_re="$4"
    local missing=""
    grep -q '^TAP version 13$' "$out" || missing="$missing TAP-version"
    grep -q "^1\\.\\.$plan$" "$out" || missing="$missing plan-1..$plan"
    tap_numbers_ok "$out" "$plan" || missing="$missing ok-set-1..$plan"
    local oks
    oks="$(grep -c '^ok ' "$out" || true)"
    [ "${oks:-0}" -eq "$ok_count" ] || missing="$missing ok-count=${oks:-0}/$ok_count"
    if grep -q '^not ok' "$out"; then missing="$missing not-ok"; fi
    local others
    others="$(grep '^PANIC' "$out" | grep -Evc "$allowed_panic_re" || true)"
    [ "${others:-0}" -eq 0 ] || missing="$missing PANIC-khác"
    printf '%s' "$missing"
}

fails=0
total_boots=0
clean_boots=0
for SMP in $SMP_LIST; do
    case "$SMP" in '' | *[!0-9]*) die "SMP '$SMP' không phải số" ;; esac
    [ "$SMP" -ge 1 ] && [ "$SMP" -le 8 ] || die "SMP=$SMP ngoài [1,8] (MAX_HARTS=8)"
    MASK="$(printf '%x' $(((1 << SMP) - 1)))"
    # KT-50: tên TAP 20 phụ thuộc cấu hình — `>= 2` đo IPI chéo ("delivered"), `1` đo IPI tự gửi
    # ("traps (self)") vì delivered=0 khi không có hart đích. grep TAP PHẢI dùng cùng tên.
    if [ "$SMP" -ge 2 ]; then
        IPI_MODE=cross
        IPI_TAP20="100 ipi delivered"
    else
        IPI_MODE=self-ipi
        IPI_TAP20="100 ipi traps (self)"
    fi
    smp_fails=0
    i=1
    while [ "$i" -le "$N_BOOT" ]; do
        total_boots=$((total_boots + 1))
        # ------------------------------------------- ảnh chính: 37 TAP (KT-49 SMP + KT-50 IPI + KT-51 TLB + KT-52 object)
        if out="$(boot_once main "$IMG_MAIN" "$SMP" "$i")"; then :; else
            echo "not ok - ktest smp=$SMP boot $i: quá $((TIMEOUT_DS / 10)) s (treo), log $out"
            fails=$((fails + 1))
            smp_fails=$((smp_fails + 1))
            i=$((i + 1))
            continue
        fi
        missing="$(common_tap_ok "$out" "$PLAN_MAIN" "$OK_MAIN" '^PANIC: nested trap \(KT-45: trap trong handler = fatal, khong resume\)$')"
        # 1..37: KT-45 (1..4), KT-46 (5..7), KT-48 (8..10), KT-47 (11..14), KT-49 (15..19),
        # KT-50 IPI (20..24), KT-51 TLB/ASID (25..29), KT-52 object (30..36), trap lồng (37).
        grep -q '^ok 1 - ktest image$' "$out" || missing="$missing ok1-image"
        grep -q '^ok 2 - trap breakpoint$' "$out" || missing="$missing ok2-breakpoint"
        grep -q '^ok 3 - trap ecall sbi$' "$out" || missing="$missing ok3-ecall-sbi"
        grep -q '^ok 4 - trap illegal inject$' "$out" || missing="$missing ok4-illegal-inject"
        grep -q '^ok 5 - mmu on$' "$out" || missing="$missing ok5-mmu-on"
        grep -q '^ok 6 - map layout$' "$out" || missing="$missing ok6-map-layout"
        grep -q '^ok 7 - walk khop$' "$out" || missing="$missing ok7-walk-khop"
        grep -q '^ok 8 - plic window$' "$out" || missing="$missing ok8-plic-window"
        grep -q '^ok 9 - uart irq$' "$out" || missing="$missing ok9-uart-irq"
        grep -q '^ok 10 - plic no fake irq$' "$out" || missing="$missing ok10-plic-no-fake-irq"
        grep -q '^ok 11 - timer 100 ticks monotonic$' "$out" || missing="$missing ok11-timer-monotonic"
        grep -q '^ok 12 - timer no lost tick$' "$out" || missing="$missing ok12-timer-no-lost-tick"
        grep -q '^ok 13 - timer irq$' "$out" || missing="$missing ok13-timer-irq"
        grep -q '^ok 14 - timer deadline drift$' "$out" || missing="$missing ok14-timer-drift"
        grep -q "^ok 15 - harts alive $SMP$" "$out" || missing="$missing ok15-harts-alive-$SMP"
        grep -q '^ok 16 - per-hart stack/guard$' "$out" || missing="$missing ok16-stack-guard"
        grep -q '^ok 17 - per-hart timer$' "$out" || missing="$missing ok17-per-hart-timer"
        grep -q '^ok 18 - plic ctx per hart$' "$out" || missing="$missing ok18-plic-ctx"
        grep -q '^ok 19 - barrier$' "$out" || missing="$missing ok19-barrier"
        grep -q "^ok 20 - $IPI_TAP20$" "$out" || missing="$missing ok20-ipi"
        grep -q '^ok 21 - ipi drain khi cho ack$' "$out" || missing="$missing ok21-ipi-drain"
        grep -q '^ok 22 - self-target khong gui$' "$out" || missing="$missing ok22-self-target"
        grep -q '^ok 23 - coalesce khong mat kind$' "$out" || missing="$missing ok23-coalesce"
        grep -q '^ok 24 - ipi khong nham timer/external$' "$out" || missing="$missing ok24-ipi-distinct"
        grep -q '^ok 25 - 100 vong shootdown Ok$' "$out" || missing="$missing ok25-shootdown"
        grep -q '^ok 26 - stale mapping caught$' "$out" || missing="$missing ok26-stale"
        grep -q '^ok 27 - hart ngoai active_on$' "$out" || missing="$missing ok27-outside"
        grep -q '^ok 28 - rollover flush$' "$out" || missing="$missing ok28-rollover"
        grep -q '^ok 29 - timeout khong treo$' "$out" || missing="$missing ok29-timeout"
        # --- KT-52: object layer `no_std` (Retype nội bộ) — VSpace/PageTable/Frame/Untyped.
        grep -q '^ok 30 - retype vspace$' "$out" || missing="$missing ok30-object-vspace"
        grep -q '^ok 31 - untyped byte invariant$' "$out" || missing="$missing ok31-object-bytes"
        grep -q '^ok 32 - frame 3 sizes$' "$out" || missing="$missing ok32-object-frames"
        grep -q '^ok 33 - wx violation x-then-w$' "$out" || missing="$missing ok33-wx-xw"
        grep -q '^ok 34 - wx violation device+exec$' "$out" || missing="$missing ok34-wx-device"
        grep -q '^ok 35 - map/unmap 4 KiB$' "$out" || missing="$missing ok35-map-unmap"
        grep -q '^ok 36 - 0 ro$' "$out" || missing="$missing ok36-0-ro"
        grep -Eq '^# obj init: arena=0x[0-9a-f]+-0x[0-9a-f]+ bytes=[0-9]+ untyped=2 window=0x40000000 pa0=0x[0-9a-f]+ pa1=0x80000000 physmap=1$' "$out" ||
            missing="$missing obj-init-evidence"
        grep -Eq '^# retype: vspace=1 root=0x[0-9a-f]+ asid=[1-9][0-9]* pt=3 frame=1 cap_ok=1 walk_empty=1$' "$out" ||
            missing="$missing obj-retype-evidence"
        grep -Eq '^# untyped bytes: cap=[0-9]+ used0=[0-9]+ after_retype=[0-9]+ after_revoke=[0-9]+ free0=[0-9]+ free1=[0-9]+ invariant=1 atomic=1 oom=1$' "$out" ||
            missing="$missing obj-bytes-evidence"
        grep -Eq '^# frame: s12=1 s21=1 s30=1 pa4=0x[0-9a-f]+ pa2=0x[0-9a-f]+ pa1g=0x80000000 aligned=1 real4=1 real2=1 window=1 bad13=InvalidArgument atomic=1$' "$out" ||
            missing="$missing obj-frame-evidence"
        grep -Eq '^# wx: x_then_w=WxViolation device_exec=WxViolation atomic=1 maps=1 w=0 x=1$' "$out" ||
            missing="$missing obj-wx-evidence"
        grep -Eq '^# map: va=0x40000000 level=0 pa=0x[0-9a-f]+ satp=1 asid=[1-9][0-9]* sum=1 write=1 read=1 protect=1 unmapped=1 flush=[1-9][0-9]* walk_after=0 kernel=1$' "$out" ||
            missing="$missing obj-map-evidence"
        grep -Eq '^# leak: cap=[0-9]+ used=0 free=[0-9]+ holes=0 frames=0 pts=0 vspaces=0 maps=0 orphans=0 check=1$' "$out" ||
            missing="$missing obj-leak-evidence"
        grep -q '^ok 37 - trap nested panic$' "$out" || missing="$missing ok37-nested"
        # --- bằng chứng KT-45..KT-49 (số đo do chính boot in ra, không viết tay)
        grep -q '^# mmu: .*mode=8' "$out" || missing="$missing mmu-evidence"
        grep -q '^# mmu on: .*alias=1 .*kernel_low=1 kernel_high=1' "$out" ||
            missing="$missing mmu-on-evidence"
        grep -q '^# mmu layout: .*none_low=1 none_high=1 none_physmap=1' "$out" ||
            missing="$missing guard-none-evidence"
        grep -q '^# mmu layout: .*physmap_2m_nx=1 uart_dev=1 plic_dev=1 kernel_high_2m=1 all_harts=1$' "$out" ||
            missing="$missing all-hart-layout-evidence"
        grep -q '^# walk khop: pre=post=1 software=8/8' "$out" ||
            missing="$missing walk-khop-evidence"
        grep -q '^# plic: .*threshold=0 priority_uart=1 enable_ctx=1 enable_mmode=0 claim_idle=0$' "$out" ||
            missing="$missing plic-window-evidence"
        grep -q '^# plic map: .* low_dev=1 high_dev=1 low_2m=1 high_2m=1 low_nx=1 high_nx=1$' "$out" ||
            missing="$missing plic-map-evidence"
        grep -q '^# uart irq: irqs=[1-9][0-9]* claims=[1-9][0-9]* completes=[1-9][0-9]* traps_delta=[1-9][0-9]* balanced=1 spurious=0 unexpected=0 last_irq=10 last_iir=0x2 ier_off=1 plic_enable=0$' "$out" ||
            missing="$missing uart-irq-evidence"
        grep -q '^# plic quiet: claim_idle=0 quiet=1 stats_unchanged=1 external_traps=[1-9][0-9]* interrupts_delta=[1-9][0-9]* spurious=0 unexpected=0$' "$out" ||
            missing="$missing plic-quiet-evidence"
        grep -q '^# timer arm: .*tick_ticks=100000 tick_hz=100 timebase_hz=10000000 sie_stie=1 sstatus_sie=1$' "$out" ||
            missing="$missing timer-arm-evidence"
        grep -q '^# timer ticks: .*non_monotonic=0 late=0 missed=0' "$out" ||
            missing="$missing timer-monotonic-evidence"
        grep -q '^# timer drift: .*in_range=1' "$out" || missing="$missing timer-drift-evidence"
        grep -q '^# timer irq: .*scause=0x8000000000000005 code=5' "$out" ||
            missing="$missing timer-irq-evidence"
        grep -q '^# timer sie: .*sret_sie=1$' "$out" || missing="$missing timer-sie-evidence"
        grep -q '^# timer disable: sie_stie=0 sstatus_sie=0$' "$out" ||
            missing="$missing timer-disable-evidence"
        # --- KT-49: SBI HSM (hart theo DTB qua hart_get_status), N harts alive, per-hart state.
        grep -Eq "^# smp: boot=[0-9]+ harts=$SMP .* timeout=0 .* dtb_magic=1 .* stride=0x15000 area=0xa8000$" "$out" ||
            missing="$missing smp-bringup-evidence"
        grep -q "^# harts alive: n=$SMP mask=0x$MASK alive=0x$MASK tickets=1 arrived=0x$MASK timeout=0$" "$out" ||
            missing="$missing harts-alive-evidence"
        grep -q "^# per-hart stack/guard: stack_ok=0x$MASK guard_ok=0x$MASK distinct=1 owner_ok=true$" "$out" ||
            missing="$missing per-hart-stack-guard-evidence"
        grep -q "^# per-hart timer: ok=0x$MASK total_ticks=[1-9][0-9]* arrived=0x$MASK timeout=0$" "$out" ||
            missing="$missing per-hart-timer-evidence"
        grep -q "^# per-hart plic: ok=0x$MASK distinct=1 uart_irqs_boot=8 stats_clean=1 arrived=0x$MASK timeout=0$" "$out" ||
            missing="$missing per-hart-plic-evidence"
        grep -Eq '^# barrier: round=5 arrived=0x[0-9a-f]+ missing=0x0 timeout=0 wait_ticks=[0-9]+ slow_hart=1 slow_delayed=[01]$' "$out" ||
            missing="$missing barrier-evidence"
        grep -Eq '^# barrier timeout: round=6 expected_missing=0x[0-9a-f]+ arrived=0x[0-9a-f]+ missing=0x[0-9a-f]+ timeout=1 wait_ticks=[0-9]+ \(may khong treo\)$' "$out" ||
            missing="$missing barrier-timeout-evidence"
        # --- KT-50: IPI thật + hộp thư per-hart. `-smp 1` không có hart đích ⇒ mode self-ipi
        # (đo 100 IPI tự gửi qua SBI, ghi rõ); matrix >= 2 đo 100 IPI **chéo** tới peer.
        if [ "$SMP" -ge 2 ]; then
            IPI_MODE=cross
            IPI_TAP20="100 ipi delivered"
            IPI_DELIVERED="sent=100 delivered=100 traps=100 coalesced=0 spurious=0 kinds=34/33/33 timeout=0"
            IPI_DRAIN="mode=cross mutual=1 boot_ok=1 peer_ok=1 traps_delta=0/0 wait_ticks=[0-9]+/[0-9]+ timeout_ended=1"
            IPI_COALESCE="mode=cross posts=6 coalesced=4 sent_delta=2 pending=0x3 peer_bits=0x3 peer_delivered=2 ok=1"
        else
            IPI_MODE=self-ipi
            IPI_TAP20="100 ipi traps (self)"
            IPI_DELIVERED="sent=100 delivered=0 traps=100 coalesced=0 spurious=0 kinds=0/0/0 timeout=0"
            IPI_DRAIN="mode=self-wait mutual=0 boot_ok=0 peer_ok=0 traps_delta=0/0 wait_ticks=[0-9]+/[0-9]+ timeout_ended=1"
            IPI_COALESCE="mode=mailbox posts=6 coalesced=4 sent_delta=0 pending=0x3 peer_bits=0x3 peer_delivered=2 ok=1"
        fi
        grep -Eq "^# ipi: harts=$SMP boot=[0-9]+ peer=-?[0-9]+ mode=$IPI_MODE target=100 sie_ok=0x$MASK setup_ok=1$" "$out" ||
            missing="$missing ipi-setup-evidence"
        grep -Fq "# ipi delivered: mode=$IPI_MODE target=100 $IPI_DELIVERED" "$out" ||
            missing="$missing ipi-delivered-evidence"
        grep -Eq "^# ipi drain: $IPI_DRAIN timeout_wait=[0-9]+ timeout_drains=[1-9][0-9]*$" "$out" ||
            missing="$missing ipi-drain-evidence"
        grep -Eq "^# ipi self: ok=0x$MASK local=[1-9][0-9]* sent_delta=0 traps_delta=0 kinds_tlb=[1-9][0-9]*$" "$out" ||
            missing="$missing ipi-self-evidence"
        grep -Fq "# ipi coalesce: $IPI_COALESCE" "$out" ||
            missing="$missing ipi-coalesce-evidence"
        grep -Eq '^# ipi distinct: ipi_traps=100 interrupts_delta=100 timer_delta=0 plic_delta=0 last_scause=0x8000000000000001 last_code=1 decode_ok=1 ok=1$' "$out" ||
            missing="$missing ipi-distinct-evidence"
        # --- KT-51: ASID thật + sfence.vma + shootdown ack (D2-65 Q4/Q5, D2-68).
        if [ "$SMP" -ge 2 ]; then
            TLB_MODE=cross
            # D2-78: `active_on` = **toàn bộ hart online** ở pha 100 vòng ⇒ mỗi vòng gửi
            # `SMP-1` IPI: smp2 = 100, smp4 = 300.
            TLB_SENT=$((100 * (SMP - 1)))
            TLB_PEER="received=100 flushes=100 page=100 asid=0 all=0 implicit=0 prime_wrong=0 verify_wrong=0 service_timeout=0"
            TLB_REMOVED=2
            if [ "$SMP" -eq 2 ]; then TLB_REMOVED=1; fi
        else
            TLB_MODE=self
            TLB_SENT=0
            TLB_PEER="received=0 flushes=0 page=0 asid=0 all=0 implicit=0 prime_wrong=0 verify_wrong=0 service_timeout=0"
            TLB_REMOVED=1
        fi
        grep -Eq '^# tlb init: asid=0 generation=0 rollover=0 satp=0x[0-9a-f]+ satp_asid=0 active_on=0x(1|2|4|8) free=255 issued=1$' "$out" ||
            missing="$missing tlb-init-evidence"
        grep -Eq "^# tlb: harts=$SMP boot=[0-9]+ peer=-?[0-9]+ mode=$TLB_MODE rounds=100 asid_bits=8 asid_count=256$" "$out" ||
            missing="$missing tlb-phase-evidence"
        grep -q '^# tlb probe: va=0x80800000 pa_a=0x80800000 pa_b=0x80a00000 probe2_va=0x80a00000 pa_c=0x80c00000 pa_d=0x80e00000 leaf_a=1 leaf_b=1 nx=1$' "$out" ||
            missing="$missing tlb-probe-evidence"
        grep -Eq "^# shootdown rounds: mode=$TLB_MODE rounds=100 ok=100 acked_ok=100 sent=$TLB_SENT sent_expected=$TLB_SENT self_slot_wrong=0 local_verify_wrong=0 barrier_bad=0 targets=0x[0-9a-f]+ active_on=0x[0-9a-f]+ wait_ticks=[0-9]+$" "$out" ||
            missing="$missing shootdown-rounds-evidence"
        grep -Eq "^# shootdown all: bad=0 served=$((SMP - 1)) expected_served=$((SMP - 1)) online=0x$MASK$" "$out" ||
            missing="$missing shootdown-all-evidence"
        grep -Fq "# shootdown peer: mode=$TLB_MODE $TLB_PEER" "$out" ||
            missing="$missing shootdown-peer-evidence"
        grep -q '^# stale: asid=0 va=0x80800000 forms=3 stale_seen=3 fresh_ok=3 wrong=0 note=stale-read-without-sfence-is-positive$' "$out" ||
            missing="$missing stale-evidence"
        for FORM in Page Asid All; do
            grep -q "^# stale form: name=$FORM fresh_a=1 stale_seen=1 fresh_b=1 wrong=0$" "$out" ||
                missing="$missing stale-form-$FORM"
        done
        # D2-78: hart bị **gỡ khỏi ACTIVE_ON thật** rồi khôi phục — không truyền mask hẹp.
        grep -Eq "^# outside: mode=$TLB_MODE removed=0x[0-9a-f]+ removed_count=$TLB_REMOVED narrowed=0x[0-9a-f]+ targets=0x[0-9a-f]+ acked=0x[0-9a-f]+ caller_in=[01] removed_acked_before=0x[0-9a-f]+ removed_acked_after=0x[0-9a-f]+ restored=1 restored_acked=0x[0-9a-f]+ outside_wrong=0 ok=1$" "$out" ||
            missing="$missing outside-evidence"
        grep -Eq "^# rollover: generation=1 exhaustions=1 rollover=1 flush=Ok asid=0 satp_asid=0 stale_seen=1 fresh_ok=1 free=255 issued=1 asid_ok=0x$MASK online=0x$MASK primed=0x4b51c000000000c2 wrong=0$" "$out" ||
            missing="$missing rollover-evidence"
        grep -Eq '^# timeout: outcome=Timeout narrowed=0x[0-9a-f]+ targets=0x[0-9a-f]+ acked=0x[0-9a-f]+ missing=0x[0-9a-f]+ waited=[0-9]+ probe=1000000 timeouts=[1-9][0-9]* mailbox_empty=1 recovered=1 wrong=0$' "$out" ||
            missing="$missing timeout-evidence"
        grep -Eq "^# tlb asid: ok=0x$MASK online=0x$MASK phase=0x[0-9a-f]+ arrived=0x$MASK timeout=0 satp_asid_boot=0 lease_asid=0 generation=1$" "$out" ||
            missing="$missing tlb-asid-evidence"
        # Mỗi hart in đúng MỘT dòng alive (id + ticket + tp) và một dòng stack per-hart.
        hart=0
        while [ "$hart" -lt "$SMP" ]; do
            grep -Eq "^# hart $hart alive ticket=[0-9]+ tp=$hart " "$out" ||
                missing="$missing hart$hart-alive-line"
            grep -Eq "^# stack hart=$hart .* owner=$hart .* frame_trap=1 rw_nx=1 guard_none=1$" "$out" ||
                missing="$missing hart$hart-stack-line"
            hart=$((hart + 1))
        done
        [ "$(grep -cE '^# timer hart=[0-9]+ .* ok=1$' "$out" || true)" -eq "$SMP" ] ||
            missing="$missing timer-hart-lines"
        [ "$(grep -cE '^# plic hart=[0-9]+ .* ok=1$' "$out" || true)" -eq "$SMP" ] ||
            missing="$missing plic-hart-lines"
        # Bằng chứng độc lập từ OpenSBI: máy ảo thật sự có N hart (không chỉ kernel tự khai).
        grep -q "^Platform HART Count *: *$SMP" "$out" || missing="$missing opensbi-hart-count"
        if [ -z "$missing" ]; then
            echo "ok - ktest smp=$SMP boot $i (37 TAP: trap+mmu+plic+timer+smp+ipi+tlb+object)"
            clean_boots=$((clean_boots + 1))
        else
            echo "not ok - ktest smp=$SMP boot $i: thiếu$missing, log $out"
            fails=$((fails + 1))
            smp_fails=$((smp_fails + 1))
        fi

        # ------------------------------------------------------- ảnh guard: 3 TAP, store fatal
        total_boots=$((total_boots + 1))
        if out="$(boot_once guard "$IMG_GUARD" "$SMP" "$i")"; then :; else
            echo "not ok - guard ktest smp=$SMP boot $i: quá $((TIMEOUT_DS / 10)) s (treo), log $out"
            fails=$((fails + 1))
            smp_fails=$((smp_fails + 1))
            i=$((i + 1))
            continue
        fi
        missing="$(common_tap_ok "$out" "$PLAN_GUARD" "$OK_GUARD" '^PANIC: guard page fault scause=0xf ')"
        grep -q '^ok 1 - guard image$' "$out" || missing="$missing ok1-guard-image"
        grep -q '^ok 2 - guard load fault$' "$out" || missing="$missing ok2-guard-load"
        grep -q '^ok 3 - guard store fault$' "$out" || missing="$missing ok3-guard-store"
        # KT-49: ảnh guard dùng guard **của hart boot thật** — runner đọc hart từ `# guard image:`
        # rồi đòi fault/load/store đúng hart đó (không ghim hart 0; OpenSBI có thể boot hart khác 0).
        BOOT_HART="$(sed -n 's/^# guard image: hart=\([0-9][0-9]*\) .*/\1/p' "$out" | head -1)"
        [ -n "$BOOT_HART" ] || missing="$missing guard-image-hart"
        if [ -n "$BOOT_HART" ]; then
            grep -q "^# guard load: hart=$BOOT_HART cham 0x" "$out" ||
                missing="$missing guard-load-line"
            grep -q "^# guard store: hart=$BOOT_HART ghi 0x" "$out" ||
                missing="$missing guard-store-line"
            grep -q "^# guard fault load scause=0xd stval=0x[0-9a-f]* sepc=0x[0-9a-f]* len=[24] recovered=1 hart=$BOOT_HART$" "$out" ||
                missing="$missing guard-load-real"
            # hart boot phải nằm trong số hart của máy ảo.
            if ! awk -v h="$BOOT_HART" '/^Platform HART Count *:/ {gsub(/\r/,""); if ($NF > h) ok=1} END {exit !ok}' "$out"; then
                missing="$missing guard-hart-out-of-range"
            fi
        fi
        grep -q '^# guard load stats: .*last_scause=0xd' "$out" ||
            missing="$missing guard-load-stats"
        grep -q '^PANIC: guard page fault scause=0xf stval=0x[0-9a-f]* sepc=0x[0-9a-f]*' "$out" ||
            missing="$missing guard-store-panic"
        grep -q "^Platform HART Count *: *$SMP" "$out" || missing="$missing opensbi-hart-count"
        if [ -z "$missing" ]; then
            echo "ok - guard smp=$SMP boot $i (3 TAP: load 13 + store 15)"
            clean_boots=$((clean_boots + 1))
        else
            echo "not ok - guard smp=$SMP boot $i: thiếu$missing, log $out"
            fails=$((fails + 1))
            smp_fails=$((smp_fails + 1))
        fi
        i=$((i + 1))
    done
    echo "# smp=$SMP: $((N_BOOT * 2 - smp_fails))/$((N_BOOT * 2)) boot ảnh test sạch"
    [ "$smp_fails" -eq 0 ] || echo "# smp=$SMP: FAIL ($smp_fails lỗi)"
done

echo "# KTEST: $clean_boots/$total_boots boot sạch (smp=$SMP_ARG, NBOOT=$N_BOOT)"
[ "$fails" -eq 0 ] || exit 1
echo "# ktest.sh: PASS"
