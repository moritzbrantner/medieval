use serde::{Deserialize, Serialize};

use crate::BattlePoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CombatArc {
    Front,
    Flank,
    Rear,
}

/// An exact integer direction, normalized by its greatest common divisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "FacingWire")]
pub struct Facing {
    x: i64,
    y: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FacingWire {
    x: i64,
    y: i64,
}

impl TryFrom<FacingWire> for Facing {
    type Error = &'static str;

    fn try_from(value: FacingWire) -> Result<Self, Self::Error> {
        Self::new(value.x, value.y).ok_or("facing must be a bounded nonzero direction")
    }
}

impl Facing {
    #[must_use]
    pub const fn east() -> Self {
        Self { x: 1, y: 0 }
    }

    #[must_use]
    pub const fn west() -> Self {
        Self { x: -1, y: 0 }
    }

    #[must_use]
    pub fn new(x: i64, y: i64) -> Option<Self> {
        if (x == 0 && y == 0)
            || x.unsigned_abs() > u64::from(u32::MAX)
            || y.unsigned_abs() > u64::from(u32::MAX)
        {
            return None;
        }
        let mut a = x.unsigned_abs();
        let mut b = y.unsigned_abs();
        while b != 0 {
            let remainder = a % b;
            a = b;
            b = remainder;
        }
        let divisor = i64::try_from(a).expect("bounded direction divisor fits i64");
        Some(Self {
            x: x / divisor,
            y: y / divisor,
        })
    }

    #[must_use]
    pub fn toward(from: BattlePoint, to: BattlePoint) -> Option<Self> {
        Self::new(
            i64::from(to.x_mm) - i64::from(from.x_mm),
            i64::from(to.y_mm) - i64::from(from.y_mm),
        )
    }

    #[must_use]
    pub const fn direction(self) -> [i64; 2] {
        [self.x, self.y]
    }

    /// Front/rear include their 45-degree boundaries; intermediate arcs are flank.
    /// Overlapping anchors have no directional advantage and classify as front.
    #[must_use]
    pub fn classify(self, defender: BattlePoint, attacker: BattlePoint) -> CombatArc {
        let dx = i128::from(attacker.x_mm) - i128::from(defender.x_mm);
        let dy = i128::from(attacker.y_mm) - i128::from(defender.y_mm);
        let dot = i128::from(self.x) * dx + i128::from(self.y) * dy;
        let cross = (i128::from(self.x) * dy - i128::from(self.y) * dx).abs();
        if dot >= cross {
            CombatArc::Front
        } else if dot <= -cross {
            CombatArc::Rear
        } else {
            CombatArc::Flank
        }
    }
}
