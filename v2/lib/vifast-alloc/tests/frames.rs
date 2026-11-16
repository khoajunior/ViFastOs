// SPDX-License-Identifier: MIT OR Apache-2.0
//! Test tích hợp **allocator Frame** (KT-67) — chạy host (D2-42), crate test dùng `std`.
//!
//! Mọi ca bám đúng ngữ nghĩa KT-52/D2-66 mà object layer dựa vào: cửa sổ nhãn PA 1 GiB, cấp
//! **giảm dần** (không đệm căn lề ngầm), hole first-fit, rewind watermark, lỗi nguyên tử và bất
//! biến `live + hole == used`.

use vifast_alloc::{AllocError, FrameArena, FRAME_SIZE_BITS, MAX_HOLES, WINDOW_BYTES};

/// Cửa sổ thử: đáy căn 2 MiB (như `OBJ_PA_BASE_U0 = 0x8200_0000`).
const BASE2M: u64 = 0x8200_0000;

fn arena(capacity: u32) -> FrameArena {
    FrameArena::new(BASE2M, capacity)
}

#[test]
fn ba_co_khop_hang_so_vao_so() {
    assert_eq!(FRAME_SIZE_BITS, [12, 21, 30]);
    assert_eq!(WINDOW_BYTES, 1 << 30);
    let a = arena(WINDOW_BYTES);
    assert_eq!(a.base(), BASE2M);
    assert_eq!(a.capacity(), WINDOW_BYTES);
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.free_bytes(), WINDOW_BYTES);
    assert_eq!(a.live_total(), 0);
    assert_eq!(a.hole_count(), 0);
}

#[test]
fn cap_4k_lien_tiep_va_pa_dung() {
    let mut a = arena(WINDOW_BYTES);
    let f0 = a.alloc(12).expect("frame 4k #0");
    let f1 = a.alloc(12).expect("frame 4k #1");
    assert_eq!(f0.off, 0);
    assert_eq!(f1.off, 4096);
    assert_eq!(a.pa(f0), BASE2M);
    assert_eq!(a.pa(f1), BASE2M + 4096);
    assert_eq!(a.used_bytes(), 8192);
    assert_eq!(a.hole_bytes(), 0);
    assert_eq!(a.live_frames(12), 2);
    assert_eq!(a.live_total(), 2);
    assert_eq!(a.alloc_count(), 2);
    assert_eq!(a.check_accounting(&[(0, 4096), (4096, 4096)]), Ok(()));
}

#[test]
fn ba_co_cap_giam_dan_va_tra_lai() {
    let mut a = arena(WINDOW_BYTES);
    let big = a.alloc(21).expect("frame 2m");
    assert_eq!(big.off, 0);
    let s = a.alloc(12).expect("frame 4k sau 2m");
    assert_eq!(s.off, 1 << 21);
    let t = a.alloc(12).expect("frame 4k ke tiep");
    assert_eq!(t.off, (1 << 21) + 4096);
    // Trả 4 KiB giữa: hole + watermark **không** đổi (hole không chạm mép).
    a.free(s).expect("tra 4k giua");
    assert_eq!(a.used_bytes(), (1 << 21) + 8192);
    assert_eq!(a.hole_count(), 1);
    assert_eq!(
        a.hole(0),
        Some(vifast_alloc::Hole {
            off: 1 << 21,
            len: 4096
        })
    );
    // Trả 4 KiB cuối: hole gộp với hole dưới ⇒ chạm mép ⇒ rewind về 2 MiB, sạch hole.
    a.free(t).expect("tra 4k cuoi");
    assert_eq!(a.used_bytes(), 1 << 21);
    assert_eq!(a.hole_count(), 0);
    assert_eq!(a.live_frames(12), 0);
    assert_eq!(a.live_frames(21), 1);
    // Trả 2 MiB: watermark về 0, hết hole.
    a.free(big).expect("tra 2m");
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.hole_count(), 0);
    assert_eq!(a.live_total(), 0);
    assert_eq!(a.free_bytes(), WINDOW_BYTES);
    assert_eq!(a.check_accounting(&[]), Ok(()));
}

#[test]
fn bump_phai_dem_can_le_tra_alignment_khong_tieu_byte() {
    let mut a = arena(WINDOW_BYTES);
    let f = a.alloc(12).expect("4k dau");
    assert_eq!(f.off, 0);
    // Đáy căn 2 MiB; sau 4 KiB thì bump không căn 2 MiB ⇒ `Alignment` (K2 KT-36), byte giữ nguyên.
    assert_eq!(a.alloc(21), Err(AllocError::Alignment));
    assert_eq!(a.used_bytes(), 4096);
    assert_eq!(a.live_frames(21), 0);
    assert_eq!(a.alloc_count(), 1);
    assert_eq!(a.check_accounting(&[(0, 4096)]), Ok(()));
    // Cấp 4 KiB nữa vẫn được (không cần căn lớn).
    let g = a.alloc(12).expect("4k thu hai");
    assert_eq!(g.off, 4096);
}

#[test]
fn het_cho_tra_not_enough_memory_nguyen_tu() {
    let mut a = arena(1 << 21); // đúng 2 MiB
    let big = a.alloc(21).expect("lap day cua so");
    assert_eq!(big.off, 0);
    assert_eq!(a.used_bytes(), 1 << 21);
    assert_eq!(a.unused_bytes(), 0);
    assert_eq!(a.alloc(12), Err(AllocError::NotEnoughMemory));
    assert_eq!(a.alloc(21), Err(AllocError::NotEnoughMemory));
    assert_eq!(a.used_bytes(), 1 << 21);
    assert_eq!(a.live_total(), 1);
    assert_eq!(a.alloc_count(), 1);
    // Frame lớn hơn cả cửa sổ: `NotEnoughMemory`, không đổi trạng thái.
    assert_eq!(a.alloc(30), Err(AllocError::NotEnoughMemory));
    // Cỡ lạ: `InvalidSize`.
    assert_eq!(a.alloc(13), Err(AllocError::InvalidSize));
    assert_eq!(a.alloc(11), Err(AllocError::InvalidSize));
    a.free(big).expect("tra het");
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.free_bytes(), 1 << 21);
}

#[test]
fn hole_first_fit_tai_dung_cho_va_gop() {
    let mut a = arena(WINDOW_BYTES);
    let f0 = a.alloc(12).unwrap();
    let f1 = a.alloc(12).unwrap();
    let f2 = a.alloc(12).unwrap();
    assert_eq!((f0.off, f1.off, f2.off), (0, 4096, 8192));
    a.free(f1).expect("tra giua");
    // Cấp lại 4 KiB: lấy hole 4 KiB, **không** tiến watermark.
    let again = a.alloc(12).expect("tai dung hole");
    assert_eq!(again.off, 4096);
    assert_eq!(a.used_bytes(), 12288);
    assert_eq!(a.hole_count(), 0);
    assert_eq!(a.live_frames(12), 3);
    assert_eq!(
        a.check_accounting(&[(0, 4096), (4096, 4096), (8192, 4096)]),
        Ok(())
    );
    // Trả frame 0 và frame vừa cấp lại (ở 4 KiB) ⇒ hai hole dính nhau gộp thành một.
    a.free(f0).unwrap();
    a.free(again).unwrap();
    assert_eq!(a.hole_count(), 1);
    assert_eq!(a.hole(0), Some(vifast_alloc::Hole { off: 0, len: 8192 }));
    // Còn sống đúng `f2` (8 KiB..12 KiB) ⇒ `live + hole == used` vẫn đúng.
    assert_eq!(a.check_accounting(&[(8192, 4096)]), Ok(()));
    a.free(f2).unwrap();
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.check_accounting(&[]), Ok(()));
}

#[test]
fn tra_frame_chua_cap_tra_not_allocated() {
    let mut a = arena(WINDOW_BYTES);
    assert_eq!(
        a.free(vifast_alloc::Frame {
            off: 0,
            size_bits: 12
        }),
        Err(AllocError::NotAllocated)
    );
    let f = a.alloc(12).unwrap();
    // Ngoài watermark (chưa từng cấp).
    assert_eq!(
        a.free(vifast_alloc::Frame {
            off: 1 << 20,
            size_bits: 12
        }),
        Err(AllocError::NotAllocated)
    );
    // Cỡ lạ.
    assert_eq!(
        a.free(vifast_alloc::Frame {
            off: 0,
            size_bits: 13
        }),
        Err(AllocError::InvalidSize)
    );
    a.free(f).unwrap();
    // Trả lần hai cùng cỡ: hết Frame sống ⇒ `NotAllocated` (không ghi hole ma).
    assert_eq!(a.free(f), Err(AllocError::NotAllocated));
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.hole_count(), 0);
    assert_eq!(a.free_count(), 1);
}

#[test]
fn cua_so_1g_cap_dung_mot_frame() {
    // Cửa sổ căn 1 GiB (như `OBJ_PA_BASE_U1 = 0x8000_0000`).
    let mut a = FrameArena::new(0x8000_0000, WINDOW_BYTES);
    let f = a.alloc(30).expect("frame 1g");
    assert_eq!(f.off, 0);
    assert_eq!(f.size_bytes(), Some(1 << 30));
    assert_eq!(a.pa(f), 0x8000_0000);
    assert_eq!(a.free_bytes(), 0);
    assert_eq!(a.alloc(30), Err(AllocError::NotEnoughMemory));
    a.free(f).expect("tra 1g");
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.free_bytes(), WINDOW_BYTES);
    let g = a.alloc(30).expect("cap lai 1g");
    assert_eq!(g.off, 0);
}

#[test]
fn rewind_giu_bat_bien_live_plus_hole_bang_used() {
    let mut a = arena(WINDOW_BYTES);
    let f: [vifast_alloc::Frame; 4] = [
        a.alloc(12).unwrap(),
        a.alloc(12).unwrap(),
        a.alloc(12).unwrap(),
        a.alloc(12).unwrap(),
    ];
    // Trả theo thứ tự lộn xộn: bất biến phải đúng sau **mỗi** bước.
    a.free(f[1]).unwrap();
    assert_eq!(
        a.check_accounting(&[(0, 4096), (8192, 4096), (12288, 4096)]),
        Ok(())
    );
    a.free(f[3]).unwrap();
    assert_eq!(a.check_accounting(&[(0, 4096), (8192, 4096)]), Ok(()));
    a.free(f[0]).unwrap();
    assert_eq!(a.check_accounting(&[(8192, 4096)]), Ok(()));
    a.free(f[2]).unwrap();
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.hole_count(), 0);
    assert_eq!(a.check_accounting(&[]), Ok(()));
    // Cấp lại toàn bộ: phải bắt đầu lại từ offset 0.
    for _ in 0..4 {
        let g = a.alloc(12).unwrap();
        assert!(g.off < 4 * 4096);
    }
}

#[test]
fn reset_ve_rong_va_bang_hole_co_bien() {
    let mut a = arena(WINDOW_BYTES);
    // Tạo nhiều hole rời (xen kẽ) — tối đa `MAX_HOLES` rồi bảng đầy trả lỗi, không panic.
    let mut frames = [vifast_alloc::Frame::default(); 2 * MAX_HOLES];
    for f in frames.iter_mut() {
        *f = a.alloc(12).expect("cap 4k");
    }
    for f in frames.iter().step_by(2) {
        a.free(*f).expect("tra so chan");
    }
    assert_eq!(a.hole_count() as usize, MAX_HOLES);
    a.reset();
    assert_eq!(a.used_bytes(), 0);
    assert_eq!(a.hole_count(), 0);
    assert_eq!(a.live_total(), 0);
    assert_eq!(a.alloc_count(), 0);
    assert_eq!(a.free_count(), 0);
    assert_eq!(a.free_bytes(), WINDOW_BYTES);
    assert_eq!(a.check_accounting(&[]), Ok(()));
}

#[test]
fn kiem_ke_toan_bat_duoc_khai_gian() {
    // Bất biến **không** tautology: caller liệt kê Frame sống (như object layer) nên khai thiếu,
    // khai chồng, hay khai ngoài watermark đều bị bắt — không thể "định nghĩa vòng tròn".
    let mut a = arena(WINDOW_BYTES);
    let f0 = a.alloc(12).unwrap();
    let f1 = a.alloc(12).unwrap();
    assert_eq!(
        a.check_accounting(&[(f0.off, 4096), (f1.off, 4096)]),
        Ok(())
    );
    // (a) Quên một Frame ⇒ tổng thiếu ⇒ không phủ kín [0, used).
    assert_eq!(
        a.check_accounting(&[(f0.off, 4096)]),
        Err(AllocError::Corrupt)
    );
    // (b) Hai Frame chồng nhau.
    assert_eq!(
        a.check_accounting(&[(f0.off, 8192), (f1.off, 4096)]),
        Err(AllocError::Corrupt)
    );
    // (c) Frame vượt watermark.
    assert_eq!(
        a.check_accounting(&[(f0.off, 4096), (f1.off, 1 << 21)]),
        Err(AllocError::Corrupt)
    );
    // (d) Frame chồng hole: trả `f1` rồi khai nó vẫn sống.
    a.free(f1).unwrap();
    assert_eq!(
        a.check_accounting(&[(f0.off, 4096), (f1.off, 4096)]),
        Err(AllocError::Corrupt)
    );
    // (e) Danh sách thật (chỉ `f0`) ⇒ xanh.
    assert_eq!(a.check_accounting(&[(f0.off, 4096)]), Ok(()));
}

#[test]
fn bang_hole_day_van_tach_duoc_nguyen_tu() {
    // Bảng hole **đầy** (`MAX_HOLES`) + một hole lớn: cấp 4 KiB từ hole 2 MiB phải tách phần dư
    // (1 insert: 11 còn lại + 1 = 12) — và nếu không đủ chỗ thì **Err nguyên tử**, không "Ok rồi
    // mất hole im lặng" (contract của crate, reviewer-kt67 #5).
    let mut a = arena(WINDOW_BYTES);
    let big = a.alloc(21).expect("frame 2m");
    assert_eq!(big.off, 0);
    let mut small = [vifast_alloc::Frame::default(); 23];
    for f in small.iter_mut() {
        *f = a.alloc(12).expect("cap 4k");
    }
    // Hole lớn [0, 2 MiB) + 11 hole 4 KiB xen kẽ Frame sống ⇒ đúng `MAX_HOLES` hole.
    a.free(big).unwrap();
    // Giữ Frame cuối (index 22) **sống** để hole cuối không chạm watermark (chạm mép ⇒ rewind,
    // đúng ngữ nghĩa, nhưng test cần bảng hole đầy).
    let mut i = 1;
    while i < small.len() {
        a.free(small[i]).unwrap();
        i += 2;
    }
    assert_eq!(a.hole_count() as usize, MAX_HOLES);
    let used_before = a.used_bytes();
    let live_before = a.live_total();
    // Tách hole 2 MiB (1 insert) khi bảng đầy ⇒ vẫn vừa; byte không đổi.
    let f = a.alloc(12).expect("tach hole 2m khi bang day");
    assert_eq!(f.off, 0);
    assert_eq!(a.used_bytes(), used_before);
    assert_eq!(a.live_total(), live_before + 1);
    assert_eq!(a.hole_count() as usize, MAX_HOLES);
    // Bất biến **từ danh sách span** (không tautology): 12 span 4 KiB + hole = watermark.
    let mut spans = [(0u32, 0u32); 32];
    let mut n = 0usize;
    spans[n] = (f.off, 4096);
    n += 1;
    for (i, frame) in small.iter().enumerate() {
        if i % 2 == 0 {
            spans[n] = (frame.off, 4096);
            n += 1;
        }
    }
    assert_eq!(a.check_accounting(&spans[..n]), Ok(()));
    // Bảng hole đầy: trả thêm Frame ⇒ `NoHoles` (không im lặng mất byte) — caller phải xử lý.
    assert_eq!(a.free(f), Err(AllocError::NoHoles));
    assert_eq!(a.check_accounting(&spans[..n]), Ok(())); // trạng thái không đổi
}
