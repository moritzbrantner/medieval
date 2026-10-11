//! Acceptance invariant for #127: presentation never decides siege objective
//! capture, unlocking or victory. Capture progress per pulse, contention and
//! the staged unlock rule are consumed only by `medieval-core`
//! (`SiegeBattleState::advance_capture` and its constant
//! `SIEGE_CAPTURE_PROGRESS_PER_PULSE`); renderer, WASM adapter and browser
//! code only project `TacticalBattle::siege_objectives`. This scan is green on
//! the baseline and fails as soon as presentation code re-derives capture.

#[test]
fn presentation_does_not_reference_objective_capture_rules() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let forbidden = [
        "advance_capture",
        "SIEGE_CAPTURE_PROGRESS_PER_PULSE",
        "siegeCaptureProgressPerPulse",
        "capture_point.contains",
        "capturePoint.contains",
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
                    "{} references core capture rule `{name}`; presentation only projects \
                     `siege_objectives`",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 0, "the presentation sources were scanned");
}
