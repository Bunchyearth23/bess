# BESS 0.3 — Acoustics and dynamics

## Result

Sound still comes from the vehicle's 56 WAVs. A new network replaces 0.2's single reflection with header, chamber and outlet, including area changes, loss and delay. A separate intake duct receives signal and envelope-modulated turbulence. Lift-off cut and pop events enrich transitions. Three editable characters (Balanced, Muted and Open) and a comparison export are available.

Executable: `dist/BESS-0.3.exe`. SHA256: `B53CB02F176D763F5CB93E844011F3B7BB47D7B51C0574D585A1F588235A7B49`. The 0.2 executable remains available. Its projects load with defaults for new fields, but the new model changes their sound.

## Evidence

- 19 tests passed: 8 unit, 9 hybrid and 2 MVP. Strict linting and formatting passed. Eight new controls each had a measured effect on a synthetic reference bank, alongside the 15 existing checks.
- Junctions conserved incident/outgoing energy at six area ratios. Network impulse responses were finite and decayed at 8, 44.1, 48 and 192 kHz under extreme geometries; this does not prove every geometry sounds natural.
- Rapid geometry/load/rpm changes produced bounded, continuous output at 44.1 and 192 kHz. `Hybrid::next/set` allocated nothing, including diameter and length changes, outside CPAL/transport.
- A was exactly unchanged when B effects changed. Pops produced nothing at constant load and died after lift-off. An early test found missing random triggers; a variable-interval lift-off clock replaced them and passed.
- Four mono PCM24 / 48 kHz WAVs, each 768,000 samples (16 seconds), under `output/acoustics-0.3`. Common RMS was 0.03041476, with <0.01 dB level difference. Source/Balanced/Muted/Open peaks were 0.19624 / 0.20291 / 0.19968 / 0.17146. `tools/check_characters.py` independently checked the renders. No clipping; signals differed at equal RMS.
- Sound BlasterX G6 at 192 kHz stereo: 30 seconds, 3,000 callbacks, 3.900 ms maximum, no overruns. Full synthesis from the real bank at zero volume; historic raw report `acoustics-audio-check.txt`. This is not proof of artifact-free listening.
- Final window opened with imported ZIP and playback stopped. An inspected egui capture showed character controls, driving, initial exhaust settings and exports. Lower controls and saving required scrolling; full remote interactive use was not verified. The French-interface capture is excluded from the public package.
- Graft was rebuilt and checked: 138 nodes, 125 links, 15 files, deterministic synchronized graph. No LLM summary layer was generated.

## Scope and next work

The user's “good, continue” feedback concerned 0.2 and did not automatically validate 0.3's realism or superiority. Comparing characters by ear remained the next subjective check.

Dimensions were sound-model settings, not ZIP-resolved parts. One aggregate header was simulated; cylinders, banks and firing order were not invented. The source WAV already contained exhaust sound, so the chain added coloration without deconvolution. Temperature was effective and coefficients stylized, with no mean-flow or thermodynamic simulation.

Next development called for rpm/load maps and periodic/residual separation, then per-bank headers once reliable engine facts became available. The 4,989 rpm bank limit and BeamNG integration remained open. Physical sources and roadmap appear in [research](../ENGINE-SOUND-RESEARCH.md).
