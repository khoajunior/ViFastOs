// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ngân sách MCS (`SchedContext` budget/period/consumed/refill) — KT-59 port từ
//! `v2/sim/vifast-sim/src/mcs.rs` (oracle).
//!
//! Model-only `[TK]`, **không** phải spec/ABI (D2-49(b,e,h)): chu kỳ **thẳng hàng toàn cục**,
//! **charge trước — refill sau**, không dồn budget, không wrap `u64`. Ngữ nghĩa đúng bốn quy
//! ước của sim:
//!
//! 1. [`ScBudget::configure`] đặt `refill` = **bội số kế tiếp** của `period` tính từ `now`;
//! 2. `Machine::tick` charge TCB đang `Running` cho `[now, now+1)` vào `consumed` của thời
//!    điểm `now`, rồi mới xử lý biên refill tại thời điểm mới;
//! 3. refill đặt `consumed = 0` (phần chưa dùng mất);
//! 4. TCB không SC hoặc SC chưa `SchedControl.Configure` **không có budget** ⇒ không được
//!    `pick`/`dispatch` (D2-50: không SC **không phải** "không giới hạn").
//!
//! **Donation (KT-21)** là phần gắn với `Call`/`Reply` — thuộc KT-60; KT-59 chỉ port công
//! thức [`donated_priority`] + hai trường `donated_to`/`donor` (luôn `None` tới KT-60) để
//! `SchedData`/`TcbData` có sẵn chỗ nối, đúng biên giới task.

/// Số liệu ngân sách của một `SchedContext` — model-only `[TK]`.
///
/// `refill` là **mốc tuyệt đối** (tick ảo) của biên nạp lại kế tiếp; `charged` là tổng tick đã
/// charge từ đầu vòng đời SC (đơn điệu, không reset khi refill).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScBudget {
    budget: u64,
    period: u64,
    consumed: u64,
    refill: u64,
    charged: u64,
}

/// Ảnh chụp `SchedContext` cho test/model API `[TK]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScInfo {
    /// Budget mỗi chu kỳ.
    pub budget: u64,
    /// Chu kỳ.
    pub period: u64,
    /// Đã charge trong chu kỳ hiện tại.
    pub consumed: u64,
    /// Mốc tuyệt đối của biên nạp lại kế tiếp (`0` khi SC chưa cấu hình).
    pub refill: u64,
    /// Tổng tick đã charge từ đầu vòng đời SC.
    pub charged: u64,
    /// `budget > 0 && period > 0`.
    pub configured: bool,
    /// TCB đang nhận donation từ SC này (KT-21; `None` tới KT-60).
    pub donated_to: Option<u32>,
    /// TCB đang cho mượn SC này (KT-21; `None` tới KT-60).
    pub donor: Option<u32>,
}

/// Priority **hiệu lực** của TCB khi chạy bằng SC được donate:
/// `p_eff = min(donor.priority, donor.mcp, donee.mcp)` (KT-21 DoD 6; D2-49(c)).
pub fn donated_priority(donor_priority: u8, donor_mcp: u8, donee_mcp: u8) -> u8 {
    donor_priority.min(donor_mcp).min(donee_mcp)
}

/// Biên thẳng hàng kế tiếp sau `now` cho `period` (bội số toàn cục của `period`).
///
/// Bão hoà ở `u64::MAX` thay vì wrap. `period` do người gọi kiểm dương.
fn next_boundary(now: u64, period: u64) -> u64 {
    debug_assert!(period > 0, "next_boundary: period phải dương");
    let base = now - now % period;
    base.saturating_add(period)
}

impl ScBudget {
    /// Ngân sách chưa cấu hình.
    pub const fn new() -> Self {
        Self {
            budget: 0,
            period: 0,
            consumed: 0,
            refill: 0,
            charged: 0,
        }
    }

    /// `SchedControl.Configure` **hợp lệ**: đặt tham số, `consumed = 0`, `refill` = biên toàn
    /// cục kế tiếp.
    pub fn configure(&mut self, now: u64, budget: u64, period: u64) {
        debug_assert!(period > 0, "configure: period phải dương");
        self.budget = budget;
        self.period = period;
        self.consumed = 0;
        self.refill = next_boundary(now, period.max(1));
    }

    /// `budget > 0 && period > 0` (SC đã được cấu hình ⇒ có giới hạn).
    pub fn is_configured(&self) -> bool {
        self.budget > 0 && self.period > 0
    }

    /// Budget mỗi chu kỳ.
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// Chu kỳ.
    pub fn period(&self) -> u64 {
        self.period
    }

    /// Đã charge trong chu kỳ hiện tại.
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    /// Mốc nạp lại kế tiếp.
    pub fn refill(&self) -> u64 {
        self.refill
    }

    /// Tổng tick đã charge từ đầu vòng đời SC.
    pub fn charged(&self) -> u64 {
        self.charged
    }

    /// Budget còn lại của chu kỳ hiện tại (`0` khi chưa cấu hình).
    pub fn remaining(&self) -> u64 {
        if self.is_configured() {
            self.budget.saturating_sub(self.consumed)
        } else {
            0
        }
    }

    /// TCB dùng SC này có được phép chạy không: **đã cấu hình** và `consumed < budget`.
    pub fn has_budget(&self) -> bool {
        self.is_configured() && self.consumed < self.budget
    }

    /// Charge **một** tick chạy; `false` nếu không charge (chưa cấu hình hoặc đã cạn).
    pub fn charge(&mut self) -> bool {
        if !self.is_configured() || self.consumed >= self.budget {
            return false;
        }
        self.consumed += 1;
        self.charged += 1;
        true
    }

    /// Xử lý biên nạp lại tại `now`: `refill ≤ now` ⇒ `consumed = 0`, đẩy `refill` qua mọi
    /// biên đã trôi, trả `true`.
    pub fn refill_due(&mut self, now: u64) -> bool {
        if !self.is_configured() || self.refill > now {
            return false;
        }
        self.consumed = 0;
        let steps = (now - self.refill) / self.period + 1;
        self.refill = self
            .refill
            .saturating_add(self.period.saturating_mul(steps));
        if self.refill <= now {
            self.refill = u64::MAX;
        }
        true
    }

    /// Ảnh chụp.
    pub fn info(&self) -> ScInfo {
        ScInfo {
            budget: self.budget,
            period: self.period,
            consumed: self.consumed,
            refill: self.refill,
            charged: self.charged,
            configured: self.is_configured(),
            donated_to: None,
            donor: None,
        }
    }

    /// Bất biến ngân sách (`Machine::check_sched`): `consumed ≤ budget`, `budget ≤ period`,
    /// `refill` ở tương lai, SC chưa cấu hình thì mọi số 0.
    pub fn check(&self, now: u64) -> Result<(), crate::CheckFailure> {
        use crate::CheckFailure;
        if !self.is_configured() {
            if self.budget == 0 && self.period == 0 && self.consumed == 0 && self.refill == 0 {
                return Ok(());
            }
            return Err(CheckFailure::one("SchedContext chua cau hinh", 0));
        }
        if self.period == 0 {
            return Err(CheckFailure::one("SchedContext period = 0", 0));
        }
        if self.budget == 0 {
            return Err(CheckFailure::one("SchedContext budget = 0", 0));
        }
        if self.budget > self.period {
            return Err(CheckFailure::two(
                "SchedContext budget > period",
                self.budget.min(u32::MAX as u64) as u32,
                self.period.min(u32::MAX as u64) as u32,
            ));
        }
        if self.consumed > self.budget {
            return Err(CheckFailure::two(
                "SchedContext consumed > budget",
                self.consumed.min(u32::MAX as u64) as u32,
                self.budget.min(u32::MAX as u64) as u32,
            ));
        }
        if self.refill <= now {
            return Err(CheckFailure::two(
                "SchedContext refill <= now",
                self.refill.min(u32::MAX as u64) as u32,
                now.min(u32::MAX as u64) as u32,
            ));
        }
        Ok(())
    }
}

impl Default for ScBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bien_thang_hang_toan_cuc_va_bao_hoa() {
        assert_eq!(next_boundary(0, 4), 4);
        assert_eq!(next_boundary(1, 4), 4);
        assert_eq!(next_boundary(3, 4), 4);
        assert_eq!(next_boundary(4, 4), 8);
        assert_eq!(next_boundary(3, 5), 5);
        assert_eq!(next_boundary(u64::MAX, 7), u64::MAX);
        assert_eq!(next_boundary(u64::MAX - 1, 2), u64::MAX);
    }

    #[test]
    fn charge_va_refill_dung_sim() {
        let mut b = ScBudget::new();
        assert!(!b.has_budget());
        b.configure(0, 4, 8);
        assert_eq!(b.refill(), 8);
        for _ in 0..4 {
            assert!(b.charge());
        }
        assert!(!b.charge());
        assert!(!b.has_budget());
        assert_eq!(b.consumed(), 4);
        assert_eq!(b.charged(), 4);
        assert!(!b.refill_due(7));
        assert!(b.refill_due(8));
        assert_eq!(b.consumed(), 0);
        assert_eq!(b.refill(), 16);
        assert!(b.has_budget());
        assert!(!b.refill_due(15));
        assert!(b.refill_due(17));
        assert_eq!(b.refill(), 24);
        assert!(b.check(17).is_ok());
    }

    #[test]
    fn donated_priority_kep_theo_mcp_hai_ben() {
        assert_eq!(donated_priority(5, 255, 255), 5);
        assert_eq!(donated_priority(9, 7, 255), 7);
        assert_eq!(donated_priority(9, 255, 6), 6);
    }
}
