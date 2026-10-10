use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::deployment::siege::{SiegeBattleState, SiegeProfile};
use crate::{
    BattlePoint, BattleSide, BattlefieldLocation, DeploymentZone, FlatBattlefield,
    TACTICAL_TERRAIN_GRID_SIZE, TacticalError, TacticalTerrain, TacticalUnit,
    standard_deployment_zones,
};

// Conservative one-metre soldier slots, plus one metre between formations.
const SOLDIER_SLOT_MM: i64 = 1_000;
const FORMATION_GAP_MM: i64 = 1_000;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TacticalBattlefieldProfile {
    Field {
        location: BattlefieldLocation,
    },
    Siege {
        location: BattlefieldLocation,
        /// Seeds recorded before fortification profiles used stone walls.
        #[serde(default)]
        fortification: SiegeProfile,
    },
}

impl Default for TacticalBattlefieldProfile {
    fn default() -> Self {
        Self::Field {
            location: BattlefieldLocation::MountainPass,
        }
    }
}

/// Campaign-owned context. Missing historical metadata retains the legacy field.
/// `fortified` marks a settlement defined with stone walls; walls built during
/// the campaign are province buildings (see [`crate::Province::fortification_level`]).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProvinceBattlefieldContext {
    pub location: BattlefieldLocation,
    pub fortified: bool,
}

impl ProvinceBattlefieldContext {
    /// The profile from definition data alone: stone walls when fortified.
    #[must_use]
    pub const fn profile(self) -> TacticalBattlefieldProfile {
        self.profile_for_fortification(if self.fortified { 2 } else { 0 })
    }

    /// The profile for a campaign fortification level: a field battle below a
    /// palisade, otherwise a siege with that level's versioned profile.
    #[must_use]
    pub const fn profile_for_fortification(self, level: u8) -> TacticalBattlefieldProfile {
        match SiegeProfile::for_fortification_level(level) {
            Some(fortification) => TacticalBattlefieldProfile::Siege {
                location: self.location,
                fortification,
            },
            None => TacticalBattlefieldProfile::Field {
                location: self.location,
            },
        }
    }
}

impl TacticalBattlefieldProfile {
    pub(crate) const fn location(self) -> BattlefieldLocation {
        match self {
            Self::Field { location } | Self::Siege { location, .. } => location,
        }
    }

    /// The siege state a battle on this profile starts with.
    pub(crate) fn initial_siege(self, battlefield: FlatBattlefield) -> Option<SiegeBattleState> {
        match self {
            Self::Field { .. } => None,
            Self::Siege { fortification, .. } => {
                Some(SiegeBattleState::for_profile(battlefield, fortification))
            }
        }
    }
}

/// Conservative bounds of all soldier slots in the current formation.
/// Signed coordinates also describe sandbox formations extending past an edge.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormationFootprint {
    pub min_x_mm: i64,
    pub max_x_mm: i64,
    pub min_y_mm: i64,
    pub max_y_mm: i64,
}

impl FormationFootprint {
    pub(crate) fn at(unit: &TacticalUnit, point: BattlePoint) -> Self {
        let files = unit.frontage_slots().max(1);
        let width = i64::from(files) * SOLDIER_SLOT_MM;
        let depth = i64::from(unit.soldiers().div_ceil(files)) * SOLDIER_SLOT_MM;
        let x = i64::from(point.x_mm);
        let y = i64::from(point.y_mm);
        Self {
            min_x_mm: x - width / 2,
            max_x_mm: x + width / 2,
            min_y_mm: y - depth / 2,
            max_y_mm: y + depth / 2,
        }
    }

    #[must_use]
    pub fn is_inside(self, zone: DeploymentZone) -> bool {
        self.min_x_mm >= i64::from(zone.min_x_mm)
            && self.max_x_mm <= i64::from(zone.max_x_mm)
            && self.min_y_mm >= i64::from(zone.min_y_mm)
            && self.max_y_mm <= i64::from(zone.max_y_mm)
    }

    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        !self.separated_by(other, 1)
    }

    fn separated_by(self, other: Self, gap: i64) -> bool {
        self.max_x_mm + gap <= other.min_x_mm
            || other.max_x_mm + gap <= self.min_x_mm
            || self.max_y_mm + gap <= other.min_y_mm
            || other.max_y_mm + gap <= self.min_y_mm
    }
}

/// Deterministic greedy packing: back edge first, then increasing ground Y.
/// Candidate boundaries come from occupied rectangles and core-owned obstacles.
/// An army that cannot fit is rejected in full; it is never clipped or overlapped.
pub(crate) fn place_campaign_units(
    battlefield: FlatBattlefield,
    profile: TacticalBattlefieldProfile,
    units: &mut [TacticalUnit],
) -> Result<(), TacticalError> {
    let terrain = TacticalTerrain::for_location(profile.location());
    let siege = profile.initial_siege(battlefield);
    let zones = siege.map_or_else(
        || standard_deployment_zones(battlefield),
        |siege| siege.layout.deployment_zones,
    );
    let mut blocked = terrain_formation_obstacles(battlefield, terrain);
    if let Some(siege) = siege {
        for area in siege
            .layout
            .wall_segments
            .into_iter()
            .chain(std::iter::once(siege.layout.gate))
        {
            blocked.push(FormationFootprint {
                min_x_mm: i64::from(area.min_x_mm),
                max_x_mm: i64::from(area.max_x_mm),
                min_y_mm: i64::from(area.min_y_mm),
                max_y_mm: i64::from(area.max_y_mm),
            });
        }
    }
    let mut placed: Vec<FormationFootprint> = Vec::new();
    for unit in units {
        let zone = zones
            .into_iter()
            .find(|zone| zone.side == unit.side())
            .expect("both deployment sides have zones");
        let size = FormationFootprint::at(unit, BattlePoint::new(0, 0));
        let half_width = size.max_x_mm;
        let half_depth = size.max_y_mm;
        let mut xs = BTreeSet::from([
            i64::from(zone.min_x_mm) + half_width,
            i64::from(zone.max_x_mm) - half_width,
        ]);
        let mut ys = BTreeSet::from([
            i64::from(zone.min_y_mm) + half_depth,
            i64::from(zone.max_y_mm) - half_depth,
        ]);
        for bounds in blocked.iter().chain(placed.iter()) {
            xs.insert(bounds.max_x_mm + FORMATION_GAP_MM + half_width);
            xs.insert(bounds.min_x_mm - FORMATION_GAP_MM - half_width);
            ys.insert(bounds.max_y_mm + FORMATION_GAP_MM + half_depth);
            ys.insert(bounds.min_y_mm - FORMATION_GAP_MM - half_depth);
        }
        let mut xs: Vec<_> = xs.into_iter().collect();
        if unit.side() == BattleSide::Defender {
            xs.reverse();
        }
        let mut selected = None;
        'candidates: for y in ys {
            for &x in &xs {
                let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                let point = BattlePoint::new(x, y);
                let bounds = FormationFootprint::at(unit, point);
                if bounds.is_inside(zone)
                    && blocked.iter().all(|obstacle| !bounds.overlaps(*obstacle))
                    && placed
                        .iter()
                        .all(|other| bounds.separated_by(*other, FORMATION_GAP_MM))
                {
                    selected = Some((point, bounds));
                    break 'candidates;
                }
            }
        }
        let Some((point, bounds)) = selected else {
            return Err(TacticalError::DeploymentFull {
                unit_id: unit.id().to_owned(),
                side: unit.side(),
            });
        };
        unit.set_initial_position(point);
        placed.push(bounds);
    }
    Ok(())
}

pub(crate) fn terrain_formation_obstacles(
    battlefield: FlatBattlefield,
    terrain: TacticalTerrain,
) -> Vec<FormationFootprint> {
    let mut blocked = Vec::new();
    for x in 0..TACTICAL_TERRAIN_GRID_SIZE {
        for y in 0..TACTICAL_TERRAIN_GRID_SIZE {
            if terrain.cell_is_passable(x, y) {
                continue;
            }
            let (x0, x1, y0, y1) = terrain
                .cell_bounds_mm(battlefield, x, y)
                .expect("terrain grid cell is valid");
            if x0 == x1 || y0 == y1 {
                continue;
            }
            blocked.push(FormationFootprint {
                min_x_mm: i64::from(x0),
                max_x_mm: i64::from(if x + 1 == TACTICAL_TERRAIN_GRID_SIZE {
                    x1
                } else {
                    x1 - 1
                }),
                min_y_mm: i64::from(y0),
                max_y_mm: i64::from(if y + 1 == TACTICAL_TERRAIN_GRID_SIZE {
                    y1
                } else {
                    y1 - 1
                }),
            });
        }
    }
    blocked
}
