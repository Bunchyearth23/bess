# BESS 0.10.0 fleet delivery — 2026-09-23

## Delivered locally

New Automation imports use the independent descriptor-guided sound path. Six character presets are available in the application: Balanced, Muted, Open, Warm, Mechanical and Grit. Four generated sound controls adjust exhaust body, exhaust edge, flow texture and mechanical detail. Saved projects keep their previous generated or source-guided mode. The character export now renders six generated interpretations and the Automation reference.

All twelve Automation ZIPs in the local `cars` directory were exported with the 0.10.0 Windows release executable to `output/bess-0.10.0-fleet-20260923`. Their twelve new selectable add-ons were installed in `D:\BeamMP\current\mods` beside the twelve original Automation vehicles. The prior twelve BESS add-ons were moved to `output/beamng-bess-backup-before-0.10.0-20260923`. The original vehicle ZIP hashes were checked against their sources before installation; the installed new add-on hashes were checked against the export manifest after copying. BeamNG was not running at installation time, so the next game launch will load these files.

## Verification

- `cargo test --release --all-targets`: 78 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `tools/verify_fleet_100.py`: 12 original/add-on pairs, 1,448 add-on entries disjoint from the original vehicles and each other, all ZIP CRCs valid, and 1,376 nonempty mono 48 kHz / 24-bit PCM WAVs checked. Per-vehicle source and add-on hashes are in the ignored local `output/bess-0.10.0-fleet-20260923/verification.json`.
- `tools/verify_delivery_100.py`: the portable archive has exactly the five allowed application and English instruction files, valid hashes and links, and no vehicle data. The standalone executable matches the release build.

These checks establish file integrity and a working export path. They do not establish that the generated sound is more natural, that BeamNG's exact in-game interpolation is smooth, or that its final spatial mix matches the file-level preview. Compare each original trim with its `(BESS)` variant in game. Cerberus around 5,200 rpm, transitions between RPM knots, and mechanical versus flow balance deserve particular attention because previous user listening found synthetic regularity and metallic character.
