// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-67b** — `client` U-mode **tiến trình thứ hai** trên ảnh `vifast-kbin-ktest-user`.
//!
//! Không có cap `Frame` nào trong CSpace lúc spawn. `Recv` trên Endpoint **của chính nó** để
//! nhận message mang cap từ `memsrv`; cap được kernel cài vào **receive window trong header IPC
//! buffer của client** (4 CPTR, spec `abi.toml` dòng 58). Sau đó client dùng **chính cap nhận
//! được** để `Frame.Map` → ghi/đọc thật 8 B → `Frame.Unmap` → map lại + ghi giá trị thứ hai để
//! kernel đọc **vật lý** (physmap) làm bằng chứng độc lập.
//!
//! Không `unsafe`, không `asm!` (R5): mọi `ecall`/đọc-ghi bộ nhớ qua shim `vifast_user_abi`.
#![no_std]
#![no_main]

extern crate vifast_user_abi;

use vifast_user_abi::{
    call, exit, info, init_ipc_buffer, peek_u64, poke_u64, put, read_received_caps, recv,
    set_recv_window, REPLY_NONE,
};

/// Các ô CSpace **riêng của client** (kernel dựng lúc spawn, xem `ktest_capx.rs`).
const SLOT_EP: u64 = 1;
const SLOT_VSPACE: u64 = 2;
/// Ô đầu của receive window — nơi kernel cài cap Frame nhận được từ memsrv.
const SLOT_RECV: u64 = 4;
/// Receive window của client (4 ô rỗng, phân biệt) — ghi vào header IPC buffer.
const WINDOW: [u64; 4] = [4, 5, 6, 7];
/// VA client map Frame nhận được (VSpace **của client**, không phải của memsrv).
const VA_MAP: u64 = 0x4100_0000;
/// `Frame.Map` — method 0 của object `Frame` (id 11); cờ `R|W|U` = 0xb.
const METHOD_MAP: u32 = 0;
/// `Frame.Unmap` — method 1.
const METHOD_UNMAP: u32 = 1;
const FLAGS_RWU: u64 = 0x0b;

/// Giá trị ghi qua vùng vừa map (kernel đọc lại bằng physmap).
const MAGIC: u64 = 0x1122_3344_5566_7788;
const MAGIC2: u64 = 0x8877_6655_4433_2211;

/// Label message của bài test (không phải ABI).
const LABEL_GRANT: u32 = 0x670;
const LABEL_NO_GRANT: u32 = 0x671;
const LABEL_NO_CAPS: u32 = 0x673;
const WORD_GRANT: u64 = 0x00C0_FFEE;
const WORD_NO_GRANT: u64 = 0x0000_BEEF;
const WORD_NO_CAPS: u64 = 0x0000_CAFE;

/// Mã thoát khi hỏng.
const EXIT_RECV1: u64 = 80;
const EXIT_CAPS1: u64 = 81;
const EXIT_MR1: u64 = 82;
const EXIT_MAP1: u64 = 83;
const EXIT_RW1: u64 = 84;
const EXIT_UNMAP: u64 = 85;
const EXIT_MAP2: u64 = 86;
const EXIT_RW2: u64 = 87;
const EXIT_RECV2: u64 = 88;
const EXIT_CAPS2: u64 = 89;
const EXIT_MR2: u64 = 90;
const EXIT_RECV3: u64 = 91;
const EXIT_CAPS3: u64 = 92;
const EXIT_MR3: u64 = 93;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init_ipc_buffer();
    set_recv_window(WINDOW);

    // 1) Nhận message mang cap Frame từ memsrv (cap cài vào window → cspace **của client**).
    let answer = recv(SLOT_EP, REPLY_NONE);
    if answer[0] != 0 {
        exit(EXIT_RECV1);
    }
    // `answer = [error, value, MessageInfo, MR0..MR3]` — ghi MessageInfo vào header để
    // `read_received_caps()` đọc số cap **thật sự** cài được.
    let raw_info = answer[2];
    vifast_user_abi::note_received(raw_info);
    let caps = read_received_caps();
    if caps.count != 1 || caps.slots[0] != SLOT_RECV {
        exit(EXIT_CAPS1);
    }
    if answer[3] != WORD_GRANT {
        exit(EXIT_MR1);
    }

    // 2) Map Frame **nhận được** vào VSpace của client, bằng cap ở cspace của client.
    let map = call(
        SLOT_RECV,
        info(METHOD_MAP, 0, false, 3),
        [SLOT_VSPACE, VA_MAP, FLAGS_RWU, 0],
    );
    if map[0] != 0 {
        exit(EXIT_MAP1);
    }
    poke_u64(VA_MAP, MAGIC);
    if peek_u64(VA_MAP) != MAGIC {
        exit(EXIT_RW1);
    }

    // 3) `Frame.Unmap` bằng **chính** cap nhận được, rồi map lại + ghi giá trị thứ hai
    //    (kernel đọc PA thấy MAGIC2 ⇒ chứng minh mapping của client trỏ đúng Frame).
    let unmap = call(
        SLOT_RECV,
        info(METHOD_UNMAP, 0, false, 2),
        [SLOT_VSPACE, VA_MAP, 0, 0],
    );
    if unmap[0] != 0 {
        exit(EXIT_UNMAP);
    }
    let map = call(
        SLOT_RECV,
        info(METHOD_MAP, 0, false, 3),
        [SLOT_VSPACE, VA_MAP, FLAGS_RWU, 0],
    );
    if map[0] != 0 {
        exit(EXIT_MAP2);
    }
    poke_u64(VA_MAP, MAGIC2);
    if peek_u64(VA_MAP) != MAGIC2 {
        exit(EXIT_RW2);
    }

    // 4) Đối chứng: cap **thiếu RIGHT_GRANT** ⇒ message giao nhưng **0 cap** (D2-53 Q6).
    let answer = recv(SLOT_EP, REPLY_NONE);
    if answer[0] != 0 {
        exit(EXIT_RECV2);
    }
    vifast_user_abi::note_received(answer[2]);
    if read_received_caps().count != 0 {
        exit(EXIT_CAPS2);
    }
    if answer[3] != WORD_NO_GRANT {
        exit(EXIT_MR2);
    }

    // 5) Đối chứng: message `caps = 0` ⇒ không chuyển cap nào.
    let answer = recv(SLOT_EP, REPLY_NONE);
    if answer[0] != 0 {
        exit(EXIT_RECV3);
    }
    vifast_user_abi::note_received(answer[2]);
    if read_received_caps().count != 0 {
        exit(EXIT_CAPS3);
    }
    if answer[3] != WORD_NO_CAPS {
        exit(EXIT_MR3);
    }

    put(b"U: client nhan cap va map/ghi/doc\n");
    exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
