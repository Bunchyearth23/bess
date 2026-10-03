# Mechanical excitation rework — 2026-10-03

## Rejected sound and attribution

The user rejects the mechanical sound after the first damped-transfer revision, both in the application and in the isolated clips. They also reject both subsequent diagnostic samples: contacts at idle and combustion at 3000 RPM. This supersedes any suggestion that the first spectral improvement established a satisfactory timbre. X-019 remains a listening acceptance gate.

The matched baseline separates contacts, combustion and piston slap through three copies of the same linear block/head transfer, including the complete warm-up. At 3000 RPM the old isolated combustion path puts 99.4–99.8% of its spectral power into ten narrow peak neighborhoods and has a 720-degree waveform correlation of 0.996–0.999. Contacts dominate the idle observation but do not have that repeated waveform. These are two different excitation problems, not evidence of a single fixed-frequency whistle.

The first source-isolation files used direct 48 kHz synthesis. They remain preserved as the samples the user actually heard. The corrected baseline uses the application's default 96 kHz synthesis and 95-tap decimation to 48 kHz. Its nine total-mechanical WAVs are byte-identical to the preceding delivered build's raw mechanical WAVs. New comparisons use this matched baseline exclusively.

## Delivered source changes

- Contacts use a finite 0.15 ms attack and a short 0.8 ms decay instead of a one-sample click and the previous white-noise burst. Each event has bounded variation and a broad, non-resonant texture. The actual net-lift closing angles include cam duration, lift, lash, intake VVT and the number of valves. No arbitrary pair of fixed crank angles remains.
- Combustion observes chemical heat separately for each cylinder. The observation accumulates `(gamma - 1) * heat / volume` across gas substeps and converts it to a thermal pressure rate once per audio frame. It no longer differentiates summed total cylinder pressure, which also includes compression, pumping and numerical effects.
- Each cylinder excites its own short, smoothly gated texture. A small coherent contribution retains the engine's rhythm. There is no autonomous oscillator, permanent noise floor, runtime normalization or automatic gain. The source and structural transfer coefficients remain perceptual estimates, not measurements of a particular real engine.
- Contact and combustion random streams are independent of the intake and the gas model. The existing intake random sequence is preserved. Piston slap, optional knock, the block/head transfer, output protection, layer controls and live retune fades remain in the common voice.
- Source state is inline and resets deterministically. A mechanical observer does not return heat, force or pressure to the physical solver. All origins, playback and newly generated WAV/BeamNG audio use the same corrected physical voice; original source A remains available.

The diagnostic `MechanicalProbe` must be created with a fresh engine, then fed every excitation including warm-up, at fixed sound tuning. It is an offline attribution tool, not a second product renderer.

## Evidence and listening

Artifacts are under `output/mechanical-rework-20261003/`: the previous executable and source snapshots, the initial and matched baselines, isolated candidate sources, raw stems and application mixes, comparison metrics, equal-level PCM24 copies and the listening page. The three real imports are B5 C I4, Genesis I6 and Advent V8, each at idle, 3000 RPM and the highest shared point up to 6000 RPM (5712 RPM for B5 C). No Automation V12 reference is invented.

The source-isolation reconstruction passes at 48/96 kHz; the nine candidate imported runs reconstruct total mechanical output to less than 6.6e-7 relative RMS. Tests also establish silence of combustion excitation during motoring, decay after spark cut, matching cam closure geometry, independent observer streams, repeatable reset and consistent source power across 48/96 kHz. The existing piston-balance criterion passes unchanged: -8.6 dB versus contacts under load and -15.7 dB at idle for the default test engine. Intentional knock and physical invariance checks pass.

Welch spectra use 32768-sample Hann windows with 50% overlap at 48 kHz. The ten-peak measure integrates ±3 Hz around distinct local peaks from 100 to 8000 Hz; it is a reproducible diagnostic, not a psychoacoustic score. Whole-layer results are:

| Engine / point | Ten-peak energy before → after | 720° correlation before → after | Raw RMS change |
| --- | ---: | ---: | ---: |
| I4 / 3000 | 59.9% → 25.1% | 0.592 → 0.256 | -0.67 dB |
| I6 / 3000 | 72.2% → 22.3% | 0.718 → 0.186 | -1.00 dB |
| V8 / 3000 | 56.4% → 26.7% | 0.541 → 0.205 | -1.54 dB |
| I4 / 5712 | 92.0% → 38.5% | 0.920 → 0.315 | -7.62 dB |
| I6 / 6000 | 89.7% → 25.2% | 0.892 → 0.201 | -7.06 dB |
| V8 / 6000 | 74.0% → 18.1% | 0.729 → 0.095 | -5.45 dB |

Isolated combustion at 3000 RPM moves from 99.4–99.8% ten-peak energy to 29.3–37.0%, and from roughly 0.997 cycle correlation to 0.295–0.302. The small coherent component deliberately preserves some engine orders. At idle, total RMS changes by -2.46 dB (I4), +3.06 dB (I6) and -3.05 dB (V8). The combustion contribution at idle rises by 24.8–29.0 dB from a very small previous level; it is not correct to describe every case as quieter. The equal-RMS listening copies separate these level differences from texture. Source-energy ratios are not additive partitions because the components can correlate.

Contacts at idle remain weakly correlated across cycles. Their centroid falls from 1.95–1.99 kHz to 1.79–1.86 kHz, but their shorter envelopes are more intermittent and their peak/RMS ratios rise from 8.48–10.19 to 10.58–13.29. A finite attack therefore does not prove that the user will find the new contacts less harsh. The detailed contact/envelope measurements remain in `comparison/REPORT.md` beside the raw JSON/CSV.

The page offers 72 PCM24 comparison copies at equal AC RMS, normally 0.04, without a limiter. The I6 idle-contact pair uses the same lower target of 0.0376199 for both files to respect a 0.5 peak ceiling. Native application mixes keep their own gain and protection. Static checks resolve all 91 references on the entry page and 34 on its detailed page; their 46 audio references decode correctly and neither page autoplays. Browser rendering/playback is not qualified.

## Verification and executable

The complete release suite passes **357 tests, zero failures, six excluded diagnostics across 46 targets**. The earlier targeted run passed 129 physical tests. The existing tests for optional knock, piston balance, physical invariance, common live/WAV/export routing, sound persistence, retune fades and allocation-free rendering retain their criteria. Strict release all-target Clippy and formatting pass, and the release executable is rebuilt.

Clippy identified an indexed loop in the new offline reconstruction diagnostic. It now traverses the four captured streams with iterators; this only changes post-render error measurement, not synthesis. The initial failed Clippy and formatting logs are preserved. The final diagnostic is rebuilt and all 36 of its re-rendered WAVs are byte-identical to the candidate (`final-diagnostic-identity.json`); no production-audio change followed the complete test run.

All 72 candidate application/diagnostic WAVs pass format, digest and finiteness checks; all 45 application PCM24 outputs remain below clipping. The eighteen intake/exhaust pairs and nine isolated piston-slap pairs are byte-identical across the rework. The nine isolated mechanical totals exactly match the common application's raw mechanical outputs. Source archives, engine definitions, settings and rendering procedures match. These results and the final log exit-code/hash receipts are recorded in `verification.json`, `validation-status.json` and `delivery-manifest.json`.

Executable: `target/release/bess.exe`, SHA-256 `c5821cc5bfe86288c90539f7fda5c32c5fff434949a01c96cc84e6c048edfe6c`. The previous executable remains archived with SHA-256 `44149cc8cdc14b81ae12f292b61155c36b81c8175702d5fdbfea30883ff7d67c`. Graft's synchronized wiring check passes; no semantic/deep-index validation is claimed.

## Historical metadata correction

During the matched-baseline check, the preceding exhaust review was found to label its voice synthesis rate incorrectly as 48 kHz. Its actual default path was already 96 kHz followed by FIR95 decimation to 48 kHz. The example, report and review/listening manifests now describe that path correctly; the isolated downstream impulse remains direct 48 kHz. Original metadata snapshots and a correction manifest are preserved. All 96 existing exhaust-review WAVs are byte-identical across this metadata-only correction.

## Acceptance boundary

Numerical tonal reduction and finite, reproducible output do not establish a natural mechanical sound. The user must judge the new isolated layers at equal volume, the complete mix and newly generated BeamNG audio. Prior installed mods and comparison WAVs retain their old sound until regenerated. No new audio-device run, browser playback, BeamNG runtime, publication or mod installation is claimed by this delivery.
