# Scratch saturation at listening volume 0.80

The user identified the background crackling as saturation, including a new engine fitted with a muffler. The stock I4 reproduction confirms hard output clipping during acceleration at listening volume 0.8.

## Cause and correction

Listening volume is a linear multiplier: 0.8 is only -1.94 dB relative to unity. The muffler shapes and attenuates acoustic propagation; it cannot guarantee digital headroom after the fixed 16x listening gain, perspective filters and sound controls.

The former listener followed each arriving sample's peak with instant attenuation and a 150 ms release. This deformed strong peaks. The output resampler could then overshoot that already-limited signal and activate its hard -1 dBFS clamp. Merely producing finite, bounded audio had not detected this distortion.

The listener now uses the pinned BDSP `Compressor` and `DelayLine`: 3 ms lookahead, 0.05 ms attack, 150 ms release, -3 dB threshold, 1 dB knee, 100:1 ratio, and zero makeup gain. Gain is bounded above by unity. This reduces strong peaks before they arrive and reserves headroom for the resampler. The final -1 dBFS clamp remains an exceptional safety guard; the tested stock and shaped examples no longer touch it. No automatic loudness matching is introduced.

The fixed calibration is preserved to avoid making idle quiet again. For the stock I4, PCM from seconds 1–2 is exactly identical to the old recording after aligning the 144 output-frame delay at 48 kHz.

## Reproduction and evidence

`tests/output_headroom.rs` reproduces a new stock I4 with the default baffled muffler, outside listening, volume 0.8 and an eight-second 850–3500–850 rpm trajectory. It failed before the fix with **272 float samples at the hard ceiling**, and passes after the fix with zero. It also requires the idle to stay above -35 dBFS RMS and useful acceleration level to remain.

Fresh files were rendered with `cargo run --release --example sound_controls -- output/physical-saturation-20260928/before` and then the corresponding `after` directory. The underlying engines, trajectories, controls and output volume are unchanged. The files are not loudness-normalized.

| Variant | Hard-ceiling PCM samples before → after | Peak before → after (dBFS) | RMS before → after (dBFS) |
| --- | --- | --- | --- |
| Neutre | 271 → 0 | -1.00 → -2.92 | -12.73 → -13.89 |
| Grave | 1532 → 0 | -1.00 → -2.93 | -9.12 → -11.20 |
| Mordant | 161 → 0 | -1.00 → -2.95 | -12.57 → -13.41 |

The float/PCM count differs by one because of PCM24 quantization. Full metrics: `output/physical-saturation-20260928/comparison.json`. Editable projects and WAVs are in the adjacent `before` and `after` folders.

An independent temporary acoustic probe ruled out the primary-pipe ±200 kPa guard for stock I4 cases: six RPM points from 850 through 6000, throttle 0.05/0.75, startup and settled windows at 48 kHz. Zero guard activations; the maximum incoming forward wave was 107.51 kPa. The probe was removed without changing physical equations or waveguide behavior.

The new dynamics stage is checked at 8, 44.1, 48, 96, 192 and 384 kHz with tones, impulses and steps at amplitudes 0.1, 1, 4, 16 and 64. All measured native peaks stay below the safety ceiling without sample clipping inside that stage. At 48 kHz the largest tested peak is 0.743. Fitted residual distortion on an amplitude-four 70 Hz sine falls from 1.36% to 0.012%; quiet signals retain unity gain and silence stays silent. These measurements are not a subjective listening verdict or a mathematical peak guarantee for arbitrary unbounded input.

## Delivery scope

`cargo test --release --all-targets` passes **257 tests**, including the existing active-control zero-allocation/deallocation check. Full log: `output/physical-saturation-20260928/tests.txt`. Strict all-target Clippy, formatting and release build pass. The final `target/release/bess.exe` is 7,604,224 bytes with SHA-256 `7A00DDDEB86AB779423B1214F47D60CE8A9DA3E90FBCBFDB0B7F42EDD9DAE57A`.

The open application session was preserved by renaming its locked executable to `target/release/bess-running-before-saturation-fix.exe`. The usual executable is rebuilt separately. Reopen it after saving the current project to hear the correction. Imported-bank output does not use this scratch listener. No new in-game validation or subjective acceptance is claimed.
