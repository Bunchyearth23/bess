# Mechanical timbre correction — 2026-10-03

## User observation and scope

The user finds the Automation engine foundation acceptable but rejects the mechanical layer as an abrasive, metallic buzz at every operating condition. This is a timbre correction in the shared physical observation, not a reduction of the mechanical slider. Original source A, imported metadata, gas/crank dynamics, intake and exhaust synthesis remain separate from this change.

The previous transfer sent head contacts, piston slap and differentiated cylinder pressure into the same eight resonant bands, nominally 1.08–8.4 kHz at neutral pitch. The neutral head/contact response had an energy centroid around 4.4 kHz. Its regression compared brightness to an older 2.4 kHz resonator, so it did not establish the naturalness of the result. The user's listening supersedes any implication that those earlier numerical checks accepted the timbre.

Historical X-019 measured combustion excitation growing about 17 dB per RPM doubling versus about 7 dB for contacts, exceeding contacts by 10 dB at 6000 RPM/0.7; the complete mechanical layer was 15.6 dB louder there than at 3000 RPM. Those describe the previous transfer/calibration, not the new nine-case results below, and were never subjective acceptance.

## Change

- Four broad block modes (nominally 312–1488 Hz) receive combustion and piston slap; four separate head modes (1080–4320 Hz) receive contacts. Existing bore, material and deterministic seed adjustments remain.
- Neutral modal Q is approximately 1–1.3, with separate structural/radiation roll-off at 1.8 kHz for the block and 3.6 kHz for contacts. Cast iron retains additional damping. This changes the frequency distribution and decay, with the existing pitch/resonance controls still active.
- Physical event timing and cylinder pressure remain the excitation. No additional free-running oscillator, continuous noise layer, RPM-dependent normalization or automatic gain is introduced.
- The piston-impact observation gain is recalibrated for the new lossy block path, preserving the existing target of roughly 7 dB below valve contacts under load. Impact timing, load dependence and the physical engine are unchanged.
- Optional knock retains its own existing bore/gas-dependent chamber ring and decay. It is observed separately at a fixed scale of `5e-8 sample/Pa`, preventing the softened block transfer from erasing intentional 5–9 kHz knock. It does not feed pressure, heat or force back into the solver. Its existing spectral and off-state tests remain unchanged.
- All new filter state is inline. Existing live retunes keep their 30 ms crossfade and preserve the gas/crank state. The common voice carries the correction into live playback, WAV and newly generated BeamNG loops.

These are bounded perceptual transfer estimates, not coefficients identified from recordings of these three engines. The existing approximate contact timing is not recalibrated by this patch.

The distinction between excitation and structural radiation is consistent with [Chiatti & Chiavola's SI-engine study](https://saemobilus.sae.org/papers/experimental-analysis-combustion-noise-spark-ignition-engine-2003-01-1422), the distinct contact transients in [Suh & Lyon's valvetrain study](https://saemobilus.sae.org/papers/investigation-valve-train-noise-sound-quality-i-c-engines-1999-01-1711), and the impact/transmission/radiation separation in [Dolatabadi et al.'s piston-slap study](https://knowledge.lancashire.ac.uk/id/eprint/32183/1/32183%201-s2.0-S0022460X15003466-main.pdf). The first two publisher abstracts and the third full paper support that design direction; they do not supply or validate BESS's numerical coefficients.

## Reproducible comparison

Artifacts: `output/mechanical-correction-20261003/TESTER.html`, `comparison.json`, `comparison.csv`, `before/`, `after/`, `matched/` and `audition/`. The unmodified previous executable and three runtime source snapshots are retained in that directory. No original car archive is modified or published.

`examples/mechanical_review.rs` renders the imported B5 C I4, Genesis I6 and Advent V8 at idle, 3000 RPM and 6000 RPM, clamped to the shared source/model range (the B5 C high point is 5712 RPM). Each point has one second of warm-up and two seconds of capture at 48 kHz. The corpus contains no Automation V12, so none is invented.

For every point the example stores raw float diagnostic stems and five PCM24 application outputs: mechanical alone, intake plus mechanics, full live mix, hood preview and cockpit preview. Application volume is fixed at 0.8; actual protection and two-emitter preview matching stay active. The example adds no normalization. Separate copies equalize mechanical AC RMS by attenuation only. To make these quiet isolated layers audible, the page's first pair then applies the same presentation gain to both equalized signals: target RMS 0.02 (−33.98 dBFS), common peak ceiling 0.5, no limiter. Actual maximum peak is 0.2104; no pair needs the peak ceiling. Both stages and their gains are recorded. Native application outputs remain unchanged. The page never autoplays.

All 144 input WAVs pass format/finiteness checks. The 18 raw intake/exhaust before/after pairs are byte-identical; source archives, engine definitions and settings also match. The mechanical-only comparison therefore does not rely on altering the underlying engine or its other source layers.

Across nine raw mechanical comparisons (Welch Hann, 8192 samples, 4096 overlap, DC removed):

| Measurement | Before | After |
| --- | ---: | ---: |
| Energy-weighted spectral centroid | 1.214–4.494 kHz | 0.619–1.941 kHz |
| Energy above 4 kHz | 4.59–39.60% | 0.30–5.62% |
| Energy above 8 kHz | 1.94–17.82% | 0.01–0.21% |

Mechanical RMS decreases by 1.40–6.57 dB depending on the case. The normalized spectral measures and equal-RMS audition copies distinguish the timbre change from that level change. These ranges cover multiple cases, not a single paired measurement or a perceptual score.

The actual hood preview also preserves the improvement through its level matching. At idle, its energy above 4 kHz changes from 6.30% to 0.28% for B5 C, 1.86% to 0.15% for Genesis and 1.40% to 0.17% for Advent; corresponding whole-mix RMS changes are only −0.57, −0.14 and −0.11 dB. Complete per-case results, including the less mechanical-dominated operating points, remain in `comparison.json`.

## Verification and delivery

The final `cargo test --release --all-targets -j 2` run passes **342 tests, zero failures, six excluded diagnostics**. Existing piston-level, knock-spectrum/off-state, physical invariance, live/WAV/export and allocation checks retain their criteria. A new normalized-brightness/transfer/decay regression covers the corrected mechanical response. Strict release all-target Clippy, formatting, release application/example builds and the synchronized Graft wiring graph pass.

The suite exposed a missing temporary fixture during concurrent tests. Timestamp-only fixture names are replaced by an atomic counter plus exclusive file creation; a sixteen-thread independent-lifetime regression passes in all four test targets sharing that helper. Earlier failed logs are retained. One subsequent compiler process ended with `STATUS_ACCESS_VIOLATION`; the complete run succeeds with two build jobs, without a source workaround or weakened test. The test-data change does not affect application sound.

Final executable: `target/release/bess.exe`, SHA-256 `7b59a5b7e8877507e2dddbdfb29cea240d7bf01a30894402a6fd2234ca310d38`. The imported Advent V8 device check completes 60.000 s on Sound BlasterX G6, 48 kHz stereo float transport, 96 kHz synthesis and a 40 ms queue: 5033 callbacks, zero underruns, missing frames, callback overruns or producer/physical failures; maximum callback 0.024 ms, callback p99 ≤1% of deadline, maximum synthesis block 8.396 ms. Output volume is zero: this checks scheduling, not listening. The previous readiness report's 21-minute run identifies the preceding executable and is not attributed to this revision.

Static page QA resolves 218 references to 110 files, with 108 audio controls and no autoplay. All comparison input hashes remain unchanged; the eighteen audible presentation copies are finite PCM24 mono 48 kHz and match their common target RMS. No browser visual inspection is claimed.

The user still needs to judge the corrected texture and balance by listening in the application and in newly generated BeamNG audio. Existing installed variants and earlier audition WAVs retain their old timbre until regenerated. No claim of subjective acceptance, measured physical authenticity, BeamNG playback or mod installation is made.
