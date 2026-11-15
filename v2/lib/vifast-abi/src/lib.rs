// SPDX-License-Identifier: MIT OR Apache-2.0
//! `vifast-abi` — hằng số ABI sinh từ `spec/abi.toml` (KT-05).
//!
//! `generated.rs` do `v2/bin/vifast-gen gen` sinh; **không sửa tay** — `gen-check` trong
//! `./v2/bin/v2 check` so byte và bắt mọi sửa tay.
//!
//! `no_std`: từ KT-58 kernel `no_std` (`vifast-core`) dùng chung hằng số sinh từ spec;
//! test vẫn chạy trên host (std).
#![cfg_attr(not(test), no_std)]

mod generated;

pub use generated::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_syscall_va_error_dung_spec() {
        assert_eq!(SYSCALL_COUNT, 16);
        // KT-62: `min_compat` sinh từ spec (DebugGetInfo trả nó) — không hardcode trong core.
        assert_eq!(MIN_COMPAT, 1);
        // KT-62/D2-88(c): tên + số hiệu syscall sinh từ `[syscall]`, không đọc chép tay.
        assert_eq!(SYS_CALL, 0);
        assert_eq!(SYS_YIELD, 8);
        assert_eq!(SYS_DEBUG_PUT_STR, 12);
        assert_eq!(SYS_EXIT, 14);
        assert_eq!(SYS_DEBUG_GET_INFO, 15);
        assert_eq!(ERROR_COUNT, 19);
        assert_eq!(SYSCALLS[0].0, "Call");
        assert_eq!(SYSCALLS[15].0, "DebugGetInfo");
        assert_eq!(SYSCALLS[15].1, 15);
        assert_eq!(ERRORS[18].1, 19);
    }

    #[test]
    fn chu_ky_method_du_13_va_co_read_state() {
        assert_eq!(METHOD_SIGNATURES.len(), 13);
        let rs = METHOD_SIGNATURES
            .iter()
            .find(|(n, _, _)| *n == "Tcb.ReadState")
            .expect("phải có Tcb.ReadState");
        assert_eq!(rs.2.len(), 4, "ReadState trả 4 giá trị (D2-33)");
        let retype = METHOD_SIGNATURES
            .iter()
            .find(|(n, _, _)| *n == "Untyped.Retype")
            .expect("phải có Untyped.Retype");
        assert!(retype.1.contains(&"type"));
    }

    #[test]
    fn fault_kind_du_bon_bien_the_dung_spec() {
        // KT-56/D2-56a: `spec/abi.toml` mở **một lần**; code sinh phải khớp đúng 4 biến thể.
        assert_eq!(FAULT_KIND_COUNT, 4);
        let names: Vec<&str> = FAULT_KINDS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["Exit", "Vm", "UnknownSyscall", "UserException"]);
        let vm = FAULT_KINDS
            .iter()
            .find(|(n, _)| *n == "Vm")
            .expect("phải có Vm");
        for field in ["vspace: u64", "va: u64", "access", "reason"] {
            assert!(
                vm.1.iter().any(|f| f.contains(field)),
                "Vm thiếu trường {field}"
            );
        }
        assert!(FAULT_KINDS
            .iter()
            .any(|(n, f)| *n == "Exit" && f.contains(&"code: u64")));
    }

    #[test]
    fn kich_thuoc_object_dung_spec_khong_hardcode() {
        // KT-58: `vifast-core` đọc các hằng này thay vì chép tay 48/64/4096 (D2-11).
        assert_eq!(SLOT_BYTES, 48);
        assert_eq!(SLOT_ALIGN_BYTES, 16);
        assert_eq!(CNODE_BASE_BYTES, 64);
        assert_eq!(DEFAULT_ALIGN_BYTES, 16);
        assert_eq!(PAGE_BYTES, 4096);
        assert_eq!(UNTYPED_MIN_BYTES, 4096);
        assert_eq!(OBJ_TCB_BYTES, 512);
        assert_eq!(OBJ_SCHED_CONTEXT_BYTES, 256);
        assert_eq!(OBJ_ENDPOINT_BYTES, 64);
        assert_eq!(OBJ_NOTIFICATION_BYTES, 64);
        assert_eq!(OBJ_REPLY_BYTES, 64);
        assert_eq!(OBJ_SCHED_CONTROL_BYTES, 64);
    }

    #[test]
    fn object_loi_va_reserved_tach_bach() {
        assert!(CORE_OBJECTS
            .iter()
            .any(|(n, id)| *n == "SchedControl" && *id == 8));
        assert!(RESERVED_OBJECTS
            .iter()
            .any(|(n, id)| *n == "VSpace" && *id == 9));
        assert_eq!(TCB_STATES.len(), 11);
    }
}
