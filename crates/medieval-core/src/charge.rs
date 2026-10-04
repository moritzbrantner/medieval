use serde::{Deserialize, Serialize};

pub(crate) const CHARGE_RUN_UP_MM: u32 = 4_000;
pub(crate) const CHARGE_RECOVERY_TICKS: u16 = 40;

/// Core-owned cavalry momentum. Contact impact is consumed by one combat pulse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CavalryChargeState {
    Ready,
    Approaching { run_up_mm: u32 },
    Charging { run_up_mm: u32 },
    Contact { run_up_mm: u32 },
    Recovering { ticks_remaining: u16 },
}

impl CavalryChargeState {
    pub(crate) const fn run_up_mm(self) -> u32 {
        match self {
            Self::Approaching { run_up_mm }
            | Self::Charging { run_up_mm }
            | Self::Contact { run_up_mm } => run_up_mm,
            Self::Ready | Self::Recovering { .. } => 0,
        }
    }

    pub(crate) const fn interrupted(self) -> Self {
        match self {
            Self::Ready | Self::Recovering { .. } => self,
            _ => Self::Recovering {
                ticks_remaining: CHARGE_RECOVERY_TICKS,
            },
        }
    }
}
