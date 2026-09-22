# BESS 0.8.2 — Engine sound synthesis revision

## Finding and research

The 0.8.1 import preset reconstructed the Automation bank with periodic and residual gains of 1 and weak coloration. All twelve 0.8 B renders were identical to 0.7. This made the criticism that B sounded too close to the source plausible, although a changed signal alone does not establish naturalness.

[Doerfler and Wyse (2026)](https://arxiv.org/html/2603.09391v2) motivate cycle excitation using a bipolar front, turbulence coupled to those events, and different response as rpm and torque rise or fall. [Jagla et al. (2012)](https://doi.org/10.1121/1.4754663) motivate cycle-synchronized windows and phase joins. [Gazon and Blaisot (2006)](https://saemobilus.sae.org/papers/cycle-cycle-fluctuations-combustion-noise-a-diesel-engine-low-speed-2006-01-3410) document cycle variation for their diesel. These sources guide structure, not BESS parameter values or proven acoustic fidelity for Automation vehicles. See [detailed research](../ENGINE-SOUND-RESEARCH.md).

## Verifiable changes

- B selects several recorded segments instead of always following their original order, with a two-cycle overlap and common phase. Anchors are computed outside the callback. A retains its playback. This 0.8.2 behavior was later revised in 0.8.4 after Cerberus level pumping was reported.
- The measured cycle component generates a filtered, normalized front that **replaces** part of direct pulses. Residual texture follows pulse activity. Small correlated variation occurs each cycle and is strongest near idle. Opening the throttle and deceleration fuel cut change pulses and texture differently.
- Initial settings adapt to WAV brightness. Three new controls are **Cycle variation**, **Pressure front** and **Pulse-linked texture**. Old projects load defaults for these fields, so their B output changes with this version.
- `.car` files are read without executing their Lua. Layout and cylinder count are shown only if the UID agrees with the blend, active `.pc` engine and JBeam part ID. All twelve archives agree. Missing aspiration data do not hide the sheet. **Adapt to vehicle** retains user-entered combustion angles. Firing order is still unknown, so per-cylinder combustion is not activated automatically.
- A/B switching estimates local level and covariance across branches to reduce phase-opposition dips. Its bounded correction does not affect pure A or B renders.

## Technical checks

- 45 release tests passed, with clean formatting and strict linting. Coverage includes segment continuity, A source preservation, no allocation during `next`/`set`, parameter checks and engine-sheet provenance.
- Twelve successful imports, twelve A renders byte-identical to the preceding reference, and twelve changed B renders. Each A/B is 16 seconds, mono 48 kHz / 24 bit, equal integrated RMS and unclipped. The historical measurements document level and spectrum differences; RMS is not loudness.
- Over 58 switching points per vehicle for B5 A and Nord, the deepest RMS dip at the middle of the fade (10.15 ms window) improved from −10.31 to −5.74 dB. Over the first 50 ms of the worst switch, the dip improved from −3.26 to −0.90 dB for B5 A and −2.87 to −0.13 dB for Nord. The largest measured peak in the middle of a fade was 0.196; the largest sample-to-sample jump stayed near the previous render. Four pure A/B WAVs were byte-identical before and after this switch fix. These numbers do not validate its audible perception.
- On the Sound BlasterX G6 at 192 kHz, two final 30-second silent-synthesis sessions each produced 3,000 callbacks without a 10 ms budget overrun. Maxima were 9.588 ms and 6.822 ms, leaving little margin in the first run. Before anchor-table optimization the initial check reached 7.643 ms; after optimization and before the fade fix it reached 6.701 ms. See [first final check](natural-0.8.2-audio-check-final.txt), [second final check](natural-0.8.2-audio-check-repeat.txt) and [earlier check](natural-0.8.2-audio-check-optimized.txt).
- Twelve comparisons, projects and sources were delivered with an A/B/C listening page (`output/corpus-0.8.2/index.html` in the project, `listening/index.html` in the delivery). C replays 0.8.1 treatment; A is source-bank playback in BESS.

## Export and remaining validation

`output/beamng-0.8.2/verification.json` verifies twelve distinct ZIPs, 688 replaced WAVs, 1,194 other entries preserved byte for byte, twelve unchanged sources, and loop continuity (largest seam difference 4.36 times adjacent-sample difference RMS, below the technical threshold of 8). BESS reimported each copy. These measures do not establish sound preference, absence of audible artifacts, stable prolonged audible playback or BeamNG operation. Those require listening and in-game testing.

Automation WAVs are already colored mono mixes. A front derived from their cycle component is an audio model, not real cylinder pressure. Declared cylinder count provides neither firing order, bank split nor real header geometry. Internal BESS driving transients are not exported as a BeamNG controller.

## Portable delivery

`dist/BESS-0.8.2-complete/START.html` groups the executable, twelve source ZIPs, twelve A/B/C comparisons, 36 reopenable projects, twelve BeamNG copies and documentation. Final verification: 190 SHA256 hashes, 78 resolved local links, 36 project paths inside the folder and 36 comparison tracks. Screenshots and measurements do not replace subjective listening or driving in BeamNG.
