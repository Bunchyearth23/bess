# BESS — BeamNG folders and complete-vehicle export, 2026-10-03

## User direction

The user wants BESS to find the BeamNG folder instead of repeatedly browsing for input and output. They explicitly clarified that the desired re-export is the complete vehicle ZIP with replacement sounds. Their separate project owns variable regrouping. D-040 records this correction to the historical default add-on exporter.

## Delivered behavior

- Startup discovers the BeamNG user/mods folder from launcher configuration, saved folder choices and bounded fallback locations. The launcher INI takes priority over stale legacy registry/version hints. Manual source and output choices persist in BESS preferences; output preferences can be restored without a detected game folder. A missing saved location gives an explicit error. Explicit discovery resets the source override, preserving a valid output choice; malformed preferences are backed up before an explicit reset.
- The vehicle list reads the mods root and its direct `repo` child. Individual `.zip.merged_backup` originals from the grouping tool remain read-only and importable, labelled **original before grouping**. Multi-vehicle or multi-blend archives are visible with disabled import and an explanation. Available originals sort first. The catalogue does not read WAV payloads, recurse through the entire disk or follow directory links; archive entry, metadata and directory bounds apply. ZIP64/multipart or oversized archives are skipped.
- **Export vehicle ZIP** uses the existing complete-copy renderer and the current edited engine. Each export creates its own output directory under the selected destination; the default is **BESS-exports** beside `mods`. Manual destinations are remembered, with a separate one-off action. The destination is checked against the detected mods directory before saving/exporting. No mods are installed, regrouped or activated by this workflow.
- The ZIP retains original vehicle member paths, blend routing and physics. It replaces referenced engine WAVs with the current mixed physical output and updates the vehicle display name with `(BESS)`. It carries the complete edited engine and imported baseline in its project and manifest. Listening volume and live A/B matching are excluded. Existing source-relative calibration and shared peak attenuation are unchanged.
- Two-emitter preview and volume estimates are labelled as add-on tools and do not describe the complete-copy routing. The selectable add-on exporter remains available to explicit command-line callers. For a direct game test, activate only the exported vehicle copy because its paths are shared with the original.
- Persistent **Vehicles**, **Exhaust sound** and **BeamNG export** shortcuts keep the source, decay/back-pressure controls and export action reachable as the source list grows.

## Validation

**59 targeted release tests pass**: 27 library tests covering the new paths/catalogue and existing BeamNG behavior, two tests in the complete-export suite (the actual package regression and concurrent fixture lifetime), and 30 application tests. Strict release all-target Clippy, formatting and the release executable/example build pass. The previous 366-test full run remains historical; this increment did not rerun that entire suite.

The package regression replaces all four fixture WAVs, checks mono 48 kHz decoding, finite/non-silent bounded output, unchanged ZIP member paths/count and unchanged non-audio bytes except the expected `(BESS)` display name. The source stays byte-identical. The project and manifest retain the complete edited engine, baseline, gains and mixed render channel; reimport succeeds. Deliberately stale compatibility fields do not override the complete engine.

An initial parallel optimized compile terminated inside `rustc` with `STATUS_ACCESS_VIOLATION`, before the export test ran. Retrying the remaining checks serially succeeded with unchanged source and build settings. The initial failure log is retained; final receipts bind each successful check to its Rust/Cargo source hashes.

Read-only machine discovery resolves the launcher-configured `D:\BeamMP\current\mods` and proposes `D:\BeamMP\current\BESS-exports`. It finds **13 available originals**: twelve `.zip.merged_backup` archives plus `bunchyearth23_pulsar_opti.zip`. No grouped entry appears in this machine's catalogue. A separate central-directory inspection of `babm_advent.zip` confirms it already integrates BESS parts/configurations and extra blends, so the BESS-package exclusion applies before multi-blend classification. This inspection is one example, not a claim to have inspected every grouped pack. Synthetic tests cover the disabled-import presentation for recognized grouped sources.

The 1180 × 860 native capture of a muted imported I4 project was inspected. It shows the detected folder, original-backup list, filtering/import actions, persistent navigation shortcuts, exhaust decay and enabled back-pressure setting. The export shortcut is visible above the central panel. Headless tests cover the full export action and summaries; native button clicking and scrolling were not automated. The capture helper was closed after verifying its process identity.

SHA-256 comparison of all **34 existing root ZIP/backup files** (about 2.34 GB) before and after discovery/native capture finds no changes. No game mod or BESS folder preference was changed during these checks; preference tests use isolated temporary directories. The default output folder is only proposed, not created by discovery.

- Delivered executable: `target/release/bess.exe`.
- SHA-256: `ba758b0475f1ab1a670408fc4d43bea5890cdcd39c36fb080cf905ca6aadd3f3`.
- Archived executable, receipts, logs, discovery JSON and archive hashes: `output/beamng-folders-20261003/`.
- Native capture: `output/beamng-folders-20261003/workshop.png`.

Test fixtures and read-only local discovery establish file behavior; they do not establish in-game sound quality or the external regrouping tool's acceptance. No audio solver or package writer was changed in this increment. No mod installation, commit or publication was performed for this increment.
