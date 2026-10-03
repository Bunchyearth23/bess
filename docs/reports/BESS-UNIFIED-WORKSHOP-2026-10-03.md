# Unified Automation workshop — 2026-10-03

The user approved execution of this six-step plan. The physical solver was already shared (D-036); this work completes the editable configuration, controls and delivery around it. W-009 tracks the work without changing earlier work-item states.

## 1. Common saved engine definition

Implemented `EngineDefinition` as the canonical complete physical configuration for imported and freely created engines: architecture, parts, operating limits/inertia, combustion, tuning and sound. Project v4 keeps source identity separate and reads v1–v3; an old Scratch project migrates without recalculating its active parameters. Default derived tuning remains omitted in saved JSON for compatibility. The legacy Scratch container remains a compatibility/editor adapter, not a second synthesis engine.

## 2. Faithful import and parameter provenance

The imported baseline is saved independently of the active engine. Each parameter records its baseline, current value, origin and source/explanation: read, converted/calculated, estimated or modified. Restoring a section compares against the saved baseline rather than a fresh import that could change with later mapping rules. Missing firing order, ambiguous exhaust units and other unavailable measurements remain explicit assumptions.

## 3. Complete shared workshop

Imported engines now use the same architecture/parts editor, detailed tuning, physical dyno, sound controls and listening rooms as free creation. Six section resets restore the imported baseline without discarding unrelated edits. The original A and physical B controls remain available. All sections remain visible under D-033. Invalid visible drafts disable saving and export instead of silently saving the previous valid configuration.

Direct audition RPM and the BeamNG sample grid retain the source bank range under D-007. Simulated physical operation uses the edited engine's operating limits. Editing sound/engine design does not rewrite vehicle physics.

## 4. Common physical commands

Both origins now use the same command scheduler for direct RPM, the comparison cycle and simulated driving. Simulated driving follows the physical crank and receives throttle, shaft load, starter, fuel cut and accessory loads. Direct/cycle operation imposes RPM. Live and WAV rendering resolve the same saved engine definition.

Engine changes are prepared off the real-time producer and handed over through the existing bounded queue; displaced models are retained until the UI can reclaim them. Sound-only edits retain the running physical state. Regression tests cover parity in all three modes and no allocation/deallocation in the tested real-time update path. A disabled listening room is bit-identical to the dry signal, including original A peaks.

## 5. BeamNG delivery from the edited configuration

Loop rendering, level analysis, saved export projects and manifests now carry the same complete active definition and imported baseline. Variant identity includes physical layer gains, so distinct engine mixes produce distinct paths; RPM/load/listening-volume changes alone do not create another identity. A muted engine layer is valid for export and level analysis.

Stationary loop export neutralizes the starter and externally requested fuel cut while preserving physical accessory settings. It exports steady RPM/load sounds; these loops do not encode the BESS live history of startup, turbo spool or triggered afterfire. Existing BeamNG event references remain intact. The manifests state this limitation, distinguish estimates from read metadata, and retain source fingerprints. Audition protection remains separate from file calibration. The source archives and vehicle physics are preserved.

## 6. Validation and acceptance

Regression coverage includes migration, save/reload, source A preservation, deterministic common-engine output, effective overrides, bounded live updates, finite audio/headroom and exported-loop continuity. The corpus demonstration checks twelve local Automation vehicles, including a complete eight-second physical comparison cycle for each, and prepares representative projects, audio comparisons and one uninstalled BeamNG variant. Automated checks do not establish perceived realism or in-game behavior.

The first longer listening demonstration exposed a cylinder-boundary validation defect on the unedited Genesis I6 during high-RPM lift-off: a positive, finite runner pressure of 84.11 Pa was rejected by an arbitrary 100 Pa input floor, although the conservative gas solver accepts positive pressure and enforces available-mass budgets. The correction aligns that boundary contract without clamping the physical state; zero, negative, non-finite and excessive pressures remain invalid. A corpus-independent low-pressure test and the real Genesis cycle protect the case.

## Evidence

- Final complete release all-target suite, including the export edge cases and cylinder correction: **299 passed, 6 existing ignored tests** (194 library, 23 interface and 82 integration tests). Log: `output/unified-workshop-20261003/tests-complete.txt`.
- `cargo clippy --release --all-targets -- -D warnings`, `cargo fmt --all --check` and `git diff --check` pass. The usual executable and reproducible corpus demonstration are built with `cargo build --release --bin bess --example unified_workshop` on this Windows machine, retaining the accepted AVX2/FMA target. Logs: `clippy-final.txt` and `build-final.txt` beside the test log.
- The app opened an edited imported v4 project; the final captured workshop was visually inspected (`workshop-final.png`). Headless GUI tests cover complete control exposure, section resets, invalid-draft saving, project persistence and export-level invalidation. Manual ergonomics acceptance remains open.
- Twelve source vehicles pass definition JSON identity, 1,024 bit-exact physical frames after conversion, and a full eight-second physical cycle. Four edited v4 projects (I4, I6, V8 and an additional verified turbo vehicle) retain their source and baseline. Each comparison proves A unchanged after physical edits and B changed by those edits.
- Listening package: [`ECOUTER.html`](../../output/unified-workshop-20261003/ECOUTER.html), four projects and twelve independently checked mono 48 kHz / PCM24 WAVs, eight seconds each. The clips share the comparison cycle; no additional level normalization is applied. All 16 local audio/project links exist. `delivery-verification.json` records file format checks and hashes, including the rebuilt executable SHA-256 `5671a05f61f648584a3df944fd2c998ab44467776acc28a667e9b674bc700ecc`.
- Device checks on Sound BlasterX G6: imported Advent V8 simulated driving and free V12 60° changing-RPM/load cycle, 30 seconds each, running concurrently while the offline variant was rendering. Both report PASS at 48 kHz output / 96 kHz synthesis, 3,000 callbacks each, zero underruns, missing frames, callback overruns or physical/producer failures; callback p99 is in the 1% budget bucket. The 40 ms producer queue absorbed synthesis-block peaks of 12.326 ms (V8) and 14.980 ms (V12). Logs: `device-v8.txt`, `device-v12.txt`. Output volume was zero; this is a short technical device check, not listening acceptance or a long endurance qualification.
- Selectable Berlingo variant delivered in `output/unified-workshop-20261003/beamng-variant/`: **132 nonempty mono 48 kHz / PCM24 WAVs in 138 entries**, all CRC reads and safe/unique paths checked. ZIP `bess-variant-berlingo_1000lingo-8f17bc777a_unified_workshop_3767b2.zip`, SHA-256 `e9fe57b032330c7344ff25ccd442ac3d0d35f1eeaab39163c85b9e97c73ef704`. Its project and manifest contain the edited engine, saved baseline, provenance and loop policy. All twelve source archive hashes remain unchanged. `validation.json` contains the complete corpus/audio/variant evidence; `INSTALLATION.txt` explains the candidate. **The variant is not installed.** No listening or BeamNG acceptance is claimed.

## User acceptance still required

- [ ] Compare A/B and edited engines at idle, steady load, acceleration and lift-off.
- [ ] Judge the complete workshop's usability.
- [ ] Drive the exported configuration in BeamNG and check RPM/load transitions and camera perspectives.
