# BeamNG export progress and performance — 2026-10-03

The user reported a long BeamNG export and requested detailed progress and faster
calculation. The existing complete-vehicle path calculates independent RPM/load
loops successively, then packages and reimports the generated archive. A generic
status message does not distinguish these stages.

## Delivery requirements

- Show preparation, sound generation, packaging and verification separately.
- Show completed/total sounds, RPM/load context, elapsed time and a qualified
  estimate of the remaining time when enough evidence exists.
- Keep progress accessible without scrolling to the export section; allow
  cooperative cancellation and retain the last successful companion destination.
- Render independent sound points with a bounded number of workers, while
  preserving each voice's initialization, warmup, frame count, signal processing
  and the complete bank's output gain.
- Preserve the source archive, non-audio content and BESS/BABM exchange contract.

## Verification setup

Local evidence is stored under `output/export-progress-20261003/`. The archived
previous executable is `ff7e4f5d7cf3a14fb1c5e614837b396089eb013df03a7962a9d26d1bf402cc36`.
The benchmark source is a disposable copy of the user's Thunderhawk original,
SHA-256 `150bed820773f27447c79c2925326ed70d825725d9d796dc7f204f453cf0169f`,
with 56 sound loops. The old and new command-line exports use the same input and
settings. An additional export uses the user's saved project and compares its
WAV bytes against the user's prior successful GUI export.

The local machine is an AMD Ryzen 7 3800X with eight cores and sixteen logical
processors. Timings qualify this machine and these settings only. Private
vehicle data and generated sound archives are excluded from Git.

Final measurements, test results and executable identities are recorded below.
No installed BeamNG mod is replaced by the benchmarks.

## Measured reference

The archived executable completed its 56-loop command-line export in
**123.7483 seconds**, exit code zero (`baseline-timing.json`). No Cargo build or
other synthesis benchmark ran concurrently. The resulting WAVs differ from the
user's saved-project export, confirming that the project-specific comparison is
necessary in addition to the default command-line benchmark.

## Delivered behavior

The top panel displays preparation, sound generation, ZIP assembly and verification
without requiring the user to scroll. It includes phase counts, current RPM/load,
elapsed time, worker count and an estimated remaining generation time. The estimate
is explicitly for generation; packaging and verification retain separate stages.
Only a fully verified and published export reaches 100 percent.

`Cancel export` cooperatively stops workers and cleans only files owned by that
export. Existing source archives, unrelated files and the last successful companion
destination are preserved. The final ZIP is published only after reimport and
sidecar completion; cancellation cannot turn an already completed export into a
failure or hide an earlier rendering error.

Independent RPM/load points now use bounded parallel workers: available logical
processors minus one, capped at eight and at the number of sounds. Each point
retains independent deterministic engine state. Warmup, sample count, quality
options, bank-wide gain and ZIP compression are unchanged. Completed results are
assembled in their original deterministic order.

## Measured results

| Check | Result |
| --- | --- |
| Same-input CLI, archived release | 123.7483 s |
| Same-input CLI, updated release | 23.9369 s |
| Measured speedup | 5.17 times; 80.7 percent less elapsed time |
| CLI audio and vehicle comparison | 56/56 WAVs byte-identical; 100 other members unchanged; marker equal except export timestamp |
| User's saved project, eight workers | 21.7065 s export, plus 0.9109 s setup |
| Saved-project comparison with prior GUI export | 56/56 WAVs byte-identical; 100 other members unchanged |
| Cancellation during actual Thunderhawk rendering | Cancelled in 0.0549 s after request; no output directory left |

The command-line timing includes process startup, import and export; hashing is
outside the timed interval. The saved-project probe times setup separately and
must not be substituted for the same-input CLI speedup. Each measurement ran
without a concurrent build or synthesis benchmark. These are single observed runs
on this PC and vehicle, not a guarantee for every engine or optional physics mode.

Receipts: `baseline-timing.json`, `parallel-cli.benchmark.json`,
`saved-parallel.probe.json` and `cancelled-render.probe.json`. The saved-project
probe observed all four active stages, progress within generation and packaging,
and the terminal Complete state. Cancellation was requested after generation
started, not before dispatch. The source hash remained unchanged.

## Regression verification

**58 targeted release tests passed**: 12 export, two progress-state, four variant,
37 application/UI, two complete-export integration and one non-audio preservation.
Coverage includes exact serial/parallel WAV equality with back pressure on and off,
cancellation during rendering and ZIP assembly, foreign-file preservation,
publication/cancellation races, original error retention, and visible progress and
cancellation controls in a settled 480-pixel UI frame.

Strict release Clippy for all targets, formatting, the release application/example
build and Python syntax validation pass. The compiled BESS/BABM fixture roundtrip
also passes for standalone and grouped vehicles: originals remain available,
non-audio members remain unchanged, and repeated/stale applications are handled
safely. Evidence is in `babm-roundtrip/verification.json` and its command log.

## Executable delivery and boundaries

The rebuilt `target/release/bess.exe`, `D:\Desktop\BESS.exe` and archived
`bess-after.exe` have the same SHA-256:
`82f8e2125b5898acff6758e7882f242d070f681a29de4af11f41f8305c951c2c`.
BABM remains compatible without a rebuild; its Desktop executable retains
`556a59e47d04f8c76a7d58831cfe3c97c036bc8049dae6404f10ce6eb1e6a47d`.

The delivered Desktop executable starts a responsive native window with the muted
saved project. Its captured screen was inspected: the imported workshop, BeamNG
shortcut and exhaust controls render correctly. The live export panel itself is
covered by the UI render assertions and real backend progress receipts, not by an
automated native click-through. See `native-startup.png` and
`native-startup-verification.json`.

The checks establish export speed, byte preservation, progress state and technical
interoperability. They do not claim new listening acceptance, in-game validation,
or uninterrupted low-latency audio while an export uses the processor.
