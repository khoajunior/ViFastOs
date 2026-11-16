// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bit trạng thái (`device_status`) và feature của virtio 1.0 — phần **thuần**.
//!
//! Nguồn: Virtio 1.0 §2.1 (device status), §6 (feature bit `VIRTIO_F_VERSION_1` = bit 32).

/// `ACKNOWLEDGE` (bit 0).
pub const ACK: u8 = 1;
/// `DRIVER` (bit 1) — driver biết cách điều khiển thiết bị.
pub const DRIVER: u8 = 2;
/// `DRIVER_OK` (bit 2) — driver sẵn sàng, queue đã cấu hình.
pub const DRIVER_OK: u8 = 4;
/// `FEATURES_OK` (bit 3) — driver đã ghi feature và thiết bị **giữ** bit này.
pub const FEATURES_OK: u8 = 8;
/// `DEVICE_NEEDS_RESET` (bit 6) — thiết bị báo lỗi cần reset.
pub const NEEDS_RESET: u8 = 64;
/// `FAILED` (bit 7) — driver từ bỏ.
pub const FAILED: u8 = 128;

/// `VIRTIO_F_VERSION_1` = feature **bit 32** ⇒ nằm ở `device_feature_select = 1`,
/// mask `1 << (32 - 32)` = 1. Đây là feature duy nhất K6 bắt buộc (modern virtio 1.0).
pub const F_VERSION_1: u32 = 32;
/// `VIRTIO_F_ACCESS_PLATFORM` = bit 33 — **không** đàm phán (không có IOMMU: DMA dùng PA thật).
pub const F_ACCESS_PLATFORM: u32 = 33;

/// Tách một feature bit thành `(device_feature_select, mask)` — bit ≥ 32 nằm ở trang 1.
///
/// Vì sao cần: thanh ghi `device_features` của virtio 1.0 chỉ 32 bit, chọn trang bằng
/// `device_feature_select`; viết sai trang là feature thầm mất (không lỗi).
pub const fn feature_high(bit: u32) -> (u16, u32) {
    if bit >= 32 {
        (1, 1u32 << (bit - 32))
    } else {
        (0, 1u32 << bit)
    }
}

/// Thiết bị **giữ** `FEATURES_OK` và không bật `FAILED` ⇒ đàm phán feature thành công.
pub const fn features_ok(status: u8) -> bool {
    status & FEATURES_OK != 0 && status & FAILED == 0
}

/// Đọc lại `device_status` sau khi ghi `FEATURES_OK`: `true` ⇔ cả `FEATURES_OK` còn và
/// `FAILED` tắt (thiết bị không từ chối bộ feature tối thiểu).
pub const fn negotiate_readback_ok(status: u8) -> bool {
    features_ok(status)
}

/// `true` ⇔ driver đã đi hết chuỗi trạng thái bắt buộc (§2.1: ACK → DRIVER → FEATURES_OK →
/// DRIVER_OK). Dùng để **fail-closed**: queue chỉ dùng khi chuỗi đã đủ.
pub const fn status_sequence_ok(status: u8) -> bool {
    let needed = ACK | DRIVER | FEATURES_OK | DRIVER_OK;
    status & needed == needed && status & FAILED == 0
}
