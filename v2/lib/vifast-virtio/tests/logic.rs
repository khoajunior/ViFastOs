// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-72 — test host cho **logic thuần** của virtio-blk (không phần cứng, không `unsafe`).
//!
//! Ba nhóm: (A) PCI — BAR (mask → size/kiểu), capability list; (B) split queue — layout byte,
//! descriptor, avail/used ring; (C) virtio-blk — header request, `blk_config`, mã status, feature.

use vifast_virtio::blk::{
    decode_config, flush_negotiated, negotiated_features, status_to_result, BlkError, BlkStatus,
    ReqHeader, BLK_CONFIG_BYTES, FLUSH_MASK, F_FLUSH, S_IOERR, S_OK, S_UNSUPP, T_FLUSH, T_IN,
    T_OUT,
};
use vifast_virtio::conchan;
use vifast_virtio::console;
use vifast_virtio::features::{
    feature_high, features_ok, negotiate_readback_ok, ACK, DRIVER, DRIVER_OK, FAILED, FEATURES_OK,
    F_VERSION_1,
};
use vifast_virtio::pci::{
    bar_probe, find_blk_function, first_conflict, is_virtio_blk, parse_caps, range_overlaps,
    resolve_cap, BarKind, CapType, PciCfg, PciError, VirtioKind, CFG_BAR0, CFG_CAP_PTR,
    CFG_COMMAND, CFG_DEVICE_ID, CFG_STATUS, CFG_VENDOR_ID, COMMAND_BUS_MASTER, COMMAND_MEMORY,
    CONFLICT_OTHER_BAR, CONFLICT_RESERVED, STATUS_CAP_LIST, VENDOR_VIRTIO, VIRTIO_BLK_MODERN,
    VIRTIO_BLK_MODERN_ONLY,
};
use vifast_virtio::queue::{Desc, DescFlags, SplitQueue, VqLayout, DESC_BYTES, VQ_ALIGN};

// ---------------------------------------------------------------- fake config space

/// 256 byte config space của một function PCI, mô phỏng quy tắc BAR của QEMU:
/// ghi `0xffff_ffff` ⇒ đọc lại mask cỡ (bit thuộc tính giữ nguyên); ghi địa chỉ ⇒ lưu địa chỉ.
struct FakeCfg {
    bytes: [u8; 256],
    bar_mask: [u32; 6],
}

impl FakeCfg {
    fn new() -> Self {
        Self {
            bytes: [0u8; 256],
            bar_mask: [0u32; 6],
        }
    }

    fn set16(&mut self, off: u8, value: u16) {
        let i = off as usize;
        self.bytes[i..i + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn set32(&mut self, off: u8, value: u32) {
        let i = off as usize;
        self.bytes[i..i + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn set8(&mut self, off: u8, value: u8) {
        self.bytes[off as usize] = value;
    }
    /// Ghi một vendor capability (16 byte) tại `at`.
    fn cap(&mut self, at: u8, cfg_type: u8, bar: u8, offset: u32, length: u32, next: u8) {
        self.set8(at, 0x09);
        self.set8(at + 1, next);
        self.set8(at + 2, 16);
        self.set8(at + 3, cfg_type);
        self.set8(at + 4, bar);
        self.set32(at + 8, offset);
        self.set32(at + 12, length);
    }
}

impl PciCfg for FakeCfg {
    fn read8(&self, off: u8) -> u8 {
        self.bytes[off as usize]
    }
    fn read16(&self, off: u8) -> u16 {
        let i = off as usize;
        u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]])
    }
    fn read32(&self, off: u8) -> u32 {
        let i = off as usize;
        u32::from_le_bytes([
            self.bytes[i],
            self.bytes[i + 1],
            self.bytes[i + 2],
            self.bytes[i + 3],
        ])
    }
    fn write32(&mut self, off: u8, value: u32) {
        let index = ((off - CFG_BAR0) / 4) as usize;
        if off >= CFG_BAR0 && index < 6 {
            if value == 0xffff_ffff {
                // QEMU: ghi toàn 1 ⇒ BAR giữ bit thuộc tính, phần còn lại là mask cỡ.
                let attr = self.bar_mask[index] & 0xf;
                self.set32(off, (self.bar_mask[index] & !0xf) | attr);
                return;
            }
            self.set32(off, value & !0xf | (self.bar_mask[index] & 0xf));
            return;
        }
        self.set32(off, value);
    }
}

/// Config space của virtio-blk hiện đại: BAR0 = I/O legacy (0x1000), BAR4 = 64-bit
/// prefetchable 0x4000 (common/isr/device/notify, mỗi cái 4 KiB).
fn virtio_blk_cfg() -> FakeCfg {
    let mut c = FakeCfg::new();
    c.set16(CFG_VENDOR_ID, VENDOR_VIRTIO);
    c.set16(CFG_DEVICE_ID, VIRTIO_BLK_MODERN);
    c.set16(CFG_STATUS, STATUS_CAP_LIST); // status bit 4: có capability list
    c.set8(CFG_CAP_PTR, 0x40);
    c.bar_mask = [0xffff_ff01, 0xffff_f00c, 0, 0, 0xffff_c00c, 0];
    c.set32(CFG_BAR0, 0);
    // BAR 4 (offset 0x20) là 64-bit: BAR5 (0x24) là nửa cao, không dùng.
    c.set32(0x20, 0);
    c.set32(0x24, 0);
    // Capability list: common 0x40 → notify 0x50 → device 0x60 → isr 0x70.
    c.cap(0x40, 1, 4, 0x0000, 0x1000, 0x60);
    c.cap(0x60, 2, 4, 0x3000, 0x1000, 0x80);
    c.set8(0x62, 20); // cap_len của notify = 20 (có notify_off_multiplier)
    c.bar_mask[5] = 0; // giữ nguyên; chỉ để rõ BAR5 là nửa cao của BAR4
    c.set32(0x70, 4); // notify_off_multiplier (cap + 16)
    c.cap(0x80, 4, 4, 0x2000, 0x1000, 0xa0);
    c.cap(0xa0, 3, 4, 0x1000, 0x1000, 0);
    c
}

// ---------------------------------------------------------------- (A) PCI

#[test]
fn bar_mask_thanh_co_va_kieu() {
    // BAR 64-bit prefetchable, cỡ 0x4000: ghi 0xffff_ffff vào CẢ HAI dword ⇒ mask
    // 0xffff_ffff_ffff_c00c (đúng thứ tự driver làm khi probe).
    let bar = bar_probe(4, 0xffff_c00c, 0xffff_ffff).expect("mask hop le");
    assert_eq!(bar.kind, BarKind::Memory64);
    assert_eq!(bar.size, 0x4000);
    assert!(bar.prefetchable);
    // BAR I/O cỡ 0x100: mask 0xffff_ff01 ⇒ giữ bit 0 = 1.
    let io = bar_probe(0, 0xffff_ff01, 0).expect("io hop le");
    assert_eq!(io.kind, BarKind::Io);
    assert_eq!(io.size, 0x100);
    // BAR 32-bit memory cỡ 0x1000.
    let mem = bar_probe(1, 0xffff_f000, 0).expect("mem hop le");
    assert_eq!(mem.kind, BarKind::Memory32);
    assert_eq!(mem.size, 0x1000);
    // Mask 0 (không implement BAR) ⇒ None, không panic.
    assert_eq!(bar_probe(2, 0, 0), None);
    // Mask không đúng dạng (0xdead_beef) ⇒ None (không đoán cỡ).
    assert_eq!(bar_probe(3, 0xdead_beef, 0), None);
}

#[test]
fn tim_thiet_bi_va_bar() {
    let cfg = virtio_blk_cfg();
    assert_eq!(
        find_blk_function(&cfg),
        Some(()),
        "phải nhận diện được 0x1af4:0x1001"
    );
    assert!(is_virtio_blk(VENDOR_VIRTIO, VIRTIO_BLK_MODERN));
    assert!(is_virtio_blk(VENDOR_VIRTIO, VIRTIO_BLK_MODERN_ONLY));
    assert!(
        !is_virtio_blk(VENDOR_VIRTIO, 0x1000),
        "0x1000 không phải virtio-blk"
    );
    assert!(
        !is_virtio_blk(0x1b36, 0x0008),
        "host bridge không phải virtio-blk"
    );
}

#[test]
fn capability_du_bon_loai_va_multiplier() {
    let mut cfg = virtio_blk_cfg();
    let caps = parse_caps(&cfg).expect("cap list hop le");
    assert_eq!(caps.common.cap_type, CapType::Common);
    assert_eq!(caps.common.offset, 0x0000);
    assert_eq!(caps.notify.cap_type, CapType::Notify);
    assert_eq!(caps.notify.offset, 0x3000);
    assert_eq!(caps.device.offset, 0x2000);
    assert_eq!(caps.isr.offset, 0x1000);
    assert_eq!(caps.notify_off_multiplier, 4);
    assert_eq!(caps.bar, 4);
    // Địa chỉ tuyệt đối = BAR + offset capability.
    assert_eq!(resolve_cap(&caps.common, 0x4000_0000), 0x4000_0000);
    assert_eq!(resolve_cap(&caps.notify, 0x4000_0000), 0x4000_3000);
    // Cap list gãy (next trỏ vào chính nó) ⇒ Err, không treo vòng lặp.
    cfg.set8(0x40 + 1, 0x40);
    assert!(matches!(parse_caps(&cfg), Err(PciError::CapLoop)));
    // Thiếu capability device ⇒ Err (không đoán offset).
    let mut c2 = virtio_blk_cfg();
    c2.set8(0x80 + 3, 5); // PCI_CFG, không phải DEVICE
    assert!(matches!(parse_caps(&c2), Err(PciError::CapMissing)));
}

#[test]
fn command_bat_memory_va_bus_master() {
    let mut cfg = virtio_blk_cfg();
    assert_eq!(
        cfg.read16(CFG_COMMAND),
        0,
        "config khởi tạo chưa bật decode — driver phải bật"
    );
    let bits = COMMAND_MEMORY | COMMAND_BUS_MASTER;
    cfg.set16(CFG_COMMAND, bits);
    assert_eq!(cfg.read16(CFG_COMMAND) & COMMAND_MEMORY, COMMAND_MEMORY);
    assert_eq!(
        cfg.read16(CFG_COMMAND) & COMMAND_BUS_MASTER,
        COMMAND_BUS_MASTER,
        "DMA cần bus master"
    );
}

// ---------------------------------------------------------------- (B) split queue

#[test]
fn layout_queue_dung_chuan() {
    let l = VqLayout::new(8).expect("size 8 hop le");
    assert_eq!(l.desc_bytes(), 8 * DESC_BYTES);
    assert_eq!(l.avail_off, 128);
    assert_eq!(l.avail_idx_off, 130);
    assert_eq!(l.avail_ring_off, 132);
    assert_eq!(l.used_off % 4, 0, "used ring phải căn 4 byte");
    assert_eq!(l.used_idx_off, l.used_off + 2);
    assert_eq!(l.used_ring_off, l.used_off + 4);
    assert!(l.total_bytes() <= VQ_ALIGN);
    // Số lẻ/không luỹ thừa 2 ⇒ từ chối, không tự làm tròn (đòi hỏi của spec).
    assert!(VqLayout::new(3).is_none());
    assert!(VqLayout::new(0).is_none());
    assert!(VqLayout::new(16).unwrap().total_bytes() <= VQ_ALIGN);
}

#[test]
fn descriptor_ghi_doc_lai() {
    let mut mem = [0u8; VQ_ALIGN];
    let mut q = SplitQueue::new(&mut mem, 8).expect("queue 8");
    assert!(q.set_desc(
        0,
        Desc {
            addr: 0x8040_1000,
            len: 16,
            flags: DescFlags::NEXT,
            next: 1,
        },
    ));
    assert!(q.set_desc(
        1,
        Desc {
            addr: 0x8040_1200,
            len: 512,
            flags: DescFlags::WRITE,
            next: 0,
        },
    ));
    assert_eq!(q.desc(0).addr, 0x8040_1000);
    assert_eq!(q.desc(0).len, 16);
    assert_eq!(q.desc(0).flags, DescFlags::NEXT);
    assert_eq!(q.desc(0).next, 1);
    assert_eq!(q.desc(1).flags, DescFlags::WRITE);
    assert_eq!(q.desc(1).len, 512);
}

#[test]
fn avail_va_used_ring() {
    let mut mem = [0u8; VQ_ALIGN];
    let mut q = SplitQueue::new(&mut mem, 8).expect("queue 8");
    let l = q.layout();
    assert_eq!(q.avail_idx(), 0);
    assert_eq!(q.used_idx(), 0);
    // Publish head 0 hai lần (hai request liên tiếp) — idx một chiều.
    q.push_avail(0);
    q.push_avail(2);
    assert_eq!(q.avail_idx(), 2);
    assert_eq!(q.avail_ring_entry(0), 0);
    assert_eq!(q.avail_ring_entry(1), 2);
    assert_eq!(l.avail_ring_off, l.avail_off + 4);
    // "Thiết bị" ghi used ring cho head 0 (len 512) rồi head 2 (len 0).
    let mut device = SplitQueue::new(&mut mem, 8).expect("queue 8");
    device.write_used(0, 0, 512);
    device.write_used(1, 2, 0);
    let q = SplitQueue::new(&mut mem, 8).expect("queue 8");
    assert_eq!(q.used_idx(), 2);
    assert_eq!(q.used_elem(0), (0, 512));
    assert_eq!(q.used_elem(1), (2, 0));
    // `used->id` khớp head là điều driver phải kiểm; đọc sai thì caller tự phát hiện.
    assert_eq!(q.used_elem(0).0, 0);
}

// ---------------------------------------------------------------- (C) virtio-blk

#[test]
fn header_request_va_flush() {
    let read = ReqHeader::new(T_IN, 7);
    let bytes = read.encode();
    assert_eq!(
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        0
    );
    assert_eq!(
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        7,
        "sector ở byte 8..16 (spec 2.6.4)"
    );
    assert_eq!(ReqHeader::decode(&bytes), read);
    let write = ReqHeader::new(T_OUT, 12345);
    assert_eq!(ReqHeader::decode(&write.encode()), write);
    let flush = ReqHeader::new(T_FLUSH, 0);
    assert_eq!(flush.kind, T_FLUSH);
    assert!(flush.is_flush());
    assert!(!write.is_flush());
}

#[test]
fn blk_config_tu_thiet_bi() {
    let mut raw = [0u8; BLK_CONFIG_BYTES];
    raw[0..8].copy_from_slice(&8192u64.to_le_bytes()); // capacity (sector 512 B)
    raw[8..12].copy_from_slice(&0x20000u32.to_le_bytes()); // size_max
    raw[12..16].copy_from_slice(&126u32.to_le_bytes()); // seg_max
    raw[20..24].copy_from_slice(&512u32.to_le_bytes()); // blk_size
    let c = decode_config(&raw).expect("config 24 byte");
    assert_eq!(c.capacity_sectors, 8192);
    assert_eq!(c.size_max, 0x20000);
    assert_eq!(c.seg_max, 126);
    assert_eq!(c.blk_size, 512);
    assert_eq!(c.capacity_bytes(), 8192 * 512);
}

#[test]
fn status_thanh_ket_qua() {
    assert_eq!(status_to_result(S_OK), Ok(()));
    assert_eq!(status_to_result(S_IOERR), Err(BlkError::Io));
    assert_eq!(status_to_result(S_UNSUPP), Err(BlkError::Unsupported));
    assert_eq!(status_to_result(0x7f), Err(BlkError::Io));
    assert_eq!(BlkError::Io.name(), "Io");
    assert_eq!(BlkError::Unsupported.name(), "Unsupported");
    assert_eq!(BlkError::OutOfRange.name(), "OutOfRange");
    assert_eq!(BlkError::Timeout.name(), "Timeout");
    assert_eq!(BlkStatus::Ok as u8, S_OK);
    // Mã FS: ngoài dải ⇒ `Invalid`, còn lại `Io` — chỗ duy nhất ánh xạ sang `vifast-fs`.
    assert_eq!(BlkError::OutOfRange.to_fs_name(), "Invalid");
    assert_eq!(PciError::NoDevice.name(), "NoDevice");
    assert_eq!(PciError::CapMissing.name(), "CapMissing");
    assert_eq!(PciError::BarConflict.name(), "BarConflict");
    assert_eq!(BlkError::Io.to_fs_name(), "Io");
}

#[test]
fn feature_version_1_va_negotiate() {
    assert_eq!(F_VERSION_1, 32);
    assert_eq!(feature_high(F_VERSION_1), (1, 1));
    assert_eq!(feature_high(F_VERSION_1 + 1), (1, 2));
    assert_eq!(feature_high(0), (0, 1));
    assert_eq!(feature_high(63), (1, 1 << 31));
    assert!(features_ok(FEATURES_OK));
    assert!(
        !features_ok(FEATURES_OK | FAILED),
        "device từ chối ⇒ FAILED"
    );
    assert!(negotiate_readback_ok(
        ACK | DRIVER | FEATURES_OK | DRIVER_OK
    ));
    assert!(!negotiate_readback_ok(ACK | DRIVER | DRIVER_OK));
    assert!(
        !negotiate_readback_ok(FEATURES_OK | FAILED),
        "FAILED ⇒ không hợp lệ"
    );
}

// ------------------------------------------------- KT-72 vá DRIFT (CAO-1/CAO-2 + medium)

#[test]
fn flush_phai_dam_phan_feature() {
    // CAO-1: driver chỉ được gửi `T_FLUSH` sau khi đàm phán `VIRTIO_BLK_F_FLUSH` (bit 9).
    assert_eq!(F_FLUSH, 9);
    assert_eq!(FLUSH_MASK, 0x200);
    // Feature thiết bị đo được trên QEMU: 0x10130006e54 có bit 9.
    let device = 0x0101_3000_6e54_u64;
    assert_eq!(device & FLUSH_MASK as u64, FLUSH_MASK as u64);
    let want = negotiated_features(device);
    assert_eq!(
        want,
        (1u64 << 32) | FLUSH_MASK as u64,
        "phải đàm phán VERSION_1 + FLUSH"
    );
    assert!(flush_negotiated(want));
    // Thiết bị KHÔNG có FLUSH ⇒ chỉ VERSION_1 và driver **không được** gửi FLUSH.
    let no_flush = 1u64 << 32;
    assert_eq!(negotiated_features(no_flush), 1u64 << 32);
    assert!(!flush_negotiated(negotiated_features(no_flush)));
    // Không bao giờ đàm phán bit lạ ngoài hai bit trên.
    assert_eq!(
        negotiated_features(!0u64) & !((1u64 << 32) | FLUSH_MASK as u64),
        0
    );
}

#[test]
fn chong_lan_bar_bi_bat() {
    // CAO-2: gán BAR mà không kiểm chồng lấn = đè im lặng. Hàm thuần phải bắt được.
    assert!(range_overlaps(0x4000_0000, 0x4000, 0x4000_2000, 0x1000));
    assert!(range_overlaps(0x4000_0000, 0x4000, 0x3fff_f000, 0x2000));
    assert!(range_overlaps(0x4000_0000, 0x4000, 0x4000_0000, 0x1000)); // chứa nhau
    assert!(!range_overlaps(0x4000_0000, 0x4000, 0x4000_4000, 0x1000)); // kề nhau = an toàn
    assert!(!range_overlaps(0x4000_0000, 0x4000, 0, 0)); // khoảng rỗng
    let other = [(0x4000_2000u64, 0x1000u64)];
    let reserved = [(0x3000_0000u64, 0x200000u64), (0x1000_0000u64, 0x1000u64)];
    // BAR function khác ⇒ báo other_bar (không phải bị "reserved" che mất)
    assert_eq!(
        first_conflict(0x4000_0000, 0x4000, &other, &reserved),
        Some(CONFLICT_OTHER_BAR)
    );
    // Vùng nền tảng (ECAM/UART) ⇒ reserved
    assert_eq!(
        first_conflict(0x3000_0000, 0x1000, &other, &reserved),
        Some(CONFLICT_RESERVED)
    );
    // Vùng trống, kề BAR khác ⇒ None (không bắt oan)
    assert_eq!(first_conflict(0x4000_4000, 0x4000, &other, &reserved), None);
    // Cỡ 0 là không hợp lệ, không được coi là an toàn
    assert_eq!(
        first_conflict(0x4000_0000, 0, &other, &reserved),
        Some(CONFLICT_RESERVED)
    );
}

#[test]
fn set_desc_ngoai_queue_size_khong_im_lang() {
    let mut mem = [0u8; VQ_ALIGN];
    let mut q = SplitQueue::new(&mut mem, 8).expect("queue 8");
    assert!(q.set_desc(0, Desc::default()));
    assert!(q.set_desc(7, Desc::default()));
    // Ngoài dải ⇒ `false` (D2-110: chain 3 descriptor cần queue_size ≥ 3; không bỏ im lặng)
    assert!(!q.set_desc(8, Desc::default()));
    assert!(!q.set_desc(255, Desc::default()));
}

#[test]
fn blk_size_khac_512_bi_tu_choi() {
    let mut raw = [0u8; BLK_CONFIG_BYTES];
    raw[0..8].copy_from_slice(&8192u64.to_le_bytes());
    raw[20..24].copy_from_slice(&4096u32.to_le_bytes());
    assert_eq!(
        decode_config(&raw),
        None,
        "sector 4 KiB chưa hỗ trợ ⇒ phải từ chối"
    );
    raw[20..24].copy_from_slice(&512u32.to_le_bytes());
    assert!(decode_config(&raw).is_some());
}

// ---------------------------------------------------------------- (D) KT-73 virtio-console

#[test]
fn console_kind_va_id_thiet_bi() {
    // Cùng một hàm quyết định cho cả hai thiết bị ⇒ thêm console không nhân đôi đường quét.
    assert!(VirtioKind::Console.matches(VENDOR_VIRTIO, 0x1003));
    assert!(VirtioKind::Console.matches(VENDOR_VIRTIO, 0x1043));
    assert!(!VirtioKind::Console.matches(VENDOR_VIRTIO, 0x1001));
    assert!(!VirtioKind::Console.matches(0x1234, 0x1003));
    assert!(VirtioKind::Blk.matches(VENDOR_VIRTIO, 0x1001));
    assert!(!VirtioKind::Blk.matches(VENDOR_VIRTIO, 0x1003));
    assert_eq!(VirtioKind::Blk.name(), "blk");
    assert_eq!(VirtioKind::Console.name(), "console");
    assert_eq!(console::negotiated_features(u64::MAX), 1u64 << 32);
    assert!(!console::multiport_negotiated(
        console::negotiated_features(u64::MAX)
    ));
    assert_eq!(console::queue_index(0, false, 0), Some(console::Q_RX));
    assert_eq!(console::queue_index(0, true, 0), Some(console::Q_TX));
    // D2-110B: port > 0 **không** có chỉ số queue khi chưa đàm phán MULTIPORT.
    assert_eq!(console::queue_index(1, true, 0), None);
    assert_eq!(console::queue_index(1, true, 1 << 1), Some(5));
    assert_eq!(console::ConsoleError::MultiPort.name(), "MultiPort");
    assert_eq!(console::rx_outcome(0, 24), Err(console::ConsoleError::Busy));
    // §2.6.8: tx **không** có buffer device-writable ⇒ `used->len` không mang nghĩa số byte đã
    // nhận; QEMU ghi 0. Chỉ cần không vượt số byte driver đã đưa (bằng chứng phân phối ở host).
    assert!(console::tx_used_len_sane(0, 24));
    assert!(console::tx_used_len_sane(24, 24));
    assert!(!console::tx_used_len_sane(25, 24));
}

#[test]
fn conchan_codec_byte_qua_slot_ring() {
    let data = b"help\necho shell-that\n";
    let mut got = Vec::new();
    let mut index = 0usize;
    while index < data.len() {
        let end = (index + conchan::CHUNK_BYTES).min(data.len());
        let (words, count) = conchan::pack(&data[index..end]);
        assert_eq!(count, end - index);
        let mut out = [0u8; conchan::CHUNK_BYTES];
        let n = conchan::unpack(&words, &mut out);
        assert_eq!(n, count);
        got.extend_from_slice(&out[..n]);
        index = end;
    }
    assert_eq!(got, data, "byte qua slot phải đúng thứ tự, không mất");
    assert_eq!(conchan::channel_bytes(), 768);
    assert_eq!(conchan::chunks_for(data.len()), data.len().div_ceil(24));
}
