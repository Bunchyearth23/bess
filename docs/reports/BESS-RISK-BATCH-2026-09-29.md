# Risk batch — 2026-09-29

Seven agents merged on `risk-batch`, then into `main`. Builds, tests and renders ran on dev17 (`tools/dev17.sh`). Technical evidence only; nothing has been listened to.

## Changes

| Risk | Change | Evidence |
| --- | --- | --- |
| X-024 torque shape | Per-cylinder intake runners (gas-column inertance + port volume, 0.40 m × stroke/86 mm, valve-head area, K = 0.5); full-load VVT schedule (late closing from 0.55 to 0.9 × redline); Chen–Flynn friction c 5000 → 6000 Pa/(m/s), d 400 → 200 Pa/(m/s)² (FMEP 2.0 bar at 6000 rpm); turbine nozzle and spring wastegate as real exhaust restrictions; compressor re-matched (0.008 × displacement, head 4 × target at the speed limit); one exhaust pipe per bank. | Dyno NA I4 169.7 Nm @ 1106 → 178.0 Nm @ 4437, 112.6 kW @ 6743; turbo 0.71 bar from 2643 rpm, ≥ 0.64 bar to 6743, 257 Nm / 157 kW; V8 386 Nm, V12 537 Nm @ 4437. `tests/dyno.rs` now asserts the shape. |
| X-023 intake | Throttle junction (Borda–Carnot resistance linearised about the jet speed; transparent at WOT, plenum isolated at idle); intake bore/plenum from `EngineTuning::resolve()`; duct noise in-duct U² law; only the audible fraction of the throttle jet (thin jets are ultrasonic); airbox 2 kHz. After the X-024 merge DUCT was re-set −4 dB. | `tests/intake_levels.rs`: NA RMS monotonic idle < cruise < loaded < WOT, idle 4–8 kHz share 1 %, cruise < 250 Hz share 47 %. |
| X-025 knock | Per-cylinder knock-sensor loop (2°/event, 12° authority, 1°/s recovery) through the spark shift; end gas isentropic at γ 1.32 from BDC (the single-zone charge ran ≈ 55 K hot at spark). Knock off bit-identical. | After X-024 only ≈ RON 95 stays within the 12° authority at 6000/1.0 (100 → 1 events per 100 cycles); lower octane saturates it. |
| X-017 wave/valve | Passive joint junction (`src/physical/wave_junction.rs`): each open-valve substep solves f = F(p₀ − Z f) with the monotone orifice, p = p̄ + 2p⁻ + Z·q; per-substep contraction on waves. Exhaust primaries only, behind `experimental.wave_coupling` (default off). | Off: 48/48 final-proof WAVs identical. On (branch base): idle spectra I4/V8/V12 96–98 % < 250 Hz; closed-loop ringdown 1e-15 vs 0.75–0.99 for the rejected delayed scheme; tuned rpm ∝ 1/L within 4–10 %. |
| X-013 latency | GUI line "Output latency ≈ N ms (device X)" from cpal `playback − callback` + BESS queue + lookahead, Bluetooth warning; `--audio-check` reports device and latency. | This machine: WH-1000XM4 AAC ≈ 290 ms, analog output ≈ 61 ms, BESS's own share ≈ 43 ms. |
| X-021, W-007.3, W-008 | UI within D-033: duplicates removed (second Listen, second RPM readout, second Idle gain, two camera selectors), combustion/geometry controls moved out of Sound shaping, ~20 help lines to tooltips; all 10 `EngineTuning` overrides in their part sections with derived display, per-field and per-group reset; dyno previous curve dashed, "Pin as reference" with Δ peaks; "sound only" tags on length controls. | Controls counted headless (`src/control_count.rs`): I4 151 → 165, V8 turbo 176 → 191 (the override sliders outnumber the removals). |
| X-014 CPU | Release profile fat LTO + one codegen unit (−5 %, bit-identical). Profile and ranked plan: [CPU profile](PHYSICAL-CPU-PROFILE-2026-09-29.md). | — |

## Verification

- dev17: `cargo test --release --no-fail-fast` 272 passed, 0 failed; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- `final_proof` 48/48 clips, none clipped. Versus the previous `main`: NA exhaust and mix within +0.9 dB (WOT centroid 343 → 307 Hz); turbo exhaust through the turbine restriction is brighter under load (WOT centroid 322 → 508 Hz, loaded 172 → 254 Hz, −0.9 to −1.3 dB) and darker at idle (146 → 77 Hz, −1.0 dB).
- Intake after all merges, versus the pre-2026-09-29 layer (NA): idle −46.0 → −67.1 dBFS, cruise −34.1 → −41.3, loaded −29.3 → −26.1, WOT −25.9 → −23.5 (peaks +5.6 dB), high −24.6 → −21.5.
- CPU, V12 at 96 kHz on dev17 (Ryzen 9 3900X, alternated runs): 6.34 µs per step on the previous `main` → 7.35 µs (+16 %) even with fat LTO; the runners and turbine restriction dominate the increase. Wave coupling on adds +20–28 % more (branch measurement).

## Open

- Listening: turbo exhaust timbre, very quiet idle intake, cruise intake −7 dB, WOT intake peaks.
- Coupled tuning too strong (±20–30 % local torque) and not re-measured on top of the X-024 runners.
- Single air cv(T) gives γ ≈ 1.39 in compression; indicated torque probably 5–10 % high and knock pessimistic.
- Re-matched compressor surges at part load / low-rpm WOT in the 0D loop.
- Knock slider mapping (ON = 95 − 25·knock) saturates the control above ≈ 0.1.
