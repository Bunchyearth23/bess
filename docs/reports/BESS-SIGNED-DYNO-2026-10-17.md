# Signed dyno display — 2026-10-17

X-040 concerns the dyno display: negative brake torque and power were clamped
to the zero row even though the computed curve and hover values retained their
sign. A failed simulated operating point could therefore look like a flat
zero plateau.

## Correction

`src/main.rs` derives both vertical scales from every torque/power sample in
the current and pinned/previous curves. If either curve has a negative sample,
both axes use symmetric signed ranges and share a visible zero baseline.
Otherwise the historical positive-only range and round tick spacing remain.
Solid curves, dashed references, peak marks and hover marks use the same signed
coordinate transform. No physics, dyno settling or saved configuration changes.

## Validation

The regression in `src/workshop_tests.rs` renders actual egui frames for
positive-only, mixed-sign, entirely negative and positive-with-negative-reference
curves. It inspects both solid paths against the drawn zero baseline, verifies
distinct negative samples retain distinct heights and checks that the larger
negative reference is included in both scales.

All **41 application tests pass in the optimized release profile**, including
the final regression inspecting both torque and power paths. The initial
development-profile run also passed 41 tests. Strict all-target Clippy passes
again on final source; formatting and diff whitespace checks pass.

Reproduction:

```powershell
cargo test --offline --locked --release --bin bess
cargo clippy --offline --locked --all-targets -- -D warnings
cargo fmt --check
git diff --check
```

Release test compilation took 2 min 9 s, and the 41 tests ran in 0.23 s.
The full physics/integration suite was not rerun for this display-only change.

## Remaining work

X-041 remains open: point convergence does not yet include slow thermal states
or expose a visible non-convergence status at the four-second cap. X-039 still
requires the user's corrected live/saved-project reception; X-042 still requires
cold-start/transport qualification. Work-item parent states are unchanged.
This source correction does not establish native-window visual acceptance,
listening, real-engine calibration or BeamNG behavior. The Desktop executable
has not been replaced in this session.
