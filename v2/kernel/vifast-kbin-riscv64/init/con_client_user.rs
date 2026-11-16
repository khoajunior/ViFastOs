// SPDX-License-Identifier: MIT OR Apache-2.0
//! **KT-73** — `con_client` U-mode: **consumer** của kênh byte UART → ring (ring của KT-69).
//!
//! Kernel map ring vào VSpace này tại `VA_RING` (0x4300_0000); chương trình **không** nhận cap
//! Frame, không map gì — nó chỉ đọc byte đã có. Mỗi slot 24 byte do kernel (handler UART) ghi;
//! codec `vifast_virtio::conchan` là **cùng một** hàm hai bên dùng (không có layout thứ hai).
//!
//! Bằng chứng: in `# con: u bytes=… sum=… first=… last=… seq=… slots=… slot_bytes=… check=…` —
//! kernel so **nguyên chuỗi** với giá trị nó tính từ payload đã bơm (D2-102: in từ biến đo,
//! không chép số). `seq` (= `head`, số slot producer đã publish) **không** dùng làm kỳ vọng vì
//! số slot phụ thuộc thiết bị gom byte mỗi ngắt (loopback: mỗi byte một ngắt ⇒ 30 slot cho 30 B).
//!
//! Không `unsafe`/`asm!`: mọi truy cập bộ nhớ đi qua shim atomic Acquire/Release của
//! `vifast_user_abi` (cùng shim `ring_common` của KT-69 dùng).
#![no_std]
#![no_main]

extern crate vifast_ring;
extern crate vifast_user_abi;
extern crate vifast_virtio;

use vifast_ring::{Mem, Ring, ERR_EMPTY};
use vifast_user_abi::{
    atomic_load_acquire, atomic_load_relaxed, atomic_store_release, atomic_store_relaxed, exit,
    put,
};
use vifast_virtio::conchan;

/// VA ring trong VSpace của chương trình (kernel map — hằng phải khớp
/// `vifast_plat_riscv64::uart_console::VA_CON_RING`).
const VA_RING: u64 = 0x4300_0000;
/// Số byte tối đa của một slot.
const SLOT: usize = 24;

/// Adapter [`Mem`] trên VA user của chính tiến trình.
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

#[no_mangle]
pub extern "C" fn _start() -> ! {
    put(b"# con: u up\n");
    let ring = Ring::new(Shm { base: VA_RING });
    let geometry = match ring.open() {
        Ok(geometry) => geometry,
        Err(_) => exit(71),
    };
    let mut words = [0u64; 4];
    let mut buffer = [0u8; SLOT];
    let mut bytes = 0u64;
    let mut sum = 0u64;
    let mut first = 0u64;
    let mut last = 0u64;
    loop {
        match ring.recv(&mut words) {
            Ok(()) => {
                let count = conchan::unpack(&words, &mut buffer);
                if count == 0 {
                    exit(72)
                }
                let mut index = 0usize;
                while index < count {
                    let byte = u64::from(buffer[index]);
                    if bytes == 0 {
                        first = byte;
                    }
                    last = byte;
                    sum += byte;
                    bytes += 1;
                    index += 1;
                }
            }
            Err(ERR_EMPTY) => break,
            Err(_) => exit(73),
        }
    }
    let stats = ring.stats();
    put(b"# con: u bytes=");
    put_u64(bytes);
    put(b" sum=");
    put_u64(sum);
    put(b" first=");
    put_u64(first);
    put(b" last=");
    put_u64(last);
    put(b" seq=");
    put_u64(stats.seq);
    put(b" slots=");
    put_u64(u64::from(geometry.size));
    put(b" slot_bytes=");
    put_u64(u64::from(geometry.slot_bytes));
    put(b" check=");
    put_u64(ring.check() as u64);
    put(b"\n");
    exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(99)
}
