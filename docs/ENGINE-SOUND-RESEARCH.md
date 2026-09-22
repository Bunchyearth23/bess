# Engine sound synthesis: BESS audio direction

## Objective accepted September 22, 2026

Import should preserve the Automation sound identity while immediately providing a livelier interpretation. The MVP sine-wave bench did not meet this goal. Better quality is a user listening criterion, not an automatic consequence of compilation or level normalization.

## Research informing the design

1. **Hybrid synthesis across rpm and load.** Li, Wang and Li describe combining samples and procedural components; rpm and pedal changes inform transients. This supports retaining WAVs while enriching their behavior. BESS does not implement their neural network or phase reconstruction because their training corpus is unavailable. [Authors' paper, Duke Kunshan](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf).

2. **Event and phase continuity.** Jagla, Maillard and Martin (JASA 2012, DOI 10.1121/1.4754663) study engine-cycle-synchronized overlap-add synthesis. The practical lesson is to prepare joins and align cycles rather than mix independent playheads. HAL access was blocked during this research; we relied on the indexed abstract and Li's account, and do not claim the complete algorithm is implemented. [HAL manuscript](https://hal.science/hal-00730614).

3. **Physics-guided acoustic models.** Baldan and coauthors present a procedural approach oriented toward interaction and real time. It motivates eventual intake, cylinder and exhaust branches with physical dimensions. The institutional record was accessible but the PDF download failed; unexamined details are not treated as specifications. [IUAV publication, 2015](https://air.iuav.it/handle/11578/264484).

4. **Excitation and acoustic coloration.** Engine Simulator code exposes continuous filtering, noise, impulse-response convolution and level control after simulation. It illustrates the distinction between excitation and output acoustics. BESS copies neither its code nor its audio resources. [Original synthesizer](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp).

5. **BeamNG destination.** BeamNG describes two lists in `sfxBlend2D`: first off-throttle, second on-throttle. The `_P` filename convention does not replace that structure. Its documentation distinguishes engine, exhaust and auxiliary events. Exporting rpm/load loops alone cannot preserve all history-dependent synthesizer transients; those would require an event/controller layer. [BeamNG team answer](https://www.beamng.com/threads/custom-audio-samples.44840/), [official engine audio documentation](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/).

## Acoustic supplement (0.3)

Julius O. Smith describes acoustic-wave junctions through pressure continuity, flow conservation and impedance-change reflections. BESS applies those relations to a reduced, lossy tube network driven by recordings. This is simplified physical coloration, not a reconstruction of real cylinder pressure. Junction energy conservation is tested separately from network damping. [Digital Waveguide Theory](https://www.dsprelated.com/freebooks/pasp/Digital_Waveguide_Theory.html), [Scattering at Impedance Changes](https://www.dsprelated.com/freebooks/pasp/Scattering_Impedance_Changes.html).

## Reference vehicle measurements

`thunderhawk_zero`: 56 mono float32 WAVs at 44,100 Hz, about 2.2 seconds each. Two layers of 28 rpm points from 803 to 4,989 rpm. JBeam declares 800 rpm idle and a 12,000 rpm damage threshold; the latter does not establish a 12,000 rpm limiter. Cylinder count is not in the audio field, and harmonic peaks cannot establish it reliably. File levels vary widely and some have DC content.

## Selected architecture

- Immutable original bank decoded outside the audio thread; load layers identified by the blend, not WAV names. Explicit selection is needed if a file has multiple blends.
- Loops prepared over integer engine cycles with overlap joins, cycle-profile correlation alignment and a common crankshaft phase.
- Neighboring rpm/load interpolation and BDSP sinc resampling.
- Dynamic state: smoothed rpm, asymmetric load response, attack, release, small correlated fluctuations and history-dependent exhaust acoustics.
- Reconstructed intake and mechanical layers, never described as physically isolated original tracks. Optional turbo is never assumed present.
- A/B listening with slow level compensation and equal-RMS comparison exports (RMS is not LUFS loudness); peaks are measured separately.

## Milestones and listening criteria

1. **Hybrid foundation:** import real WAVs, retain identity, avoid join clicks and generate reproducible original/enhanced comparisons.
2. **Engine model:** identify cylinders/firing order, geometry and active parts; separate periodic and residual signals; model per-bank headers.
3. **Customization:** rpm × load maps for sound families, intake, mufflers, resonators, mechanical sound, turbo, limiter and afterfire.
4. **Game export:** validated loops, load transitions, one-shot sounds and possibly a controller, then actual inside/outside BeamNG testing.

The first hybrid delivery alone does not meet the “realistic and freely adjustable” target. At each step, check idle, slow acceleration, full load, equal rpm at different loads, lift-off, rapid changes and bank limits at comparable playback level. Subjective outcomes remain open until listened to.

## Research update for BESS 0.8.2

BESS 0.8.1 largely preserved direct bank playback in B: periodic and residual gains both defaulted to 1, calibrated coloration was mild and explicit combustion was off. This technically explains why B could remain close to A, but does not itself establish perceived naturalness.

- [Doerfler and Wyse, EUSIPCO 2026](https://arxiv.org/html/2603.09391v2) model pressure fronts as bipolar pulses, their variation, pulse-coupled turbulence and exhaust resonators. They also show that rising and falling conditions at equal rpm can differ. Their numeric result belongs to their corpus and learned model; BESS claims none of those scores. BESS 0.8.2 derives a filtered pressure front from measured cycle content and replaces part of the direct signal instead of adding an arbitrary pulse per cylinder.
- [Jagla, Maillard and Martin, JASA 2012](https://doi.org/10.1121/1.4754663) motivate cycle-synchronized sequence assembly. In 0.8.2 BESS varied four-cycle segment anchors, kept a common phase and overlap joins; its pseudo-random selection was a BESS implementation choice, not a result attributed to that paper. In 0.8.4, continuous prepared-loop playback replaced those anchor jumps after a Cerberus level-pumping report.
- [Gazon and Blaisot, SAE 2006](https://saemobilus.sae.org/papers/cycle-cycle-fluctuations-combustion-noise-a-diesel-engine-low-speed-2006-01-3410) study cycle-to-cycle fluctuations in a low-speed diesel. BESS uses small correlated per-cycle variation attenuated at high rpm; the study's amplitude is not extrapolated to every corpus engine.
- [Baldan et al., SIVE 2015](https://air.iuav.it/handle/11578/264484) separate excitation, intake and exhaust in their model. Automation WAVs remain mono mixes: BESS cannot isolate those sources exactly or recover true cylinder pressure from these files.

All twelve ZIPs include a `.car` sheet declaring layout and cylinder count. BESS uses it only if its UID matches the audio blend, active `.pc` engine and JBeam. All twelve agree, but none gives firing order. A “4 cylinders” JBeam value is commented out even for V8s and is ignored. Per-cylinder events remain optional rather than automatically added to WAVs that already contain pulses.
