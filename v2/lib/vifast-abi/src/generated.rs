// SPDX-License-Identifier: MIT OR Apache-2.0
// SINH TỰ ĐỘNG bởi v2/tools/vifast-gen từ spec/abi.toml — ĐỪNG SỬA TAY (gen-check sẽ bắt).
#![allow(clippy::all)]

#[rustfmt::skip]
pub const ABI_VERSION: u32 = 1;
#[rustfmt::skip]
pub const MIN_COMPAT: u32 = 1;
#[rustfmt::skip]
pub const SYSCALL_COUNT: usize = 16;
#[rustfmt::skip]
pub const SYSCALLS: [(&str, u16); SYSCALL_COUNT] = [
    ("Call", 0),
    ("Send", 1),
    ("NBSend", 2),
    ("Recv", 3),
    ("NBRecv", 4),
    ("ReplyRecv", 5),
    ("NBSendRecv", 6),
    ("NBSendWait", 7),
    ("Yield", 8),
    ("WaitAddr", 9),
    ("WakeAddr", 10),
    ("ClockRead", 11),
    ("DebugPutStr", 12),
    ("DebugHalt", 13),
    ("Exit", 14),
    ("DebugGetInfo", 15),
];
#[rustfmt::skip]
pub const ERROR_COUNT: usize = 19;
#[rustfmt::skip]
pub const SYS_CALL: u64 = 0;
#[rustfmt::skip]
pub const SYS_SEND: u64 = 1;
#[rustfmt::skip]
pub const SYS_NB_SEND: u64 = 2;
#[rustfmt::skip]
pub const SYS_RECV: u64 = 3;
#[rustfmt::skip]
pub const SYS_NB_RECV: u64 = 4;
#[rustfmt::skip]
pub const SYS_REPLY_RECV: u64 = 5;
#[rustfmt::skip]
pub const SYS_NB_SEND_RECV: u64 = 6;
#[rustfmt::skip]
pub const SYS_NB_SEND_WAIT: u64 = 7;
#[rustfmt::skip]
pub const SYS_YIELD: u64 = 8;
#[rustfmt::skip]
pub const SYS_WAIT_ADDR: u64 = 9;
#[rustfmt::skip]
pub const SYS_WAKE_ADDR: u64 = 10;
#[rustfmt::skip]
pub const SYS_CLOCK_READ: u64 = 11;
#[rustfmt::skip]
pub const SYS_DEBUG_PUT_STR: u64 = 12;
#[rustfmt::skip]
pub const SYS_DEBUG_HALT: u64 = 13;
#[rustfmt::skip]
pub const SYS_EXIT: u64 = 14;
#[rustfmt::skip]
pub const SYS_DEBUG_GET_INFO: u64 = 15;
#[rustfmt::skip]
pub const ERRORS: [(&str, u16); ERROR_COUNT] = [
    ("InvalidCapability", 1),
    ("WrongObjectType", 2),
    ("InsufficientRights", 3),
    ("WxViolation", 4),
    ("InvalidArgument", 5),
    ("Alignment", 6),
    ("SlotOccupied", 7),
    ("SlotEmpty", 8),
    ("NotEnoughMemory", 9),
    ("AlreadyMapped", 10),
    ("NotMapped", 11),
    ("MissingPageTable", 12),
    ("HasChildren", 13),
    ("Busy", 14),
    ("Cancelled", 15),
    ("Timeout", 16),
    ("IllegalOperation", 17),
    ("NotSupported", 18),
    ("RangeError", 19),
];
#[rustfmt::skip]
pub const CORE_OBJECTS: [(&str, u16); 8] = [
    ("Untyped", 1),
    ("CNode", 2),
    ("Tcb", 3),
    ("SchedContext", 4),
    ("Endpoint", 5),
    ("Notification", 6),
    ("Reply", 7),
    ("SchedControl", 8),
];
#[rustfmt::skip]
pub const RESERVED_OBJECTS: [(&str, u16); 8] = [
    ("VSpace", 9),
    ("PageTable", 10),
    ("Frame", 11),
    ("IrqControl", 12),
    ("IrqHandler", 13),
    ("IOSpace", 14),
    ("VCpu", 15),
    ("PowerControl", 16),
];
#[rustfmt::skip]
pub const CNODE_METHODS: [(&str, u16); 7] = [
    ("Copy", 0),
    ("Mint", 1),
    ("Move", 2),
    ("Mutate", 3),
    ("Delete", 4),
    ("Revoke", 5),
    ("Rotate", 6),
];
#[rustfmt::skip]
pub const FRAME_METHODS: [(&str, u16); 5] = [
    ("Map", 0),
    ("Unmap", 1),
    ("GetAddress", 2),
    ("MapIo", 3),
    ("UnmapIo", 4),
];
#[rustfmt::skip]
pub const IOSPACE_METHODS: [(&str, u16); 2] = [
    ("Flush", 0),
    ("Attach", 1),
];
#[rustfmt::skip]
pub const IRQCONTROL_METHODS: [(&str, u16); 2] = [
    ("GetLine", 0),
    ("GetMsi", 1),
];
#[rustfmt::skip]
pub const IRQHANDLER_METHODS: [(&str, u16); 4] = [
    ("SetNotification", 0),
    ("Ack", 1),
    ("Clear", 2),
    ("SetAffinity", 3),
];
#[rustfmt::skip]
pub const PAGETABLE_METHODS: [(&str, u16); 2] = [
    ("Map", 0),
    ("Unmap", 1),
];
#[rustfmt::skip]
pub const POWERCONTROL_METHODS: [(&str, u16); 3] = [
    ("HartOffline", 0),
    ("HartOnline", 1),
    ("Suspend", 2),
];
#[rustfmt::skip]
pub const SCHEDCONTEXT_METHODS: [(&str, u16); 4] = [
    ("Bind", 0),
    ("Unbind", 1),
    ("Consumed", 2),
    ("YieldTo", 3),
];
#[rustfmt::skip]
pub const SCHEDCONTROL_METHODS: [(&str, u16); 1] = [
    ("Configure", 0),
];
#[rustfmt::skip]
pub const TCB_METHODS: [(&str, u16); 16] = [
    ("Configure", 0),
    ("SetFaultEp", 1),
    ("ReadState", 2),
    ("Suspend", 3),
    ("SetPriority", 4),
    ("SetMcp", 5),
    ("SetAffinity", 6),
    ("SetSchedParams", 7),
    ("SetTimeoutEp", 8),
    ("ReadRegs", 9),
    ("WriteRegs", 10),
    ("CopyRegs", 11),
    ("Resume", 12),
    ("BindNotification", 13),
    ("UnbindNotification", 14),
    ("SetTls", 15),
];
#[rustfmt::skip]
pub const UNTYPED_METHODS: [(&str, u16); 1] = [
    ("Retype", 0),
];
#[rustfmt::skip]
pub const VCPU_METHODS: [(&str, u16); 6] = [
    ("Run", 0),
    ("ReadRegs", 1),
    ("WriteRegs", 2),
    ("InjectIrq", 3),
    ("SetTsc", 4),
    ("Configure", 5),
];
#[rustfmt::skip]
pub const VSPACE_METHODS: [(&str, u16); 1] = [
    ("GetAsid", 0),
];
#[rustfmt::skip]
pub const SLOT_BYTES: usize = 48;
#[rustfmt::skip]
pub const SLOT_ALIGN_BYTES: usize = 16;
#[rustfmt::skip]
pub const DEFAULT_ALIGN_BYTES: usize = 16;
#[rustfmt::skip]
pub const PAGE_BYTES: usize = 4096;
#[rustfmt::skip]
pub const UNTYPED_MIN_BYTES: usize = 4096;
#[rustfmt::skip]
pub const CNODE_BASE_BYTES: usize = 64;
#[rustfmt::skip]
pub const OBJ_ENDPOINT_BYTES: usize = 64;
#[rustfmt::skip]
pub const OBJ_NOTIFICATION_BYTES: usize = 64;
#[rustfmt::skip]
pub const OBJ_REPLY_BYTES: usize = 64;
#[rustfmt::skip]
pub const OBJ_SCHED_CONTEXT_BYTES: usize = 256;
#[rustfmt::skip]
pub const OBJ_SCHED_CONTROL_BYTES: usize = 64;
#[rustfmt::skip]
pub const OBJ_TCB_BYTES: usize = 512;
#[rustfmt::skip]
/// (method, params, returns) — từ `[method_signature]` (stable-draft).
#[rustfmt::skip]
pub const METHOD_SIGNATURES: [(&str, &[&str], &[&str]); 15] = [
    ("CNode.Copy", &["cnode", "src", "dst", "rights?"], &[]),
    ("CNode.Delete", &["cnode", "slot"], &[]),
    ("CNode.Mint", &["cnode", "src", "dst", "rights?", "badge?"], &[]),
    ("CNode.Move", &["cnode", "src", "dst"], &[]),
    ("CNode.Revoke", &["cnode", "slot?"], &[]),
    ("Frame.Map", &["frame", "vspace", "va", "flags"], &[]),
    ("Frame.Unmap", &["frame", "vspace", "va"], &[]),
    ("SchedContext.Bind", &["sched", "target"], &[]),
    ("SchedControl.Configure", &["sched_control", "sched", "budget?", "period?", "refills?"], &[]),
    ("Tcb.Configure", &["tcb", "cspace?", "vspace?", "entry?", "ipc_buffer?", "priority?"], &[]),
    ("Tcb.ReadState", &["tcb"], &["state", "suspended", "blocked_on_kind", "exit_code"]),
    ("Tcb.Resume", &["tcb"], &[]),
    ("Tcb.SetFaultEp", &["tcb", "cap"], &[]),
    ("Tcb.Suspend", &["tcb", "mode"], &[]),
    ("Untyped.Retype", &["cap", "dst", "type", "size_bits", "flags"], &["slot"]),
];
pub const TRACE_RECORD_BYTES: usize = 32;
#[rustfmt::skip]
pub const TRACE_FIELDS: [(&str, usize, usize); 6] = [
    ("t_ns", 0, 8),
    ("hart", 8, 2),
    ("kind", 10, 2),
    ("tcb", 12, 4),
    ("a", 16, 8),
    ("b", 24, 8),
];
#[rustfmt::skip]
pub const TCB_STATES: [&str; 11] = ["Inactive", "Ready", "Running", "BlockedOnSend", "BlockedOnRecv", "BlockedOnReply", "BlockedOnNotification", "BlockedOnAddress", "Sleeping", "Stopping", "Dead"];
#[rustfmt::skip]
pub const FAULT_KIND_COUNT: usize = 4;
#[rustfmt::skip]
pub const FAULT_KINDS: [(&str, &[&str]); 4] = [
    ("Exit", &["code: u64"]),
    ("Vm", &["vspace: u64", "va: u64", "access: Read|Write|Execute", "reason: NotMapped|Permission|Wx"]),
    ("UnknownSyscall", &["nr: u64"]),
    ("UserException", &["ip: u64", "kind: u64"]),
];
