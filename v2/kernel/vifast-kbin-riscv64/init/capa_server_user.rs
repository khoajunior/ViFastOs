// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-68** — `capa_server` U-mode **thật** trên ảnh `vifast-kbin-ktest-user`: **capalloc** —
//! cấp cap từ slot rảnh của CNode **client** bằng `CNode.Copy` (invocation, D2-99), thu hồi
//! bằng `CNode.Revoke` + `CNode.Delete`, rồi cấp lại **cùng slot** (không rò).
//!
//! # Giao thức (label = op, bài test — không phải ABI)
//!
//! | Op | Chiều | Nghĩa |
//! |---|---|---|
//! | `OP_ALLOC` | client → server | quét slot rảnh của CNode client (lỗi 7 = đã dùng) rồi `Copy` Frame của server vào slot đầu tiên trống; trả slot ở `MR0` |
//! | `OP_REVOKE_DELETE` | client → server | `Revoke(slot)` (xoá cây con) rồi `Delete(slot)` (trả slot); trả `[số cap bị revoke, mã lỗi delete]` |
//! | `OP_REVOKE_SOURCE` | client → server | `Revoke` trên **cap nguồn Frame của server** ⇒ xoá **xuyên CNode** mọi bản sao đã cấp; trả số cap bị xoá |
//! | `OP_DONE` | client → server | xoá cap CNode client khỏi CSpace server (không rò cap đăng ký) rồi `Exit(0)` |
//!
//! CNode của client đến từ **chuyển cap** của message đầu tiên: client gửi `MessageInfo.caps = 1`
//! + CPTR nguồn trong header IPC buffer (KT-67b); kernel cài cap vào **receive window** của
//! server (slot 8..11) — server dùng `read_received_caps()` đọc CPTR đích.
//!
//! Không `unsafe`, không `asm!` (luật R5): mọi `ecall`/đọc-ghi bộ nhớ qua shim
//! `vifast_user_abi` (`kernel/vifast-plat-user/lib.rs`, danh sách đóng D2-17/D2-101).
#![no_std]
#![no_main]

extern crate vifast_user_abi;

use vifast_user_abi::{
    call, exit, info, info_label, init_ipc_buffer, nb_recv, nb_send, put, read_received_caps,
    set_recv_window, yield_now, MAX_CAPS,
};

/// Slot CSpace **riêng của server** (kernel dựng lúc spawn, xem `ktest_capa.rs`).
const SLOT_SELF: u64 = 0;
const SLOT_EP_REQ: u64 = 1;
const SLOT_EP_RESP: u64 = 2;
const SLOT_FRAME: u64 = 5;
/// Receive window của server — nơi kernel cài cap CNode client (message đầu mang cap).
const WINDOW: [u64; MAX_CAPS] = [8, 9, 10, 11];
/// Số slot của CNode client (khớp rig `add_cnode(16)`).
const CLIENT_SLOTS: u64 = 16;

/// Số method **lấy từ `spec/abi.toml`** `[object].CNode`: 0 Copy, 4 Delete, 5 Revoke.
const METHOD_CNODE_COPY: u32 = 0;
const METHOD_CNODE_DELETE: u32 = 4;
const METHOD_CNODE_REVOKE: u32 = 5;

/// Op của bài test (label) — không phải ABI.
const OP_ALLOC: u32 = 0x680;
const OP_REVOKE_DELETE: u32 = 0x681;
const OP_REVOKE_SOURCE: u32 = 0x682;
const OP_DONE: u32 = 0x683;

/// Mã lỗi ABI dùng trong chương trình (spec `[error]`).
const ERR_BUSY: u64 = 14;
const ERR_SLOT_OCCUPIED: u64 = 7;

/// Mã thoát khi hỏng.
const EXIT_RECV: u64 = 60;
const EXIT_CAPS: u64 = 61;
const EXIT_ALLOC: u64 = 62;
const EXIT_NO_SLOT: u64 = 63;
const EXIT_REVOKE: u64 = 64;
const EXIT_DELETE: u64 = 65;
const EXIT_REPLY: u64 = 66;

/// Quét slot rảnh của CNode `cnode` bằng `CNode.Copy` từ cap Frame của server.
///
/// Chính sách cấp phát **duy nhất** của capalloc: duyệt slot 1..N, lỗi `SlotOccupied` (7) =
/// đã dùng ⇒ thử tiếp; lỗi khác = hỏng thật (không im lặng). Trả slot đầu tiên cấp được.
fn alloc_frame(cnode: u64) -> u64 {
    let mut slot = 1u64;
    while slot < CLIENT_SLOTS {
        let answer = call(
            cnode,
            info(METHOD_CNODE_COPY, 0, false, 2),
            [SLOT_FRAME, slot, 0, 0],
        );
        if answer[0] == 0 {
            return slot;
        }
        if answer[0] != ERR_SLOT_OCCUPIED {
            exit(EXIT_ALLOC);
        }
        slot += 1;
    }
    exit(EXIT_NO_SLOT)
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init_ipc_buffer();
    set_recv_window(WINDOW);
    let mut client_cnode = 0u64;
    loop {
        let answer = nb_recv(SLOT_EP_REQ);
        if answer[0] == ERR_BUSY {
            let _ = yield_now();
            continue;
        }
        if answer[0] != 0 {
            exit(EXIT_RECV);
        }
        // Message đầu mang cap CNode của client trong receive window.
        if client_cnode == 0 {
            // `NBRecv` trả `MessageInfo` của message vừa nhận ở `answer[2]`; ghi vào header để
            // `read_received_caps()` đọc đúng `caps` **thật sự cài được** (KT-67b).
            vifast_user_abi::note_received(answer[2]);
            let caps = read_received_caps();
            if caps.count != 1 {
                exit(EXIT_CAPS);
            }
            client_cnode = caps.slots[0];
        }
        match info_label(answer[2]) {
            OP_ALLOC => {
                let slot = alloc_frame(client_cnode);
                let sent = nb_send(
                    SLOT_EP_RESP,
                    info(OP_ALLOC, 0, false, 1),
                    [slot, 0, 0, 0],
                );
                if sent[0] != 0 {
                    exit(EXIT_REPLY);
                }
            }
            OP_REVOKE_DELETE => {
                let target = answer[3];
                let revoked = call(
                    client_cnode,
                    info(METHOD_CNODE_REVOKE, 0, false, 1),
                    [target, 0, 0, 0],
                );
                if revoked[0] != 0 {
                    exit(EXIT_REVOKE);
                }
                let deleted = call(
                    client_cnode,
                    info(METHOD_CNODE_DELETE, 0, false, 1),
                    [target, 0, 0, 0],
                );
                let sent = nb_send(
                    SLOT_EP_RESP,
                    info(OP_REVOKE_DELETE, 0, false, 2),
                    [revoked[1], deleted[0], 0, 0],
                );
                if sent[0] != 0 {
                    exit(EXIT_REPLY);
                }
            }
            OP_REVOKE_SOURCE => {
                // Revoke **cap nguồn** trong CSpace server ⇒ xoá bản sao ở CNode client
                // (Revoke theo cây MDB, xuyên CNode).
                let revoked = call(
                    SLOT_SELF,
                    info(METHOD_CNODE_REVOKE, 0, false, 1),
                    [SLOT_FRAME, 0, 0, 0],
                );
                if revoked[0] != 0 {
                    exit(EXIT_REVOKE);
                }
                let sent = nb_send(
                    SLOT_EP_RESP,
                    info(OP_REVOKE_SOURCE, 0, false, 1),
                    [revoked[1], 0, 0, 0],
                );
                if sent[0] != 0 {
                    exit(EXIT_REPLY);
                }
            }
            OP_DONE => {
                // Không để rò cap đăng ký: xoá cap CNode client khỏi CSpace của server.
                let deleted = call(
                    SLOT_SELF,
                    info(METHOD_CNODE_DELETE, 0, false, 1),
                    [client_cnode, 0, 0, 0],
                );
                if deleted[0] != 0 {
                    exit(EXIT_DELETE);
                }
                // Ack để client `Exit` **sau** khi server xử lý xong (Exit của client không
                // mang fault_ep ⇒ TCB Inactive ngay, có thể làm rơi message đang xếp hàng).
                let ack = nb_send(SLOT_EP_RESP, info(OP_DONE, 0, false, 1), [0, 0, 0, 0]);
                if ack[0] != 0 {
                    exit(EXIT_REPLY);
                }
                put(b"U: capa server cap/thu hoi xong\n");
                // **Yield trước `Exit`**: `Exit` không mang `fault_ep` làm TCB `Inactive` ngay
                // và message vừa gửi còn xếp hàng có thể bị rơi (D2-25) — nhường CPU để client
                // nhận ack trước, rồi mới thoát ở lượt sau.
                let _ = yield_now();
                exit(0)
            }
            _ => exit(EXIT_REVOKE),
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
