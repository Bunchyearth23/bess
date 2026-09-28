# Scratch listening level correction — 2026-09-28

The user reported barely audible scratch engines near −50 dBFS with listening
volume at its old maximum of 0.8. The stock I4 reproduces the low signal:
−53.98 dBFS RMS outside and −60.30 dBFS RMS in the cabin at idle, volume 0.8.
These RMS measurements differ from the GUI's peak meter.

The listening slider and saved-parameter validation now accept 0–1. Scratch
listening and its shared WAV transport add a fixed ×16 (+24.08 dB) after the
perspective filters and before the existing −1 dBFS peak limiter. There is no
automatic quiet-signal normalization. Imported-bank synthesis and BeamNG
calibration do not pass through this gain. Stored listening values are retained.

## Measured levels

48 kHz, three-second steady renders, first second excluded, volume 0.8.
Experimental voice, stock I4 derived from the builder, idle load 0.05:

| Position | Before RMS | After RMS | Before peak | After peak |
| --- | ---: | ---: | ---: | ---: |
| Outside | −53.98 | −29.90 | −37.87 | −13.79 |
| Tailpipe | −48.92 | −24.84 | −32.46 | −8.38 |
| Cabin | −60.30 | −36.21 | −45.94 | −21.86 |
| Engine bay | −61.37 | −37.29 | −44.83 | −20.75 |

All values are dBFS. The explicit-event voice also gains 24.08 dB on the stock
engine. Open V12 renders engage the limiter, reaching −1 dBFS peak; their
dynamics can therefore be compressed at high listening settings. This is a
listening calibration, not a physical sound-pressure measurement.

## Validation and delivery

- `cargo test --release`: 123 passed, zero failures. New checks cover idle
  audibility for both voices in four positions, mute and 0.5/0.8/1 scaling,
  project round-trip at 1 and rejection outside 0–1. Extreme open-engine
  cycle coverage now runs at volume 1. Existing lift-off and rebuild checks pass.
- `cargo clippy --release --all-targets -- -D warnings`: passed.
- `cargo build --release --bin bess`: passed; `git diff --check`: passed.
- Repository-wide `cargo fmt --check` reports existing formatting differences;
  the new probe and MVP test file pass their formatting check.
- Reproduce the measurement matrix with `cargo run --release --example scratch_levels`.
  Local before/after CSVs and logs are under `output/scratch-*20260928*`.
- Rebuilt `target/release/bess.exe`, SHA256
  `33B9C6F33CBBABB5FDA0A24CBC87E6AB39D358D756593AD656EFDF1B1C4A73C4`.
  The running old executable was preserved as
  `target/release/bess-before-volume-fix-20260928.exe`; that session was not stopped.
  Restart from the usual executable path to use the correction.

No new subjective listening acceptance, hardware-output validation or BeamNG
gameplay validation is claimed.
