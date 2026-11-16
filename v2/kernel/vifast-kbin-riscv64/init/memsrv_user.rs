// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-67b** — `memsrv` U-mode **thật** trên ảnh `vifast-kbin-ktest-user`: giữ cap `Untyped`
//! trong CSpace **riêng của mình** (slot 3), tự `Untyped.Retype` ra `Frame` (slot 5), rồi
//! **chuyển cap Frame cho một tiến trình khác** bằng `NBSend` mang cap trong **header IPC
//! buffer** (D2-98/spec `abi.toml` dòng 58): `MessageInfo.caps = 1` + CPTR nguồn ở header.
//!
//! Không `unsafe`, không `asm!` trong tệp này (luật R5): mọi `ecall`/đọc-ghi bộ nhớ đi qua shim
//! `vifast_user_abi` (`kernel/vifast-plat-user/lib.rs`, thuộc danh sách đóng D2-17).
//!
//! Ca **âm** (không được im lặng): `caps = 1` nhưng CPTR nguồn trỏ ô **rỗng** ⇒ kernel phải trả
//! `InvalidCapability` (1) — chứng minh kernel đọc CPTR nguồn từ header, không bỏ qua cap.
//!
//! Ca **mềm** (D2-53 Q6): Frame thứ hai được kernel cấp sẵn nhưng **thiếu `RIGHT_GRANT`** ⇒ gửi
//! được nhưng phía nhận **0 cap** (message vẫn giao) — chứng minh luật chuyển cap vẫn nguyên.
//!
//! Kết thúc bằng `Yield` (**không** `Exit`): `Exit` huỷ mọi message của TCB còn trong hàng
//! Endpoint (D2-25) — memsrv phải sống tới khi client `Recv` xong.
#![no_std]
#![no_main]

extern crate vifast_user_abi;

use vifast_user_abi::{
    call, exit, info, init_ipc_buffer, nb_send, put, set_recv_window, set_send_caps, yield_now,
};

/// Các ô CSpace **riêng của memsrv** (kernel dựng lúc spawn, xem `ktest_capx.rs`).
const SLOT_EP: u64 = 1;
const SLOT_UNTYPED: u64 = 3;
const SLOT_FRAME: u64 = 5;
const SLOT_FRAME_NO_GRANT: u64 = 6;
/// Receive window của memsrv (4 ô rỗng, phân biệt) — header phải có window hợp lệ để kernel
/// nhận header (magic) mà không từ chối IPC.
const WINDOW: [u64; 4] = [8, 9, 10, 11];

/// `Untyped.Retype` — method 0 của object `Untyped` (id 1); type `Frame` = 11 (spec `[object]`).
const METHOD_RETYPE_LABEL: u32 = 0;
const TYPE_FRAME: u64 = 11;
const SIZE_BITS_4K: u64 = 12;

/// Label message (giao thức memsrv↔client của **bài test**, không phải ABI).
const LABEL_GRANT: u32 = 0x670;
const LABEL_NO_GRANT: u32 = 0x671;
const LABEL_EMPTY_CAP: u32 = 0x672;
const LABEL_NO_CAPS: u32 = 0x673;
/// Giá trị MR0 của hai message — client đối chiếu để chắc đúng thứ tự.
const WORD_GRANT: u64 = 0x00C0_FFEE;
const WORD_NO_GRANT: u64 = 0x0000_BEEF;
const WORD_NO_CAPS: u64 = 0x0000_CAFE;

/// Mã thoát khi hỏng (client/kernel đọc để in TAP đỏ).
const EXIT_RETYPE: u64 = 70;
const EXIT_EMPTY_CAP: u64 = 71;
const EXIT_SEND_GRANT: u64 = 72;
const EXIT_SEND_NO_GRANT: u64 = 73;
const EXIT_SEND_NO_CAPS: u64 = 74;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Header IPC buffer: magic + receive window (4 CPTR) — kernel lấy window **từ đây**
    // (`build_message`), không đặt ngoài băng.
    init_ipc_buffer();
    set_recv_window(WINDOW);

    // 1) memsrv tự retype Frame từ cap Untyped **của mình** (invocation 1.0, D2-99).
    let answer = call(
        SLOT_UNTYPED,
        info(METHOD_RETYPE_LABEL, 0, false, 4),
        [SLOT_FRAME, TYPE_FRAME, SIZE_BITS_4K, 0],
    );
    if answer[0] != 0 || answer[1] != SLOT_FRAME {
        exit(EXIT_RETYPE);
    }

    // 2) Ca âm: `caps = 1` nhưng CPTR nguồn (slot 12) **rỗng** ⇒ InvalidCapability 1.
    let _ = set_send_caps(&[12]);
    let answer = nb_send(SLOT_EP, info(LABEL_EMPTY_CAP, 1, false, 1), [0, 0, 0, 0]);
    if answer[0] != 1 {
        exit(EXIT_EMPTY_CAP);
    }

    // 3) Chuyển cap Frame thật (cap có `RIGHT_GRANT`) sang client qua Endpoint.
    let _ = set_send_caps(&[SLOT_FRAME]);
    let answer = nb_send(
        SLOT_EP,
        info(LABEL_GRANT, 1, false, 1),
        [WORD_GRANT, 0, 0, 0],
    );
    if answer[0] != 0 {
        exit(EXIT_SEND_GRANT);
    }

    // 4) Frame thiếu `RIGHT_GRANT` ⇒ chuyển **mềm** (0 cap, message vẫn giao; D2-53 Q6).
    let _ = set_send_caps(&[SLOT_FRAME_NO_GRANT]);
    let answer = nb_send(
        SLOT_EP,
        info(LABEL_NO_GRANT, 1, false, 1),
        [WORD_NO_GRANT, 0, 0, 0],
    );
    if answer[0] != 0 {
        exit(EXIT_SEND_NO_GRANT);
    }

    // 5) Đối chứng: message **không** mang cap (`caps = 0`) ⇒ không chuyển cap nào.
    let answer = nb_send(SLOT_EP, info(LABEL_NO_CAPS, 0, false, 1), [WORD_NO_CAPS, 0, 0, 0]);
    if answer[0] != 0 {
        exit(EXIT_SEND_NO_CAPS);
    }

    put(b"U: memsrv chuyen cap Frame\n");
    // **Không** `Exit`: `Exit` huỷ message đang chờ trong hàng Endpoint (D2-25) — memsrv phải
    // sống tới khi client `Recv` xong. `Yield` trả CPU cho kernel-scheduler của ảnh test.
    let _ = yield_now();
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
