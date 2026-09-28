# Automation resynthesis: original A and physical B — 2026-09-28

## Delivered behavior

At the user's request, Automation imports now use the same physical engine as scratch engines. There are exactly two product choices: A replays the original bank; B runs the physical solver. The alternative synthesis implementations, standalone render executables, selector controls and obsolete comparison commands have been removed. Compatibility data schemas remain so old projects can load, but do not render another voice.

Verified metadata links the selected blend UID to the Automation Variant/Family, active configuration and engine JBeam. The mapper uses Variant bore/stroke, capacity, architecture, aspiration, supported component tags, idle and rev limit. Detailed geometry, firing order and unavailable parameters are disclosed as assumptions. Unsupported essential metadata leaves A available and makes B unavailable explicitly. Twelve local vehicles produce valid, distinct models.

Physical B runs at twice the output rate and decimates exhaust/intake/mechanics separately. The source PCM never excites B. All 19 physical sound controls apply live without resetting gas/crank/thermal state or allocating/deallocating on the producer. Scratch and imported controls share the interface helper. Fixed gain ×16 and the previous lookahead saturation fix are retained. Optional imported A/B matching can attenuate to 0.02 and is checked within 1 dB after settling; scratch retains fixed gain.

The same physical stems feed WAV/BeamNG exports. Listening protection is not printed into separate exported stems; offline calibration and PCM headroom apply. Engine-layer gain remains effective after export calibration. Source audio and verified engine data have separate fingerprints; a .car-only engine change is rejected for a saved physical reference even if the WAVs are unchanged. Older projects acquire the engine reference on opening/saving.

## Evidence

- Release all-target suite: **209 passing tests** in `output/automation-physical-final-tests.txt`, followed by focused migration checks after final compatibility-only cleanup.
- Strict all-target Clippy and formatting passed. Release binary rebuilt.
- Original A checked sample-for-sample against source playback; B is unchanged when the source PCM changes, including after live sound tuning.
- Physical live/render identity, zero producer allocations/deallocations, engine-state-preserving tuning and output rates 8–192 kHz checked.
- Source-only .car mutation changes the engine fingerprint and preserves the audio fingerprint; old references remain readable.
- The complete 12-vehicle corpus validates the physical mapping. Portable core fixtures require no local vehicle corpus.
- A solver failure is reported instead of silently exporting a scratch WAV of zeros.
- Real default-device check: imported V8, 30.001 seconds, 48 kHz float stereo; 3000 callbacks, zero underruns/missing frames/budget overruns, no solver or producer failure. Maximum 10 ms producer block: 7.446 ms. Silent physical computation, not subjective listening; report at `output/automation-physical-20260928/audio-check.txt`.
- UI captured and inspected: `output/automation-physical-20260928/interface.png`; only A and physical B are offered.

## Listening and export artifacts

`output/automation-physical-20260928/` contains six 8-second A/B WAVs, three reopenable projects, measured levels and one uninstalled I4 BeamNG add-on. Commands: idle, acceleration to 3500 RPM, steady load, lift-off and idle. Volume 0.8, fixed B calibration, no inter-clip normalization and no optional live matching in these examples.

| Engine | A RMS dBFS | B RMS dBFS | B peak dBFS |
| --- | ---: | ---: | ---: |
| i4-b5c | -16.24 | -11.91 | -2.76 |
| i6-genesis | -26.51 | -10.57 | -2.70 |
| v8-advent | -20.57 | -12.77 | -2.80 |

The I6 A/B loudness difference above is deliberate evidence of the unmatched files, not a claim of equal loudness. Use the application's matching option for tonal comparison.

The prototype contains **126 entries / 120 mono 48 kHz PCM24 WAVs** (60 exhaust + 60 engine). All CRCs were read; paths are safe and unique; original source archive SHA256 values remain unchanged. ZIP SHA256: `3591ecbb6adbca452b087df66407b26cc94ac268779df27ff5c68dba9cbdca25`. File size: 48,297,195 bytes. The source engine fingerprints were attached to demo projects/manifests after rendering by reloading the fully unchanged source archives; WAV/ZIP bytes stayed unchanged.

## Delivery and limits

Executable: `target/release/bess.exe`, 7,494,144 bytes. SHA256: `4c2ea459a414d2c291bb47e50096e02c80f71e58182474748767147950b62855`.

The user's open application was preserved during replacement; reopen the usual executable to load this revision. Work is local, including the preceding saturation fix. No new commit/push or vehicle installation was performed for this request.

Measurements and archive checks do not establish subjective realism, exact vehicle identity or BeamNG game behavior. Those remain listening/in-game acceptance checks; the mapper's assumptions remain visible.
