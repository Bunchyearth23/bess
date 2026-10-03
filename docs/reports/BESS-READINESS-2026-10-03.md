# Complete workshop delivery and user reception — 2026-10-03

The user requested completion of the remaining work and explicitly requested all work items to be `landed` before their own tests. In this delivery, `landed` describes implemented, integrated, technically checked software available for those tests. It does not record a listening preference, an actual BeamNG session, or calibration against an unavailable measured engine.

## 1. Imported vehicle and shared workshop

The physical engine remains common to Automation and free creation. Import now also reads a uniquely selected vehicle configuration and its reachable active parts. Literal JBeam data, simple named variables, selected slot defaults and explicit `.pc` overrides supply the listening bench's declared mass, forward ratios, final drive, tyre radius and clutch reference when unambiguous. Expressions are not executed. Missing or contradictory values preserve the estimated bench value and have an explanation in the interface.

The bench supports one to twelve forward gears. The original six-element JSON field remains compatible; old projects default to six gears. New imports apply available vehicle values; reopening an existing project retains its saved driving settings. A separate restore action applies source vehicle values intentionally. The declared torque sizes the clutch; driving torque still comes from the shared cylinder solver. Vehicle physics inside the source ZIP and BeamNG export are unchanged.

Integration review also corrected four boundaries: part edits retain imported/manual clutch and vehicle controls; reducing and restoring the gear count retains valid hidden custom ratios; invalid engine drafts cannot close the working audio chain when changing rate; the device-check CLI reports a failed log write as an error. The complete release suite covers the persistent settings and gear regressions.

## 2. Independent intake and exhaust tuning

Intake and exhaust cams have independent duration, lift and phase with distinct resets. Old shared overrides remain a compatibility baseline. ITB area and equivalent supply-volume semantics are explicit, and dormant legacy event timbre values do not secretly set physical layer levels. See [the tuning report](BESS-TUNING-COMPLETION-2026-10-03.md).

## 3. Optional acoustics and measured coupled level

The common editor exposes pressure-wave feedback, output-rate acoustics with high-rate gas substeps, an alternative low-speed VVT schedule, and a measured fixed exhaust-level correction. The correction worker only applies its result if the complete engine still matches the measured engine. It is a constant, not live RMS matching. Changes to the acoustic rate rebuild the audio chain. Geometry labels no longer incorrectly promise sound-only behavior when feedback is enabled.

Bank delay and bank gain are observation controls; they must not alter physical cylinder torque. The coupled dyno and length tests use a controlled calculation rate. Detailed evidence, including the strict ten-percent length-law check and measured CPU cost, is in [the advanced-physics report](BESS-ADVANCED-PHYSICS-2026-10-03.md).

## 4. Offline quality and reference calibration

Optional conservative finite-volume primary pipes are integrated into the shared rendering engine. Their cost is adaptive and they are exposed as offline WAV/BeamNG quality; selecting them closes live audio until the option is disabled. The regular default remains suitable for interactive listening. The finite-volume domain, conservation, CFL, positivity and small-signal comparison are documented in [the quality report](BESS-QUALITY-CALIBRATION-2026-10-03.md).

The shipped grid is 4 mm, finer than the original 20–40 mm proposal: it stays within 0.862 dB over the tested 80–4000 Hz grid, whereas 20 mm loses almost 50 dB in the worst 4 kHz case. The measured short I4 cost is 15–17 seconds per simulated second. Finite-volume primaries and pressure feedback cannot be combined while their gas-reference states remain inconsistent; validation and the interface both enforce this boundary.

The application can fit a bounded linear filter from a generated stationary WAV and a reference at matching RPM/load/microphone conditions. It writes a new WAV and reusable filter report, including results on held-out audio and input hashes. Neither input file changes. This is spectral calibration of an observation chain; it does not identify physical engine constants from exhaust audio. Synthetic demonstration references are explicitly labelled. The CLI also exposes `--calibrate-wav <generated.wav> <reference.wav> <new-directory>`.

The linear calibration uses direct regularized spectral estimation and a finite impulse response, rather than the original proposal for an automatic-differentiation optimizer. This narrower implementation gives a bounded, inspectable filter and a separate held-out measurement; no differentiable physical-engine identification is claimed.

## 5. Technical qualification

Final logs and source identities are recorded in `output/final-readiness-20261003/`, including `delivery-manifest.json`. The local machine is an AMD Ryzen 7 3800X, 8 cores / 16 threads; local timing cannot qualify the previously named i7-8565U laptop or dev17. Device checks render with zero output volume and assess scheduling/failures, not subjective listening.

The existing 10,000-design thermodynamic test is a deterministic closed compression/expansion sweep with energy accounting. It is not 10,000 complete fired engine models. Complete fired-engine tests separately cover one to twelve cylinders, abnormal wiring, finite combustion, starter/accessory behavior, gas conservation, injection cut, turbo stability, harmonics and export continuity. These scopes must remain distinct.

The final `cargo test --release --all-targets` pass has **337 successful tests, zero failures and six explicitly ignored diagnostics**. The ignored cases are four measurement tables, a preset-dyno table and the unresolved strict length-law qualification; they are not counted as successes. Strict release all-target Clippy and formatting checks pass. Graft's wiring graph is synchronized; no deep semantic build is claimed. The first full pass exposed a shared temporary-ZIP race and an obsolete pre-bank-delay assertion; both were corrected and the complete suite rerun. The last new regression verifies smooth gain changes and exact preservation of physical states.

The application and demonstration programs build in release. A final UI-only change places provenance after the engine controls, retains restoration buttons at the top and rounds displayed numbers; all 26 application tests, strict all-target Clippy, formatting and the application rebuild pass again. The final Genesis driving capture was inspected: engine controls are immediately visible, A/B and seven imported forward gears remain available. Final executable SHA-256: `01bbd16e1b17cd6baf5a87e70ec5d3987d084ebcc56c874603a6ee9514e8809d`; the usual executable and the device-test copy are identical.

Fresh workshop validation covers twelve complete source-model cycles, four saved/reloaded projects and twelve eight-second PCM24 listening WAVs. All source hashes are unchanged and A is identical after editing B. The new uninstalled Berlingo variant has 138 unique safe entries, including 132 mono 48 kHz PCM24 WAVs; every entry was read for CRC verification. ZIP SHA-256: `6cf43d2621dcb4a928f4f998e32f457184b380c877044647389ca1fca3d6c3b9`. The independent setup-import audit also passes on twelve archives, including all seven Genesis ratios.

Six twelve-second physical scenarios (I4/V8/V12, fuel cut, idle/accessory load and deliberately repeated firing) complete without engine failure or nonfinite states. Their raw IEEE-float WAVs are diagnostic data, not the listening delivery. The follow-up thirty-second idle/accessory run ends at 855.59 RPM against an 850 RPM target; the full startup range includes an initial drop to 418.83 RPM after releasing the imposed 1200 RPM condition. The cut scenario ends at 13.99 kPa MAP, outside the original 17–24 kPa research band. The reported rolling brake-work variation is not a direct IMEP/combustion-COV qualification. These observations preserve the unqualified physical-calibration targets under X-018 rather than turning numerical finiteness into a realism claim. Their timing was collected alongside other work and is not the isolated CPU benchmark.

The isolated final CPU run measures V12 coupled native-rate work at 0.706 seconds per second of audio versus 0.799 at the historical rate (about 11.6% lower). The ten acoustic comparison WAVs have byte-identical SHA-256 hashes before and after the gain-transition correction. The four-second finite-volume load/cut/recovery export completes in 71.82 seconds, with exactly one second of physical fuel cut and successful restart of injection. The final application's calibration CLI produces the same calibrated WAV and coefficients as the library demonstration.

The silent device checks use Sound BlasterX G6, 48 kHz stereo float transport, a 40 ms producer queue and Hall wet mix 70%. The V12 cycle holds five points for three seconds each: 1200 RPM/0.1 throttle, 3000/0.8, 6000/0.8, 6000/0 lift-off and 3000/0.8 recovery.

| Device run | Duration | Callbacks | Underruns / missing frames | Callback p99 deadline share | Maximum callback / synthesis block | Physical / producer failure |
| --- | ---: | ---: | --- | --- | --- | --- |
| V12 + Hall, historical 96 kHz synthesis | 600.001 s | 60,000 | 0 / 0 | ≤1% | 0.037 / 9.887 ms | no / no |
| V12 + Hall, native acoustics + pressure feedback | 600.001 s | 60,000 | 0 / 0 | ≤1% | 0.013 / 10.514 ms | no / no |
| Imported V8 | 60.001 s | 6,000 | 0 / 0 | ≤1% | 0.005 / 7.351 ms | no / no |

All three runs also have zero callback budget overruns; observed total output latencies are 96.0, 100.9 and 102.2 ms respectively. The native/coupled run uses 48 kHz synthesis and the other two use 96 kHz. Its maximum synthesis block slightly exceeds the nominal 10 ms block duration, while the 40 ms producer queue avoids missing output. These are scheduling and numerical checks at zero listening volume. They do not record audibility, preference or a gameplay session. Complete logs and executable identity accompany the delivery.

The audio-check CLI's report-error behavior is also verified on the final executable: the same one-second Inline-4 check succeeds with a writable report and exits with code 1 when its report destination has no parent directory. A missing report can no longer silently count as a successful check.

Two historical engineering targets remain explicitly unqualified, despite delivery of the corresponding optional features. The ten-percent full-engine primary-length law remains unsatisfied (equal-rate historical fits −11.29% and −13.42%); a separate anechoic-reference diagnostic also fails to establish it. Its numerical threshold is unchanged and its diagnostic stays opt-in, rather than being counted as a passing regression. The historical V12 target below 5 microseconds per step is also not demonstrated on this machine or the named laptop. Neither target is closed by a successful device callback check. X-014 and X-028 retain these open conditions in the index.

## 6. User reception

`output/final-readiness-20261003/TESTER.html` provides six steps, unmodified A/B demonstration audio, project links, a quality-rating exercise with hidden reference/harmonic anchor, and sixteen source-recognition trials. Blind exercises use separately AC-RMS-matched copies, with measured gains in `listening-protocol.json`; ordinary listening clips retain their original level. Results stay in the browser and can be downloaded. Repeated trials across four vehicles are exploratory, not a proof of perceptual equivalence or a certified MUSHRA result.

The page's 51 local references resolve, all 42 distinct linked WAVs pass mono 48 kHz PCM24/non-silence checks, the twelve blind copies match their recorded target RMS within 1e-6, and JavaScript syntax passes. Automated browser visual inspection of the local file was unavailable: the browser tool rejected `file:` navigation. No browser-policy workaround was used. The application itself was captured through its dedicated capture mode and the final screenshot inspected.

Still requiring user evidence:

- [ ] Preferred sound and identity, especially idle, lift-off, intake and high-RPM mechanical balance.
- [ ] Complete workshop ergonomics, wheel interaction and dyno reference overlay.
- [ ] Real BeamNG transitions, camera mix and the historically reported Cerberus region around 5,200 RPM.
- [ ] Reference-recording/OBD and knock-map calibration where physical authenticity is required.
- [ ] The historical laptop performance target on that actual laptop.

Stationary BeamNG loops preserve the original vehicle's transient-event references. They do not encode the BESS live history of turbo spool, startup or triggered afterfire. Candidate ZIP verification is separate from installation and gameplay acceptance.
