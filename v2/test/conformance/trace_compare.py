#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""KT-55 — so trace conformance **sim (host) ↔ QEMU (ktest)**.

Nguồn: dòng `# trace: <scn> <step> <op> [k=v …] => <kết quả>` do **cùng** một định dạng
(`vifast-traceformat::TraceLine`) sinh ra ở hai backend; IR/số bước do `vifast-scenario` chiếu từ
`v2/test/scn/{22..26}-*.scn`. Script này **chỉ** so các trường được phép.

Trường **volatile** bị bỏ trước khi so (yêu cầu KT-55: bỏ địa chỉ tuyệt đối/thời gian/hart):
`hart, harts, pa, phys, addr, t_ns, ticks, wait_ticks, rounds, seq, asid, messages`.
Mọi trường khác phải khớp **từng dòng, từng thứ tự**; lệch ⇒ exit 1 kèm chẩn đoán.

Dùng:
    python3 trace_compare.py --sim <sim.log> --qemu <qemu.log-or-ktest.log> [--max-report 20]
    python3 trace_compare.py --sim <sim.log> --qemu <...> --require 22,23,24,25,26
"""

import argparse
import re
import sys
from collections import defaultdict

from image_digest import fnv1a, text_section

TRACE_RE = re.compile(
    r"^# trace: (?P<scn>[0-9]+) (?P<step>[0-9]+) (?P<op>[a-z_]+)"
    r"(?P<fields>(?: [a-z_]+=[^ ]+)*) => (?P<result>[^ ]+)$"
)

# Trường bị **bỏ** khi so — model/hardware để lộ khác nhau có lý do, không mang ngữ nghĩa.
VOLATILE = {
    "hart",
    "harts",
    "pa",
    "phys",
    "addr",
    "t_ns",
    "ticks",
    "wait_ticks",
    "rounds",
    "seq",
    "asid",
    "messages",
}


def parse_trace(path, label):
    """Trả list record + số dòng trace thô; lỗi định dạng là FAIL (không bỏ qua âm thầm)."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            lines = fh.read().splitlines()
    except FileNotFoundError:
        print(f"not ok - {label}: không thấy tệp {path}")
        return None, 0, 0
    recs, bad = [], 0
    for line in lines:
        m = TRACE_RE.match(line)
        if not m:
            if line.startswith("# trace:"):
                bad += 1
                if bad <= 5:
                    print(f"not ok - {label}: dòng trace sai định dạng: {line!r}")
            continue
        fields = {}
        for tok in m.group("fields").split():
            k, _, v = tok.partition("=")
            if k in VOLATILE:
                continue
            if k in fields:
                bad += 1
                print(f"not ok - {label}: trường {k!r} lặp trong {line!r}")
                continue
            fields[k] = v
        recs.append(
            {
                "scn": int(m.group("scn")),
                "step": int(m.group("step")),
                "op": m.group("op"),
                "fields": tuple(sorted(fields.items())),
                "result": m.group("result"),
                "raw": line,
            }
        )
    return recs, bad, len(recs)


def fmt(rec):
    fields = " ".join(f"{k}={v}" for k, v in rec["fields"])
    return f"#{rec['scn']} b{rec['step']} {rec['op']} {fields} => {rec['result']}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sim", required=True)
    ap.add_argument("--qemu", required=True)
    ap.add_argument("--max-report", type=int, default=20)
    ap.add_argument("--require", default="22,23,24,25,26")
    ap.add_argument(
        "--image",
        required=True,
        help="D2-80: ảnh kernel của log QEMU — script tự băm `.text` và đòi log khai đúng BUILD_ID",
    )
    args = ap.parse_args()

    # D2-80: log QEMU phải tự khai BUILD_ID **khớp ảnh đang dùng** — chặn việc ghép trace của sim
    # (hoặc của ảnh khác) vào log QEMU.
    try:
        with open(args.qemu, "r", encoding="utf-8", errors="replace") as fh:
            qemu_raw = fh.read()
    except FileNotFoundError:
        print(f"not ok - qemu: không thấy tệp {args.qemu}")
        return 1
    img_line = re.search(r"^# conf image: build=([0-9a-f]{16}) text=([0-9]+)$", qemu_raw, re.M)
    if not img_line:
        print("not ok - qemu: thiếu '# conf image: build=<16 hex> text=<len>' (D2-80)")
        return 1
    try:
        img_text = text_section(args.image)
    except (SystemExit, OSError) as exc:
        print(f"not ok - image: không băm được {args.image}: {exc}")
        return 1
    want_build = f"{fnv1a(img_text):016x}"
    if img_line.group(1) != want_build or int(img_line.group(2)) != len(img_text):
        print(
            f"not ok - qemu: BUILD_ID {img_line.group(1)}/{img_line.group(2)}"
            f" ≠ ảnh {want_build}/{len(img_text)} (D2-80)"
        )
        return 1

    sim, sim_bad, sim_n = parse_trace(args.sim, "sim")
    qemu, qemu_bad, qemu_n = parse_trace(args.qemu, "qemu")
    if sim is None or qemu is None:
        return 1
    fails = 0
    if sim_bad or qemu_bad:
        print(f"not ok - dòng trace hỏng: sim={sim_bad} qemu={qemu_bad}")
        fails += 1
    if sim_n == 0 or qemu_n == 0:
        print(f"not ok - trace rỗng: sim={sim_n} dòng, qemu={qemu_n} dòng")
        print("# GỢI Ý: qemu rỗng ⇒ kernel chưa in `# trace:` (đỏ trước của KT-55);"
              " sim rỗng ⇒ chưa chạy `cargo test --test kt42_scn -- --nocapture --test-threads=1`")
        return 1

    if sim_n != qemu_n:
        print(f"not ok - số dòng trace lệch: sim={sim_n} qemu={qemu_n}")
        fails += 1

    n = min(sim_n, qemu_n)
    diffs = 0
    for i in range(n):
        a, b = sim[i], qemu[i]
        if (a["scn"], a["step"], a["op"], a["fields"], a["result"]) == (
            b["scn"],
            b["step"],
            b["op"],
            b["fields"],
            b["result"],
        ):
            continue
        diffs += 1
        if diffs <= args.max_report:
            print(f"not ok - dòng {i + 1}:")
            print(f"    sim : {fmt(a)}")
            print(f"    qemu: {fmt(b)}")
    if diffs:
        print(f"not ok - {diffs}/{n} dòng trace lệch ngoài trường cho phép")
        fails += 1

    # Đủ 5 ca (trừ khi người gọi nới bằng `--require`) + mỗi ca có ≥1 dòng ở **cả hai** phía.
    require = [int(x) for x in args.require.split(",") if x.strip()]
    by_scn_sim = defaultdict(int)
    by_scn_qemu = defaultdict(int)
    for r in sim:
        by_scn_sim[r["scn"]] += 1
    for r in qemu:
        by_scn_qemu[r["scn"]] += 1
    for scn in require:
        if by_scn_sim[scn] == 0:
            print(f"not ok - sim thiếu ca {scn}")
            fails += 1
        if by_scn_qemu[scn] == 0:
            print(f"not ok - qemu thiếu ca {scn} (smp<2 thì ca 24 được phép vắng —"
                  f" dùng log smp≥2)")
            fails += 1

    if fails:
        print(f"# trace-compare: FAIL (sim={sim_n} qemu={qemu_n})")
        return 1
    per = " ".join(f"{scn}:{by_scn_sim[scn]}" for scn in sorted(by_scn_sim))
    matched = sum(1 for scn in require if by_scn_sim[scn] and by_scn_qemu[scn])
    print(
        f"ok - trace-compare: {sim_n}/{qemu_n} dòng khớp, {matched}/{len(require)} ca ({per})"
        f", build={want_build}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
