#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""KT-04c — kiểm tĩnh kịch bản `.scn` (chiếu văn bản của thiết kế §9.3).

Đỏ trước: viết khi `v2/test/scn/*.scn` chưa có tệp nào.

Dùng: `python3 check_scn.py [--dir v2/test/scn] [--spec spec/abi.toml] [--trace <sim trace>]`

**KT-55** — `--trace <tệp>` kiểm **chiều sim**: tệp `# trace:` do runner sim in ra phải (a) đúng
định dạng chuẩn hoá, (b) đúng **tập bước conformance** chiếu từ chính 5 tệp `.scn` (cùng luật với
crate `vifast-scenario`: bỏ bước `frame=f1`/`va=0x80000000`, bỏ op mô hình TLB per-hart và op cần
TCB/procsrv), (c) mang **giá trị quan sát** khớp kỳ vọng ghi trong tệp (`size`, `maps/w/x`, `n`,
`reason`, kết quả map/walk/unmap/protect), và (d) `# conf:` tự khai `steps=N` bằng đúng số dòng
trace của ca. Đây là đầu sim của hợp đồng KT-55; đầu QEMU do `v2/test/conformance/trace_compare.py`
so từng dòng.
"""

import pathlib
import re
import sys

SPDX = "SPDX-License-Identifier"

# 16 syscall của spec (0..15), tên snake_case -> tên trong abi.toml
SYSCALL_NAMES = {
    "call": "Call",
    "send": "Send",
    "nb_send": "NBSend",
    "recv": "Recv",
    "nb_recv": "NBRecv",
    "reply_recv": "ReplyRecv",
    "nb_send_recv": "NBSendRecv",
    "nb_send_wait": "NBSendWait",
    "yield": "Yield",
    "wait_addr": "WaitAddr",
    "wake_addr": "WakeAddr",
    "clock_read": "ClockRead",
    "debug_put_str": "DebugPutStr",
    "debug_halt": "DebugHalt",
    "debug_get_info": "DebugGetInfo",
    "exit": "Exit",
}

# 11 nhóm phủ (9 nhóm QD §4 + notification + exit tách riêng)
TAG_GROUPS = {
    "cap-copy",
    "cap-revoke",
    "call-reply",
    "reply-deleted",
    "suspend-blocked",
    "kill-blocked",
    "exit",
    "four-cap",
    "no-reply",
    "donation",
    "notification",
    # KT-42 — 5 nhóm VM model-only (k2-vm.md §16; spec/ không đổi)
    "vm-map",
    "vm-wx",
    "vm-tlb",
    "vm-fault",
    "vm-shared",
}

# KT-42 — op VM **model-only** của K2 (Frame/PageTable/VSpace/TLB/fault): KHÔNG có trong
# `spec/abi.toml` (D2-65 Q6 — `FaultKind::Vm`/VM model API là model-only, K3 mới vào ABI), nên
# kiểm theo bảng đóng dưới đây thay vì `method_signature`. Mọi op khác vẫn đi đúng đường spec.
VM_OPS = {
    "vm_activate": {"vs", "hart"},
    "vm_map": {"vs", "va", "frame", "rights", "attrs"},
    "vm_unmap": {"hart", "vs", "va"},
    "vm_protect": {"hart", "vs", "va", "rights", "attrs"},
    "vm_walk": {"vs", "va"},
    "vm_expect_map": {"vs", "va", "frame", "size"},
    "vm_expect_counters": {"frame", "maps", "w", "x"},
    "vm_expect_frames": {"n"},
    "vm_tlb_fill": {"hart", "vs", "va"},
    "vm_tlb_expect": {"hart", "vs", "va", "present"},
    "vm_shootdown_begin": {"hart", "vs", "va"},
    "vm_shootdown_wait": {"hart"},
    "vm_expect_pending": {"hart", "targets", "acked", "messages", "completed"},
    "vm_expect_shootdown": {"targets", "acked", "messages", "completed"},
    "vm_dispatch": {"hart", "tcb"},
    "vm_touch": {"hart", "vs", "va", "access"},
    "vm_expect_fault": {"tcb", "vs", "va", "access", "reason"},
    "vm_expect_state": {"tcb", "state"},
    "vm_procsrv_recv": set(),
    "vm_procsrv_resume": {"tcb"},
    "vm_check": set(),
}

CAP_KEYS = {"cap", "src", "dst", "cnode", "ep", "reply", "notif", "tcb", "u", "sched", "sched_control"}

fails = []


def split_fields(body):
    """Tách `a, b=c, d=E{1}` theo dấu phẩy ở mức ngoài cùng."""
    out, depth, cur = [], 0, ""
    for ch in body:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def check_state_value(fname, key, val, ty):
    """D2-41: `expect_states` phải khớp cả **trường** (`call=false`, `fault=Exit{code}`),
    không chỉ tên trường. Chỉ kiểm khi giá trị được ghi ra (trường trần như `ep` được phép)."""
    if ty == "bool":
        if val not in ("true", "false"):
            fails.append(f"{fname}: giá trị {val!r} của trường {key!r} không hợp lệ (bool)")
    elif ty in ("Option<FaultKind>", "FaultKind"):
        if not re.fullmatch(r"Exit(\{\d+\})?", val):
            fails.append(
                f"{fname}: giá trị {val!r} của trường {key!r} không hợp lệ (FaultKind: Exit{{code}})"
            )
    elif ty in ("Option<u64>", "u64"):
        if not re.fullmatch(r"\d+", val):
            fails.append(f"{fname}: giá trị {val!r} của trường {key!r} không hợp lệ (u64)")


def check_state(fname, st, sig, state_names):
    """Kiểm `Name` hoặc `Name{field=value,...}` theo enum + field của draft D2-33."""
    if "{" in st:
        name, body = st.split("{", 1)
        body = body.rsplit("}", 1)[0]
    else:
        name, body = st, ""
    name = name.strip()
    if name not in state_names:
        fails.append(f"{fname}: trạng thái {name!r} không có trong enum D2-33")
        return
    declared = {}
    for f in (sig.get("state", {}).get(name, {}) or {}).get("fields", []) or []:
        key, _, ty = f.partition(":")
        declared[key.strip()] = ty.strip()
    for field in split_fields(body) if body else []:
        key, sep, val = field.partition("=")
        key, val = key.strip(), val.strip()
        if declared and key not in declared:
            fails.append(f"{fname}: {name} không có trường {key!r} (D2-33: {sorted(declared)})")
            continue
        if sep and val:
            check_state_value(fname, key, val, declared.get(key, ""))


def check(cond, msg):
    if not cond:
        fails.append(msg)


def load_toml(path):
    import tomllib

    raw = pathlib.Path(path).read_bytes()
    return raw, tomllib.loads(raw.decode("utf-8"))


def parse_step(line):
    """Trả (op_kind, op_name, params, expect) hoặc raise ValueError."""
    if "=>" not in line:
        raise ValueError("thiếu `=> <kết quả>`")
    body, expect = line.rsplit("=>", 1)
    expect = expect.strip()
    toks = body.split()
    if not toks:
        raise ValueError("bước rỗng")
    if toks[0] == "invoke":
        if len(toks) < 2 or "." not in toks[1]:
            raise ValueError("invoke phải là `invoke Object.Method`")
        kind, name = "invoke", toks[1]
        rest = toks[2:]
    else:
        kind, name, rest = "syscall", toks[0], toks[1:]
    params = {}
    for tok in rest:
        if "=" not in tok:
            raise ValueError(f"tham số không phải k=v: {tok!r}")
        k, v = tok.split("=", 1)
        params[k] = v
    return kind, name, params, expect


# ---------------------------------------------------------------------------------------------
# KT-55 — kiểm chiều **sim** của trace conformance
# ---------------------------------------------------------------------------------------------

# 5 ca nền tảng (D2-69) — trùng `vifast_scenario::CASES`.
CONF_CASES = [
    (22, "vm-map", "22-frame-map-unmap-walk.scn"),
    (23, "vm-wx", "23-wx-violation-counters.scn"),
    (24, "vm-tlb", "24-shootdown-2-hart.scn"),
    (25, "vm-fault", "25-vm-fault-notmapped-resume.scn"),
    (26, "vm-shared", "26-shared-readonly-frame.scn"),
]

# Luật chiếu — phải **trùng** `vifast-scenario::{skipped,project}` (hai bên đọc độc lập; lệch là
# trace sim không khớp tập bước kỳ vọng ⇒ đỏ ngay tại đây).
CONF_SKIP_FRAME = {"f1"}
CONF_SKIP_VA = {"0x80000000"}
CONF_SKIP_OPS = {
    "vm_tlb_fill",
    "vm_tlb_expect",
    "vm_expect_pending",
    "vm_expect_shootdown",
    "vm_expect_state",
    "vm_dispatch",
    "vm_procsrv_recv",
    "vm_procsrv_resume",
}
CONF_OP = {
    "vm_map": "map",
    "vm_unmap": "unmap",
    "vm_protect": "protect",
    "vm_walk": "walk",
    "vm_expect_map": "expect_map",
    "vm_expect_counters": "counters",
    "vm_expect_frames": "frames",
    "vm_check": "check",
    "vm_activate": "activate",
    "vm_shootdown_wait": "shootdown",
    "vm_expect_fault": "fault",
}
# Trường **được phép** của từng op trace (không có hart/PA/thời gian — `trace_compare.py` cũng
# bỏ các trường volatile khi so QEMU, nhưng ở đây kiểm chặt: đúng tập này).
CONF_FIELDS = {
    "map": {"va", "frame", "rights", "attrs"},
    "unmap": {"va"},
    "protect": {"va", "rights", "attrs"},
    "walk": {"va"},
    "expect_map": {"va", "frame", "size"},
    "counters": {"frame", "maps", "w", "x"},
    "frames": {"n"},
    "check": set(),
    "activate": {"vs"},
    "shootdown": {"va", "targets", "acked", "completed"},
    "fault": {"va", "access", "reason"},
}
TRACE_RE = re.compile(
    r"^# trace: (?P<scn>[0-9]+) (?P<step>[0-9]+) (?P<op>[a-z_]+)"
    r"(?P<fields>(?: [a-z_]+=[^ ]+)*) => (?P<result>[^ ]+)$"
)


def conf_steps(text):
    """Đọc bước của một tệp `.scn` (mọi chuỗi có `=>`) — cùng cách runner K2/IR đọc."""
    steps = []
    rest = text
    while True:
        start = rest.find('"')
        if start < 0:
            break
        after = rest[start + 1:]
        end = after.find('"')
        if end < 0:
            break
        inner = after[:end]
        rest = after[end + 1:]
        if "=>" not in inner:
            continue
        body, expect = inner.rsplit("=>", 1)
        toks = body.split()
        op = toks[0]
        params = {}
        for tok in toks[1:]:
            k, _, v = tok.partition("=")
            params[k] = v
        steps.append({"op": op, "params": params, "expect": expect.strip()})
    return steps


def conf_project(scn, steps):
    """Chiếu sang bước trace kỳ vọng: list `(step_no, op_trace, params, expect)`."""
    out = []
    pending_va = None
    for index, st in enumerate(steps, start=1):
        if st["op"] == "vm_shootdown_begin":
            pending_va = st["params"].get("va")
            continue
        if st["params"].get("frame") in CONF_SKIP_FRAME:
            continue
        if st["params"].get("va") in CONF_SKIP_VA:
            continue
        if st["op"] in CONF_SKIP_OPS:
            continue
        op = CONF_OP.get(st["op"])
        if op is None:
            continue
        params = dict(st["params"])
        if op == "shootdown":
            params = {"va": pending_va or "0x0"}
        out.append((index, op, params, st["expect"]))
    return out


def conf_num(value):
    try:
        return int(value, 0)
    except (TypeError, ValueError):
        return None


def check_trace(trace_path, scn_dir, files):
    """Kiểm tệp trace của **sim** theo hợp đồng KT-55. Trả True nếu sạch."""
    try:
        raw = trace_path.read_text().splitlines()
    except FileNotFoundError:
        print(f"not ok - trace: không thấy {trace_path}")
        return False

    lines = []
    bad = 0
    conf_re = re.compile(r"^# conf: scn=([0-9]+) name=([a-z-]+) steps=([0-9]+) ok=1$")
    for line in raw:
        if conf_re.match(line):
            continue  # dòng tổng kết của ca — kiểm riêng bên dưới
        m = TRACE_RE.match(line)
        if not m:
            bad += 1
            print(f"not ok - trace: dòng sai định dạng: {line!r}")
            continue
        fields = {}
        for tok in m.group("fields").split():
            k, _, v = tok.partition("=")
            fields[k] = v
        lines.append(
            {
                "scn": int(m.group("scn")),
                "step": int(m.group("step")),
                "op": m.group("op"),
                "fields": fields,
                "result": m.group("result"),
            }
        )
    if bad:
        return False

    # `# conf:` của sim — mỗi ca tự khai số bước; phải bằng số dòng trace của ca đó.
    conf = {}
    for line in raw:
        m = conf_re.match(line)
        if m:
            conf[int(m.group(1))] = (m.group(2), int(m.group(3)))
    if set(conf) != {c[0] for c in CONF_CASES}:
        print(f"not ok - trace: thiếu # conf: của ca (có {sorted(conf)})")
        return False

    ok = True
    pos = 0
    expected_all = []
    for scn, tag, name in CONF_CASES:
        path = scn_dir / name
        if not path.exists():
            print(f"not ok - trace: không thấy {path}")
            return False
        steps = conf_steps(path.read_text())
        proj = conf_project(scn, steps)
        expected_all.extend((scn, tag) + item for item in proj)
        if conf[scn][0] != tag:
            print(f"not ok - trace: # conf: ca {scn} tag {conf[scn][0]!r} ≠ {tag!r}")
            ok = False
        if conf[scn][1] != len(proj):
            print(f"not ok - trace: # conf: ca {scn} khai {conf[scn][1]} bước ≠ chiếu {len(proj)}")
            ok = False

    # So **từng dòng** theo thứ tự với tập bước kỳ vọng.
    if len(lines) != len(expected_all):
        print(f"not ok - trace: {len(lines)} dòng, kỳ vọng {len(expected_all)}")
        ok = False
    n = min(len(lines), len(expected_all))
    for i in range(n):
        scn, tag, step, op, params, expect = expected_all[i]
        got = lines[i]
        if (got["scn"], got["step"], got["op"]) != (scn, step, op):
            print(
                f"not ok - trace: dòng {i + 1} là {got['scn']} b{got['step']} {got['op']}"
                f", kỳ vọng {scn} b{step} {op}"
            )
            ok = False
            continue
        allowed = CONF_FIELDS[op]
        if set(got["fields"]) != allowed:
            print(
                f"not ok - trace: {scn} b{step} {op} có trường {sorted(got['fields'])}"
                f" ≠ {sorted(allowed)}"
            )
            ok = False
            continue
        f = got["fields"]
        # Giá trị **quan sát** phải khớp kỳ vọng của tệp.
        if op in ("map", "unmap", "protect", "walk"):
            if got["result"] != expect:
                print(f"not ok - trace: {scn} b{step} {op} kết quả {got['result']} ≠ {expect}")
                ok = False
        elif op == "expect_map":
            if got["result"] != "ok" or conf_num(f["size"]) != conf_num(params.get("size")):
                print(f"not ok - trace: {scn} b{step} expect_map {f} ≠ {params}")
                ok = False
        elif op == "counters":
            got_t = (conf_num(f["maps"]), conf_num(f["w"]), conf_num(f["x"]))
            want_t = (
                conf_num(params.get("maps")),
                conf_num(params.get("w")),
                conf_num(params.get("x")),
            )
            if got["result"] != "ok" or got_t != want_t or f["frame"] != params.get("frame"):
                print(f"not ok - trace: {scn} b{step} counters {f} ≠ {params}")
                ok = False
        elif op == "frames":
            if got["result"] != "ok" or conf_num(f["n"]) != conf_num(params.get("n")):
                print(f"not ok - trace: {scn} b{step} frames {f} ≠ {params}")
                ok = False
        elif op == "check":
            if got["result"] != "ok":
                print(f"not ok - trace: {scn} b{step} check lỗi: {got['result']}")
                ok = False
        elif op == "activate":
            if got["result"] != "ok" or f["vs"] != params.get("vs"):
                print(f"not ok - trace: {scn} b{step} activate {f} ≠ {params}")
                ok = False
        elif op == "shootdown":
            # Kỳ vọng lấy từ bước `vm_expect_shootdown` của cùng tệp (targets/acked/completed).
            want = None
            for st in steps:
                if st["op"] == "vm_expect_shootdown":
                    want = st["params"]
                    break
            if got["result"] != "ok" or conf_num(f["completed"]) != 1:
                print(f"not ok - trace: {scn} b{step} shootdown chưa xong: {f} => {got['result']}")
                ok = False
            elif want is not None and (
                conf_num(f["targets"]) != conf_num(want.get("targets"))
                or conf_num(f["acked"]) != conf_num(want.get("acked"))
                or (1 if want.get("completed") == "true" else 0) != conf_num(f["completed"])
            ):
                print(f"not ok - trace: {scn} b{step} shootdown {f} ≠ {want}")
                ok = False
        elif op == "fault":
            if got["result"] != "ok" or f["reason"] != params.get("reason"):
                print(f"not ok - trace: {scn} b{step} fault {f} ≠ reason={params.get('reason')}")
                ok = False
    if not ok:
        return False
    per = " ".join(f"{scn}:{len(conf_project(scn, conf_steps((scn_dir / name).read_text())))}"
                   for scn, _tag, name in CONF_CASES)
    print(f"# trace sim: OK ({len(lines)} dòng, 5 ca — {per})")
    return True


def main():
    scn_dir = pathlib.Path("v2/test/scn")
    spec_path = pathlib.Path("spec/abi.toml")
    draft_path = pathlib.Path("v2/docs/abi-reserved-objects.toml")
    sig_path = pathlib.Path("v2/docs/abi-call-signatures.toml")
    args = sys.argv[1:]
    trace_path = None
    steps_scn = None
    i = 0
    while i < len(args):
        if args[i] == "--dir":
            scn_dir = pathlib.Path(args[i + 1]); i += 2
        elif args[i] == "--spec":
            spec_path = pathlib.Path(args[i + 1]); i += 2
        elif args[i] == "--trace":
            trace_path = pathlib.Path(args[i + 1]); i += 2
        elif args[i] == "--steps":
            steps_scn = int(args[i + 1]); i += 2
        else:
            print(f"tham số lạ: {args[i]}")
            return 2

    # KT-55: in **tập số bước** conformance của một ca (nguồn độc lập với crate Rust) — dùng bởi
    # `ktest.sh check_trace` để đòi log QEMU có **đúng tập** `scn=NN step=K`, không chỉ đếm.
    if steps_scn is not None:
        name = dict((c[0], c[2]) for c in CONF_CASES).get(steps_scn)
        if name is None:
            print(f"không có ca {steps_scn}")
            return 2
        steps = conf_steps((scn_dir / name).read_text())
        proj = conf_project(steps_scn, steps)
        print(" ".join(str(item[0]) for item in proj))
        return 0

    try:
        _, spec = load_toml(spec_path)
    except FileNotFoundError:
        print(f"scn-check: KHÔNG có {spec_path}")
        return 1

    syscall_by_name = {v["name"] for v in spec.get("syscall", {}).values()}
    errors = {v["name"] for v in spec.get("error", {}).values()}
    objects = dict(spec.get("object", {}))
    if draft_path.exists():
        try:
            _, draft = load_toml(draft_path)
            objects.update(draft.get("object", {}))
        except Exception:  # noqa: BLE001 - draft hỏng thì bỏ qua, spec vẫn là chuẩn
            pass

    # KT-05: chữ ký đã vào `spec/[method_signature]` (stable-draft) — spec là nguồn.
    sig = {}
    spec_sig = spec.get("method_signature") or {}
    if spec_sig.get("stability") == "stable-draft":
        sig = {
            "method": {k: v for k, v in spec_sig.items() if isinstance(v, dict) and "params" in v},
            "state": spec_sig.get("state", {}),
        }
    elif sig_path.exists():
        try:
            _, sig = load_toml(sig_path)
        except Exception as exc:  # noqa: BLE001
            print(f"scn-check: draft chữ ký không parse được: {exc}")
            return 1
    sig_methods = sig.get("method", {})
    state_names = set((sig.get("state", {}) or {}).get("names", []) or [])

    def signature_ok(op, params):
        """Trả (ok, lý do). Kiểm đúng chữ ký draft: đủ tham số bắt buộc, không tham số lạ."""
        entry = sig_methods.get(op)
        if entry is None:
            return False, f"method {op} không có chữ ký trong draft D2-33"
        declared = entry.get("params", [])
        required = [x for x in declared if not x.endswith("?")]
        optional = [x[:-1] for x in declared if x.endswith("?")]
        missing = [x for x in required if x not in params]
        if missing:
            return False, f"{op} thiếu tham số bắt buộc {missing}"
        unknown = [k for k in params if k not in required and k not in optional]
        if unknown:
            return False, f"{op} có tham số lạ {unknown}"
        for key, allowed in (entry.get("variants") or {}).items():
            if key in params and params[key] not in allowed:
                return False, f"{op}.{key}={params[key]!r} không thuộc {allowed}"
        return True, ""

    # D2-11: nếu spec đã có chuỗi `args` cho một method thì draft không được nói khác
    def spec_params(obj_name, meth_name):
        obj = objects.get(obj_name) or {}
        for m in (obj.get("method") or {}).values():
            if isinstance(m, dict) and m.get("name") == meth_name and m.get("args"):
                return [t.split(":")[0].strip() for t in m["args"].split(",")]
        return None

    for op, entry in sig_methods.items():
        if "." not in op:
            continue
        obj_name, meth = op.split(".", 1)
        have = spec_params(obj_name, meth)
        if have is None:
            continue
        draft = [x.rstrip("?") for x in entry.get("params", [])]
        for param in have:
            if param not in draft:
                fails.append(f"draft lệch spec: {op} có {param!r} trong spec nhưng không có trong draft")

    def method_exists(name):
        if "." not in name:
            return False
        obj_name, meth = name.split(".", 1)
        obj = objects.get(obj_name)
        if not obj:
            return False
        return any((m.get("name") if isinstance(m, dict) else m) == meth
                   for m in obj.get("method", {}).values())

    files = sorted(scn_dir.glob("*.scn")) if scn_dir.exists() else []
    check(len(files) == 27, f"phải có đúng 27 tệp .scn, đang có {len(files)}")

    names, tags_seen, blocked, has_exit_case = set(), set(), [], False

    for path in files:
        try:
            raw, doc = load_toml(path)
        except Exception as exc:  # noqa: BLE001
            fails.append(f"{path.name}: TOML không parse được: {exc}")
            continue

        check(SPDX.encode() in raw, f"{path.name}: thiếu SPDX")
        name = doc.get("name")
        check(name == path.stem, f"{path.name}: name={name!r} phải trùng tên tệp")
        check(doc.get("version") == 1, f"{path.name}: version phải = 1")
        check(name not in names, f"{path.name}: name {name!r} trùng")
        names.add(name)

        if doc.get("blocked"):
            blocked.append((path.name, doc["blocked"]))
            check(not doc.get("steps") and not doc.get("setup"),
                  f"{path.name}: tệp blocked không được có bước")
            continue

        for tag in doc.get("tags", []):
            check(tag in TAG_GROUPS, f"{path.name}: tag lạ {tag!r}")
            tags_seen.add(tag)

        for st in doc.get("expect_states", []) or []:
            check_state(path.name, st, sig, state_names)
        _ = 0

        caps = doc.get("caps", {})
        steps = list(doc.get("setup", []))
        for thread in doc.get("thread", []):
            steps.extend(thread.get("steps", []))

        for line in steps:
            try:
                kind, op, params, expect = parse_step(line)
            except ValueError as exc:
                fails.append(f"{path.name}: bước {line!r}: {exc}")
                continue
            if kind == "syscall" and op in VM_OPS:
                # KT-42: op VM model-only (không có trong spec/ABI); kiểm tham số theo bảng
                # đóng — không thay kiểm `method_signature` của ABI.
                allowed = VM_OPS[op]
                missing = sorted(allowed - set(params))
                unknown = sorted(set(params) - allowed)
                check(not missing, f"{path.name}: {op} thiếu tham số {missing}")
                check(not unknown, f"{path.name}: {op} có tham số lạ {unknown}")
            elif kind == "syscall":
                want = SYSCALL_NAMES.get(op)
                check(want is not None, f"{path.name}: syscall lạ {op!r}")
                if want is not None:
                    check(want in syscall_by_name, f"{path.name}: {op} không có trong spec/abi.toml")
            else:
                check(method_exists(op), f"{path.name}: {op} không có trong spec/abi.toml (hoặc draft)")
                ok, why = signature_ok(op, params)
                check(ok, f"{path.name}: {why}")
            if expect == "halt":
                check(kind == "syscall" and op in ("exit", "debug_halt"),
                      f"{path.name}: `halt` chỉ dùng cho exit/debug_halt")
            else:
                check(expect == "ok" or expect in errors,
                      f"{path.name}: kết quả {expect!r} không phải ok/19 mã lỗi")
            for k, v in params.items():
                if k in CAP_KEYS and v not in caps:
                    fails.append(f"{path.name}: cap {v!r} dùng ở {op} chưa khai trong caps")

        if path.name == "20-exit-procsrv-chua-recv.scn":
            has_exit_case = any(parse_step(l)[0] == "syscall" and parse_step(l)[1] == "exit"
                                for l in steps if "=>" in l)

    check(has_exit_case, "thiếu ca bắt buộc 20-exit-procsrv-chua-recv với bước exit")
    missing = TAG_GROUPS - tags_seen
    check(not missing, f"thiếu nhóm phủ: {sorted(missing)}")

    check(not blocked, f"DoD KT-04c/KT-42/KT-64a: phải 27/27 tệp không blocked, còn {len(blocked)}")
    for name, reason in blocked:
        print(f"# blocked: {name} — {reason}")

    if fails:
        for msg in fails:
            print(f"not ok - {msg}")
        print(f"# {len(fails)} mục sai")
        return 1

    if trace_path is not None:
        if not check_trace(trace_path, scn_dir, files):
            print("# scn-check: FAIL (trace sim)")
            return 1

    print(f"scn-check: PASS ({len(files)} tệp, {len(blocked)} blocked, {len(tags_seen)} nhóm)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
