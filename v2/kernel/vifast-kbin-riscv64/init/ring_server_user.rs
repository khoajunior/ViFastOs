// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-69** — `ring_server` U-mode: **producer** của ring 1P/1C + phía server của interface IDL
//! `ringchan` (stub sinh từ `v2/lib/vifast-idl/idl/ring.vidl`).
//!
//! Giao thức (không thêm syscall; thủ tục theo đúng giao thức của rig `ktest_ring.rs`, kernel
//! đóng vai scheduler tuần tự):
//!
//! 1. map **Frame ring** (cap ở slot 5, VSpace cap slot 3) bằng invocation `Frame.Map` (số method
//!    lấy từ `vifast_abi::FRAME_MAP` sinh từ spec — không chép tay); `Ring::init` 8 slot × 32 B.
//! 2. gửi `attach` (**mang cap Frame** trong send caps header) sang client, kèm `slots`.
//! 3. nhận `attach_reply`, rồi **lấp đầy** ring tới 8 message ⇒ `send` thứ 9 phải trả mã 9.
//! 4. mỗi `sync` của client ⇒ gửi lô 4 message; sau lô, nếu ring đầy thì **probe** `send` để ghi
//!    mã 9 (không ghi đè). Nhận `finish` ⇒ reply + `ServerDone=1`, `Exit(0)`.
//!
//! Mọi số đo ghi vào khối stats trong Frame chia sẻ (một bên sở hữu từng trường); **không** in số
//! (D2-102: bằng chứng do kernel đọc/ in). Không `unsafe`, không `asm!` (R5); mọi truy cập bộ nhớ
//! qua shim an toàn.
#![no_std]
#![no_main]

extern crate vifast_abi;
extern crate vifast_idl;
extern crate vifast_ring;
extern crate vifast_user_abi;

#[path = "ring_common.rs"]
mod common;

use core::sync::atomic::{AtomicU64, Ordering};

use common::{
    message, Shm, BATCH, FLAGS_RWU, SLOT_BYTES, SLOTS, TOTAL, VA_RING,
};
use vifast_idl::{
    decode, decode_attach_reply, encode_attach, encode_finish_reply, encode_sync_reply, Request,
    METHOD_ATTACH, METHOD_FINISH, METHOD_SYNC,
};
use vifast_ring::{Ring, Stat, STATS_MAGIC};
use vifast_user_abi::{
    call, exit, info, info_label, info_length, nb_recv, nb_send, note_received, put,
    set_recv_window, set_send_caps, yield_now,
};

/// Ô CSpace **của server** (kernel dựng lúc spawn — xem `ktest_ring.rs`).
const S_EP_SEND: u64 = 1;
const S_EP_RECV: u64 = 2;
const S_VSPACE: u64 = 3;
const S_FRAME: u64 = 5;

/// Số bước tối đa tự thân (trước khi kernel chặn ở `MAX_STEPS`) — thoát mã 90 nếu vượt.
const MAX_SELF_STEPS: u64 = 120;

/// Trạng thái server (một tiến trình một luồng ⇒ atomic là cách không cần `static mut`).
static STEP: AtomicU64 = AtomicU64::new(0);
static SENT: AtomicU64 = AtomicU64::new(0);
static ATTACHED: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU64 = AtomicU64::new(0);

fn labels_add(ring: &Ring<Shm>, method: u32) {
    let seen = ring.stat(Stat::LabelsSeen) | (1u64 << method);
    ring.set_stat(Stat::LabelsSeen, seen);
}

/// Ghi một lần `send` gặp đầy: đếm + mã **thật** + `head` lúc đầy (lần đầu).
fn record_full(ring: &Ring<Shm>, code: u32) {
    if ring.stat(Stat::FullErr) == 0 {
        ring.set_stat(Stat::FullCode, u64::from(code));
        ring.set_stat(Stat::FullHead, ring.stats().head);
    }
    ring.add_stat(Stat::FullErr, 1);
}

/// Gửi một message (nếu còn chỗ); trả `false` khi ring đầy.
fn try_send(ring: &Ring<Shm>) -> bool {
    let seq = SENT.load(Ordering::Relaxed);
    if seq >= TOTAL {
        return false;
    }
    match ring.send(&message(seq)) {
        Ok(()) => {
            SENT.store(seq + 1, Ordering::Relaxed);
            ring.add_stat(Stat::SentOk, 1);
            ring.add_stat(Stat::ProdSum, seq.wrapping_add(1));
            true
        }
        Err(code) => {
            record_full(ring, code);
            false
        }
    }
}

/// Lấp đầy ring tới sức chứa: `send` thứ `SLOTS + 1` trả mã 9 (không ghi đè).
fn fill_full(ring: &Ring<Shm>) {
    while SENT.load(Ordering::Relaxed) < TOTAL {
        if !try_send(ring) {
            break;
        }
    }
}

/// Gửi lô `BATCH` sau `sync`; sau lô, probe `send` khi ring đầy để chứng minh mã 9.
fn send_batch(ring: &Ring<Shm>) {
    let mut sent = 0u64;
    while sent < BATCH && SENT.load(Ordering::Relaxed) < TOTAL {
        if !try_send(ring) {
            return;
        }
        sent += 1;
    }
    if SENT.load(Ordering::Relaxed) < TOTAL
        && ring.stats().used == u64::from(SLOTS)
        && !try_send(ring)
    {
        // `try_send` đã ghi mã/đếm đầy.
    }
}

fn step(ring: &Ring<Shm>) {
    let count = STEP.load(Ordering::Relaxed) + 1;
    STEP.store(count, Ordering::Relaxed);
    ring.set_stat(Stat::ServerSteps, count);
    let mut handled = 0;
    while handled < 4 {
        let answer = nb_recv(S_EP_RECV);
        if answer[0] != 0 {
            break;
        }
        note_received(answer[2]);
        let label = info_label(answer[2]) as u32;
        let length = info_length(answer[2]) as u8;
        let mr = [answer[3], answer[4], answer[5], answer[6]];
        if let Some(status) = decode_attach_reply(label, length, &mr) {
            if status == 0 {
                ring.set_stat(Stat::AttachMsgs, 1);
                labels_add(ring, METHOD_ATTACH);
                ATTACHED.store(1, Ordering::Relaxed);
                fill_full(ring);
            }
            handled += 1;
            continue;
        }
        match decode(label, length, &mr) {
            Some(Request::Sync { phase: _, count }) => {
                ring.add_stat(Stat::SyncMsgs, 1);
                labels_add(ring, METHOD_SYNC);
                let (reply_info, reply_mr) = encode_sync_reply(0);
                let _ = nb_send(S_EP_SEND, reply_info, reply_mr);
                if ATTACHED.load(Ordering::Relaxed) == 1
                    && count < TOTAL
                    && SENT.load(Ordering::Relaxed) < TOTAL
                {
                    send_batch(ring);
                }
                handled += 1;
            }
            Some(Request::Finish {
                err: _err,
                sent: _sent,
                got: _got,
            }) => {
                let (reply_info, reply_mr) = encode_finish_reply(0);
                let _ = nb_send(S_EP_SEND, reply_info, reply_mr);
                labels_add(ring, METHOD_FINISH);
                ring.set_stat(Stat::Wrap, ring.stats().head / u64::from(SLOTS));
                ring.set_stat(Stat::ServerDone, 1);
                DONE.store(1, Ordering::Relaxed);
                handled += 1;
            }
            _ => {
                handled += 1;
            }
        }
    }
    if count >= MAX_SELF_STEPS {
        exit(90)
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Window 4 ô **phân biệt** trong CSpace server: `apply_ipc_header` kiểm window ở **mọi**
    // lần gửi (kể cả khi chỉ gửi cap), nên không được để [0,0,0,0]; server không nhận cap.
    set_recv_window([12, 13, 14, 15]);
    let ring = Ring::new(Shm { base: VA_RING });
    // 1) map Frame ring vào VSpace của server (invocation Frame.Map = 11.0, số từ spec).
    let mapped = call(
        S_FRAME,
        info(vifast_abi::FRAME_MAP, 0, false, 3),
        [S_VSPACE, VA_RING, FLAGS_RWU, 0],
    );
    if mapped[0] != 0 {
        exit(60)
    }
    if ring.init(SLOTS, SLOT_BYTES).is_err() {
        exit(61)
    }
    ring.set_stat(Stat::Magic, STATS_MAGIC);
    ring.set_stat(Stat::Slots, u64::from(SLOTS));
    ring.set_stat(Stat::SlotBytes, u64::from(SLOT_BYTES));
    ring.set_stat(Stat::RingCheck, 1);
    labels_add(&ring, METHOD_ATTACH);
    put(b"U: ring server up\n");
    // 2) `attach` mang **cap Frame** (send caps header) sang client.
    let (attach_info, attach_mr) = encode_attach(SLOTS);
    if set_send_caps(&[S_FRAME]) != 1 {
        exit(62)
    }
    if nb_send(S_EP_SEND, attach_info, attach_mr)[0] != 0 {
        exit(63)
    }
    // 3) vòng scheduler: mỗi lượt một bước rồi `Yield`.
    while DONE.load(Ordering::Relaxed) == 0 {
        step(&ring);
        if DONE.load(Ordering::Relaxed) == 0 {
            yield_now();
        }
    }
    // 4) **Linger** trước `Exit`: reply `finish` vừa được đẩy vào hàng Endpoint; `Exit` của
    // sender finalise IPC và có thể huỷ message **còn trong hàng** (D2-25). Nhường 4 lượt để
    // client (chạy xen kẽ) kịp `NBRecv` reply rồi mới thoát — không đổi dữ liệu ring.
    let mut linger = 0u64;
    while linger < 4 {
        yield_now();
        linger += 1;
    }
    exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
