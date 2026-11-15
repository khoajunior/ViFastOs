// SPDX-License-Identifier: MIT OR Apache-2.0
//! Test **host** cho ngữ nghĩa U-mode/syscall của KT-53 (`vifast-plat-common::user`).
//!
//! ```text
//! cargo test --manifest-path v2/kernel/Cargo.toml -p vifast-plat-common
//! ```
//!
//! D2-72 Q12: Miri **không** áp kernel `no_std`/asm/MMIO; phần *ngữ nghĩa* syscall (bảng số hiệu
//! ABI, mã lỗi, trạng thái tác vụ, kiểm dải user) đã tách khỏi asm nên chạy được trên host —
//! tệp này kiểm **cùng** mã sẽ chạy trong trap handler U-mode trên QEMU:
//!
//! - 3 syscall của KT-53 (`DebugPutStr` 12, `DebugHalt` 13, `Exit` 14) trả đúng kênh ABI
//!   `a0 = error` / `a1 = value`; `Exit` ghi `exit_code` + trạng thái `Exited`;
//! - syscall **lạ** không panic: `0..=15` chưa cài ⇒ `NotSupported` (18), ngoài dải ⇒
//!   `InvalidArgument` (5); bảng 19 mã lỗi đọc đúng `spec/abi.toml` (chỉ đọc, không sửa `spec/`);
//! - `DebugPutStr` kiểm độ dài/dải user **trước** khi chạm bộ nhớ; lỗi từ lớp đọc user được
//!   truyền nguyên mã (không panic, không in rác).

use vifast_plat_common::user::{
    self, SyscallAction, TaskState, UserMem, UserRange, UserTask, ERR_INVALID_ARGUMENT,
    ERR_NOT_MAPPED, ERR_NOT_SUPPORTED, ERR_RANGE_ERROR, MAX_DEBUG_PUT, SYS_DEBUG_HALT,
    SYS_DEBUG_PUT_STR, SYS_EXIT,
};

/// Lớp đọc user giả: ghi lại lần gọi, trả `len` hoặc mã lỗi cấu hình trước.
struct FakeMem {
    calls: usize,
    last_ptr: u64,
    last_len: u64,
    fail_with: Option<u64>,
}

impl FakeMem {
    fn new() -> Self {
        Self {
            calls: 0,
            last_ptr: 0,
            last_len: 0,
            fail_with: None,
        }
    }
}

impl UserMem for FakeMem {
    fn put_str(&mut self, ptr: u64, len: u64) -> Result<u64, u64> {
        self.calls += 1;
        self.last_ptr = ptr;
        self.last_len = len;
        match self.fail_with {
            Some(code) => Err(code),
            None => Ok(len),
        }
    }
}

fn args(values: &[u64]) -> [u64; 6] {
    let mut out = [0u64; 6];
    out[..values.len().min(6)].copy_from_slice(&values[..values.len().min(6)]);
    out
}

#[test]
fn debug_put_str_tra_kenh_error_value_va_dem_byte() {
    let task = UserTask::new();
    let mut mem = FakeMem::new();
    let action = user::dispatch(SYS_DEBUG_PUT_STR, args(&[0x4000_0000, 26]), &mut mem, &task);
    assert_eq!(
        action,
        SyscallAction::Return {
            error: 0,
            value: 26
        }
    );
    assert_eq!(mem.calls, 1);
    assert_eq!(mem.last_ptr, 0x4000_0000);
    assert_eq!(mem.last_len, 26);
    let snap = task.snapshot();
    assert_eq!(snap.syscalls, 1);
    assert_eq!(snap.last_nr, SYS_DEBUG_PUT_STR);
    assert_eq!(snap.last_error, 0);
    assert_eq!(snap.last_value, 26);
    assert_eq!(snap.put_calls, 1);
    assert_eq!(snap.put_bytes, 26);
}

#[test]
fn debug_put_str_qua_dai_khong_cham_bo_nho() {
    let task = UserTask::new();
    let mut mem = FakeMem::new();
    let action = user::dispatch(
        SYS_DEBUG_PUT_STR,
        args(&[0x4000_0000, MAX_DEBUG_PUT + 1]),
        &mut mem,
        &task,
    );
    assert_eq!(
        action,
        SyscallAction::Return {
            error: ERR_INVALID_ARGUMENT,
            value: 0
        }
    );
    assert_eq!(mem.calls, 0, "phải từ chối trước khi đọc user");
    let snap = task.snapshot();
    assert_eq!(snap.last_error, ERR_INVALID_ARGUMENT);
    assert_eq!(snap.put_calls, 0);
}

#[test]
fn debug_put_str_truyen_loi_lop_doc_user() {
    let task = UserTask::new();
    let mut mem = FakeMem::new();
    mem.fail_with = Some(ERR_NOT_MAPPED);
    let action = user::dispatch(SYS_DEBUG_PUT_STR, args(&[0x4000_0000, 4]), &mut mem, &task);
    assert_eq!(
        action,
        SyscallAction::Return {
            error: ERR_NOT_MAPPED,
            value: 0
        }
    );
    let snap = task.snapshot();
    assert_eq!(snap.last_error, ERR_NOT_MAPPED);
    assert_eq!(snap.put_calls, 0);
}

#[test]
fn exit_luu_ma_va_trang_thai_exited() {
    let task = UserTask::new();
    let mut mem = FakeMem::new();
    let action = user::dispatch(SYS_EXIT, args(&[42]), &mut mem, &task);
    assert_eq!(action, SyscallAction::Exit { code: 42 });
    let snap = task.snapshot();
    assert_eq!(snap.state, TaskState::Exited);
    assert_eq!(snap.exit_code, 42);
    assert_eq!(snap.exits, 1);
    assert_eq!(snap.last_nr, SYS_EXIT);
}

#[test]
fn debug_halt_khong_tra_ma_giu_trang_thai() {
    let task = UserTask::new();
    task.begin_run(false);
    let mut mem = FakeMem::new();
    let action = user::dispatch(SYS_DEBUG_HALT, args(&[7]), &mut mem, &task);
    assert_eq!(action, SyscallAction::Halt { code: 7 });
    // `DebugHalt` kết thúc boot, không phải exit sạch của tác vụ.
    assert_eq!(task.snapshot().state, TaskState::Running);
    assert_eq!(task.snapshot().exits, 0);
}

#[test]
fn syscall_la_khong_panic_va_tra_ma_loi_abi() {
    let task = UserTask::new();
    task.begin_run(false);
    let mut mem = FakeMem::new();
    // Ngoài dải 0..15: RangeError (19, D2-79) — gồm cả EID SBI (U-mode không gọi SBI được).
    for nr in [16u64, 99, 0x5352_5354] {
        let action = user::dispatch(nr, args(&[0, 0]), &mut mem, &task);
        assert_eq!(
            action,
            SyscallAction::Return {
                error: ERR_RANGE_ERROR,
                value: 0
            }
        );
    }
    let snap = task.snapshot();
    assert_eq!(snap.syscalls, 3);
    assert_eq!(snap.bad_calls, 3);
    assert_eq!(snap.last_bad_nr, 0x5352_5354);
    assert_eq!(snap.last_bad_error, ERR_RANGE_ERROR);
    assert_eq!(
        snap.state,
        TaskState::Running,
        "syscall lạ không đổi trạng thái"
    );
}

#[test]
fn syscall_trong_dai_chua_cai_tra_not_supported() {
    let task = UserTask::new();
    let mut mem = FakeMem::new();
    for nr in [0u64, 8, 11, 15] {
        let action = user::dispatch(nr, args(&[]), &mut mem, &task);
        assert_eq!(
            action,
            SyscallAction::Return {
                error: ERR_NOT_SUPPORTED,
                value: 0
            }
        );
    }
    assert_eq!(task.snapshot().bad_calls, 0);
}

#[test]
fn bang_19_ma_loi_doc_dung_ten() {
    assert_eq!(user::error_name(0), "OK");
    assert_eq!(user::error_name(5), "InvalidArgument");
    assert_eq!(user::error_name(18), "NotSupported");
    assert_eq!(user::error_name(19), "RangeError");
    assert_eq!(user::error_name(20), "Unknown");
    for code in 1..=19u64 {
        assert_ne!(user::error_name(code), "Unknown", "mã {code} phải có tên");
    }
}

#[test]
fn user_range_kiem_dai_va_tran() {
    let range = UserRange::new(0x4000_0000, 0x8000_0000);
    assert!(range.contains(0x4000_0000, 0));
    assert!(range.contains(0x4000_0000, 26));
    assert!(range.contains(0x7fff_ffff, 1));
    assert!(!range.contains(0x7fff_ffff, 2));
    assert!(!range.contains(0x3fff_ffff, 1));
    assert!(!range.contains(u64::MAX, 1));
    assert!(!range.contains(0x4000_0000, u64::MAX));
    assert!(range.contains_addr(0x4000_0000));
    assert!(!range.contains_addr(0x8000_0000));
}

#[test]
fn fault_ghi_trang_thai_va_bang_chung() {
    let task = UserTask::new();
    task.record_fault(13, 0x8020_0000, 0x4000_0010);
    let snap = task.snapshot();
    assert_eq!(snap.state, TaskState::Faulted);
    assert_eq!(snap.faults, 1);
    assert_eq!(snap.fault_scause, 13);
    assert_eq!(snap.fault_stval, 0x8020_0000);
    assert_eq!(snap.fault_sepc, 0x4000_0010);
    assert_eq!(TaskState::Faulted.name(), "Faulted");
    assert_eq!(TaskState::from_code(snap.state.code()), TaskState::Faulted);
}

#[test]
fn enter_ghi_spp_cua_lan_vao() {
    let task = UserTask::new();
    task.begin_run(false);
    task.record_ecall(false);
    task.record_enter_sstatus(0x2_0000_0000);
    task.record_trap_sstatus(0x2_0000_6000);
    let snap = task.snapshot();
    assert_eq!(snap.state, TaskState::Running);
    assert_eq!(snap.entries, 1);
    assert_eq!(snap.enter_spp, 0);
    assert_eq!(snap.ecall_spp, 0);
    assert_eq!(snap.ecalls, 1);
    assert_eq!(snap.enter_sstatus, 0x2_0000_0000);
    assert_eq!(snap.trap_sstatus, 0x2_0000_6000);
}
