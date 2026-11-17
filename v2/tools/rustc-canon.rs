// SPDX-License-Identifier: MIT OR Apache-2.0
//! KT-79 (OD-K6-3) — `RUSTC_WRAPPER` cho build tái lập byte (bản POSIX của `repro-build.sh`).
//!
//! Hai nguồn khác sha **không phải** mã nguồn, mà là siêu dữ liệu do cargo/rustc sinh từ
//! **đường dẫn tuyệt đối** của chỗ checkout (log: `JOURNAL/kt79-machines.log`):
//!
//! 1. **Chuỗi panic location** (`file!()`): rustc nhúng đường dẫn nguồn **nguyên văn**; trên
//!    Windows là `\`, trên macOS/Linux là `/`. Wrapper chuẩn hoá `\` → `/` cho mọi đối số là tệp
//!    `.rs` và cho `--remap-path-prefix=…`; phần tiền tố tuyệt đối do script build remap tiếp.
//! 2. **`-C metadata=…`**: cargo băm đường dẫn tuyệt đối (và RUSTFLAGS) vào hash crate, nên cùng
//!    mã nguồn ở hai chỗ khác nhau cho hash khác nhau. Hash này quyết định **crate nào sở hữu**
//!    một monomorphization dùng chung (đo được: `core::fmt::Display::fmt` thuộc `vifast_pte` hay
//!    `vifast_core` tuỳ bản) ⇒ thứ tự hàm trong `.text` đổi ⇒ sha đổi. Wrapper thay **mọi**
//!    `-C metadata=…` bằng một hằng số ⇒ hash crate không còn phụ thuộc đường dẫn.
//!
//! Không `unsafe`. Đặt qua `RUSTC_WRAPPER` (cargo gọi `rustc-canon <rustc> <args…>`):
//!   rustc -O v2/tools/rustc-canon.rs -o <tmp>/rustc-canon
//!   RUSTC_WRAPPER=<tmp>/rustc-canon cargo build …
//!
//! Env: `VIFAST_REPRO_BREAK=1` ⇒ **không** sửa gì (ca âm; phải ra sha khác).

use std::env;
use std::process::Command;

/// Hằng số thay cho metadata do cargo sinh (hash đường dẫn/RUSTFLAGS). Tên có tiền tố `vifast-`
/// để thấy ngay trong symbol khi đọc bản không strip.
const METADATA: &str = "metadata=vifast-kt79-repro";

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<String>>();
    if args.len() < 2 {
        eprintln!("rustc-canon: cần <rustc> <args…> (đặt qua RUSTC_WRAPPER)");
        std::process::exit(2);
    }
    let rustc = args.remove(0);
    let break_mode = env::var_os("VIFAST_REPRO_BREAK").is_some();

    let mut out: Vec<String> = Vec::with_capacity(args.len() + 2);
    for a in args {
        let mut a = a;
        if !break_mode {
            // 1) dấu phân cách đường dẫn cho nguồn và remap.
            if a.ends_with(".rs") || a.starts_with("--remap-path-prefix=") {
                a = a.replace('\\', "/");
            }
            // 2) metadata cố định (cargo truyền `-C metadata=…` hoặc `-Cmetadata=…`).
            if a.starts_with("metadata=") {
                a = METADATA.to_string();
            } else if a.starts_with("-Cmetadata=") {
                a = format!("-C{}", METADATA);
            }
        }
        out.push(a);
    }

    let status = match Command::new(&rustc).args(&out).status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("rustc-canon: không chạy được {rustc}: {e}");
            std::process::exit(2);
        }
    };
    std::process::exit(status.code().unwrap_or(1));
}
