# Standard resynthesis profiles — local development candidate, 2026-09-23

## Decision

The experimental listening mode remains in the interface. BeamNG export continues to use standard resynthesis. The user defined 100% Automation timbre as retaining the recorded tone **inside** BESS processing, not bypassing those effects. At 0%, standard resynthesis should use ZIP-derived measurements without replaying the original PCM waveform. The user asked for 12 vehicles, both simple and advanced controls, and several named profiles per vehicle for listening comparisons. The first local set is Natural, Smooth and Raw.

## Implementation

The imported RPM/load bank already separates a cycle-locked pressure component and irregular texture. A descriptor analysis also measures level, broad tonal bands, low engine orders and stochastic bands. Standard resynthesis now blends those two excitation sources before its pulse shaping, texture envelope, intake duct, mechanical detail and exhaust network. The `source_timbre` setting controls the blend. At 100%, the recorded components supply the excitation and the BESS acoustic path still applies. At 0%, the measured descriptors drive newly generated excitation; the original waveform remains available only as the A reference and as a level guide.

The interface offers direct Automation timbre, resynthesis, exhaust, intake and mechanical controls. Detailed pressure, texture, dynamics and acoustic geometry controls are in an advanced section. A free name is saved in each project and used to create distinct BeamNG configuration, part, blend and WAV paths. The named add-on is installed alongside the original Automation ZIP. Several profiles can coexist. The experimental mode is still excluded from both the selectable add-on and the BeamNG level preview.

This is a source-guided approximation. Automation supplies exhaust audio, so no independent intake microphone, actual firing order, combustion pressure or vehicle pipe geometry can be recovered from the ZIP. The generated detail and its taste must be checked by listening. Relevant model precedents include [cycle-synchronized engine synthesis](https://pubmed.ncbi.nlm.nih.gov/23145595/), [pressure-pulse and resonator modeling](https://arxiv.org/abs/2603.09391), and [order plus stochastic decomposition](https://arxiv.org/abs/2606.21521). BESS implements its own bounded approximation rather than claiming those studies' methods or results.

## Technical checks

Pending fleet verification and installation results. The local generation and audit tools are `tools/build_resynthesis_fleet.ps1` and `tools/audit_resynthesis_fleet.py`. Vehicle ZIPs and generated add-ons remain ignored local files, outside Git and public application packages.

## Listening gate

Compare Natural, Smooth and Raw against the original configuration at idle, steady midrange (especially Cerberus near 5,200 rpm), acceleration and lift-off, from cockpit, hood and tailpipe positions. The acoustic quality and BeamNG spatial mix remain unverified until this in-game listening pass.
