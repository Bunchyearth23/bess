# BESS 0.10.1 standard export correction — 2026-09-23

## Cause and correction

Version 0.10.0 made descriptor-driven synthesis the default for new imports and passed that mode into the selectable BeamNG exporter. The user identified the result as the wrong BeamNG sound. Version 0.10.1 starts new imports in the source-guided mode. The independent generator remains an explicit experimental listening option in the interface, and saved projects retain their chosen listening mode.

The selectable BeamNG exporter now forces source-guided settings even if a project is auditioning the experimental mode. The legacy full-replacement exporter does the same. Conversion of a previously rendered experimental bank into a selectable add-on is rejected. The BeamNG two-emitter preview and export-level panel use the standard path so they describe the sound written to the add-on. Listening WAV comparisons may still use the experimental mode.

The corrected Cerberus add-on has the same filename and SHA-256 as the source-guided Cerberus add-on backed up before version 0.10.0. It is a restoration of the standard export, not a claim of a newly improved Cerberus sound.

## Local fleet and validation

The twelve local Automation sources were exported into `output/bess-0.10.1-standard-fleet-20260923`. Their add-ons are installed beside the unchanged original vehicles in `D:\BeamMP\current\mods`; the prior 0.10.0 add-ons are backed up in `output/beamng-bess-backup-before-0.10.1-20260923`. No vehicle ZIP is tracked in Git or placed in the application package. The local verification manifest contains the per-vehicle source and add-on hashes.

All twelve add-on archives pass CRC and path-separation checks: 1,448 paths are disjoint from the original vehicles and each other. Their 1,376 sound files are nonempty mono 48 kHz / 24-bit PCM, and their saved projects and manifests explicitly set `procedural: false`. The 80 Rust release tests, strict Clippy checks and application-only Windows package check pass. These establish file integrity and export mode. BeamNG was running while the add-ons were replaced; the user should reload or reselect each `(BESS)` configuration to hear the new files. The user's in-game listening remains the acceptance test for naturalness, transitions and spatial balance.
