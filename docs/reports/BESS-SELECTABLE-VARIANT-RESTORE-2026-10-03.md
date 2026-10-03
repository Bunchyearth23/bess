# Restore the selectable BESS variant — 2026-10-03

The user reported that the BESS configuration disappeared after export and
explicitly requested its restoration, with BABM responsible for merging. The
earlier return to a complete ZIP had incorrectly coupled that packaging choice to
replacement of the original configuration's sound. D-043 corrects the behavior.

## Delivery contract

- BESS creates a complete vehicle ZIP containing the original members unchanged
  and an additional named, selectable BESS configuration.
- The BESS configuration has its own cloned engine part, exhaust and engine/intake
  blends and WAVs. Original configuration, sounds and vehicle physics remain.
- BABM owns merging and installing/updating the variant in standalone or grouped
  vehicles. Source and profile identify the same variant across revisions.
- Historical replacement and add-on-only CLI exports remain explicit legacy modes.
- Detailed progress, cancellation and bounded parallel rendering remain available;
  both audio stems are calculated in one engine pass per RPM/load point.
- During implementation the user additionally requested a general engine-sound
  gain from 0 to 1 (D-044). It applies to generated BESS listening and exports,
  after calibration, with 1 preserving the previous level and 0 producing silence.
  It is independent of audition volume and defaults to 1 in existing projects.

## Validation setup

Evidence is kept under `output/variant-restore-20261003/` and excluded from Git.
Vehicle data is used only on disposable copies. The Thunderhawk source hash is
`150bed820773f27447c79c2925326ed70d825725d9d796dc7f204f453cf0169f`.
The archived executable is
`82f8e2125b5898acff6758e7882f242d070f681a29de4af11f41f8305c951c2c`.

Before edits were built, its legacy `--beamng` exporter generated the reference
selectable add-on in **211.4942 seconds**. This contains 112 generated WAVs, with
separate exhaust and engine stems. The original-plus-variant output will be checked
against that reference by stem, load and RPM, independent of renamed paths.

Final validation and delivery continued after midnight on 2026-10-04. Actual
BeamNG listening and driving remain separate user checks.

## Automated checks

- BESS release library: **281 passed, 4 ignored measurement diagnostics** at the
  gain/restoration freeze. The suite verifies 0, 0.5 and 1 on complete, historical add-on and replacement
  exports, including exact silent PCM and attenuation after calibration. Live
  playback, room tails, preview, comparison WAV, original A and legacy project
  defaults are also covered. An initial test harness failure used a legacy success
  message as a filename; reading `manifest.json.zip_file` corrected the harness.
- BESS application: **39 release tests passed**, including slider visibility,
  persistence and existing export/folder behavior.
- Legacy complete-replacement integration: **2 release tests passed**.
- BABM: **25 release tests passed**, including first merge, profile revision,
  preservation, conflict refusal and legacy-replacement migration.
- Strict all-target release Clippy and formatting pass in both repositories.
- Compiled BABM passes five complete-variant scenarios in
  `variant-roundtrip-final/verification.json`: standalone import, grouped import,
  initial merge with another profile, import before merge and rejection of altered
  original payload. Routing resolves the configuration through its engine part to
  both sound emitters. Revision, stale/repeated import, original rediscovery and
  unmerge are checked. When no author exists, merge may change the display fallback
  from `Unknown` to `BESS / Automation`; all other source metadata stays identical.

These are technical checks of executables and packages, not in-game acceptance.

## Real-source verifier correction

The first Thunderhawk complete export exposed an over-strict newly added duration
check. The old selectable exporter produces 191,999 frames for the four-second
engine loop at 2,370 RPM because its whole-cycle calculation uses `f32` then
truncates to a frame count. The new verifier incorrectly required at least 192,000.
The failed run published no ZIP and retained only `error.txt`.

The verifier now requires the exact historical frame calculation for each RPM.
Rendering and audio bytes are unchanged. A regression renders that operating
point, accepts its exact 97,215/191,999 frame counts (also when intentionally
silent), and rejects both one missing and one extra frame. The verification
correction is separate from the master-gain behavior.

After this correction, **all 10 release variant tests passed**, including the new
length regression and repeated 0/0.5/1 gain checks. Strict all-target Clippy and
formatting passed again. No synthesis or UI code changed in this correction.

## Final complete exports and performance

The rebuilt executable exported Thunderhawk successfully in **35.6921 s**, against
**211.4942 s** for the archived selectable exporter: approximately **5.93 times
faster** on this PC. These are single end-to-end CLI runs without simultaneous
compilation or rendering. Both produce the same two audio emitters; the new ZIP
additionally carries the complete original vehicle. This comparison is separate
from the earlier performance report for the one-emitter replacement format.

`complete-comparison-final.json` verifies all **156 original members unchanged**,
two configurations instead of one, independently resolved sound routes, correct
mono 48 kHz PCM24, and **all 112 generated WAVs byte-identical** to the old reference
at master gain 1. The current configuration is `Zero (BESS - Natural)`.

The user's saved project also exports successfully: **35.5935 s** including probe
startup and import, of which **34.2723 s** is export work. Its original payload and
112 generated recordings pass `saved-verification.json`. The probe observes
Preparing, Rendering, Packaging, Verifying and final Complete. A separate real
cancellation during Rendering reaches Cancelled in **43.7 ms** after the request,
removes its output directory and publishes no ZIP.

The final executables also pass the existing compiled standalone/grouped legacy
replacement roundtrip in `legacy-roundtrip-final/verification.json`, including
reopening BABM output in BESS, stale/repeated apply and unmerge preservation.

## Migration of the user's existing pack on a copy

`real-thunderhawk/migration-verification.json` records an import into a disposable
copy of the installed Thunderhawk pack and its original backups. The pack had an
older independent BESS add-on plus a legacy replacement receipt.

- BABM restores **56 original Automation WAVs** from the verified pristine source.
- **218 previous members** stay unchanged, including all **118 older add-on
  members / 112 older BESS WAVs**.
- Configurations increase from two to three: original, older independent BESS,
  and new named BESS. The new engine part resolves both emitters and all 112 WAVs.
- The exact pre-import ZIP is retained as history. Repeated import is refused,
  the migrated legacy receipt is retired, and original backups stay intact.
- The **four actual installed files** still match their pre-test hashes. Only the
  disposable copy was modified.

## Desktop delivery

Both final release executables are copied to the Desktop and match their build
and archived evidence hashes:

| Executable | SHA-256 |
| --- | --- |
| `D:\Desktop\BESS.exe` | `ac174bab9a74d5948fbff457e1b5eb87a16f2b7e3f6f32c30638cbf56dbba787` |
| `D:\Desktop\BABM.exe` | `79ca4a34702567db31f7e7bc223f3b1275c12e085cde39affe08a54b8740ede5` |

Both Desktop programs start with responsive native windows using the disposable
folders. The inspected `native-bess.png` shows **Engine sound gain 0.50** at the
top of the comparison bench, separate listening volume at zero, the BESS variant
export guidance, Exhaust decay and Experimental back pressure. The check uses a
muted project copy; only the smoke-check processes are stopped afterward.

The project index is updated. Publication targets are both repositories' `main`
branches; the final commit IDs and matching remote heads are recorded in the
ignored `publication.json` receipt after publication. In-game selection, listening
and subjective acceptance remain the user's next checks.
