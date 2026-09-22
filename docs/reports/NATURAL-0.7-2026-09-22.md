# BESS 0.7 — Controls and first per-bank calibration

Delivery: `dist/BESS-0.7.0.exe`. SHA256: `25BF33EEE19EEE9058F70BF3E5AEEED473F837AE6C3A49304632B6AF2E7CEEF7`.

## Delivered behavior

- Mouse wheel over a slider makes a bounded adjustment; Shift makes it ten times finer. Panel scrolling is consumed only over the hovered slider.
- **At high rpm** and **At full load** replace editing grids. They generate four distinct interpolated and smoothed curves, not just a global volume change. Old curves remain active until the controls are changed, with an on-screen indication.
- Import calibration uses two WAV descriptors: cycle similarity and relative adjacent-sample difference energy. Bass, rasp, exhaust and coloration gains adapt; intake and texture stay restrained. Added pops, turbo and fluctuation are off by default.
- **Added color** smoothly blends prepared source and acoustic treatment. Zero removes additions but does not select the original A branch or remove recorded source defects.
- Presets and demos use the calibrated basis. Existing projects retain their settings; **Adapt to vehicle / natural base** explicitly resets hybrid settings except A/B selection and level compensation.

## Evidence

- 37 passing tests: 15 library, 1 egui wheel interaction, 5 driving, 14 hybrid, 2 MVP. The egui test covers ordinary/fine wheel movement, consumption, no off-hover change and saturation; it is not a physical mouse test.
- Release build and strict linting passed without warnings.
- Twelve successful imports/renders in `output/corpus-0.7.0`: 16-second comparisons, comparable RMS, peak limits and projects verified. The page compares A source, B 0.7 and C 0.6; all twelve A files are byte-identical to 0.6. `corpus.json` records descriptors and settings.
- Executable launched and `natural-0.7-ui.png` inspected: simplified controls, calibrated Genesis import and integrated driving.
- Silent CPAL output on the Sound BlasterX G6 at 192 kHz for 30 seconds: 3,000 callbacks, 6.983 ms maximum, no overrun. `natural-0.7-audio.txt` records this. It used generic test settings rather than calibrated import defaults, establishing computation and transport but not audible quality.

## Limits and next work

S-06 delivered an initial **heuristic calibration of additions**. It does not measure physical resonances, identify active parts or deconvolve exhaust sound already in the source. No subjective realism gain is considered validated; user tests remain grouped under S-10. At the time of this report S-07 still required documented or explicitly entered engine data, combustion events and hybrid fallback. BeamNG export and in-game validation remained S-08/S-10.
