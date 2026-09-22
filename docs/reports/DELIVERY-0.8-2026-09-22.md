# BESS 0.8 — Technical delivery and final tests

Entry point: `dist/BESS-0.8.0-complete/START.html`. Final executable: `dist/BESS-0.8.0-final.exe`, also delivered as `BESS.exe` in the complete folder. SHA256: `F539BC99DD51BFE5BB3C643CE949DC9D8B9A1C57ED610A3136DE35723B15568D`.

## S-07 — Configuration and events

An optional module uses zero cylinders to mean unknown data and no added excitation. Users can enter 1–12 cylinders and their firing angles over 720°, force, pressure duration and exhaust-opening delay. The suggested even spacing is an explicit starting configuration, not an identification.

Smooth pressure windows follow the engine cycle and load, combine with the source and propagate through the existing acoustics. This phenomenological model does not calculate thermodynamic combustion, separate banks or physical valve flow. The source remains dominant; additions are optional and adjustable. A is independent.

`output/combustion-0.8.0` contains three 16-second WAVs and three projects: source bank alone, evenly spaced four cylinders and uneven four cylinders. These example settings do not claim to describe Genesis. Measurable effects, bounded output, serialized state and zero allocations were checked; no subjective superiority is claimed.

## S-08 — BeamNG copies

`output/beamng-0.8.0` contains twelve complete copies and their audit. For every rpm/load point in the blend, the same audio engine generates a stable loop after one second of warm-up, with at least two useful seconds, cyclic overlap and a common safety gain. Playback fades are not applied at every loop turn.

Verified on real ZIPs:

- Twelve exports succeeded and were reimported by BESS; **688 WAVs** are 48 kHz / 24 bit.
- **1,194 other entries** were preserved identically by content comparison.
- All source ZIPs retained their SHA256 hashes.
- Every WAV had finite, audible measured RMS and peak ≤ 0.951.
- The maximum seam difference divided by adjacent-difference RMS was **4.206**, below the chosen discontinuity detection threshold of 8. This does not prove clicks cannot be heard.
- A focused copy/reimport test on a synthetic bank checks bounds, provenance, format, non-audio content and continuity.

The copy retains the original vehicle's internal paths, so disable the original before enabling BESS. There is no automatic installation or game modification. Start, stop, pops and turbo files remain the vehicle's. BESS transients, micro-variations and deceleration fuel cut are not encoded into the loops. Playback volume and A/B compensation are not applied; common gain provides peak headroom. Intermediate map states are not reproduced exactly by the blend's two load layers.

This division follows continuous-layer and event roles in [BeamNG documentation](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/). The structure comes from real archives: blend and JBeam stay identical. Structural validity and BESS reimport do not prove operation in game.

## S-09 — Consolidated delivery

The complete folder contains the executable, twelve sources, A/B/C listening tracks, projects, combustion demos and twelve mod copies. Projects were rewritten with relative references to bundled sources. `SHA256.json` inventories delivered files. `tools/package_delivery.py` refuses to overwrite an existing delivery.

The single test guide is [FINAL-TESTS.md](../FINAL-TESTS.md). All twelve automatic-preset B renders are identical to 0.7 because no engine layout is imposed on import. A redundant C selector was removed from the listening page; three dedicated demos demonstrate the new optional effect. Twelve A files remain identical to 0.7.

## Final technical checks

- **40 passing tests:** 16 library, 1 egui wheel interaction, 5 driving, 16 hybrid/export, 2 MVP.
- Release build, formatting and strict linting passed without warnings.
- Deterministic Graft graph: 260 nodes, 282 links, synchronized; no LLM pass.
- Inspected `delivery-0.8-final-ui.png` capture: correct accents, visible optional configuration, BeamNG export button and integrated driving. An initial capture exposed broken accents; the `-final` executable fixed them.
- Silent CPAL output with a **12-cylinder** test configuration on the Sound BlasterX G6 at 192 kHz for 30 seconds: **3,000 callbacks, 7.674 ms maximum and zero overruns**. Evidence: `delivery-0.8-final-audio.txt`. This is not a listening test.

## Remaining S-10

Comparative listening, preference and sound identity, full UI use, a prolonged session and actual BeamNG driving. No in-game test was performed here; W-002.4, W-003.6 and W-003.17 remain open. S-01 through S-09 describe technical delivery rather than acoustic or game acceptance.
