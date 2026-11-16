// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-69** — `ring_client` U-mode: **consumer** của ring 1P/1C + phía client của interface IDL
//! `ringchan` (stub sinh từ `v2/lib/vifast-idl/idl/ring.vidl`).
//!
//! 1. chờ `attach` từ server, **nhận cap Frame** qua receive window (KT-67b), kiểm
//!    `MessageInfo.caps` = 1 + CPTR đúng ô 4; map Frame vào VSpace của client (`Frame.Map`).
//! 2. `Ring::open` kiểm header; thử `recv` trên ring **rỗng** đúng `EMPTY_PROBES` lần ⇒ mỗi lần
//!    phải trả mã 14 (`Busy`) và **không** chặn; gửi `attach_reply`.
//! 3. mỗi bước tiêu thụ ≤ `BATCH` message, kiểm `seq`/nội dung **đúng thứ tự**; gửi `sync(count)`
//!    để server gửi lô kế. Đủ 40 message (5 vòng wrap) ⇒ gửi `finish`, chờ reply, `ClientDone=1`.
//!
//! Không `unsafe`/`asm!` (R5); mọi số đo ghi vào stats của Frame, kernel đọc lại (D2-102).
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
    verify, Shm, BATCH, EMPTY_PROBES, FLAGS_RWU, SLOT_BYTES, SLOTS, TOTAL, VA_RING,
};
use vifast_idl::{
    decode, decode_finish_reply, decode_sync_reply, encode_attach_reply, encode_finish,
    encode_sync, Request,
};
use vifast_ring::{Ring, Stat};
use vifast_user_abi::{
    call, exit, info, info_label, info_length, init_ipc_buffer, nb_recv, nb_send, note_received,
    put, read_received_caps, set_recv_window, yield_now,
};

/// Ô CSpace **của client** (kernel dựng lúc spawn — xem `ktest_ring.rs`).
const C_EP_RECV: u64 = 1;
const C_EP_SEND: u64 = 2;
const C_VSPACE: u64 = 3;
/// Ô đầu receive window — nơi kernel cài cap Frame nhận từ server.
const C_RECV: u64 = 4;
/// Receive window 4 ô (phân biệt) — ghi vào header IPC buffer trước khi nhận.
const WINDOW: [u64; 4] = [4, 5, 6, 7];
/// Số bước tối đa tự thân.
const MAX_SELF_STEPS: u64 = 160;
/// Mã lỗi `Busy` (rỗng) — đối chiếu hằng của ring, không viết số.
const EMPTY_CODE: u32 = vifast_ring::ERR_EMPTY;

static STEP: AtomicU64 = AtomicU64::new(0);
static GOT: AtomicU64 = AtomicU64::new(0);
static PHASE: AtomicU64 = AtomicU64::new(0);
static ATTACHED: AtomicU64 = AtomicU64::new(0);
static FINISH_SENT: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU64 = AtomicU64::new(0);
static ORDER: AtomicU64 = AtomicU64::new(1);

/// Nhận `attach`: cap Frame + hình học, map, kiểm rỗng, trả lời. `false` nếu chưa có message.
fn poll_attach(ring: &Ring<Shm>) -> bool {
    let answer = nb_recv(C_EP_RECV);
    if answer[0] != 0 {
        return false;
    }
    note_received(answer[2]);
    let label = info_label(answer[2]) as u32;
    let length = info_length(answer[2]) as u8;
    let mr = [answer[3], answer[4], answer[5], answer[6]];
    let Some(Request::Attach { slots }) = decode(label, length, &mr) else {
        return false;
    };
    let caps = read_received_caps();
    if caps.count != 1 || caps.slots[0] != C_RECV || slots != SLOTS {
        exit(70)
    }
    // Map **cap nhận được** vào VSpace của client (VSpace cap slot 3).
    let mapped = call(
        C_RECV,
        info(vifast_abi::FRAME_MAP, 0, false, 3),
        [C_VSPACE, VA_RING, FLAGS_RWU, 0],
    );
    if mapped[0] != 0 {
        exit(71)
    }
    if ring.open().is_err() {
        exit(72)
    }
    // Chỉ ghi **quan sát của client**; hình học ring/TAP lấy từ header do kernel đọc physmap.
    ring.set_stat(Stat::AttachCaps, caps.count as u64);
    // Ring **rỗng** trước lô đầu: recv phải trả mã 14 (Busy), tail không đổi.
    let mut out = [0u64; 4];
    let mut empty = 0u64;
    let mut first_code = 0u32;
    let mut last_code = 0u32;
    while empty < EMPTY_PROBES {
        match ring.recv(&mut out) {
            Err(code) => {
                if empty == 0 {
                    first_code = code;
                }
                if code != EMPTY_CODE {
                    ring.add_stat(Stat::HdrErr, 1);
                }
                last_code = code;
                empty += 1;
            }
            Ok(()) => exit(73),
        }
    }
    ring.set_stat(Stat::EmptyErr, empty);
    // D2-106: in **mã thật** (đầu/cuối), không in hằng kỳ vọng.
    ring.set_stat(Stat::EmptyCode, u64::from(first_code));
    ring.set_stat(Stat::EmptyCodeLast, u64::from(last_code));
    ring.set_stat(Stat::EmptyTail, ring.stats().tail);
    let (reply_info, reply_mr) = encode_attach_reply(0);
    if nb_send(C_EP_SEND, reply_info, reply_mr)[0] != 0 {
        exit(74)
    }
    ATTACHED.store(1, Ordering::Relaxed);
    true
}

/// Tiêu thụ ≤ `BATCH` message; kiểm thứ tự/nội dung; trả số đã nhận.
fn drain(ring: &Ring<Shm>) -> u64 {
    let mut n = 0u64;
    let mut out = [0u64; 4];
    while n < BATCH && GOT.load(Ordering::Relaxed) < TOTAL {
        let got = GOT.load(Ordering::Relaxed);
        match ring.recv(&mut out) {
            Ok(()) => {
                if got == 0 {
                    ring.set_stat(Stat::FirstSeq, out[0]);
                }
                ring.add_stat(Stat::RecvOk, 1);
                ring.add_stat(Stat::ConsSum, out[1]);
                if out[0] == got {
                    if !verify(got, &out) {
                        ring.add_stat(Stat::BadSlot, 1);
                        ORDER.store(0, Ordering::Relaxed);
                    }
                } else {
                    ORDER.store(0, Ordering::Relaxed);
                    if out[0] < got {
                        ring.add_stat(Stat::Dups, 1);
                    } else {
                        ring.add_stat(Stat::Lost, out[0] - got);
                    }
                }
                ring.set_stat(Stat::LastSeq, out[0]);
                GOT.store(got + 1, Ordering::Relaxed);
                n += 1;
            }
            Err(code) if code == EMPTY_CODE => break,
            Err(_) => {
                ring.add_stat(Stat::HdrErr, 1);
                ORDER.store(0, Ordering::Relaxed);
                break;
            }
        }
    }
    n
}

/// Gom reply của server (`sync`/`finish`); `true` ⇔ đã thấy reply `finish`.
fn poll_replies(ring: &Ring<Shm>) -> bool {
    let mut done = false;
    let mut handled = 0;
    while handled < 4 {
        let answer = nb_recv(C_EP_RECV);
        if answer[0] != 0 {
            break;
        }
        note_received(answer[2]);
        let label = info_label(answer[2]) as u32;
        let length = info_length(answer[2]) as u8;
        let mr = [answer[3], answer[4], answer[5], answer[6]];
        if let Some(status) = decode_finish_reply(label, length, &mr) {
            if status == 0 {
                ring.set_stat(Stat::ClientDone, 1);
                done = true;
            }
        } else if decode_sync_reply(label, length, &mr).is_some() {
            ring.add_stat(Stat::SyncAcks, 1);
        }
        handled += 1;
    }
    done
}

fn step(ring: &Ring<Shm>) {
    let count = STEP.load(Ordering::Relaxed) + 1;
    STEP.store(count, Ordering::Relaxed);
    if ATTACHED.load(Ordering::Relaxed) == 0 {
        poll_attach(ring);
    }
    if ATTACHED.load(Ordering::Relaxed) == 0 {
        if count >= MAX_SELF_STEPS {
            exit(80)
        }
        return;
    }
    // Chỉ ghi stats **sau** khi Frame đã map (trước đó VA_RING chưa có PTE).
    ring.set_stat(Stat::ClientSteps, count);
    if poll_replies(ring) {
        ring.set_stat(Stat::OrderOk, ORDER.load(Ordering::Relaxed));
        DONE.store(1, Ordering::Relaxed);
        return;
    }
    // Bằng chứng ring từng **đầy** khi cả hai bên còn sống.
    let used = ring.stats().used;
    if used > ring.stat(Stat::PeakUsed) {
        ring.set_stat(Stat::PeakUsed, used);
    }
    if FINISH_SENT.load(Ordering::Relaxed) == 0 {
        drain(ring);
        let got = GOT.load(Ordering::Relaxed);
        if got < TOTAL {
            let phase = PHASE.load(Ordering::Relaxed);
            PHASE.store(phase + 1, Ordering::Relaxed);
            let (sync_info, sync_mr) = encode_sync(phase as u32, got);
            let _ = nb_send(C_EP_SEND, sync_info, sync_mr);
        } else {
            ring.set_stat(Stat::FinishMsgs, 1);
            let (finish_info, finish_mr) = encode_finish(0, TOTAL, got);
            if nb_send(C_EP_SEND, finish_info, finish_mr)[0] != 0 {
                exit(75)
            }
            FINISH_SENT.store(1, Ordering::Relaxed);
        }
    }
    if count >= MAX_SELF_STEPS {
        exit(81)
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init_ipc_buffer();
    set_recv_window(WINDOW);
    put(b"U: ring client up\n");
    let ring = Ring::new(Shm { base: VA_RING });
    while DONE.load(Ordering::Relaxed) == 0 {
        step(&ring);
        if DONE.load(Ordering::Relaxed) == 0 {
            yield_now();
        }
    }
    exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
