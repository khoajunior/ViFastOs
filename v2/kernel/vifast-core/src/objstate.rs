// SPDX-License-Identifier: MIT OR Apache-2.0
//! Body **tĩnh** của Endpoint/Reply/Notification + hàng chờ IPC + bảng WaitOnAddress —
//! KT-60 port từ `EndpointState`/`ReplyData`/`NotificationState`/`QueuedMessage` của
//! `v2/sim/vifast-sim/src/kobj.rs` (oracle), thay `Vec`/`Box` bằng mảng có biên.
//!
//! Model-only `[TK]`, không phải ABI. Mọi bảng nằm trong `.bss` của [`crate::Machine`];
//! đầy bảng ⇒ lỗi `NotEnoughMemory` **nguyên tử** ở tầng thao tác (biên triển khai, ghi trong
//! `docs/k3b-core.md` §8), không panic, không cắt im lặng.
//!
//! # Vì sao message nằm trong **pool chung**
//!
//! Sim bọc mỗi `QueuedMessage` trong `Box` (heap); core không có `alloc` nên message đang chờ
//! nằm trong pool [`crate::machine::MAX_QUEUED_MESSAGES`], hàng của Endpoint giữ **chỉ số**
//! (`u16`). Sức chứa **mỗi** Endpoint vẫn là [`ENDPOINT_QUEUE_CAP`] = 8 như
//! `MAILBOX_CAP` của sim; pool chung nhỏ hơn tổng 8 × `MAX_ENDPOINTS` là biên tĩnh của mô
//! hình (một `Machine` biểu diễn tối đa `MAX_QUEUED_MESSAGES` message đang chờ, giống
//! `MAX_TCBS`).

use crate::error::{CapId, ObjId};
use crate::ipc::Message;
use crate::tcb::FaultKind;

/// Sức chứa hàng gửi của **một** Endpoint — bằng `MAILBOX_CAP` của sim (KT-22: "cố định 8").
pub const ENDPOINT_QUEUE_CAP: usize = 8;
/// Số bucket bảng băm WaitOnAddress (`[TK]`, D2-53 Q3 — như sim `WAITADDR_BUCKETS`).
pub const WAITADDR_BUCKETS: usize = 64;
/// Số ô giá trị "từ" tối đa của mô hình WaitOnAddress (biên tĩnh `[TK]`).
pub const MAX_ADDR_WORDS: usize = 64;
/// Số nút waiter WaitOnAddress tối đa (mỗi TCB tối đa một nút ⇒ bằng `MAX_TCBS`).
pub const MAX_WAIT_NODES: usize = 64;

/// Hàm băm bucket WaitOnAddress — **nguyên văn** sim (`(frame * 0x9E37_79B1) ^ offset) % 64`).
pub const fn waitaddr_bucket(frame: ObjId, offset: u32) -> usize {
    ((frame.wrapping_mul(0x9E37_79B1) ^ offset) as usize) % WAITADDR_BUCKETS
}

/// Message đang chờ trong hàng gửi của Endpoint (port `QueuedMessage`).
///
/// - `msg: Some` = message người dùng (Send/NBSend/Call) đã được cấp `msg_id`;
///   `msg: None` = **fault** (`Exit` không có message — D2-25).
/// - `call` = message từ `Call` đang chờ `Reply`; `false` = `Send`/`NBSend`/`Call no_reply`.
/// - `blocking` = người gửi đang `BlockedOnSend` chờ giao; `false` = gửi không chặn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QueuedMsg {
    pub(crate) msg: Option<Message>,
    pub(crate) sender: ObjId,
    pub(crate) call: bool,
    pub(crate) fault: Option<FaultKind>,
    pub(crate) blocking: bool,
    pub(crate) alive: bool,
}

impl QueuedMsg {
    pub(crate) const EMPTY: Self = Self {
        msg: None,
        sender: 0,
        call: false,
        fault: None,
        blocking: false,
        alive: false,
    };
}

/// Hàng gửi FIFO **cố định** của một Endpoint — chỉ số vào pool message của `Machine`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IpcQueue {
    slots: [u16; ENDPOINT_QUEUE_CAP],
    head: u8,
    len: u8,
    pub(crate) pushed: u64,
    pub(crate) popped: u64,
    pub(crate) removed: u64,
}

impl IpcQueue {
    pub(crate) const EMPTY: Self = Self {
        slots: [0; ENDPOINT_QUEUE_CAP],
        head: 0,
        len: 0,
        pushed: 0,
        popped: 0,
        removed: 0,
    };

    /// Sức chứa (luôn [`ENDPOINT_QUEUE_CAP`]).
    pub(crate) fn capacity(&self) -> usize {
        ENDPOINT_QUEUE_CAP
    }

    /// Số message đang chờ.
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }

    /// Hàng rỗng?
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Hàng đầy?
    pub(crate) fn is_full(&self) -> bool {
        self.len as usize >= ENDPOINT_QUEUE_CAP
    }

    /// Đẩy chỉ số message vào cuối hàng — trả lại chỉ số nếu đầy (không ghi đè).
    pub(crate) fn try_push(&mut self, idx: u16) -> Result<(), u16> {
        if self.is_full() {
            return Err(idx);
        }
        let tail = (self.head as usize + self.len as usize) % ENDPOINT_QUEUE_CAP;
        self.slots[tail] = idx;
        self.len += 1;
        self.pushed += 1;
        Ok(())
    }

    /// Lấy chỉ số message đầu hàng.
    pub(crate) fn pop_front(&mut self) -> Option<u16> {
        if self.len == 0 {
            return None;
        }
        let idx = self.slots[self.head as usize];
        self.head = ((self.head as usize + 1) % ENDPOINT_QUEUE_CAP) as u8;
        self.len -= 1;
        self.popped += 1;
        Some(idx)
    }

    /// Duyệt chỉ số theo FIFO.
    pub(crate) fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        let head = self.head as usize;
        let len = self.len as usize;
        (0..len).map(move |i| self.slots[(head + i) % ENDPOINT_QUEUE_CAP])
    }

    /// Thay nội dung hàng bằng `kept` (giữ thứ tự), cộng `removed` phần bị bỏ.
    pub(crate) fn kept(&mut self, kept: &[u16]) {
        let old = self.len as usize;
        for (i, &mi) in kept.iter().enumerate().take(ENDPOINT_QUEUE_CAP) {
            self.slots[i] = mi;
        }
        self.head = 0;
        self.len = kept.len().min(ENDPOINT_QUEUE_CAP) as u8;
        self.removed += (old - self.len as usize) as u64;
    }

    /// Bất biến cục bộ: `pushed == popped + removed + len`, `len ≤ cap`.
    pub(crate) fn invariant(&self) -> bool {
        self.len as usize <= ENDPOINT_QUEUE_CAP
            && self.pushed == self.popped + self.removed + self.len as u64
    }
}

/// Body Endpoint — port `EndpointState` của sim.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EndpointData {
    /// `(TCB đang chờ Recv, Reply cap người nhận đã cấp)` — tối đa **một** waiter.
    pub(crate) recv: Option<(ObjId, Option<CapId>)>,
    /// Hàng gửi FIFO cố định.
    pub(crate) send: IpcQueue,
    /// MR0 của message gần nhất (model API).
    pub(crate) last_mr: Option<u64>,
}

impl EndpointData {
    pub(crate) const EMPTY: Self = Self {
        recv: None,
        send: IpcQueue::EMPTY,
        last_mr: None,
    };
}

/// Body Reply — port `ReplyData` của sim.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReplyData {
    /// TCB đang `BlockedOnReply` trên Reply này (bất biến C2: `caller ⇔ BlockedOnReply`).
    pub(crate) caller: Option<ObjId>,
}

impl ReplyData {
    pub(crate) const EMPTY: Self = Self { caller: None };
}

/// Body Notification — port `NotificationState` của sim (KT-27).
///
/// `word` OR badge của mọi `Signal` chưa được `Wait`/`Poll` lấy; `badge` OR song song (bất
/// biến `badge == word`); **một** waiter (TCB chờ thứ hai ⇒ `Busy`); `bound_sc` là quan hệ
/// 1:1 `SchedContext.Bind(sc, notification)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct NotificationData {
    pub(crate) word: u64,
    pub(crate) badge: u64,
    pub(crate) waiter: Option<ObjId>,
    pub(crate) bound_sc: Option<ObjId>,
}

/// Một nút waiter WaitOnAddress — nằm trong danh sách liên kết của bucket.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WaitNode {
    pub(crate) frame: ObjId,
    pub(crate) offset: u32,
    pub(crate) tcb: ObjId,
    /// Chỉ số nút kế tiếp (`NODE_NONE` = hết).
    pub(crate) next: u16,
    pub(crate) alive: bool,
}

/// Chỉ số "không có nút".
pub(crate) const NODE_NONE: u16 = u16::MAX;

impl WaitNode {
    pub(crate) const EMPTY: Self = Self {
        frame: 0,
        offset: 0,
        tcb: 0,
        next: NODE_NONE,
        alive: false,
    };
}

/// Một ô giá trị "từ" của WaitOnAddress (`(frame, offset) -> u64`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct AddrWord {
    pub(crate) frame: ObjId,
    pub(crate) offset: u32,
    pub(crate) value: u64,
    pub(crate) alive: bool,
}

impl AddrWord {
    pub(crate) const EMPTY: Self = Self {
        frame: 0,
        offset: 0,
        value: 0,
        alive: false,
    };
}
