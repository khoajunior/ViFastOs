// SPDX-License-Identifier: MIT OR Apache-2.0
//! Test host cho `vifast-ring` (KT-69): SPSC, đầy/rỗng, wrap, thứ tự, hai thread thật.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vifast_ring::{
    Mem, Ring, Stat, ERR_EMPTY, ERR_FULL, H_HEAD, H_SEQ, H_TAIL, MSG_WORDS, PAYLOAD_OFF,
};

/// Bộ nhớ giả 4 KiB + nhật ký thứ tự **ghi** (để chứng minh slot ghi trước khi publish head).
struct FakeMem {
    words: [AtomicU64; 512],
    writes: Mutex<Vec<usize>>,
}

impl FakeMem {
    fn new() -> Self {
        Self {
            words: [const { AtomicU64::new(0) }; 512],
            writes: Mutex::new(Vec::new()),
        }
    }

    fn word(&self, offset: usize) -> &AtomicU64 {
        &self.words[offset / 8]
    }

    fn log(&self, offset: usize) {
        self.writes.lock().unwrap().push(offset);
    }
}

/// Newtype để impl `Mem` trên bộ nhớ chia sẻ giữa hai thread (orphan rule).
#[derive(Clone)]
struct Shared(Arc<FakeMem>);

impl Mem for Shared {
    fn load(&self, offset: usize) -> u64 {
        self.0.load(offset)
    }
    fn store(&self, offset: usize, value: u64) {
        self.0.store(offset, value)
    }
    fn acquire(&self, offset: usize) -> u64 {
        self.0.acquire(offset)
    }
    fn store_rel(&self, offset: usize, value: u64) {
        self.0.store_rel(offset, value)
    }
}

impl Mem for FakeMem {
    fn load(&self, offset: usize) -> u64 {
        self.word(offset).load(Ordering::Relaxed)
    }
    fn store(&self, offset: usize, value: u64) {
        self.log(offset);
        self.word(offset).store(value, Ordering::Relaxed);
    }
    fn acquire(&self, offset: usize) -> u64 {
        self.word(offset).load(Ordering::Acquire)
    }
    fn store_rel(&self, offset: usize, value: u64) {
        self.log(offset);
        self.word(offset).store(value, Ordering::Release);
    }
}

fn message(value: u64) -> [u64; MSG_WORDS] {
    [value, !value, value.wrapping_mul(0x9E37_79B9_7F4A_7C15), 0]
}

#[test]
fn init_open_va_bat_bien_header() {
    // Giữ vùng nhớ trong scope (không dùng helper `ring_moi` để tránh leak ngoài ý muốn).
    let mem = FakeMem::new();
    let ring = Ring::new(&mem);
    assert_eq!(ring.open(), Err(ERR_EMPTY), "chưa init ⇒ rỗng");
    let geometry = ring.init(8, 32).expect("init");
    assert_eq!(
        (geometry.size, geometry.mask, geometry.slot_bytes),
        (8, 7, 32)
    );
    assert_eq!(ring.open(), Ok(geometry));
    assert!(ring.check());
    let stats = ring.stats();
    assert_eq!(
        (stats.head, stats.tail, stats.seq, stats.used),
        (0, 0, 0, 0)
    );
}

#[test]
fn rong_tra_busy_khong_chan_va_khong_doi_tail() {
    let mem = FakeMem::new();
    let ring = Ring::new(&mem);
    ring.init(8, 32).unwrap();
    let mut out = [0u64; MSG_WORDS];
    for _ in 0..5 {
        assert_eq!(ring.recv(&mut out), Err(ERR_EMPTY));
    }
    assert_eq!(ring.stats().tail, 0);
    assert!(ring.check());
}

#[test]
fn day_tra_notenoughmemory_khong_ghi_de() {
    let mem = FakeMem::new();
    let ring = Ring::new(&mem);
    ring.init(8, 32).unwrap();
    for value in 0..8u64 {
        ring.send(&message(value)).expect("gửi trong sức chứa");
    }
    assert_eq!(ring.stats().head, 8);
    // Đầy: 9 ⇒ lỗi mã 9, head KHÔNG đổi (không ghi đè slot 0).
    assert_eq!(ring.send(&message(99)), Err(ERR_FULL));
    assert_eq!(ring.stats().head, 8);
    let mut out = [0u64; MSG_WORDS];
    ring.recv(&mut out).unwrap();
    assert_eq!(out[0], 0, "slot 0 còn nguyên giá trị đầu");
    // Có chỗ lại: gửi được và rơi đúng slot vừa trống (index 0 của vòng mới).
    ring.send(&message(8)).unwrap();
    assert_eq!(ring.stats().head, 9);
    assert!(ring.check());
}

#[test]
fn thu_tu_va_wrap_sau_nhieu_vong() {
    let mem = FakeMem::new();
    let ring = Ring::new(&mem);
    ring.init(8, 32).unwrap();
    let total = 40u64;
    let mut sent = 0u64;
    let mut got = 0u64;
    let mut out = [0u64; MSG_WORDS];
    while got < total {
        while sent < total && ring.send(&message(sent)).is_ok() {
            sent += 1;
        }
        while got < sent {
            ring.recv(&mut out).expect("có dữ liệu");
            assert_eq!(out, message(got), "thứ tự/nội dung sai tại seq {got}");
            got += 1;
        }
    }
    let stats = ring.stats();
    assert_eq!(stats.head, total);
    assert_eq!(stats.tail, total);
    assert_eq!(stats.seq, total);
    assert!(
        stats.head / 8 >= 4,
        "phải wrap ≥ 1 vòng (đo được {})",
        stats.head / 8
    );
    assert!(ring.check());
}

#[test]
fn publish_head_sau_khi_ghi_slot() {
    let mem = FakeMem::new();
    let ring = Ring::new(&mem);
    ring.init(8, 32).unwrap();
    mem.writes.lock().unwrap().clear();
    ring.send(&message(7)).unwrap();
    let writes = mem.writes.lock().unwrap().clone();
    let head_pos = writes
        .iter()
        .position(|offset| *offset == H_HEAD)
        .expect("send phải publish head");
    let first_payload = writes
        .iter()
        .position(|offset| *offset >= PAYLOAD_OFF)
        .expect("send phải ghi payload");
    assert!(
        first_payload < head_pos,
        "payload phải ghi TRƯỚC khi publish head: {writes:?}"
    );
    let seq_pos = writes
        .iter()
        .position(|offset| *offset == H_SEQ)
        .expect("send phải cập nhật seq");
    assert!(head_pos < seq_pos, "head publish trước seq: {writes:?}");
    // recv: tail chỉ tăng sau khi đọc slot (không có ghi slot trong recv).
    mem.writes.lock().unwrap().clear();
    let mut out = [0u64; MSG_WORDS];
    ring.recv(&mut out).unwrap();
    let writes = mem.writes.lock().unwrap().clone();
    assert_eq!(writes, vec![H_TAIL], "recv chỉ được ghi tail: {writes:?}");
}

#[test]
fn khoi_quan_sat_mot_ben_ghi_ben_kia_doc() {
    let mem = FakeMem::new();
    let ring = Ring::new(&mem);
    ring.init(8, 32).unwrap();
    assert_eq!(ring.stat(Stat::Magic), vifast_ring::STATS_MAGIC);
    ring.set_stat(Stat::SentOk, 5);
    ring.add_stat(Stat::SentOk, 2);
    assert_eq!(ring.stat(Stat::SentOk), 7);
    assert_eq!(ring.stat(Stat::RecvOk), 0, "trường khác không bị đụng");
}

/// Hai **thread thật**: một producer, một consumer, kiểm không mất/nhân đôi + Acquire/Release
/// (host x86 dùng rào thật; test này chạy dưới `cargo test` bình thường, không cần Miri).
#[test]
fn hai_thread_that_khong_mat_khong_nhan_doi() {
    let shared = Arc::new(FakeMem::new());
    Ring::new(&*shared).init(8, 32).unwrap();
    let total = 2_000u64;
    let done = Arc::new(AtomicU64::new(0));

    let producer = {
        let shared = Shared(Arc::clone(&shared));
        std::thread::spawn(move || {
            let ring = Ring::new(shared);
            let mut sent = 0u64;
            let deadline = Instant::now() + Duration::from_secs(10);
            while sent < total {
                match ring.send(&message(sent)) {
                    Ok(()) => sent += 1,
                    Err(ERR_FULL) => {
                        assert!(Instant::now() < deadline, "producer kẹt vì ring đầy");
                        std::thread::yield_now();
                    }
                    Err(other) => panic!("send lỗi lạ {other}"),
                }
            }
        })
    };
    let consumer = {
        let shared = Shared(Arc::clone(&shared));
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let ring = Ring::new(shared);
            let mut out = [0u64; MSG_WORDS];
            let mut got = 0u64;
            let deadline = Instant::now() + Duration::from_secs(10);
            while got < total {
                match ring.recv(&mut out) {
                    Ok(()) => {
                        assert_eq!(out, message(got), "sai thứ tự/nội dung tại {got}");
                        got += 1;
                    }
                    Err(ERR_EMPTY) => {
                        assert!(Instant::now() < deadline, "consumer kẹt vì ring rỗng");
                        std::thread::yield_now();
                    }
                    Err(other) => panic!("recv lỗi lạ {other}"),
                }
            }
            done.store(got, Ordering::Release);
        })
    };
    producer.join().expect("producer panic");
    consumer.join().expect("consumer panic");
    assert_eq!(done.load(Ordering::Acquire), total);
    let ring = Ring::new(&*shared);
    let stats = ring.stats();
    assert_eq!(stats.head, total);
    assert_eq!(stats.tail, total);
    assert!(ring.check());
}
