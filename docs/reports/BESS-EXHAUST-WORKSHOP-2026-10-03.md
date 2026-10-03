# BESS — Exhaust controls and BeamNG export access, 2026-10-03

## Scope and user evidence

The user launches `target/release/bess.exe`, likes the experimental back-pressure sound, could not find the exhaust reflection control and asked to check BeamNG export. This is positive feedback on that option, not acceptance of every engine, physical calibration or game behavior.

## Delivered interface

- `Exhaust sound` precedes `Block` in the shared imported/free workshop. It groups `Exhaust decay (ms)`, `Experimental back pressure`, fixed coupled output gain and its measurement action. The 10–250 ms range and 120 ms compatibility default are retained. The visible hint suggests 40 ms for a shorter tail.
- Exhaust geometry no longer carries a misleading sound-only badge: with coupling active it can change simulated torque. Coupled level is explicitly an output correction, not pressure strength, and is disabled while coupling is off.
- `BeamNG export` stays above the central scrolling area and scrolls to the export section. The creation action now precedes optional volume analysis. The section displays current decay/coupling/gain and explains add-on installation and selection. A free engine sees the import requirement and WAV alternative.

## Export audit

`Settings::for_beamng_export` retains the complete engine; export resolves that definition and builds the common physical voice. Exhaust decay, wave coupling and coupled level survive in the rendered settings and manifests. They are baked into WAV loops, not BeamNG controls or vehicle physics. Source-relative export RMS calibration can compensate a coupled output-gain change; that control should not be presented as stronger physical feedback.

The historical `output/final-readiness-20261003/workshop/beamng-variant/manifest.json` still records coupling off and zero coupled gain; absent decay uses the historical 120 ms default. Its ZIP contains the BESS configuration and 132 loops. It cannot demonstrate a recently enabled option. Existing mods must be regenerated after sound edits; keep the original vehicle and choose its named BESS configuration.

No audio solver, export normalization policy or package writer was changed in this UI increment. No mod was installed and no new in-game behavior was observed. X-028 strict 10 percent length-law qualification and X-014 laptop/performance limits remain open.

## Validation

Final executable, checks and visual inspection are recorded after the build in the accompanying validation receipts. The existing workshop UI regression covers section order, a single decay slider and coupling checkbox for both engine origins, an enabled imported export action, actual setting summaries, and the free-engine explanation. Its initial failure was a test harness mistake (egui Label text is stored in AccessKit value rather than label); the production widgets had bounds.

The existing stationary export regression was rerun: one passed, demonstrating saved complete-engine edits reach generated loops. This is not a new three-setting ZIP or in-game audio acceptance test.

The 1180 × 860 native app capture of a muted imported I4 project was inspected: exhaust decay, the active back-pressure checkbox and the coupled level are visible before Block on the initial screen. The export shortcut is at the top of the central panel. A missing arrow glyph was removed after this inspection; the final label is plain `BeamNG export`. Native clicking/scrolling and in-game selection were not automated.

Final checks: **28 application release tests passed**, **one stationary-export regression passed**, strict release all-target Clippy, formatting and executable build passed. The final source receipts bind the four application checks to current hashes. Only `src/main.rs` and `src/workshop_tests.rs` differ from the prior archived Rust/Cargo source snapshot; the library/DSP/export implementation is identical. The previous 366-test full-suite result remains historical and was not represented as a fresh full run.

- Current executable: `target/release/bess.exe`.
- SHA-256: `4706d922151d813c2710a8f11c89c73516fff15240244a8389ff3d8740229ff3`.
- Archived executable and logs: `output/exhaust-workshop-20261003/`.
- Final native capture: `output/exhaust-workshop-20261003/workshop-final.png`.
- The prior audio executable remains archived under `output/intake-rework-20261003/`; the initial accessibility-test failure is retained in `tests-initial-accessibility.log`. Final passing receipts describe the delivered executable.
