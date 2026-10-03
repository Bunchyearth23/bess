# Adjustable exhaust decay — 2026-10-03

The user reports excessive exhaust reverberation even with Room Off. The shared exhaust model previously fixed its propagation-loss duration at 120 ms. Muffler absorption changes packing losses in the muffler only, so it cannot independently shorten all internal reflections.

## Delivered control

`Exhaust decay (ms)` is visible alongside header and tailpipe length in both imported Automation and free engine editors. Its range is 10–250 ms. Lower values damp reflections faster; 40 ms is a useful starting comparison. The default stays at 120 ms to preserve existing projects. No muffler is required.

The value is stored in the common engine definition as `sound.exhaust_decay_ms`, validated at project/engine boundaries, and carried into listening, WAV and BeamNG generation. It is independent of the listening Room effect. Both sound-shaping resets preserve it because it belongs to acoustic propagation rather than observation EQ. Changing parts also preserves it.

The ordinary primary waveguides and downstream collector/muffler/tail network use the duration to set per-pass losses. A running change interpolates those loss coefficients over 30 ms without clearing delayed waves. This duration is a loss-model parameter, not a promise that the complete audible response reaches −60 dB in exactly that time: pipe dimensions, junctions, filters and muffler packing also contribute.

Optional finite-volume primaries retain their conservative solver and previous observation losses; only their downstream network follows this control. With pressure feedback enabled, exhaust damping can also change cylinder feedback and torque, and therefore participates in the dyno cache key. Without feedback, the change affects only exhaust observation.

Original Automation reference A and source archives are retained. Existing WAVs or BeamNG packages must be regenerated to contain a newly chosen setting.

## Verification

- **348 release tests passed, zero failures, six excluded diagnostics**, across 45 targets. Strict release Clippy and formatting pass. The executable and both comparison examples build successfully. Graft's wiring graph is synchronized; its semantic tier is not built.
- Loss interpolation, retained wave buffers, normalized impulse tails at 48/96 kHz, prepared retunes without allocation/destruction, field validation, migration, persistence, dyno cache scope and visibility in both editors pass. The initial UI assertion mistakenly included the unit in the label; it now respects the existing separate unit suffix, with the failure log retained.
- The nine imported I4/I6/V8 operating points in `mechanical_review` produce **72 WAVs byte-identical** to the preceding mechanical-correction build at 120 ms, including raw stems and five application mixes per point.
- The imported B5C comparison changes only decay across 10/40/120/250 ms at 3000 RPM and throttle 0.5. All intake/mechanical stems are bit-identical, exhaust changes, and the source archive SHA-256 is unchanged. These raw voice diagnostics synthesize at 96 kHz, then use the same 95-tap FIR and 2:1 decimation as AutomationVoice to produce 48 kHz output. The separate downstream-network impulse runs directly at 48 kHz.

The isolated downstream network receives one impulse and then exactly zero input. Its remaining energy after 40 ms, divided by total response energy, is:

| Decay setting | Remaining energy fraction after 40 ms |
| --- | --- |
| 10 ms | 4.64e-21 |
| 40 ms | 6.92e-7 |
| 120 ms | 9.20e-4 |
| 250 ms | 7.34e-3 |

This establishes a change in the relative tail, beyond a uniform gain change. The complete running B5C's raw exhaust RMS also falls from 0.05458 at 120 ms to 0.01993 at 40 ms. The comparison page therefore provides both common-gain excerpts and a separate equal-RMS group (0.04), with a common peak ceiling of 0.5 and no limiter. These are presentation copies; raw diagnostics remain untouched.

Artifacts: `output/exhaust-decay-20261003/TESTER.html`, `review/review.json`, `neutral/review.json`, `verification.json`, `listening/manifest.json` and the adjacent logs. The page and media receive static file/format/reference checks, not a browser playback qualification. No new hardware-device endurance or BeamNG session is claimed for this build. The previous 60 s device result identifies the older executable in the mechanical-timbre report. User listening preference and BeamNG reception remain open.

Metadata correction: the earlier claim of direct 48 kHz synthesis confused the output sample rate with the imported voice's internal rate. The example's reported metadata, this report, the page builder's rate assertion and the recorded metadata were corrected; no audio was regenerated or modified. Original metadata and delivery hashes are preserved as `review/review-before-rate-metadata-correction.json` and `delivery-manifest-before-rate-metadata-correction.json`. `metadata-rate-correction.json` records before/after hashes and unchanged WAV hashes. The original listening manifest and page-builder script are also preserved; the current listening manifest references the corrected review metadata while retaining identical audio hashes.

Current executable SHA-256: `44149cc8cdc14b81ae12f292b61155c36b81c8175702d5fdbfea30883ff7d67c`.

## Prior executable

`output/exhaust-decay-20261003/bess-before.exe` preserves the preceding mechanical-correction build, SHA-256 `7b59a5b7e8877507e2dddbdfb29cea240d7bf01a30894402a6fd2234ca310d38`.

This is a local working-tree delivery; no commit, publication or mod installation is implied.
