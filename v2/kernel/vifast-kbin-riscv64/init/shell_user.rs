// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-65 — **shell tối thiểu** chạy U-mode (Rust, ELF riêng nạp như `init`).
//!
//! **KT-73**: đọc **byte thật** từ kênh UART → ring của KT-69. Kernel nhận **ngắt RX của UART**,
//! đẩy byte vào ring (`vifast_ring`, slot 24 B do `vifast_virtio::conchan` mã hoá) và map Frame
//! đó vào VSpace này tại `VA_CON_RING = 0x4300_0000`; shell `Ring::recv` từng slot, ghép dòng theo
//! `\n` rồi chạy lệnh. **Không** còn message `LINE` do kernel cắm sẵn — shell đọc đúng đường
//! IRQ → RBR → ring → U-mode (đóng nợ "shell chưa UART IRQ" từ K3.5).
//!
//! Lệnh thật:
//! - `help`  — in trợ giúp (do **shell** in, không phải kernel).
//! - `echo <text>` — in đúng phần text.
//! - `ps`    — gửi `PS_REQ`, nhận bảng TCB **do kernel đọc từ `Machine`** rồi tự định dạng/in.
//! - `wait <pid>` / `kill <pid>` — gọi **giao thức procsrv** (`NBSend(proc_ep)` + `NBRecv(reply_ep)`),
//!   in mã trả về. Không phải syscall mới.
//! - `exit`  — `Exit(0)`.
//!
//! Vòng lặp polling + `Yield`: kernel là scheduler tuần tự của pha demo (không có scheduler
//! U-mode trong core; xem `docs/k35-procsrv.md` §2).
#![no_std]
#![no_main]


extern crate vifast_ring;
extern crate vifast_user_abi;
extern crate vifast_virtio;
use vifast_ring::{Mem, Ring, ERR_EMPTY};
use vifast_user_abi::{
    atomic_load_acquire, atomic_load_relaxed, atomic_store_release, atomic_store_relaxed, exit,
    put, syscall7, SYS_EXIT, SYS_NB_RECV, SYS_NB_SEND, SYS_YIELD,
};
use vifast_virtio::conchan;

// Cspace dùng chung của ảnh demo (kernel dựng; xem `proc_demo.rs`).
const EP_PROC: u64 = 2;
const EP_REPLY: u64 = 4;
const EP_PS: u64 = 5;
const EP_EVENTS: u64 = 8;

/// VA ring UART → shell (kernel map; khớp `vifast_plat_riscv64::uart_console::VA_CON_RING`).
const VA_CON_RING: u64 = 0x4300_0000;
/// Cỡ dòng tối đa shell nhận (script demo ≤ 24 B + `\n`).
const MAX_LINE: usize = 32;
/// Số lệnh shell biết (in trong `help`) — **một** chỗ để `cmds=` không lệch với danh sách.
const CMDS: u64 = 6;

/// Adapter [`Mem`] trên VA user (cùng shim `ring_common` của KT-69).
struct Shm {
    base: u64,
}

impl Mem for Shm {
    fn load(&self, offset: usize) -> u64 {
        atomic_load_relaxed(self.base + offset as u64)
    }
    fn store(&self, offset: usize, value: u64) {
        atomic_store_relaxed(self.base + offset as u64, value);
    }
    fn acquire(&self, offset: usize) -> u64 {
        atomic_load_acquire(self.base + offset as u64)
    }
    fn store_rel(&self, offset: usize, value: u64) {
        atomic_store_release(self.base + offset as u64, value);
    }
}

const PS_REQ: u64 = 0x602;
const PS_REPLY: u64 = 0x603;
const WAIT_REQ: u64 = 0x604;
const KILL_REQ: u64 = 0x605;
const REPLY_WAIT: u64 = 0x606;
const REPLY_KILL: u64 = 0x607;
const EV_HELP: u64 = 0x507;
const EV_ECHO: u64 = 0x506;
const EV_PS: u64 = 0x505;

const INFO_LEN_SHIFT: u64 = 28;

/// In hex (không `unsafe`, không `alloc`) — để evidence in từ **hằng**, không viết tay chuỗi.
fn put_hex(mut value: u64) {
    let digits = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut i = buf.len();
    if value == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while value > 0 {
        i -= 1;
        buf[i] = digits[(value & 0xf) as usize];
        value >>= 4;
    }
    put(&buf[i..]);
}

fn put_u64(mut value: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if value == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while value > 0 {
        i -= 1;
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    put(&buf[i..]);
}

fn yield_cpu() {
    // Ghi chú: syscall 8 không tham số; trong pha đa nhiệm kernel dùng làm điểm nhường CPU.
    
    let _ = syscall7(SYS_YIELD, 0, 0, 0, 0, 0, 0);

}

fn send(ep: u64, label: u64, mr: [u64; 4]) {
    let words = if mr[3] != 0 {
        4
    } else if mr[2] != 0 {
        3
    } else if mr[1] != 0 {
        2
    } else if mr[0] != 0 {
        1
    } else {
        0
    };
    let info = label | (words << INFO_LEN_SHIFT);
    // Ghi chú: syscall 2 với cap Endpoint trong cspace của chính tiến trình.
    
    let _ = syscall7(SYS_NB_SEND, ep, info, mr[0], mr[1], mr[2], mr[3]);

}

fn recv(ep: u64) -> (u64, u64, [u64; 4]) {
    // Ghi chú: syscall 4 với cap Endpoint; kernel trả info a2 + MR a3..a6.
    let r =  syscall7(SYS_NB_RECV, ep, 0, 0, 0, 0, 0) ;
    (r[0], r[2], [r[3], r[4], r[5], r[6]])
}

fn starts_with(line: &[u8], prefix: &[u8]) -> bool {
    line.len() >= prefix.len() && &line[..prefix.len()] == prefix
}

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(value)
}

fn state_name(code: u64) -> &'static [u8] {
    match code {
        0 => b"Inactive",
        1 => b"Ready",
        2 => b"Running",
        3 => b"BlockedOnSend",
        4 => b"BlockedOnRecv",
        5 => b"BlockedOnReply",
        6 => b"BlockedOnNotification",
        7 => b"BlockedOnAddress",
        8 => b"Sleeping",
        9 => b"Stopping",
        10 => b"Dead",
        _ => b"Unknown",
    }
}

/// Chờ **một** reply có đúng `want` label; `Busy`/message khác ⇒ `Yield` rồi thử lại.
fn wait_reply(want: u64) -> (u64, [u64; 4]) {
    loop {
        let (err, info, mr) = recv(EP_REPLY);
        if err == 0 && info & 0xFFFFF == want {
            return (info & 0xFFFFF, mr);
        }
        yield_cpu();
    }
}

fn cmd_ps() {
    send(EP_PS, PS_REQ, [0, 0, 0, 0]);
    let (_, mr) = wait_reply(PS_REPLY);
    let mut list = [0u64; 8];
    let mut count = 0usize;
    for word in mr {
        for half in 0..2 {
            let entry = if half == 0 { word & 0xFFFF } else { word >> 16 };
            if entry != 0 && count < list.len() {
                list[count] = entry;
                count += 1;
            }
        }
    }
    put(b"# shell: ps n=");
    put_u64(count as u64);
    put(b"\n");
    for entry in list.iter().take(count) {
        let pid = entry >> 5;
        let state = (entry >> 1) & 0x1F;
        put(b"  pid=");
        put_u64(pid);
        put(b" state=");
        put(state_name(state));
        if entry & 1 != 0 {
            put(b" suspended");
        }
        put(b"\n");
    }
    send(EP_EVENTS, EV_PS, [count as u64, 0, 0, 0]);
}

fn cmd_wait(pid: u64) {
    send(EP_PROC, WAIT_REQ, [pid, 0, 0, 0]);
    let (_, mr) = wait_reply(REPLY_WAIT);
    put(b"# shell: wait pid=");
    put_u64(mr[0]);
    put(b" code=");
    put_u64(mr[1]);
    put(b"\n");
}

fn cmd_kill(pid: u64) {
    send(EP_PROC, KILL_REQ, [pid, 0, 0, 0]);
    let (_, mr) = wait_reply(REPLY_KILL);
    put(b"# shell: kill pid=");
    put_u64(mr[0]);
    put(b" code=");
    put_u64(mr[1]);
    put(b"\n");
}

fn run_line(line: &[u8]) {
    if starts_with(line, b"echo ") {
        put(b"# shell: echo ");
        put(&line[5..]);
        put(b"\n");
        send(EP_EVENTS, EV_ECHO, [line.len() as u64, 0, 0, 0]);
    } else if line == b"help" {
        put(b"# shell: help cmds=");
        put_u64(CMDS);
        put(b"\n");
        put(b"  help | echo <text> | ps | wait <pid> | kill <pid> | exit\n");
        send(EP_EVENTS, EV_HELP, [CMDS, 0, 0, 0]);
    } else if line == b"ps" {
        cmd_ps();
    } else if starts_with(line, b"wait ") {
        if let Some(pid) = parse_u64(&line[5..]) {
            cmd_wait(pid);
        }
    } else if starts_with(line, b"kill ") {
        if let Some(pid) = parse_u64(&line[5..]) {
            cmd_kill(pid);
        }
    } else if line == b"exit" {
        put(b"# shell: exit code=0\n");
        // Kernel ghi `FaultKind::Exit{0}` vào fault_ep của shell (procsrv nhận).
        exit(0);
    } else {
        put(b"# shell: ?\n");
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    put(b"# shell: up cmds=help/echo/ps/wait/kill/exit\n");
    put(b"# shell: input=ring va=0x");
    put_hex(VA_CON_RING as u64);
    put(b"\n");
    // KT-77b: prompt của shell. In **một dòng riêng** (`shell>\n`, không phải tiền tố `shell> `
    // cùng dòng) để mọi dòng kết quả `# shell: …` vẫn bắt đầu ở cột 0 — kịch bản parity
    // (`spec/v1-parity.toml`, `expect` dạng `^# shell: …$`) so khớp theo dòng.
    put(b"shell>\n");
    let ring = Ring::new(Shm {
        base: VA_CON_RING,
    });
    // Ring do **kernel** khởi tạo (header KT-69); shell chỉ mở/kiểm.
    if ring.open().is_err() {
        put(b"# shell: ring open FAIL\n");
        exit(90)
    }
    let mut words = [0u64; 4];
    let mut line = [0u8; MAX_LINE];
    let mut len = 0usize;
    loop {
        match ring.recv(&mut words) {
            Ok(()) => {
                let mut slot = [0u8; 24];
                let count = conchan::unpack(&words, &mut slot);
                if count == 0 {
                    put(b"# shell: slot hong\n");
                    exit(91)
                }
                for byte in &slot[..count] {
                    if *byte == b'\n' {
                        run_line(&line[..len]);
                        len = 0;
                        // Prompt sau **mỗi** dòng lệnh (kể cả dòng sai) — trừ khi `run_line`
                        // đã `exit` (không trả về).
                        put(b"shell>\n");
                    } else if len < MAX_LINE {
                        line[len] = *byte;
                        len += 1;
                    } else {
                        // Dòng quá dài: bỏ phần dư, không tràn buffer (báo để không im lặng).
                        put(b"# shell: dong qua dai\n");
                    }
                }
            }
            Err(ERR_EMPTY) => yield_cpu(),
            Err(_) => {
                put(b"# shell: ring loi\n");
                exit(92)
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // Panic U-mode không phục hồi; thoát 99 để kernel đỏ (không giả xanh).
    exit(99)
}
