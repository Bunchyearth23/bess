# BESS — Operational index

Identifiers are stable. W states change only on explicit user instruction; checkboxes follow evidence.

## Resume

- User priority: replace the generic bench with hybrid synthesis based on Automation audio, then improve realism and dynamics; see [engine sound research](ENGINE-SOUND-RESEARCH.md).
- Current technical delivery: `dist/BESS-0.8.4-public/START.html`, including fixed-rpm Cerberus listening at 5,200 rpm and mono 48 kHz / 24-bit WAVs. The English release package passed 189 hashes and 93 links; twelve A tracks stayed unchanged and twelve B tracks were recalculated. The final English G6 run opened 48 kHz with 2,999 callbacks and no overruns. See the [0.8.4 report](reports/CERBERUS-5200-0.8.4-2026-09-22.md). A later local export installed twelve `(BESS)`-labelled vehicle copies in BeamNG's mods folder, with 688 replaced loops and the originals backed up; see the [installation report](reports/BEAMNG-BESS-LABELS-2026-09-23.md). S-10 remains open.
- Standing instruction: continue without waiting for intermediate listening; group user testing at the end. Follow the [work sequence](STEPLIST.md), keeping technical evidence separate from final sound acceptance.
- Remaining validation: subjective preference, an extended audible session and in-game BeamNG operation.

## Decisions

| ID | State | Decision |
| --- | --- | --- |
| D-001 | accepted | Rust with egui/eframe and BDSP as DSP library; Git dependency pinned to `b2981d431d321d29224752a2f855495da965d4bb`. |
| D-002 | accepted | Playback and WAV export use the same synthesis; BESS owns engine-specific models. |
| D-003 | accepted | Standalone MVP without ZIP; defer BeamNG export until a real vehicle is inspected. Do not claim game-compatible export before validation. |
| D-004 | accepted | First MVP divided into modules in one crate; read-only ZIP inspection without interpreting JBeam parts. See [architecture](ARCHITECTURE.md). |
| D-005 | accepted | Main sound source is the Automation bank. Hybrid enrichment and level-comparable A/B; generic bench becomes a fallback tool. |
| D-006 | accepted | Version 0.3 acoustic geometry is an editable WAV-driven model, not ZIP part inference; do not invent cylinder count. V2 projects remain readable, though 0.2 sound changes. |
| D-007 | accepted | At user request, clamp rpm to exported minimum/maximum in slider, numeric input, projects, playback and rendering. JBeam damage threshold does not set the range. |
| D-008 | accepted | Simulated driving is separate from tone: throttle, gearbox and resistance drive rpm/load. WAV rendering starts at standstill with held controls; it does not capture input history or export BeamNG physics. UI projects are v3; v1/v2 remain readable. |
| D-009 | accepted | At user request, realism-plan steps count as implemented only with a delivered and checked functional demo; tests alone are insufficient. User listening validates realism, and a BeamNG test validates final export. See [audio realism plan](AUDIO-REALISM-PLAN.md). |
| D-010 | accepted | Group user tests at the end; continue technical demos without pauses and keep subjective/BeamNG validation open until final testing. |
| D-011 | accepted | At user request, mouse wheel adjusts sliders (Shift for fine adjustment) and two character controls simplify maps. Old curves stay until changed; import calibration does not automatically replace settings in existing projects. |
| D-012 | accepted | Optional combustion needs explicit configuration: assume neither real cylinders nor firing order. Version 0.8 export is a full vehicle copy with blend WAVs replaced and original intact; only one version active in game. Manifest separates loops from transient controls. Check in-game compatibility under S-10. |
| D-013 | accepted | Tone presets preserve entered engine configuration. Twelve exported ZIPs have unique external names. As automatic 0.8 sound equals 0.7, the listening page omits a redundant C track. |
| D-014 | accepted | From 0.8.2, B replaces part of measured pulses with a derived filtered front, varies cycles and couples texture with load; A stays intact. Read `.car` cylinder/layout data only when UIDs, `.pc` and JBeam agree; infer no firing order; explicit events remain optional. |
| D-015 | accepted | At user request, direct playback prefers 48 kHz over the Sound BlasterX's 192 kHz default; device transport may be 32-bit float. All delivered WAVs remain mono 48 kHz / PCM24. Show actual format and fallback. |
| D-016 | accepted | After the Cerberus 5,200 rpm report, B follows the prepared loop in order and uses a shared rpm-implied cycle duration for neighboring samples. Random anchor jumps and independently estimated periods caused pumping; A remains the unchanged reference. |
| D-017 | accepted | BESS means Bunchy's Engine Synthesis System. Present its core purpose as the intermediary between Automation vehicle sound exports and BeamNG.drive vehicle copies: import, synthesize and preview, then export. |

## Work items

| ID | State | Milestone | Work | Depends on | Exit criterion |
| --- | --- | --- | --- | --- | --- |
| W-001 | building | MVP | Engine bench, presets, saving, playback and WAV | D-001 | Executable, audio tests and launch verified |
| W-002 | planned | Integration | Automation import and BeamNG export | Real ZIP | Audible vehicle and in-game transitions validated |
| W-003 | building | Hybrid audio | Read bank, enrich dynamics, compare source/enhanced | D-005 | Audible import, continuity/control tests and user listening preference |
| W-004 | building | Driving | Simulate throttle, gearbox and output load for playback | D-007, D-008 | Working controls, sound effect, explicit WAV behavior and user validation |

## Tasks

### W-001 — engine bench

- [x] W-001.1 Cargo and local Graft graph initialized; deterministic graph without an LLM pass.
- [x] W-001.2 Four-stroke synthesis and BDSP layers implemented; alternating firing intervals, without a manufacturer's order.
- [x] W-001.3 Initial UI and presets implemented, accessibility tree observed and engine fade tested. This was the former French UI; current public release is being translated to English.
- [x] W-001.4 JSON round trip and WAV verified; export implemented on a worker.
- [x] W-001.5 Signal and harmonics checked by seven tests; see MVP report.
- [x] W-001.6 Release executable compiled and window launched; CPAL output opened.
- [ ] W-001.7 Subjective listening on user hardware.
- [ ] W-001.8 Complete GUI workflow: remote control was unavailable for input/captures.

### W-002 — integration

- [x] W-002.1 ZIP received and inspector verified: thunderhawk_zero, 56 WAVs, one blend and 800 rpm idle.
- [ ] W-002.2 Identify active parts, engine characteristics and load maps.
- [x] W-002.3 Separate mod copy and prepared loop joins in 0.8: twelve archives reimported, 688 bounded WAVs, sources intact. Continuity measured; audible absence of clicks awaits W-002.4.
- [x] W-002.5 Local 0.8.1 audit found the configured BeamNG mods folder; structure and 688 WAV references checked. Twelve ZIP names are unique; no copy installed or in-game test performed. See 0.8.1 QA report.
- [x] W-002.6 Local `(BESS)` export and installation: twelve vehicle selector names edited, 688 mono 48 kHz / PCM24 loops regenerated, 1,182 other entries unchanged, twelve originals backed up outside active mods, twelve BESS ZIPs installed and SHA-256 verified. Full suite: 48 passed. The v0.8.4 release assets were not changed; see the [installation report](reports/BEAMNG-BESS-LABELS-2026-09-23.md).
- [ ] W-002.4 Validate transitions and sound in BeamNG.

### W-003 — audio foundation

- [x] W-003.1 Research and analysis of 56 WAVs; see [engine sound research](ENGINE-SOUND-RESEARCH.md).
- [x] W-003.2 Real bank loaded: 28 rpm points × two loads, joins and BDSP sinc interpolation.
- [x] W-003.3 Load/transients, reflections and additional layers implemented; controls tested.
- [x] W-003.4 A/B UI captured, v2 hash-referencing project, two equal-RMS WAVs generated.
- [x] W-003.5 Thirteen tests; zero allocation in `next/set`; 500 callbacks with no overrun on then-current hardware.
- [ ] W-003.6 Subjective validation: preferable to Automation while preserving engine identity.
- [x] W-003.7 Header/chamber/outlet network and separate intake; eight controls, 19 tests including energy, damping, lift-off and rapid changes.
- [x] W-003.8 Three characters, four equal-RMS WAVs and reopenable projects; measurements and 0.3 capture described in report.
- [x] W-003.9 Real-bank 0.3 check: 3,000 callbacks in 30 seconds, no overrun, 3.900 ms maximum at 192 kHz and zero volume.
- [x] W-003.10 Version 0.3.1 fix: ZIP-derived rpm bounds, 19 existing tests and static checks passed; see fix report.
- [x] W-003.11 Step 1: twelve real imports, A/B/C demo and projects delivered, playback/switching observed; see [0.5 report](reports/REALISM-0.5-2026-09-22.md).
- [x] W-003.12 Step 2 technical: overlapping four-cycle segments and smooth rpm interpolation; continuity, real corpus and 0.6 executable checked. Final listening deferred to S-10 under D-010.
- [x] W-003.13 Step 3 technical: gains and isolated pulse/texture clips delivered; reconstruction verified on four 0.6 banks. Identity and preference remain W-003.6 / S-10.
- [x] W-003.14 Step 4 technical: four editable persisted maps, local Genesis effect, inspected UI capture and common transport; see 0.6 report. User tests remain S-10.
- [x] W-003.15 Step 5 technical: first heuristic per-bank calibration of additions and coloration blend in 0.7, with twelve comparisons. Geometry and preference remain unconfirmed under W-003.6 / S-10 per D-010; see 0.7 report.
- [x] W-003.16 Step 6 technical: explicit engine configuration, event-based pressure and exhaust-opening delay, three WAV/project demos in 0.8. Effect and zero allocation checked. Identity/preference remain W-003.6 / S-10.
- [ ] W-003.17 Step 7: import/customize/export workflow demonstrated in BeamNG alongside W-002.3 / W-002.4.
- [x] W-003.18 Version 0.7 controls: wheel/fine adjustment tested in egui, two controls replace grids, old maps retained and UI capture inspected; manual final test remains S-10.
- [x] W-003.19 Consolidated 0.8 delivery with portable references, comparisons, BeamNG copies, SHA256 hashes and final protocol; see report. W-003.17 stays open without actual BeamNG driving.
- [x] W-003.20 Version 0.8.1 audit: character buttons preserve entered cylinder/angle data; redundant C removed from twelve cards. Focused test and asset audit; see 0.8.1 QA report.
- [x] W-003.21 Version 0.8.2 revision: twelve A banks identical, twelve new equal-RMS B tracks, 45 tests, engine metadata on 12/12, compensated A/B fade and two 3,000-callback sessions without overrun at 192 kHz. Twelve BeamNG copies reimported: 688 loops and 1,194 other entries preserved. Portable delivery checked (190 hashes, 78 links, 36 projects); see 0.8.2 report. Preference and in-game checks remain W-003.6 / W-003.17 / S-10.
- [x] W-003.22 Version 0.8.3 format: prefer advertised 48 kHz, otherwise fall back; G6 opened 48 kHz float, 3,000 callbacks, no overruns, 2.010 ms maximum within 10 ms. All mono 48 kHz / 24-bit WAVs retained; 24 comparison tracks byte-identical to 0.8.2, twelve BeamNG copies reimported with 688 loops and 1,194 other entries unchanged. Portable delivery checked (194 hashes, 78 links, 36 projects) and executable tried on Genesis; see 0.8.3 report. Extended audible and in-game tests remain.
- [x] W-003.23 Cerberus 0.8.4: at steady 5,200 rpm, 50 ms B RMS variation fell from 0.121 to 0.078 at full load and 0.218 to 0.112 at low load; A stayed identical on twelve vehicles, and all twelve B tracks were recalculated. 47 release tests passed; the final English G6 build at 48 kHz had 2,999 callbacks, no overruns and 1.522 ms maximum. Twelve BeamNG copies reimported (688 loops, 1,194 other entries unchanged). The English portable delivery passed 189 hashes, 93 links and 36 projects; its executable reimported Cerberus and reproduced A/B WAVs. See the 0.8.4 report. User listening and in-game driving remain open.

### W-004 — simulated driving

- [x] W-004.1 Engine/clutch/wheels, N/1–6 gearbox, resistance after final drive, braking and grade; functional/extreme tests passed.
- [x] W-004.2 Controls and telemetry exposed, three modes and saved/restored driving projects; 0.4 capture inspected.
- [x] W-004.3 Common CPAL/WAV transport, exact 48 kHz simulated-render comparison, pause/reset and zero allocation checked.
- [x] W-004.4 Three real WAVs with CSV traces; 26 tests and 2,999 callbacks in 30 seconds without overrun at 192 kHz; see report.
- [ ] W-004.5 User validation of controls and sound effect; vehicle characteristics are still generic.
- [x] W-004.6 At user request, driving moved to a top button and dedicated window; build, strict linting and 0.4.1 capture checked.
- [x] W-004.7 User sketch applied in 0.5.1: integrated driving at upper right, graph beneath; replaces W-004.6's floating window. Build, strict linting and capture checked; see [dock report](reports/DRIVE-DOCK-0.5.1.md).

## Risks and questions

| ID | State | Priority | Topic | Resolution condition |
| --- | --- | --- | --- | --- |
| X-001 | open | P1 | Simplified sound model without proven acoustic fidelity | Comparative listening and iteration |
| X-002 | open | P1 | Audio stability depends on device and buffer size | Extended real-device test |
| X-003 | open | P2 | A and B playback still have an average ≈2 dB dip between Cerberus 4,989/5,338 rpm samples; steady-rpm pumping is fixed but the dip may be audible on a sweep | Slow listen around 5,200 rpm; adjust blending if distracting |
| Q-001 | answered | P1 | `bunchyearth23_thunderhawk_zero` ZIP received and inspected | See `reports/vehicle-check.txt`; JBeam commas are sometimes omitted |
| X-007 | mitigated | P2 | Parameter smoothing and A/B fade tested in the hybrid chain | Extended listening still needed |
| X-004 | open | P1 | Thunderhawk stops at 4,989 rpm; 0.5 corpus limits vary from 3,557 to 7,487. JBeam damage threshold is not a rev limiter | Respect each bank's range; extend only with source evidence |
| X-005 | open | P1 | Declared cylinders/layout recovered from twelve `.car` files, but firing order, bank phasing, headers and active parts remain unknown; header acoustics are aggregated | Reliable engine data and in-game test; see 0.8.2 report |
| X-006 | open | P2 | Version 0.4 bench performance uses generic torque/mass/ratios and does not predict the real vehicle | Identify active parts and engine/transmission data |
