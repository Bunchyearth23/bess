# Experimental engine model: twelve-vehicle development pass — 2026-09-23

## Scope and decision

The user asked for a substantial revision of the experimental live mode across all twelve local Automation vehicles from the start and allowed existing experimental projects to change sound. The mode remains an interface-only listening option. New imports and every BeamNG export continue to use the standard source-guided sound. No experimental BeamNG add-on was installed or published in this pass.

## Research used

The [physically informed engine model by Baldan and colleagues](https://air.iuav.it/handle/11578/264484) describes ignition, valve timing and imperfectly regular cylinder motion as separate causes of engine sound. The [pulse-train and resonator study by Doerfler and Wyse](https://arxiv.org/abs/2603.09391) treats successive pressure pulses and exhaust propagation as distinct from a sustained harmonic tone. The [parametric representation study by Doerfler and colleagues](https://arxiv.org/abs/2606.21521) combines engine orders with stochastic components learned over RPM and torque. These informed the structure of this revision; BESS does not implement or claim the papers' complete models.

## Changes

- Import-time analysis now separates seven bands around 80, 200, 500, 1,200, 2,500 and 5,000 Hz. The 80 Hz split distinguishes a firing fundamental below BeamNG's declared exhaust cutoff from audible bass above it.
- Each RPM/load knot measures the source's 80–500 Hz share and cycle-level variation. A broad, bounded pressure release replaces the overly narrow harmonic-heavy pulse when the reference is bass-dominant. Duration responds to the interpolated bass descriptor and load, while a restrained full-load upper-band reduction addresses the remaining Cerberus mismatch.
- Event strength, onset and width vary slightly without moving the continuous crank phase. Stable small cylinder differences and independently drawn event variation reduce the perfectly repeated buzz. The model still assumes evenly spaced firing events; the ZIP does not identify firing order.
- Exhaust texture has less continuous noise between pulses. Generated intake and mechanical components have lower, broader bands and restrained upper clicks. Both remain inferred from exhaust-only audio.
- The experimental mode description in the interface now reflects the generated combustion, texture and mechanical paths. A repeatable fleet audit produces level-matched A, standard B and experimental B WAVs and measures the result without packaging vehicle assets.

## Technical evidence

The local audit rendered all twelve vehicles for six seconds at three held conditions: 1,500 rpm / 0.15 load, 3,000 rpm / 0.7 load, and 5,000 rpm / full load. Cerberus A used 5,200 rpm in the latter two conditions. The CLI clamps any requested RPM outside a vehicle's source range. Each comparison used the same source ZIP and settled AC RMS matching. At the middle condition, the pre-revision 80–500 Hz share missed the source by a median **11.1 percentage points** across the twelve; the final pass missed by **0.63 points** (largest miss **2.66 points**). The median 500–2,000 Hz miss changed from **11.3** to about **0.6 points**. Cerberus A's middle-condition 80–500 Hz share changed from **42.4%** to **58.9%**, against **58.5%** in the source. At full load and 5,200 rpm, its generated share is **54.5%** against **51.7%** in the source. These are spectral diagnostics, not perceived-quality scores.

The three final fleet audits and 108 mono 48 kHz / PCM24 listening WAVs are under `output/experimental-fleet-final-{low,mid,high}-20260923/`; these directories are ignored by Git and contain no vehicle ZIPs. The generated clips had no silence or clipping in the audit. Across the twelve, median absolute 80–500 Hz share misses were **0.63**, **0.63** and **3.42 percentage points** at low, middle and full load. The largest individual misses were **11.84**, **2.66** and **11.36 points**, respectively. `cargo test --release --all-targets` passed 81 tests, including the guard that selectable BeamNG export ignores experimental live mode. Strict release Clippy passed. A ten-second muted default-device check at 48 kHz stereo float processed 999 callbacks with a 0.860 ms maximum callback time and zero budget overruns (`output/experimental-audio-check-final-20260923.txt`). This is a short transport check.

## Limits and next listening gate

The short-time level motion remains lower than Automation in several vehicles. At Cerberus A's middle held point, the 50 ms RMS p95–p5 span is **0.42 dB** in experimental B versus **1.75 dB** in the source. Berlingo at low load remains **11.84 points** too bass-heavy in the 80–500 Hz share; Nord Optis at full load is **11.36 points** too light in that band. Artificial repetition, intake authenticity, engine-bay balance and transient driving behavior still require listening. Matching broad spectrum does not establish realism, and the Automation exhaust ZIP cannot reveal isolated intake, cylinder pressure, actual exhaust geometry or firing order. No BeamNG in-game test applies to this interface-only revision.
