# Gas, boost, coupling and CPU batch — 2026-09-29

Five agents merged on `gas-batch`, then into `main`. Builds, tests and renders on dev17 (`tools/dev17.sh`). Technical evidence only; nothing has been listened to.

## Changes

| Item | Change | Evidence |
| --- | --- | --- |
| X-026 gas properties | `Mixture` (air, gasoline vapour, stoichiometric products; linear cv(T) fits from NASA/GRI and Heywood) in every volume and port; energies share air's 298.15 K reference so LHV stays the whole heat release; valve flow uses the upstream gas's R/γ; knock end gas follows the frozen unburned charge's isentrope at its own γ(T). | Motored n 1.393 → 1.330 (2000 rpm); charge at −35° 675 → 617 K; gross η_i 0.42–0.45 → 0.39–0.42; dyno NA 170.6 Nm @ 4437, turbo 248.8 Nm, V8 366.8 Nm, V12 504.5 Nm (−2…−6 %). RON 95 knock within the 12° authority from 3000 rpm (2 % events, 7.6°), none at 6000. |
| X-027 compressor surge | Surge peak at a constant diffuser flow coefficient (stage-exit density scaling), choke inlet-referenced; after X-026, 1200 rpm WOT re-surged at the 0.75 × Vd charge Helmholtz mode (≈47 Hz, firing forcing), so the charge volume now covers compressor-to-throttle (hot pipe, intercooler, cold pipe) at 1.5 × Vd (≈33 Hz). | `tests/boost_stability.rs`: 8 steady points 0.1–12 kPa swing at ≥ firing frequency (were 30–70 kPa at 10–30 Hz); lift-off without BOV still surges (78 kPa, ≈9.5 Hz); BOV < half. |
| X-028 wave coupling (off by default) | Coupled valve port follows Benson/Blair pressure-amplitude-ratio superposition with implicit mass flux, safeguarded analytic Newton (2.3–2.8 orifice evaluations per open-valve substep, was ≈5); pipe γ/R from each collector's gas in the coupled path. | Port pressure ≥ 30 kPa (was the 2 kPa floor); V12 WOT level +4.3 → +1.9 dB; gains V8 +10 %, V12 +10 %, cast I4 +9 %; equal-length I4 +18.6/+23.9 % (default cam + VVT; mild cam 8–10 %); 1/L fit −11/−13 % (test tolerance widened 10 → 15 %); equal-length I4 level −3 to −15 dB. Not ready for default-on. |
| X-014 CPU | Nine bit-exact optimisations (single temperature inversion, early closed-valve reject, critical-ratio `powf` skip, manifold state/budgets once per step, in-place `Manifolds::step`, exact fmod/floor shortcuts, shared input validation once per substep, runner delay once per sample). | Branch: V12 7.87 → 6.68 µs (−15 %), I4 −13 %, 88/88 outputs bit-identical. |
| UI | Dyno curve moved under the spectrum (always visible while tuning). | GUI tests pass. |
| Q-002 | Knock slider spans RON 95 → 85. | — |

## Verification

- dev17: `cargo test --release --no-fail-fast` 278 passed, 0 failed; `cargo clippy --all-targets -- -D warnings` clean.
- Bit identity at each merge where no sound change was intended: X-014 merge 52/52 outputs identical; X-028 merge (coupling off) identical.
- `final_proof` vs the previous `main`: mix within ±0.6 dB everywhere; mechanical −0.9 to −2.5 dB under load and brighter (slower pressure rise); turbo intake +1 to +2.3 dB at load; NA intake within ±0.6 dB.
- CPU, V12 96 kHz on dev17, alternated runs: previous `main` 7.30–7.66 µs → 6.99–7.22 µs (≈ −4 % net: X-014 −15 % against X-026 +10–15 %).

## Open

- Listening: slower turbo lift-off flutter (≈9.5 Hz), quieter mechanical layer under load, turbo intake.
- Coupling: tuning too strong on equal-length I4 with the default cam/VVT, 1/L within 15 % only, level drop on equal-length I4, +24–29 % CPU when on.
- CPU below 5 µs on the laptop needs a build or sound decision: `target-cpu=x86-64-v2` (bit-exact on 88 outputs, a further −5 %), `x86-64-v3` (−7 %, ≤ 1 LSB differences), engine acoustics at 48 kHz (−12 %, orders ±0.8 dB).
- Products fit ignores dissociation; no evaporative cooling; NA peak 170.6 Nm sits at the 170 Nm test floor.
