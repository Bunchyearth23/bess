# Realism batch, dyno and tuning model — 2026-09-29

Seven parallel branches merged on `realism-batch`, then into `main`. Technical evidence only; nothing here has been listened to.

## What changed

| Area | Change | Fixed calibration |
| --- | --- | --- |
| Intake tone | `src/physical/intake_acoustic.rs`: per-cylinder delay-line runners (valve R = 1 shut, 0.4 flowing), plenum 1.25 × displacement, throttle duct 0.35 m, airbox 4 × displacement, snorkel 0.25 m; ITB = open trumpets at 0.5 × `intake_length_m`. Monopole radiation at 1 m. Replaces the summed-flow 750 Hz layer. Impulse peaks 45/99/284/524/678/1013 Hz vs hand values ≈64 (coupled)/100/230 (raised by the plenum)/500/660/1000 Hz. | 1.08e-3 per Pa |
| Intake noise | `Radiation` breath: duct shedding ∝ U³ with a Strouhal band on a 15 %-of-bore plate scale; throttle jet hiss from the isentropic pressure ratio (Lighthill ṁU⁷), Q 12 edge tone below the choke ratio 0.528; turbo blade-pass whine (6 blades + splitter order, ∝ ṁ·U_tip², faded by 0.45 fs). | DUCT 0.0158, HISS 0.0286, WHISTLE 0.015, WHINE 9.2e-5 |
| Mechanics | `src/physical/mechanical.rs`: piston slap on side-thrust reversal ((p − p_atm)·A − m·ẍ)·tan β into the modal block input; −7.0 dB vs valve contacts at 3000 rpm/0.7, −12.6 dB at 850/0.1. Opt-in knock (`experimental.knock`, default 0): Livengood–Wu with Douaud–Eyzat (ON = 95 − 25·knock), chamber ring 1.84·c/(πB) ≈ 6.5 kHz, observation only. | slap 7.5e-7, ring 0.05·p·(1 − x_b) |
| Listening | `src/room.rs`: uniformly partitioned FFT convolution, synthetic Garage / Open road / Parking hall, equal-power wet/dry, 30 ms room crossfade; live scratch listening only, default Off bit-identical; exports and level analysis never construct it. `rustfft` 6.4 added as a direct dependency (already built by bdsp). | IR energy-normalised 100 Hz–4 kHz |
| Dyno | `src/dyno.rs`: WOT sweep of the physical engine (8 kHz, whole-cycle brake torque = gas − friction, per-point settling); builder panel plots torque/power with peaks and hover; computed peak sizes the clutch unless set by hand. | — |
| Tuning model | `EngineTuning` (`src/engine_build.rs`) with optional overrides (cam duration/lift/LSA/intake advance, valve/bore ratios, rod/stroke, throttle mm, plenum ratio, turbo size), one `resolve()` feeding `CylinderConfig`, `Manifolds`, `Induction`. `Scratch.tuning` skipped from JSON when derived, never hashed into `Scratch::life`. No UI yet. | resolve(None) bit-equal to the former formulas |

## Verification

- `cargo test --release`: 247 passed, 0 failed. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.
- Branch-level evidence (from each branch before merging): knock off with slap zeroed hashes identical to base; tuning model 29/29 PCM/CSV hashes identical; room Off bit-identical, partitioned vs direct convolution error 3.5e-7, zero allocations while switching rooms; intake network stable at 8–384 kHz and 0.15–1.5 m.
- CPU, V12 at 96 kHz (`examples/physical_engine`, alternated runs, loaded laptop): 7.67/8.57 µs per step merged vs 7.16/8.01 µs on the previous `main`, ≈ +7 %. The Hall room adds ≈ 5 % of a core on the producer (branch measurement).
- Dyno, default I4 (25 points, 850–7000 rpm): NA 169.7 Nm @ 1106 rpm, 89.6 kW @ 6999 rpm (builder estimate 193 Nm / 105 kW); turbo 0.8 bar 222.2 Nm @ 4181 rpm, 103.2 kW @ 4693 rpm, max MAP 146 kPa. NA sweep ≈ 0.9 s, turbo ≈ 20 s.

## Final proof (merged vs previous `main`)

`cargo run --release --example final_proof -- <dir>` renders six points (idle 850/0.1, cruise 2000/0.3, loaded 3000/0.7, WOT 4500/1.0, high 6000/0.7, 1000→6000 rpm pull + lift-off) × NA / turbo 0.8 bar × exhaust / intake / mechanical / mix, 48 kHz, fixed gain; `python3 tools/proof_compare.py before after` measures them. 48/48 clips rendered, none clipped, no solver failure.

- **Exhaust:** unchanged at every point (≤ 0.2 dB).
- **Mix:** within 0.3 dB everywhere; the exhaust dominates the default mix.
- **Mechanical:** +0.3 to +0.7 dB, centroid within ±200 Hz.
- **Intake** (RMS dBFS, centroid Hz, before → after):

| Point | NA | Turbo |
| --- | --- | --- |
| Idle | −46.0 → −49.7, 940 → 2985 (26 % in 4–8 kHz) | −46.2 → −49.7, 941 → 2989 |
| Cruise 2000/0.3 | −34.1 → −39.7, 921 → 116 (94 % < 250 Hz) | −33.7 → −39.3, 938 → 150 |
| Loaded 3000/0.7 | −29.3 → −27.1, 855 → 434 | −28.7 → −28.0, 904 → 328 |
| WOT 4500/1.0 | −25.9 → −20.9, 868 → 1196 | −24.8 → −24.9, 929 → 2004 |
| High 6000/0.7 | −24.6 → −20.0, 969 → 1269 | −24.0 → −20.2, 964 → 1742 |
| Pull + lift-off | −28.9 → −25.2, peak −12.5 → −4.9 dBFS | −28.0 → −27.2 |

The two intake branches were calibrated separately against the former layer; together the intake is now quieter and dull at cruise, brighter (throttle hiss) at idle, and up to +5 dB at WOT/high rpm with peaks 7 dB higher.

## Open (listening and model)

- Idle throttle hiss puts 26 % of intake energy in 4–8 kHz, the region of the earlier user hiss complaint (X-009).
- Dyno shape: NA torque peaks near 1100 rpm and falls steadily; turbo reaches ≈ 0.45 of its 0.8 bar target and loses boost above 4400 rpm at the 172k rpm shaft limit. A model issue shown by the dyno, not a dyno defect.
- `IntakeAcoustic` sizes its throttle bore from displacement, not from the new `intake.throttle_mm` override.
- The optional `tone.rs` intake resonator now stacks on the physical runner resonance when enabled.
- Gains and geometry constants are estimates (X-018); listening acceptance remains W-006.6.
