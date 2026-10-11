//! Acceptance invariant for #126: presentation never decides siege-engine
//! damage, range, reload or ammunition. Those rules
//! (`CATAPULT_MIN_RANGE_MM`, `CATAPULT_MAX_RANGE_MM`, `CATAPULT_RELOAD_PULSES`,
//! `CATAPULT_AMMUNITION`, `SiegeProfile::ram_damage_per_pulse`,
//! `SiegeProfile::catapult_gate_damage_per_shot`) are consumed only by
//! `medieval-core`; renderer, WASM adapter and browser code only project
//! `TacticalBattle::siege_engines`. This scan is green on the baseline and
//! fails as soon as presentation code re-derives engine rules.

#[test]
fn presentation_does_not_reference_siege_engine_rules() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let forbidden = [
        "CATAPULT_MIN_RANGE_MM",
        "CATAPULT_MAX_RANGE_MM",
        "CATAPULT_RELOAD_PULSES",
        "CATAPULT_AMMUNITION",
        "ram_damage_per_pulse",
        "catapult_gate_damage_per_shot",
        "catapultMinRangeMm",
        "catapultMaxRangeMm",
        "catapultReloadPulses",
        "catapultAmmunition",
        "ramDamagePerPulse",
        "catapultGateDamagePerShot",
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
                    "{} references core siege-engine rule `{name}`; presentation only projects \
                     `siege_engines`",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 0, "the presentation sources were scanned");
}
