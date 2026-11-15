// SPDX-License-Identifier: MIT OR Apache-2.0
//! `vifast-scenario` — **IR rút gọn** của 5 kịch bản VM nền tảng (KT-55, K3-PLAN §4.1).
//!
//! Nguồn sự thật là chính các tệp `v2/test/scn/{22..26}-*.scn` (D2-69): crate này **đọc** chuỗi
//! bước `op k=v … => kết quả` của tệp (không sửa, không chép tay) rồi **chiếu** sang tập bước
//! *conformance* — những bước mà **cả hai** backend (sim host và kernel QEMU) quan sát được bằng
//! trạng thái thật của mình:
//!
//! | Nhóm | Bước trace | Vì sao loại phần còn lại |
//! |---|---|---|
//! | `vm-map` | `map`, `expect_map`, `counters`, `walk`, `unmap`, `frames`, `check` | Frame 1 GiB **chưa** cho map (KT-52 L7 / D2-66: `NotSupported`) nên bước `frame=f1` / `va=0x80000000` bị loại |
//! | `vm-wx` | như trên + `protect` | như trên (loại bước `f1`) |
//! | `vm-tlb` | `activate`, `map`, `shootdown` (tại bước `vm_shootdown_wait`), `unmap`, `walk`, `counters`, `check` | `vm_tlb_fill`/`vm_tlb_expect`/`vm_expect_pending` là mô hình TLB per-hart **model-only** của K2; TLB thật (stale→fresh, 3 dạng sfence) đã có TAP 25..29 của KT-51 |
//! | `vm-fault` | `map`, `fault` (tại bước `vm_expect_fault`), `walk`, `frames`, `check` | `vm_dispatch`/`vm_expect_state`/`vm_procsrv_*` cần TCB/procsrv — K3 **chưa** có (Q13, K3.5/KT-58) |
//! | `vm-shared` | `map`, `counters`, `expect_map`, `fault`, `unmap`, `frames`, `check` | như `vm-fault` |
//!
//! Nhờ vậy **hai** backend chạy đúng *cùng* danh sách bước, cùng số thứ tự bước trong tệp, và
//! in ra *cùng* định dạng `# trace:` ([`vifast_traceformat::TraceLine`]).
//!
//! `no_std`, **không cấp phát**: kết quả ghi vào mảng do người gọi cấp (`[_; MAX_STEPS]`), nên
//! kernel `include_str!` tệp `.scn` rồi parse tại chỗ.

#![no_std]
#![forbid(unsafe_code)]

/// Số bước tối đa của một kịch bản (tệp dài nhất: 27 bước — dư biên).
pub const MAX_STEPS: usize = 40;
/// Số tham số tối đa một bước (`k=v`).
pub const MAX_PARAMS: usize = 8;

/// 5 kịch bản nền tảng: `(id, tag nhóm, tên tệp)` — đúng D2-69, thứ tự boot.
pub const CASES: [(u8, &str, &str); 5] = [
    (22, "vm-map", "22-frame-map-unmap-walk.scn"),
    (23, "vm-wx", "23-wx-violation-counters.scn"),
    (24, "vm-tlb", "24-shootdown-2-hart.scn"),
    (25, "vm-fault", "25-vm-fault-notmapped-resume.scn"),
    (26, "vm-shared", "26-shared-readonly-frame.scn"),
];

/// Tên tệp của ca `id` (`None` nếu không thuộc 5 ca nền tảng).
pub fn file_of(id: u8) -> Option<&'static str> {
    let mut i = 0;
    while i < CASES.len() {
        if CASES[i].0 == id {
            return Some(CASES[i].2);
        }
        i += 1;
    }
    None
}

/// Nhóm tag của ca `id`.
pub fn tag_of(id: u8) -> Option<&'static str> {
    let mut i = 0;
    while i < CASES.len() {
        if CASES[i].0 == id {
            return Some(CASES[i].1);
        }
        i += 1;
    }
    None
}

/// Một bước `.scn` đọc được: `op k=v … => expect`, `index` 1-based theo **toàn bộ** tệp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawStep<'a> {
    /// Số thứ tự bước trong tệp (1-based, tính **mọi** bước kể cả bước không trace).
    pub index: u16,
    /// Op (`vm_map`, …).
    pub op: &'a str,
    /// Kết quả kỳ vọng sau `=>` (`ok` / `WxViolation` / …).
    pub expect: &'a str,
    /// Tham số theo thứ tự xuất hiện trong tệp.
    pub params: [(&'a str, &'a str); MAX_PARAMS],
    /// Số tham số thật.
    pub n_params: usize,
}

impl<'a> RawStep<'a> {
    /// Bước rỗng (giá trị khởi tạo cho mảng).
    pub const EMPTY: Self = Self {
        index: 0,
        op: "",
        expect: "",
        params: [("", ""); MAX_PARAMS],
        n_params: 0,
    };

    /// Giá trị tham số `key` (nếu có).
    pub fn get(&self, key: &str) -> Option<&'a str> {
        let mut i = 0;
        while i < self.n_params {
            if eq(self.params[i].0, key) {
                return Some(self.params[i].1);
            }
            i += 1;
        }
        None
    }

    /// `true` ⇔ bước có tham số `key` = `value` (chuỗi).
    pub fn has(&self, key: &str, value: &str) -> bool {
        matches!(self.get(key), Some(v) if eq(v, value))
    }
}

/// Bước conformance sau khi chiếu: giữ `index` **của tệp** để hai log so được từng dòng.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IrStep<'a> {
    /// Ca (22..26).
    pub scn: u8,
    /// Số thứ tự bước trong tệp (1-based).
    pub step: u16,
    /// Loại bước trace.
    pub op: Op,
    /// Tham số (đã lọc: bỏ `hart`; `shootdown` nhận `va` từ bước `vm_shootdown_begin`).
    pub params: [(&'a str, &'a str); MAX_PARAMS],
    /// Số tham số.
    pub n_params: usize,
    /// Kỳ vọng của tệp (`ok`/`WxViolation`/`NotMapped`/`Permission`).
    pub expect: &'a str,
}

impl<'a> IrStep<'a> {
    /// Bước rỗng.
    pub const EMPTY: Self = Self {
        scn: 0,
        step: 0,
        op: Op::Check,
        params: [("", ""); MAX_PARAMS],
        n_params: 0,
        expect: "",
    };

    /// Giá trị tham số `key`.
    pub fn get(&self, key: &str) -> Option<&'a str> {
        let mut i = 0;
        while i < self.n_params {
            if eq(self.params[i].0, key) {
                return Some(self.params[i].1);
            }
            i += 1;
        }
        None
    }

    /// Token op trong dòng `# trace:` (ổn định — script so dùng).
    pub fn op_name(&self) -> &'static str {
        self.op.name()
    }
}

/// Loại bước của IR conformance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `vm_map` — map frame vào VSpace.
    Map,
    /// `vm_unmap`.
    Unmap,
    /// `vm_protect`.
    Protect,
    /// `vm_walk` — kết quả quan sát: `ok` (có mapping) / `NotMapped`.
    Walk,
    /// `vm_expect_map` — quan sát `size_bits` (12/21) + frame tại VA.
    ExpectMap,
    /// `vm_expect_counters` — quan sát `maps/w/x` của Frame.
    Counters,
    /// `vm_expect_frames` — quan sát số Frame sống.
    Frames,
    /// `vm_check` — bất biến object layer.
    Check,
    /// `vm_activate` — VSpace active trên hart (bỏ `hart` khỏi trace).
    Activate,
    /// `vm_shootdown_wait` — shootdown thật; trường `va` lấy từ `vm_shootdown_begin`.
    Shootdown,
    /// `vm_expect_fault` — quan sát `reason` của fault VM (`NotMapped`/`Permission`).
    Fault,
}

impl Op {
    /// Tên ổn định trong dòng trace.
    pub const fn name(self) -> &'static str {
        match self {
            Op::Map => "map",
            Op::Unmap => "unmap",
            Op::Protect => "protect",
            Op::Walk => "walk",
            Op::ExpectMap => "expect_map",
            Op::Counters => "counters",
            Op::Frames => "frames",
            Op::Check => "check",
            Op::Activate => "activate",
            Op::Shootdown => "shootdown",
            Op::Fault => "fault",
        }
    }
}

fn eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.as_bytes() == b.as_bytes()
}

fn trim(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }
    while end > start
        && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t' || bytes[end - 1] == b'\r')
    {
        end -= 1;
    }
    &s[start..end]
}

/// Đọc **mọi** bước `.scn` (các chuỗi có `=>`) theo thứ tự xuất hiện — `setup` trước, `thread`
/// sau, đúng thứ tự chạy của runner K2. Trả số bước; `0` nếu không có bước nào.
///
/// Cố ý **không** parse TOML đầy đủ: runner K2 (`kt42_scn.rs`) cũng đọc đúng tập chuỗi này, nên
/// hai bên không thể lệch thứ tự.
pub fn parse<'a>(text: &'a str, out: &mut [RawStep<'a>; MAX_STEPS]) -> usize {
    let mut n = 0usize;
    let mut index = 0u16;
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        let inner = &after[..end];
        rest = &after[end + 1..];
        let Some((body, expect)) = inner.rsplit_once("=>") else {
            continue;
        };
        index += 1;
        if n >= MAX_STEPS {
            break;
        }
        let mut step = RawStep::EMPTY;
        step.index = index;
        step.expect = trim(expect);
        let mut toks = body.split_whitespace();
        step.op = toks.next().unwrap_or("");
        for tok in toks {
            let Some((k, v)) = tok.split_once('=') else {
                continue;
            };
            if step.n_params < MAX_PARAMS {
                step.params[step.n_params] = (k, v);
                step.n_params += 1;
            }
        }
        out[n] = step;
        n += 1;
    }
    n
}

/// `true` ⇔ bước bị loại khỏi tập con conformance. Ba luật **có lý do đo được** (xem doc crate):
///
/// 1. `frame=f1` / `va=0x80000000`: Frame 1 GiB chưa cho map (KT-52 L7, D2-66) ⇒ kernel `NotSupported`.
/// 2. op mô hình TLB per-hart (`vm_tlb_fill`/`vm_tlb_expect`/`vm_expect_pending`/`vm_expect_shootdown`).
/// 3. op cần TCB/procsrv (`vm_dispatch`/`vm_expect_state`/`vm_procsrv_*`) — K3 chưa có (Q13).
fn skipped(step: &RawStep<'_>) -> bool {
    if step.has("frame", "f1") {
        return true;
    }
    if let Some(va) = step.get("va") {
        if eq(va, "0x80000000") {
            return true;
        }
    }
    matches!(
        step.op,
        "vm_tlb_fill"
            | "vm_tlb_expect"
            | "vm_expect_pending"
            | "vm_expect_shootdown"
            | "vm_expect_state"
            | "vm_dispatch"
            | "vm_procsrv_recv"
            | "vm_procsrv_resume"
    )
}

/// Chiếu các bước `.scn` sang IR conformance. Trả số bước trace.
///
/// `vm_shootdown_wait` là bước **duy nhất** cần ngữ cảnh: trường `va` lấy từ
/// `vm_shootdown_begin` ngay trước (bước `begin` không phát dòng riêng — mô hình sim chỉ quan sát
/// được trạng thái cuối, kernel chỉ có shootdown blocking).
pub fn project<'a>(scn: u8, steps: &[RawStep<'a>], out: &mut [IrStep<'a>; MAX_STEPS]) -> usize {
    let mut n = 0usize;
    let mut pending_va: Option<&'a str> = None;
    for step in steps {
        if eq(step.op, "vm_shootdown_begin") {
            pending_va = step.get("va");
            continue;
        }
        if skipped(step) {
            continue;
        }
        let op = match step.op {
            "vm_map" => Op::Map,
            "vm_unmap" => Op::Unmap,
            "vm_protect" => Op::Protect,
            "vm_walk" => Op::Walk,
            "vm_expect_map" => Op::ExpectMap,
            "vm_expect_counters" => Op::Counters,
            "vm_expect_frames" => Op::Frames,
            "vm_check" => Op::Check,
            "vm_activate" => Op::Activate,
            "vm_shootdown_wait" => Op::Shootdown,
            "vm_expect_fault" => Op::Fault,
            _ => continue,
        };
        if n >= MAX_STEPS {
            break;
        }
        let mut ir = IrStep::EMPTY;
        ir.scn = scn;
        ir.step = step.index;
        ir.op = op;
        ir.expect = step.expect;
        // Giữ **nguyên** tham số (kể cả `hart`): kernel cần `hart` của `vm_activate`; script so
        // bỏ trường volatile (`hart`) ở bước đối chiếu, không bỏ khỏi dòng trace.
        let mut i = 0;
        while i < step.n_params {
            let (k, v) = step.params[i];
            if ir.n_params < MAX_PARAMS {
                ir.params[ir.n_params] = (k, v);
                ir.n_params += 1;
            }
            i += 1;
        }
        if op == Op::Shootdown {
            // Bước `wait` không mang `va`; lấy từ `vm_shootdown_begin` ngay trước.
            ir.params[0] = ("va", pending_va.unwrap_or("0x0"));
            ir.n_params = 1;
        }
        out[n] = ir;
        n += 1;
    }
    n
}

/// `true` ⇔ bước `step` (1-based) của ca `scn` nằm trong tập con conformance — dùng cho runner
/// sim (nó chạy **mọi** bước như K2, chỉ in trace cho bước được chiếu).
pub fn is_traced(scn: u8, step: u16, traced: &[IrStep<'_>]) -> bool {
    let mut i = 0;
    while i < traced.len() {
        if traced[i].scn == scn && traced[i].step == step {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCN22: &str = r#"
version = 1
name = "22-frame-map-unmap-walk"
harts = 1
thread = [
  { name = "t0", steps = [
    "vm_map vs=vs0 va=0x1000 frame=f4 rights=READ|WRITE attrs=WB => ok",
    "vm_expect_map vs=vs0 va=0x1000 frame=f4 size=12 => ok",
    "vm_map vs=vs0 va=0x80000000 frame=f1 rights=READ attrs=WB|EXEC => ok",
    "vm_unmap hart=0 vs=vs0 va=0x80000000 => ok",
    "vm_check => ok",
  ] },
]
"#;

    #[test]
    fn parse_dem_buoc_va_tham_so() {
        let mut raw = [RawStep::EMPTY; MAX_STEPS];
        let n = parse(SCN22, &mut raw);
        assert_eq!(n, 5);
        assert_eq!(raw[0].index, 1);
        assert_eq!(raw[0].op, "vm_map");
        assert_eq!(raw[0].get("frame"), Some("f4"));
        assert_eq!(raw[0].get("rights"), Some("READ|WRITE"));
        assert_eq!(raw[0].expect, "ok");
        assert_eq!(raw[1].op, "vm_expect_map");
        assert_eq!(raw[1].get("size"), Some("12"));
        assert_eq!(raw[3].get("hart"), Some("0"));
    }

    #[test]
    fn project_bo_frame_1g_va_1g_giu_tham_so() {
        let mut raw = [RawStep::EMPTY; MAX_STEPS];
        let n = parse(SCN22, &mut raw);
        let mut ir = [IrStep::EMPTY; MAX_STEPS];
        let m = project(22, &raw[..n], &mut ir);
        // Bước 3 (map f1) và 4 (unmap 0x80000000) bị loại; còn 1,2,5.
        assert_eq!(m, 3);
        assert_eq!(ir[0].step, 1);
        assert_eq!(ir[0].op, Op::Map);
        assert_eq!(ir[1].step, 2);
        assert_eq!(ir[1].op, Op::ExpectMap);
        assert_eq!(ir[2].step, 5);
        assert_eq!(ir[2].op, Op::Check);
        // `hart` vẫn nằm trong IR (script so bỏ ở bước đối chiếu — trường volatile).
        assert_eq!(ir[0].get("hart"), None);
    }

    #[test]
    fn shootdown_lay_va_tu_begin_va_khong_co_hart() {
        const SCN24: &str = r#"
thread = [
  { name = "t0", steps = [
    "vm_activate vs=vs0 hart=0 => ok",
    "vm_shootdown_begin hart=0 vs=vs0 va=0x1000 => ok",
    "vm_expect_pending hart=0 targets=3 acked=1 messages=1 completed=false => ok",
    "vm_shootdown_wait hart=0 => ok",
    "vm_check => ok",
  ] },
]
"#;
        let mut raw = [RawStep::EMPTY; MAX_STEPS];
        let n = parse(SCN24, &mut raw);
        let mut ir = [IrStep::EMPTY; MAX_STEPS];
        let m = project(24, &raw[..n], &mut ir);
        assert_eq!(m, 3); // activate, shootdown(wait), check
        assert_eq!(ir[1].op, Op::Shootdown);
        assert_eq!(ir[1].step, 4);
        assert_eq!(ir[1].get("va"), Some("0x1000"));
        assert_eq!(ir[1].get("hart"), None); // shootdown: chỉ còn `va` (bước wait)
    }

    #[test]
    fn fault_va_op_khop_ten_trace() {
        assert_eq!(Op::Fault.name(), "fault");
        assert_eq!(Op::ExpectMap.name(), "expect_map");
        assert_eq!(Op::Shootdown.name(), "shootdown");
        assert_eq!(file_of(26), Some("26-shared-readonly-frame.scn"));
        assert_eq!(tag_of(24), Some("vm-tlb"));
        assert_eq!(file_of(99), None);
    }
}
