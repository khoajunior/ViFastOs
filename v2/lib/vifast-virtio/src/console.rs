// SPDX-License-Identifier: MIT OR Apache-2.0
//! `virtio_console` — **logic thuần** device id, feature, chỉ số queue (§5.3 Virtio 1.0) — KT-73.
//!
//! # Vì sao chỉ hai queue và **không** đàm phán `MULTIPORT` (D2-110B)
//!
//! Virtio 1.0 §5.3.2: thiết bị console có `VIRTIO_CONSOLE_F_MULTIPORT` (bit 1). Nếu driver
//! **không** đàm phán bit này thì thiết bị chỉ có **một** port (port 0) với `rx` = queue 0 và
//! `tx` = queue 1 (§5.3.4). Đàm phán `MULTIPORT` sẽ kéo theo **control queue** (queue 2, 3) và
//! giao thức `VIRTIO_CONSOLE_*_PORT` — KT-73 **không** cần. Luật D2-110B: chỉ dùng lệnh của
//! feature đã đàm phán ⇒ driver (a) **không** gửi lệnh control nào, (b) kiểm
//! [`multiport_negotiated`] **trước** mọi truy cập queue > 1, (c) fail-closed
//! ([`ConsoleError::MultiPort`]) nếu ai đó gọi đường control.
//!
//! Console **không** có byte status như virtio-blk. **`used->len` chỉ có nghĩa cho `rx`** (thiết bị
//! ghi vào buffer device-writable, spec §2.6.8/§5.3.6). `tx` không có buffer đó ⇒ `len` **không**
//! phải số byte đã nhận (QEMU ghi 0) — xem [`tx_used_len_sane`]; đòi `len == want` cho `tx` là
//! **sai spec** (đã tự gặp ở KT-73, D2-110B).
//!
//! Crate thuần: không MMIO, không `unsafe`; host test phủ `negotiated_features`/`queue_index`.

/// `virtio-console` bản transitional (device id 0x1003) — K6 nhắm id này.
pub const VIRTIO_CONSOLE_MODERN: u16 = 0x1003;
/// `virtio-console` bản modern-only (`disable-legacy=on`) — cùng driver.
pub const VIRTIO_CONSOLE_MODERN_ONLY: u16 = 0x1043;

/// `VIRTIO_CONSOLE_F_SIZE` (bit 0) — có `cols`/`rows` trong config. K6 **không** đàm phán.
pub const F_SIZE: u32 = 0;
/// `VIRTIO_CONSOLE_F_MULTIPORT` (bit 1) — nhiều port + control queue. K6 **không** đàm phán.
pub const F_MULTIPORT: u32 = 1;
/// `VIRTIO_F_VERSION_1` (bit 32) — bắt buộc, xem `features::F_VERSION_1`.
pub const F_VERSION_1: u32 = 32;

/// Queue `rx` (host → guest) khi **không** đàm phán `MULTIPORT` (§5.3.4).
pub const Q_RX: u16 = 0;
/// Queue `tx` (guest → host) khi **không** đàm phán `MULTIPORT` (§5.3.4).
pub const Q_TX: u16 = 1;
/// Số queue driver **cần** ở chế độ một port.
pub const QUEUES_REQUIRED: u16 = 2;
/// Cỡ queue tối thiểu dùng được: một descriptor cho mỗi hướng (chain 1 descriptor).
pub const MIN_QUEUE_SIZE: u16 = 2;
/// Số byte tối đa một lần `write` phát trong **một** descriptor (cỡ slot của kênh byte).
pub const TX_CHUNK_BYTES: usize = 24;
/// Số byte `virtio_console_config` tối thiểu (cols 2 + rows 2) — chỉ đọc khi đàm phán `F_SIZE`.
pub const CONSOLE_CONFIG_BYTES: usize = 4;

/// `(vendor, device)` là virtio-console mà KT-73 chấp nhận.
pub const fn is_virtio_console(vendor: u16, device: u16) -> bool {
    vendor == crate::pci::VENDOR_VIRTIO
        && (device == VIRTIO_CONSOLE_MODERN || device == VIRTIO_CONSOLE_MODERN_ONLY)
}

/// Feature driver **chọn** từ `device_features`: **chỉ** `VIRTIO_F_VERSION_1`.
///
/// Vì sao không tự động thêm bit nào khác: mỗi bit thêm là một bộ lệnh/nghĩa vụ mới (§2.2.1).
/// `MULTIPORT` cần control queue + giao thức port — ngoài phạm vi KT-73; `SIZE` chỉ thêm hai
/// trường config driver không dùng. Hàm là **một** chỗ quyết định để "driver có bit X" và "driver
/// dùng đường của X" không lệch nhau (D2-110B).
pub const fn negotiated_features(_device_features: u64) -> u64 {
    1u64 << F_VERSION_1
}

/// `true` ⇔ `driver_features` **đã** có `VIRTIO_CONSOLE_F_MULTIPORT` (được phép dùng control queue
/// và chỉ số queue của port > 0).
pub const fn multiport_negotiated(driver_features: u64) -> bool {
    driver_features & (1u64 << F_MULTIPORT) != 0
}

/// `true` ⇔ `driver_features` đã đàm phán `VIRTIO_CONSOLE_F_SIZE` (được đọc `cols`/`rows`).
pub const fn size_negotiated(driver_features: u64) -> bool {
    driver_features & (1u64 << F_SIZE) != 0
}

/// Chỉ số queue của `port` theo hướng `tx` — `None` nếu cần `MULTIPORT` mà chưa đàm phán.
///
/// Port 0 ở chế độ một port dùng [`Q_RX`]/[`Q_TX`]; port > 0 **chỉ** tồn tại khi đàm phán
/// `MULTIPORT` (queue `2 + 2*port`, `3 + 2*port`) ⇒ trả `None` thay vì đoán chỉ số.
pub const fn queue_index(port: u16, tx: bool, driver_features: u64) -> Option<u16> {
    if port == 0 {
        return Some(if tx { Q_TX } else { Q_RX });
    }
    if !multiport_negotiated(driver_features) {
        return None;
    }
    Some(2 + 2 * port + if tx { 1 } else { 0 })
}

/// Lỗi của driver console — có **tên** để in evidence (D2-102), không in số trần.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleError {
    /// Chưa init / chưa `DRIVER_OK`.
    NotReady,
    /// Queue `rx` chưa có byte nào (mã `Busy = 14`, cùng mã ABI của `NBRecv` rỗng).
    Busy,
    /// Ring/kênh byte **đầy** — backpressure, không ghi đè (mã `NotEnoughMemory = 9`).
    Full,
    /// Thiết bị trả trạng thái lỗi / không hoàn tất request.
    Io,
    /// Cỡ một lần ghi vượt [`TX_CHUNK_BYTES`] hoặc queue quá nhỏ.
    TooLarge,
    /// Đường control/port > 0 khi **chưa** đàm phán `MULTIPORT` (D2-110B) — từ chối rõ.
    MultiPort,
}

impl ConsoleError {
    /// Tên để in evidence.
    pub const fn name(self) -> &'static str {
        match self {
            ConsoleError::NotReady => "NotReady",
            ConsoleError::Busy => "Busy",
            ConsoleError::Full => "Full",
            ConsoleError::Io => "Io",
            ConsoleError::TooLarge => "TooLarge",
            ConsoleError::MultiPort => "MultiPort",
        }
    }

    /// Mã ABI tương ứng của kênh byte (trùng `vifast_ring::ERR_*`, không thêm mã mới).
    pub const fn abi_code(self) -> u32 {
        match self {
            ConsoleError::Full => 9,
            ConsoleError::Busy => 14,
            _ => 0,
        }
    }
}

/// Kết quả một lần `read` từ queue `rx`: số byte thiết bị đã ghi vào đệm (`used->len`).
///
/// Hàm thuần để test host: `Ok(0)` = chưa có dữ liệu (thiết bị hoàn tất một descriptor rỗng);
/// `used->len > len` ⇒ **lỗi** (thiết bị ghi quá đệm ⇒ dữ liệu không tin được).
pub const fn rx_outcome(used_len: u32, buffer_len: u32) -> Result<u32, ConsoleError> {
    if used_len > buffer_len {
        return Err(ConsoleError::Io);
    }
    if used_len == 0 {
        return Err(ConsoleError::Busy);
    }
    Ok(used_len)
}

/// `true` ⇔ `used->len` của **tx** là hợp lệ: `≤` số byte driver đưa vào.
///
/// Vì sao **không** đòi `== want`: Virtio 1.0 §2.6.8 định nghĩa `used.len` là số byte thiết bị
/// ghi vào **device-writable** buffer; queue `tx` của console **không** có buffer device-writable
/// ⇒ `len` không mang nghĩa "đã nhận bao nhiêu byte" và QEMU 9.x ghi **0**. Kỳ vọng `len == 24`
/// từng làm TAP đỏ dù thiết bị đã đẩy đủ byte ra chardev (bằng chứng: tệp `.out` của host có dư
/// 24 byte) — đây là **cùng lớp lỗi D2-110B**: không đòi ngữ nghĩa spec không bảo đảm. Bằng chứng
/// "byte tới host" lấy từ **host** (`ktest.sh` kiểm tệp chardev), không từ `len`.
pub const fn tx_used_len_sane(used_len: u32, want: u32) -> bool {
    used_len <= want
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_chi_version_1() {
        assert_eq!(negotiated_features(0), 1u64 << 32);
        assert_eq!(negotiated_features(u64::MAX), 1u64 << 32);
        assert!(!multiport_negotiated(negotiated_features(u64::MAX)));
        assert!(multiport_negotiated(1u64 << 32 | 1 << 1));
    }

    #[test]
    fn queue_port0_hai_huong() {
        assert_eq!(queue_index(0, false, 0), Some(0));
        assert_eq!(queue_index(0, true, 0), Some(1));
        assert_eq!(queue_index(1, true, 0), None);
        assert_eq!(queue_index(1, true, 1 << 1), Some(5));
        assert_eq!(queue_index(1, false, 1 << 1), Some(4));
    }

    #[test]
    fn rx_outcome_bien() {
        assert_eq!(rx_outcome(0, 24), Err(ConsoleError::Busy));
        assert_eq!(rx_outcome(5, 24), Ok(5));
        assert_eq!(rx_outcome(25, 24), Err(ConsoleError::Io));
        assert!(tx_used_len_sane(0, 6));
        assert!(tx_used_len_sane(6, 6));
        assert!(!tx_used_len_sane(7, 6));
        assert_eq!(ConsoleError::Full.abi_code(), 9);
        assert_eq!(ConsoleError::Busy.abi_code(), 14);
    }
}
