// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-68** — `capa_client` U-mode **tiến trình thứ hai**: **dùng** cap do capalloc cấp, tự
//! dựng **cây 2 tầng** trong CSpace của mình, yêu cầu `Revoke`/`Delete`, rồi **cấp lại cùng
//! slot**.
//!
//! Trình tự đo được (kernel đối chiếu trước/sau, `ktest_capa.rs`):
//! 1. gửi `OP_ALLOC` **kèm cap CNode của chính mình** (header IPC buffer, KT-67b) → nhận slot;
//! 2. `Frame.Map` cap đó vào VSpace **của client**, ghi/đọc 8 B thật, `Frame.Unmap`;
//! 3. `CNode.Copy` (invocation trên cap CNode của chính mình): grant → slot+1 → slot+2 (cháu)
//!    và một **nhánh khác** (EP → slot+3);
//! 4. `OP_REVOKE_DELETE` ⇒ server `Revoke(slot)` (xoá đúng 2 con) + `Delete(slot)` (trả slot);
//!    client kiểm slot+1/slot+2 rỗng (8), nhánh khác **còn** (Delete xoá được);
//! 5. `OP_ALLOC` lần hai ⇒ **cùng slot**; map lại, ghi giá trị thứ hai (kernel đọc physmap);
//! 6. `OP_REVOKE_SOURCE` ⇒ server `Revoke` cap nguồn, xoá **xuyên CNode** cap đã cấp; client
//!    kiểm CPTR cũ ⇒ `SlotEmpty` (8) — không còn cap trỏ Frame.
//!
//! Không `unsafe`, không `asm!` (R5): mọi `ecall`/đọc-ghi bộ nhớ qua shim `vifast_user_abi`.
#![no_std]
#![no_main]

extern crate vifast_user_abi;

use vifast_user_abi::{
    call, exit, info, init_ipc_buffer, nb_recv, nb_send, peek_u64, poke_u64, put, set_recv_window,
    set_send_caps, yield_now, MAX_CAPS,
};

/// Slot CSpace **riêng của client** (kernel dựng lúc spawn).
const SLOT_SELF: u64 = 0;
const SLOT_EP_REQ: u64 = 1;
const SLOT_EP_RESP: u64 = 2;
const SLOT_VSPACE: u64 = 3;
/// Receive window của client (4 CPTR phân biệt, không dùng cap nào) — header hợp lệ.
const WINDOW: [u64; MAX_CAPS] = [12, 13, 14, 15];
/// VA map Frame (VSpace **của client**).
const VA_MAP: u64 = 0x4100_0000;

/// Số method **lấy từ `spec/abi.toml`**: CNode `Copy=0, Delete=4, Revoke=5`; Frame `Map=0`,
/// `Unmap=1` (id 11).
const METHOD_CNODE_COPY: u32 = 0;
const METHOD_CNODE_DELETE: u32 = 4;
const METHOD_FRAME_MAP: u32 = 0;
const METHOD_FRAME_UNMAP: u32 = 1;
/// Cờ `R|W|U` của `Frame.Map` (D2-99).
const FLAGS_RWU: u64 = 0x0b;

/// Op của bài test (khớp `capa_server_user.rs`).
const OP_ALLOC: u32 = 0x680;
const OP_REVOKE_DELETE: u32 = 0x681;
const OP_REVOKE_SOURCE: u32 = 0x682;
const OP_DONE: u32 = 0x683;

const ERR_BUSY: u64 = 14;
const ERR_SLOT_EMPTY: u64 = 8;

/// Giá trị ghi qua vùng đã map — kernel đọc **vật lý** (physmap) để đối chiếu.
const MAGIC: u64 = 0x1122_3344_5566_7788;
const MAGIC2: u64 = 0x8877_6655_4433_2211;

/// Mã thoát khi hỏng.
const EXIT_SEND: u64 = 70;
const EXIT_REPLY: u64 = 71;
const EXIT_MAP: u64 = 72;
const EXIT_RW: u64 = 73;
const EXIT_UNMAP: u64 = 74;
const EXIT_COPY: u64 = 75;
const EXIT_REVOKE: u64 = 76;
const EXIT_LEAK: u64 = 77;
const EXIT_REALLOC: u64 = 78;
const EXIT_STALE: u64 = 79;

/// `NBRecv(EP_RESP)` + `Yield` tới khi có message (không chặn cứng — scheduler tuần tự).
fn wait_reply() -> [u64; 7] {
    loop {
        let answer = nb_recv(SLOT_EP_RESP);
        if answer[0] == ERR_BUSY {
            let _ = yield_now();
            continue;
        }
        return answer;
    }
}

/// Gửi request tới server (label = op, `MR0`/`MR1` tuỳ op).
fn request(op: u32, len: u8, mrs: [u64; 4]) -> [u64; 7] {
    let sent = nb_send(SLOT_EP_REQ, info(op, 0, false, len), mrs);
    if sent[0] != 0 {
        exit(EXIT_SEND);
    }
    wait_reply()
}

/// `CNode.*` trên **cap CNode của chính client** (slot 0). Trả `[error, value]`.
fn cnode(method: u32, len: u8, mrs: [u64; 4]) -> [u64; 7] {
    call(SLOT_SELF, info(method, 0, false, len), mrs)
}

/// `Frame.Map` cap Frame ở `slot` vào VSpace của client.
fn map_frame(slot: u64) {
    let mapped = call(
        slot,
        info(METHOD_FRAME_MAP, 0, false, 3),
        [SLOT_VSPACE, VA_MAP, FLAGS_RWU, 0],
    );
    if mapped[0] != 0 {
        exit(EXIT_MAP);
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init_ipc_buffer();
    set_recv_window(WINDOW);

    // 1) Đăng ký với capalloc: message mang cap CNode **của chính mình** (slot 0).
    let _ = set_send_caps(&[SLOT_SELF]);
    let sent = nb_send(SLOT_EP_REQ, info(OP_ALLOC, 1, false, 0), [0, 0, 0, 0]);
    if sent[0] != 0 {
        exit(EXIT_SEND);
    }
    let reply = wait_reply();
    if reply[0] != 0 {
        exit(EXIT_REPLY);
    }
    let slot = reply[3];
    if slot < 1 || slot >= 16 {
        exit(EXIT_REPLY);
    }

    // 2) **Dùng** cap được cấp: map Frame, ghi/đọc thật, rồi unmap.
    map_frame(slot);
    poke_u64(VA_MAP, MAGIC);
    if peek_u64(VA_MAP) != MAGIC {
        exit(EXIT_RW);
    }
    let unmapped = call(
        slot,
        info(METHOD_FRAME_UNMAP, 0, false, 2),
        [SLOT_VSPACE, VA_MAP, 0, 0],
    );
    if unmapped[0] != 0 {
        exit(EXIT_UNMAP);
    }

    // 3) Cây 2 tầng trong CSpace của client: con slot+1, cháu slot+2, nhánh khác slot+3.
    let child = cnode(METHOD_CNODE_COPY, 2, [slot, slot + 1, 0, 0]);
    if child[0] != 0 {
        exit(EXIT_COPY);
    }
    let grandchild = cnode(METHOD_CNODE_COPY, 2, [slot + 1, slot + 2, 0, 0]);
    if grandchild[0] != 0 {
        exit(EXIT_COPY);
    }
    let sibling = cnode(METHOD_CNODE_COPY, 2, [SLOT_EP_REQ, slot + 3, 0, 0]);
    if sibling[0] != 0 {
        exit(EXIT_COPY);
    }

    // 4) Revoke đúng cây con (2 cap) + Delete trả slot.
    let reply = request(OP_REVOKE_DELETE, 1, [slot, 0, 0, 0]);
    if reply[0] != 0 || reply[3] != 2 || reply[4] != 0 {
        exit(EXIT_REVOKE);
    }
    for freed in [slot + 1, slot + 2] {
        let deleted = cnode(METHOD_CNODE_DELETE, 1, [freed, 0, 0, 0]);
        if deleted[0] != ERR_SLOT_EMPTY {
            exit(EXIT_LEAK);
        }
    }
    let sibling_deleted = cnode(METHOD_CNODE_DELETE, 1, [slot + 3, 0, 0, 0]);
    if sibling_deleted[0] != 0 {
        exit(EXIT_LEAK);
    }

    // 5) Cấp lại ⇒ **cùng slot**; map lại + ghi giá trị thứ hai (kernel đọc physmap).
    let reply = request(OP_ALLOC, 0, [0, 0, 0, 0]);
    if reply[0] != 0 || reply[3] != slot {
        exit(EXIT_REALLOC);
    }
    map_frame(slot);
    poke_u64(VA_MAP, MAGIC2);
    if peek_u64(VA_MAP) != MAGIC2 {
        exit(EXIT_RW);
    }

    // 6) Revoke **cap nguồn** ⇒ xoá xuyên CNode cap đã cấp; CPTR cũ phải rỗng.
    let reply = request(OP_REVOKE_SOURCE, 0, [0, 0, 0, 0]);
    if reply[0] != 0 || reply[3] != 1 {
        exit(EXIT_REVOKE);
    }
    let stale = cnode(METHOD_CNODE_DELETE, 1, [slot, 0, 0, 0]);
    if stale[0] != ERR_SLOT_EMPTY {
        exit(EXIT_STALE);
    }

    let ack = request(OP_DONE, 0, [0, 0, 0, 0]);
    if ack[0] != 0 {
        exit(EXIT_REPLY);
    }
    put(b"U: capa client cap/dung/thu hoi\n");
    exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
