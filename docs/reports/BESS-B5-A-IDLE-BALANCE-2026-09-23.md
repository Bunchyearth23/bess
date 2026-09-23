# b5_a idle level correction — 2026-09-23

## Finding

The user reported that the b5_a idle is louder than the rest of the range in both standard and experimental B. The imported Automation bank has a pronounced early-RPM recording-level hump. At 1,052 rpm, its processed source RMS is 0.34493 in the low-load layer and 0.27620 in the high-load layer; around 3,000 rpm it is roughly 0.024–0.080. The B modes followed this recorded level profile. This diagnosis does not imply an authentic engine is quieter at idle; it identifies a disproportionate level jump in this particular bank.

Across the twelve local vehicle archives, five show the same pattern in both load layers: `b5_a`, `b5_c`, `b5_gt`, `nord_optis`, and `volk_icarus_ii`. Their early/upper source-RMS ratios exceed 2.5 in each layer. The other seven do not meet this gate.

## Correction

At bank preparation, BESS compares the strongest WAV level in the first 600 rpm above the minimum to the median level in the upper half of that load layer. If both layers exceed 2.5 times the upper median, it builds a bounded gain curve for B. The curve limits the early excess toward half that median and releases smoothly across the lower 2,200 rpm. It interpolates between the two load layers and follows changing rpm/load with a 40 ms control smoother. This is a recording-level correction, not a change to cylinder timing, spectral fit or user volume.

The correction multiplies the wet B sound and its engine/exhaust stems consistently in both standard and experimental modes. Source A and `source_reference` remain unchanged. Banks without the two-layer anomaly use unity gain. Future standard BeamNG exports follow the corrected B rendering; already installed add-ons were not regenerated.

## Steady checks

These are direct 48 kHz renders at identical volume and controls, using the last second of a four-second steady hold. No per-file normalization is used. The full probe output is in `output/b5-a-level-probe-20260923.txt`.

| Load | RPM | A RMS | Standard B before → after | Experimental B before → after |
| ---: | ---: | ---: | ---: | ---: |
| 0.12 | 1,052 | 0.117413 | 0.091617 → 0.005933 | 0.117662 → 0.007619 |
| 0.12 | 3,000 | 0.009723 | 0.006552 → 0.006541 | 0.011332 → 0.011314 |
| 0.65 | 1,052 | 0.104129 | 0.082624 → 0.009408 | 0.104958 → 0.011951 |
| 0.65 | 3,000 | 0.018863 | 0.009634 → 0.009611 | 0.021523 → 0.021471 |

Additional points from 803 to 4,358 rpm were checked at both loads. At 0.12 load, standard B goes from 0.005273 at 803 rpm to 0.006541 at 3,000 rpm; experimental B goes from 0.007187 to 0.011314. At 0.65 load, the corresponding values are 0.008137 to 0.009611 and 0.011116 to 0.021471. The extended release avoids a new jump near 1,900 rpm.

Nine unnormalized listening clips for A, standard B and experimental B at 803, 1,052 and 3,000 rpm are in `output/b5-a-idle-balance-20260923/`. The three Cerberus comparison WAVs at 3,000 rpm remain byte-identical to the previous build, confirming that an unaffected bank keeps its sound. Two synthetic balance tests and one local b5_a regression test pass. The full release suite and strict Clippy passed. The corrected GUI was first built separately while the prior app was open; after it closed, the usual `target/release/bess.exe` was rebuilt.

Both corrected GUI builds rendered the b5_a 1,052 rpm comparison with identical WAV hashes. That comparison deliberately adjusts B up to A's unusually high idle level for timbre listening; use the unnormalized nine WAVs above to judge the actual RPM level balance. Numerical checks do not establish perceptual quality or live-device acceptance.
