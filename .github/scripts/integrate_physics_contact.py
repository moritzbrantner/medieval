from pathlib import Path

PHYSICS_REV = "26f75db7c033d9c25e0c16cc88e8e9c6197d3b4c"


def replace_once(text: str, old: str, new: str, description: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{description}: expected one anchor, found {count}")
    return text.replace(old, new)


root = Path("Cargo.toml")
text = root.read_text()
text = replace_once(
    text,
    '[workspace.dependencies]\nserde = { version = "1", features = ["derive"] }\nserde_json = "1"\n',
    '[workspace.dependencies]\n'
    f'physics-engine = {{ git = "https://github.com/moritzbrantner/physics-engine", rev = "{PHYSICS_REV}" }}\n'
    'serde = { version = "1", features = ["derive"] }\n'
    'serde_json = "1"\n',
    "workspace dependency block",
)
root.write_text(text)

core_manifest = Path("crates/medieval-core/Cargo.toml")
text = core_manifest.read_text()
text = replace_once(
    text,
    "[dependencies]\nserde.workspace = true\nserde_json.workspace = true\n",
    "[dependencies]\nphysics-engine.workspace = true\nserde.workspace = true\nserde_json.workspace = true\n",
    "medieval-core dependency block",
)
core_manifest.write_text(text)

tactical = Path("crates/medieval-core/src/tactical.rs")
text = tactical.read_text()
text = replace_once(
    text,
    "use serde::{Deserialize, Serialize};\n",
    "use physics_engine::{Collider, ColliderShape, Vec3i, collider_contact};\n"
    "use serde::{Deserialize, Serialize};\n",
    "tactical import",
)

replacements = [
    (
        """        if let Some(target) = target
            && target.state == TacticalUnitState::Routed
            && point_distance_squared(unit.position, target.position)
                > square_u32(PURSUIT_DISTANCE_MM)
        {
""",
        """        if let Some(target) = target
            && target.state == TacticalUnitState::Routed
            && !points_within_distance(unit.position, target.position, PURSUIT_DISTANCE_MM)
        {
""",
        "pursuit cancellation",
    ),
    (
        """        if unit.engagement_target.is_some()
            && point_distance_squared(unit.position, destination)
                <= square_u32(COMBAT_CONTACT_DISTANCE_MM)
        {
""",
        """        if unit.engagement_target.is_some()
            && points_within_distance(unit.position, destination, COMBAT_CONTACT_DISTANCE_MM)
        {
""",
        "engagement movement stop",
    ),
    (
        """                left.state == TacticalUnitState::Formed
                    && right.state == TacticalUnitState::Formed
                    && point_distance_squared(left.position, right.position)
                        <= square_u32(COMBAT_CONTACT_DISTANCE_MM)
""",
        """                left.state == TacticalUnitState::Formed
                    && right.state == TacticalUnitState::Formed
                    && points_within_distance(
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    )
""",
        "formed contact counting",
    ),
    (
        """            let distance_squared = point_distance_squared(left.position, right.position);

            match (left.state, right.state) {
                (TacticalUnitState::Formed, TacticalUnitState::Formed)
                    if distance_squared <= square_u32(COMBAT_CONTACT_DISTANCE_MM) =>
""",
        """            match (left.state, right.state) {
                (TacticalUnitState::Formed, TacticalUnitState::Formed)
                    if points_within_distance(
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    ) =>
""",
        "formed combat contact",
    ),
    (
        """                (TacticalUnitState::Formed, TacticalUnitState::Routed)
                    if left.engagement_target.as_deref() == Some(right.id.as_str())
                        && distance_squared <= square_u32(PURSUIT_DISTANCE_MM) =>
""",
        """                (TacticalUnitState::Formed, TacticalUnitState::Routed)
                    if left.engagement_target.as_deref() == Some(right.id.as_str())
                        && points_within_distance(
                            left.position,
                            right.position,
                            PURSUIT_DISTANCE_MM,
                        ) =>
""",
        "left pursuit contact",
    ),
    (
        """                (TacticalUnitState::Routed, TacticalUnitState::Formed)
                    if right.engagement_target.as_deref() == Some(left.id.as_str())
                        && distance_squared <= square_u32(PURSUIT_DISTANCE_MM) =>
""",
        """                (TacticalUnitState::Routed, TacticalUnitState::Formed)
                    if right.engagement_target.as_deref() == Some(left.id.as_str())
                        && points_within_distance(
                            left.position,
                            right.position,
                            PURSUIT_DISTANCE_MM,
                        ) =>
""",
        "right pursuit contact",
    ),
]
for old, new, description in replacements:
    text = replace_once(text, old, new, description)

helper_anchor = "fn point_distance_squared(left: BattlePoint, right: BattlePoint) -> u128 {\n"
helper = """/// Uses the shared physics kernel for exact deterministic tactical proximity.
///
/// Tactical positions use the full `u32` battlefield range while the physics kernel's public
/// vectors are compact `i32` coordinates. Contact is translation invariant, so rebase the left
/// point to the local origin and map the battle ground plane onto physics X/Z. The cheap axis
/// rejection keeps every converted delta inside the requested contact radius.
fn points_within_distance(left: BattlePoint, right: BattlePoint, distance_mm: u32) -> bool {
    let radius = i32::try_from(distance_mm).expect("tactical contact radius must fit physics Vec3i");
    let dx = i64::from(right.x_mm) - i64::from(left.x_mm);
    let dz = i64::from(right.y_mm) - i64::from(left.y_mm);
    let distance = u64::from(distance_mm);
    if dx.unsigned_abs() > distance || dz.unsigned_abs() > distance {
        return false;
    }

    let offset = Vec3i::new(
        i32::try_from(dx).expect("contact x delta is bounded by the tactical radius"),
        0,
        i32::try_from(dz).expect("contact z delta is bounded by the tactical radius"),
    );
    collider_contact(
        Collider::new(Vec3i::ZERO, ColliderShape::sphere(radius)),
        Collider::new(offset, ColliderShape::sphere(0)),
    )
    .expect("validated tactical contact geometry must be representable")
    .overlaps()
}

"""
text = replace_once(text, helper_anchor, helper + helper_anchor, "physics contact helper")
text = replace_once(
    text,
    "const fn square_u32(value: u32) -> u128 {\n",
    "#[cfg(test)]\nconst fn square_u32(value: u32) -> u128 {\n",
    "test-only square helper",
)

test_anchor = """    #[test]
    fn setup_validation_rejects_duplicate_ids_and_invalid_units() {
"""
tests = """    #[test]
    fn physics_contact_preserves_tactical_distance_boundaries() {
        let origin = BattlePoint::new(0, 0);
        assert!(points_within_distance(
            origin,
            BattlePoint::new(900, 1_200),
            COMBAT_CONTACT_DISTANCE_MM,
        ));
        assert!(!points_within_distance(
            origin,
            BattlePoint::new(901, 1_200),
            COMBAT_CONTACT_DISTANCE_MM,
        ));
    }

    #[test]
    fn physics_contact_rebases_large_battlefield_coordinates() {
        let edge = BattlePoint::new(u32::MAX, u32::MAX);
        assert!(points_within_distance(
            BattlePoint::new(u32::MAX - 900, u32::MAX - 1_200),
            edge,
            COMBAT_CONTACT_DISTANCE_MM,
        ));
        assert!(!points_within_distance(
            BattlePoint::new(0, 0),
            edge,
            PURSUIT_DISTANCE_MM,
        ));
    }

"""
text = replace_once(text, test_anchor, tests + test_anchor, "physics contact tests")
tactical.write_text(text)

docs = Path("docs/physics-engine-integration.md")
docs.write_text(
    f"""# Physics engine integration

`medieval-core` consumes `physics-engine` from the exact revision `{PHYSICS_REV}`. The integration is deliberately narrow so shared physics replaces duplicate geometry without changing battle rules by accident.

## Ownership boundary

- `medieval-core` remains authoritative for tactical state, movement orders, combat thresholds, morale, fatigue, routing, casualties, and deterministic tick sequencing.
- `physics-engine` is authoritative for reusable integer collision/contact geometry. The first production consumer is melee and pursuit proximity.
- `medieval-renderer` remains projection-only. Camera, GPU geometry, and presentation floats do not become simulation truth.
- Terrain remains renderer-owned until a later gameplay slice explicitly promotes terrain collision into the core contract.

## Coordinate adapter

Tactical `BattlePoint` values use `u32` millimetres and can span a wider absolute range than the physics engine's compact public `Vec3i`. Contact is translation invariant, so the adapter rebases the left tactical point to the physics origin and maps battlefield X/Y to physics X/Z. Axis rejection happens before the checked `i32` conversion, preserving the full tactical battlefield range while keeping contact arithmetic inside the shared engine.

## Deliberate non-goals of this slice

This does not make rigid-body response authoritative for formations, add pathfinding, change fixed-step movement, add individual soldier bodies, or couple terrain height to gameplay. Those are separate contracts that should move only when their gameplay semantics are explicit and covered by deterministic regression tests.

## Next physics consumers

1. Give formations explicit gameplay footprints and use physics sweeps/contact queries to prevent tunnelling and overlap while preserving deterministic order-independent resolution.
2. Add projectiles through the engine's continuous collision detection rather than frame-sampled hit checks.
3. Promote terrain collision to `medieval-core` only when terrain affects movement or combat rules; keep rendering as a pure projection of that authoritative state.
"""
)
