# Intake source balance and default mechanical level — 2026-10-03

## User report and attribution

The user reports that the default intake is too loud and sounds like plain airflow. The common engine already generates per-cylinder acoustic pulsations through the runners, throttle, plenum and airbox. A separate duct/jet/edge-noise observer was strong enough to dominate those pulses on most of the matched I4/I6/V8 points. The earlier noise bandwidth calibration is not listening acceptance. X-023 remains open.

The diagnostic records the existing pulse contribution and the residual airflow/compressor contribution immediately before the intake Tone stage. It does not introduce a second synthesis implementation. The baseline uses the application's 96 kHz internal rate, 95-tap FIR decimation to 48 kHz, one second of warm-up and two seconds of capture. All nine baseline intake totals match the preceding delivered build byte for byte.

The raw flow/pulse RMS gap ranges from -5.23 dB to +41.04 dB. This is not a universal noise-only layer: the I4 at 3000 RPM already has a strong 100 Hz firing component. Noise is especially dominant in the V8 at high RPM. These ratios describe signal energy, not perceptual masking thresholds.

## Source correction and control

`SoundTuning.intake_air_noise` controls only the aerodynamic duct, jet and edge contribution after its existing airbox filter. The physical pipe pulses and compressor blade-pass sound are preserved. Filtering and random draws continue even at zero so that changing the control never restarts the source sequence. Live edits follow a 20 ms time constant; construction and reset apply the saved value immediately. The observer does not feed back into cylinder or manifold physics.

The default is **0.008 linear gain**, approximately **-42 dB relative to the previous airflow calibration**. This is the reduction of the added airflow source, not of the entire intake layer. The interface exposes `Intake air noise` in dB, with Off at the lower stop and 0 dB restoring the historical airflow level. The normal discrete wheel behavior and Shift fine adjustment apply. The saved field stays linear and is not rewritten merely by displaying the logarithmic control.

Existing projects missing the field receive the new default. Explicit settings and existing intake, mechanical and exhaust layer levels remain intact. No uniform reduction is imposed on the physical pulses, no new oscillator or additional body noise is introduced, and no automatic gain is added. The same setting reaches Automation and scratch engines through the common physical voice and their live/WAV/BeamNG paths. Existing exported audio must be regenerated.

## Candidate choice

Four actual source renders use gains 1, 0.2, 0.02 and 0.008 with otherwise identical definitions and settings. Across 108 WAVs the pulse files are identical. Reconstructing `flow(g) = g * aerodynamic + fixed` from the endpoint renders predicts the two intermediate renders with maximum relative RMS error below 2.72e-7. The preserved fixed component is resolved on the turbo I4; code inspection identifies the unchanged compressor path. It is below the numerical floor on the I6 and V8.

| Point | Total intake RMS change at 0.008 | Flow/pulse before → after | Cycle correlation before → after |
| --- | ---: | ---: | ---: |
| I4 / 1200 | -11.03 dB | +11.85 → -5.63 dB | 0.018 → 0.781 |
| I4 / 3000 | -1.11 dB | -5.23 → -19.75 dB | 0.771 → 0.989 |
| I4 / 5712 | -8.55 dB | +16.86 → +7.73 dB | 0.059 → 0.657 |
| I6 / 803 | -15.94 dB | +15.83 → -26.11 dB | 0.024 → 0.998 |
| I6 / 3000 | -26.34 dB | +26.45 → -15.49 dB | 0.004 → 0.973 |
| I6 / 6000 | -28.93 dB | +29.15 → -12.79 dB | 0.020 → 0.951 |
| V8 / 1100 | -31.69 dB | +32.12 → -9.82 dB | -0.004 → 0.905 |
| V8 / 3000 | -32.38 dB | +32.88 → -9.05 dB | -0.021 → 0.887 |
| V8 / 6000 | -38.42 dB | +41.04 → -0.90 dB | 0.007 → 0.558 |

The chosen candidate is the only tested gain that puts the aerodynamic contribution below pulse RMS at all nine points. At the limiting V8/6000 point, 0.02 still leaves flow 7.06 dB above pulses, whereas 0.008 gives -0.90 dB. This is an explicit engineering heuristic for the next listening candidate, not a recording-based calibration. The I4 high-RPM residual includes preserved turbo whine and therefore remains above pulse RMS. A higher correlation only establishes that the existing engine rhythm is less obscured; it does not prove naturalness.

Artifacts: `output/intake-rework-20261003/analysis/CANDIDATES.md` and `candidates.json`. All source coefficients remain estimates. A later geometry-dependent airflow transfer would require separate evidence; this change does not silently substitute one.

## Additional requested initial mix

During delivery the user explicitly requested zero mechanical noise by default. Fresh Automation imports, scratch engines and factory presets now start with the mechanical layer at zero. Explicitly saved or edited layer levels remain available, and the generator itself is retained. Deriving scratch engine parts no longer overwrites an explicit mechanical level. This requested initial balance does not establish acceptance of the mechanical timbre; X-019 stays open.

The intake before/after experiment keeps its historical comparison mix fixed at intake 0.30, mechanical 0.12, exhaust 0.80 and listening volume 0.80. Separate `new-default-*` application renders use the fresh-import layer balance: intake 0.25, mechanical zero and exhaust 0.80. Listening volume remains fixed at 0.80 for the experiment, rather than the product's initial 0.35. The listening page distinguishes these outputs so a simultaneous mechanical-level change cannot be mistaken for an intake-source change.

## Delivery verification

The pre-existing intake-level test encoded the historical noise-heavy calibration: monotonically rising RMS across four different RPM/load points and substantial cruise energy above 250 Hz. At the new default the default NA I4 gives -105.7/-66.0/-49.3/-64.5 dB across its 850/2000/3000/4500 RPM points, with 99.9% of cruise energy below 250 Hz. Reduced noise exposes the existing damped, RPM-dependent pipe response, so monotonic overall intake loudness is not claimed. That historical test now explicitly selects gain 1, preserving every old threshold. The new source, retune and matched-corpus checks cover the reduced default; listening across the RPM range remains necessary.

The cycle-texture test now measures the isolated airflow residual alongside the mechanical stem, retaining its existing minimum-energy and inter-cycle-difference thresholds. Applying that noise criterion to the whole intake had required the noise to obscure the phase-locked pulses. The NA reference has no compressor contribution, and the difference/energy ratio is independent of a fixed source gain. This is a source-attribution correction, not a relaxed numerical tolerance; independent silence/decay tests cover absence of autonomous noise.

The final release suite passes **366 tests, zero failures, six excluded diagnostics across 47 targets**. Strict release all-target Clippy, formatting and the release build pass. Tests cover migration and explicit saved values, fresh mechanical defaults and save/reimport, gain bounds, 48/96 kHz source invariance, smoothed direct/prepared retunes with non-neutral Tone and an existing crossfade, allocation-free rendering, original A, and common playback/export paths. The initially over-strict +0/-0 bit assertion was corrected to exact numerical equality; nonzero tolerances were not relaxed. Interrupted and failed earlier logs remain available separately.

The final diagnostic and application renders contain **27 source WAVs and 99 application/diagnostic WAVs**. The latter comprise 27 raw float32 stems and 72 PCM24 mixes, including the 27 new-default live/hood/cockpit outputs. Eighteen raw mechanical/exhaust pairs and nine isolated pipe-pulse pairs remain byte-identical. All nine probe intake totals match the actual common application voice; reconstruction error stays below 1.5e-7 relative RMS. Gain 1 reproduces all 27 historical probe WAVs byte for byte. Source archives and all other engine-definition fields are unchanged.

At the fixed comparison mix, the engine-only intake+mechanical RMS falls by 1.12–27.64 dB across the nine points. Full Live RMS changes by less than 0.01 dB because exhaust dominates these captures. Normalized two-emitter previews do **not** get uniformly quieter: the fixed-mix I4 hood preview rises 2.48 dB at 3000 RPM and 3.94 dB at 5712 RPM. With the new layer defaults the I4/5712 hood rise is 4.13 dB versus the old mix; the V8/6000 hood falls 6.26 dB. These latter comparisons combine airflow attenuation with intake 0.30→0.25 and mechanical 0.12→0. The detailed tables remain in `comparison/REPORT.md` and `comparison.json`. X-023 explicitly retains this normalization and listening boundary.

`TESTER.html` offers the new layer defaults first, then the fixed-mix A/B and isolated intake comparisons with common gain or equal AC RMS. Its 108 additional PCM24 comparison copies use fixed gains, normally target RMS 0.04 and peak ceiling 0.5, without a limiter. Static QA checks 63 audio controls, 112 local references, readable labels and no autoplay. Browser rendering/playback was not qualified.

Executable: `target/release/bess.exe`, SHA-256 **`c8c8fd02cebafd32852c97bab529fba8e0beb11856454b50a6780301ff5f9dda`**. The same file is archived as `output/intake-rework-20261003/bess-final.exe`, with the matching Rust/Cargo source snapshot in `source-final/`. The preceding executable remains `bess-before.exe`, SHA-256 `c5821cc5bfe86288c90539f7fda5c32c5fff434949a01c96cc84e6c048edfe6c`.

Completed command receipts bind source hashes to all four validation passes, final executable and example hashes to the build, and diagnostic hashes to completed render logs and manifests. `verify_delivery.py` checks these links and every audio/analysis identity before writing `verification.json` and `delivery-manifest.json`. Graft's synchronized wiring check passes; no deep semantic tier is claimed. W-006.35 and W-006.36 are technically delivered. User listening, device playback and actual BeamNG reception remain unqualified; no new mod installation or publication is claimed.
