# Standalone instrument validation — 2026-09-23

## Automated checks

`cargo test --release --test standalone` passed seven tests: 25 monocylinder and 100 four-cylinder firing events per second at 3,000 rpm; one- and four-second boundary counts; identical output across block sizes 1, 257 and 48,000; deterministic variable-rpm output; zero-rpm freeze with acoustic tail; explicit fuel-cut/motoring/zero-load behavior; phase and route sensitivity with name invariance; JSON and nonfinite-value rejection; finite bounded output at 8, 48 and 192 kHz over 0–12,000 rpm, including a 12-cylinder high-rpm case; load-dependent waveform shape. `cargo test --release --all-targets` passed the full 92-test repository suite, and `cargo clippy --release --all-targets -- -D warnings` passed. The expanded sample-rate test was rerun separately after the full-suite pass.

Six four-second, mono 48 kHz / PCM24 WAVs were rendered to `output/standalone-validation-20260923/`, which is local and Git-ignored. No per-file loudness normalization was used.

| File | Combustion events | AC RMS | Peak | Mean |
| --- | ---: | ---: | ---: | ---: |
| `single-steady.wav` | 100 | 0.017376 | 0.061473 | 0.000001 |
| `four-even-steady.wav` | 400 | 0.029461 | 0.067940 | 0.000022 |
| `four-split-steady.wav` | 400 | 0.028318 | 0.089499 | 0.000035 |
| `four-even-ramp.wav` | 456 | 0.063727 | 0.374652 | 0.000020 |
| `four-even-load.wav` | 400 | 0.028262 | 0.069691 | 0.000024 |
| `four-even-shutdown.wav` | 200 | 0.021762 | 0.067940 | 0.000000 |

The level/spectrum script found zero samples at the ±0.98 safety ceiling in all six WAVs. In the last 8,192 samples of the even four-cylinder steady WAV, the strongest resolved peaks were near 99.61, 199.22 and 398.44 Hz. The four-cylinder combustion event rate is 100 events/s; other peaks are harmonics or resonances. The single-cylinder's strongest peak near 175.78 Hz reflects the modeled block resonance, despite its 25 events/s firing cadence. The moving-rpm clips' single short-window FFT is not a stable pitch estimate.

Rendering the same four-cylinder preset from its saved JSON produced a WAV with the same SHA-256 as rendering by preset name.
The `--compare` command rendered all three presets under identical commands without level matching; each WAV was byte-identical to its separately rendered steady scenario. The report is in `output/standalone-validation-20260923/comparison/comparison.txt`.

The final build's ten-second offline benchmark at 48 kHz, 256-sample blocks, four-split configuration and rpm ramp measured 480,000 frames in 0.091 s, about 109.7 times faster than audio duration on this machine. This includes no audio-device scheduling and is not a worst-case callback guarantee. A three-second muted CPAL check opened 48 kHz, two-channel float output and processed 300 callbacks with a maximum measured callback body of 0.184 ms. The mono instrument sample was duplicated; the device mix and subjective sound were not evaluated. Results vary with hardware and load.

A second muted interactive run sent six valid edits (rpm, load, exhaust, intake, block and fuel-cut state). The callback reported all six applied, 1,792 callbacks and a maximum measured callback body of 0.213 ms. This checks the command path without judging audible quality.

## Remaining evidence

The user subsequently reported metallic hiss and electrical buzzing in all initial standalone clips. Revised defaults and freshly rendered WAVs are documented in the [noise correction report](reports/BESS-NOISE-CORRECTION-2026-09-23.md). The six WAVs above remain the original baseline; listen to `output/standalone-noise-fix-20260923/` for the correction. Perceptual acceptance remains open.

The pulse window and noise filtering have no quantified alias-rejection spectrum yet. The simplified delay/reflection network and block resonator are numerically stable over tested ranges, but mechanical realism, exhaust geometry and sound quality have not been established. Listen to held rpm, ramp, load change and shutdown on the intended output device; compare named presets at matched user volume. Existing Automation-guided BeamNG export remains a separate workflow and requires its own in-game validation.
