//! Acceptance invariant for #124: presentation never decides siege tower
//! targets or casualties. Tower range and casualty rules
//! (`SiegeProfile::tower_range_mm`, `SiegeProfile::tower_max_casualties_per_pulse`)
//! are consumed only by `medieval-core`; renderer, WASM adapter and browser code
//! only project `TacticalBattle::siege_tower_activity`. This scan is green on
//! the baseline and fails as soon as presentation code re-derives tower combat.

/// Presentation code must not re-derive tower targeting: the range and
/// casualty parameters are consumed only by the core.
#[test]
fn presentation_does_not_reference_tower_combat_rules() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let forbidden = [
        "tower_range_mm",
        "tower_max_casualties_per_pulse",
        "towerRangeMm",
        "towerMaxCasualtiesPerPulse",
    ];
    let mut scanned = 0;
    for directory in ["crates/medieval-renderer/src", "web-battle-wasm/src", "web"] {
        let mut pending = vec![root.join(directory)];
        while let Some(path) = pending.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).unwrap() {
                    let entry = entry.unwrap().path();
                    if entry.file_name().is_some_and(|name| {
                        name == "vendor" || name == "generated-assets" || name == "node_modules"
                    }) {
                        continue;
                    }
                    pending.push(entry);
                }
                continue;
            }
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                continue;
            };
            if !matches!(extension, "rs" | "js" | "mjs" | "ts") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            scanned += 1;
            for name in forbidden {
                assert!(
                    !source.contains(name),
                    "{} references core tower combat rule `{name}`; presentation only projects \
                     `siege_tower_activity`",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 0, "the presentation sources were scanned");
}
