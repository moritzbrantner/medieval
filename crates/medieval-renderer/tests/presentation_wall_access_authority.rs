//! Acceptance invariant for #125: presentation never decides wall access.
//! Ladder legality, climbing time and whether defenders hold a ladder are
//! consumed only by `medieval-core`; renderer, WASM adapter and browser code
//! only project `TacticalBattle::siege_ladders` (`SiegeLadder::area`,
//! `SiegeLadder::held_by_defender`). This scan is green on the baseline and
//! fails as soon as presentation code re-derives climbing rules.

#[test]
fn presentation_does_not_reference_ladder_climbing_rules() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let forbidden = ["ladder_climb_ticks", "ladderClimbTicks"];
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
                    "{} references core ladder rule `{name}`; presentation only projects \
                     `siege_ladders`",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 0, "the presentation sources were scanned");
}
