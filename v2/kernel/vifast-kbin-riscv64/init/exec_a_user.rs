// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-74** — chương trình U-mode bước **A** của chuỗi `exec` (A→B→C).
//!
//! Mỗi bước là một **ELF riêng**: `exec(path)` nạp ảnh này **thay** ảnh trước, nên nếu cơ chế
//! sai (không đổi VSpace/entry, rò Frame, giữ thanh ghi cũ) thì TAP/evidence của ảnh test sẽ lộ.
//!
//! In marker qua `DebugPutStr` (syscall 12 — **trap thật**, nên kernel quan sát được `x4`/`tp`
//! của U-mode **tại trap**: bằng chứng TLS phần cứng, không phải kernel tự khai) rồi
//! `Exit(1)` (`FaultKind::Exit{1}`) để kernel biết bước đã chạy xong và chuyển sang
//! `exec` kế tiếp.
#![no_std]
#![no_main]

extern crate vifast_abi;
extern crate vifast_user_abi;
use vifast_user_abi::{call, exit, init_ipc_buffer, put};

/// Giá trị TLS mà bước A đặt qua **invocation** `Tcb.SetTls` (method 15 của `[object].Tcb`).
/// Hằng **đầu vào** của ca kiểm (không phải số đo) — kernel đối chiếu `tcb_tls` **và** `x4`/`tp`
/// quan sát tại trap với chính giá trị này.
const TLS_A: u64 = 0x7431_0001;

/// CPTR của cap TCB **của chính tiến trình** trong cspace của rig ảnh test (slot 1).
const TCB_CPTR: u64 = 1;

/// `MessageInfo` 35 bit: `label` = method (bit 0..19), `length` = số MR (bit 28..34), `caps` = 0.
const fn info(method: u64, len: u64) -> u64 {
    (len << 28) | method
}

/// Method `Tcb.SetTls` — **hằng sinh tự động** từ `spec/abi.toml` `[object].Tcb` (không chép số;
/// D5 reviewer: phía U-mode và phía kernel dùng **cùng** nguồn sinh, không hardcode).
const METHOD_SET_TLS: u64 = vifast_abi::TCB_SETTLS as u64;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init_ipc_buffer();
    // `Call(cap TCB, MessageInfo{label=15,length=1}, MR0 = TLS_A)` ⇒ invocation `Tcb.SetTls`.
    let reply = call(TCB_CPTR, info(METHOD_SET_TLS, 1), [TLS_A, 0, 0, 0]);
    if reply[0] != 0 {
        // **Không** tự in mã lỗi: kernel đã ghi `a0/a1` của syscall 0 (`record_sys`) — evidence
        // `# sys:` của kernel mới là nguồn (D2-102: không viết số tay, không tự khai). Ở đây chỉ
        // cần thoát mã **khác 1** để TAP `tls dat qua invocation` của kernel thấy ngay.
        put(b"# exec: a settls FAIL\n");
        exit(98);
    }
    put(b"# exec: step a up\n");
    exit(1);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
