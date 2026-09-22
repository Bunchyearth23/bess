# BESS 0.6 — Segments and maps

Instruction: continue without intermediate listening feedback and group user tests at the end under [the work sequence](../STEPLIST.md).

## Delivered demonstrations

- `dist/BESS-0.6.0.exe`, SHA256 `7086AB345EEC7890ED9FBDD036B09FDCD41B86ABFB8E488E81EE013BA482CD31`.
- `output/corpus-0.6.0/index.html`: A source, B 0.6 and C 0.5 for all twelve vehicles. All twelve A references are byte-identical to 0.5.
- Four banks include isolated components: B5 A, Genesis Phantom, Thunderhawk Zero and Volk Icarus II. Maximum reconstruction error ≤ 2.9802322e-8.
- `output/maps-0.6.0`: four six-second WAVs and projects for Genesis Phantom. At mid-rpm, compare neutral/mapped at low load, then neutral/mapped at full load. RMS is not matched, to retain the effect of gains.
- A native UI capture was inspected with Genesis imported, maps visible and driving integrated on the right. The historic capture is not included in the public source package because it shows the former French interface.

## Implementation at 0.6

B read four-cycle segments with starts two cycles apart and complementary cosine windows. Start positions were integer cycle multiples in the source loop and advanced five cycles per segment. A period had to be accepted and the loop at least eight cycles long; otherwise prior playback remained. This rearranged some temporal texture without creating new engine measurements. BESS 0.8.4 later replaced segment jumps with continuous prepared-loop playback after Cerberus level pumping was reported.

Rpm-neighbor blending used a curve with zero slope at the ends; identical weights applied to the source and its periodic component. A retained its former preparation, playback and blending. Before gains, texture remained the exact complement of periodic content.

Four 3 × 3 maps controlled pulses, texture, intake and exhaust. Columns were bank minimum, middle and maximum rpm; rows were 0/50/100% load. Multipliers ranged 0–2, with bilinear interpolation and 40 ms smoothing during edits. Missing maps in older projects stayed neutral. They fit into audio commands without allocation during processing. Character presets also reset these sound settings.

## Checks

- 34 release tests passed: 15 library, 5 driving, 12 hybrid, 2 MVP. New checks covered continuity over 500 segment joins, map interpolation, invalid-value rejection, neutral legacy maps, regional effect and save/load. Allocation and common-transport tests remained.
- Twelve imports and 24 comparison renders passed corpus format, level, duration and A/B RMS checks.
- Genesis demo: neutral/mapped RMS difference exactly zero at low load and 0.03023585 at full load. This shows a local effect, not listening preference.
- Release build, strict linting and formatting passed; UI capture was inspected.
- CPAL check: 30 seconds on the Sound BlasterX G6 at 192 kHz stereo, 3,000 callbacks, 7.262 ms maximum, zero overruns. Volume was zero, so audible artifact absence was not established. Additional segment reads increased cost.

S-03 through S-05 were technically demonstrated; listening and manual use remained S-10. Per-bank calibration, documented engine excitation and BeamNG export were still open at this stage. Neither in-game testing nor completion of the overall objective was claimed.

## Reproduction

```text
cargo run --release --example corpus -- cars output/new-corpus
cargo run --release --example maps_demo -- cars/bunchyearth23_genesis_phantom.zip output/new-maps
cargo run --release --example components -- cars/bunchyearth23_genesis_phantom.zip output/new-components
```

Use new folders; existing references are not overwritten.
