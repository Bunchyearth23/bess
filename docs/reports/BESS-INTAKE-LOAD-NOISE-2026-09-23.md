# BESS Intake and load-noise follow-up — 2026-09-23

## User observation

On b5_a, raising Intake level makes a crackling sound most apparent above roughly 40% Engine Load. A breath-like noise remains during RPM rise. The previous idle-level and B hiss corrections are the baseline for this pass.

## Diagnosis and change

- Both B paths increased Intake with Engine Load. The standard path multiplied a source residual by load at excitation and output, opened the intake filter progressively at high load, and applied a broad 1.7 kHz final filter. The experimental path multiplied generated valve noise by two load-dependent terms and retained upper bands.
- The experimental exhaust texture also had a load-proportional noise floor. Engine Load therefore directly raised a continuous airflow sound during acceleration. There is no code threshold at 40%; the user's threshold describes audibility.
- Standard B now opens its intake filter less at high load, has a 1.2 kHz final low-pass, and uses a gentler output load/transient curve. Experimental B uses less upper-band valve noise, a 950 Hz final low-pass and a gentler high-load curve. Generated exhaust flow has a smaller load-only floor and a 2.2 kHz low-pass before mixing with combustion texture.
- Source A rendering, Mechanics, RPM balance and installed BeamNG add-ons were not edited in this pass.

## Measured b5_a evidence

The `load_noise_probe` renders two otherwise identical B voices and subtracts their mixed output to isolate the named control. At 3,000 rpm, 90% load, 48 kHz, with level matching disabled, settled values were:

| Contribution | Before RMS | After RMS | Before >2 kHz RMS | After >2 kHz RMS |
| --- | ---: | ---: | ---: | ---: |
| Standard Intake | 0.0005708 | 0.0002664 | 0.0002177 | 0.0000669 |
| Experimental Intake | 0.0008604 | 0.0004770 | 0.0002690 | 0.0001092 |
| Experimental generated texture control | 0.0022282 | 0.0015954 | 0.0010578 | 0.0005403 |

The generated texture control includes both airflow and combustion texture, so its subtraction is not a pure airflow measurement. At 20/40/65/90% load, standard Intake RMS is now 0.0000374 / 0.0000691 / 0.0001449 / 0.0002664; experimental Intake RMS is 0.0000972 / 0.0001708 / 0.0003001 / 0.0004770. The response remains continuous and grows with load.

Raw eight-second b5_a acceleration clips, with one second at idle and six seconds ramping RPM and Engine Load, are in `output/b5-a-load-noise-after/`. Each B mode has a matching `no-intake` clip; experimental B also has `no-flow` and standard B a `no-texture` diagnostic. None of these whole-engine clips was normalized separately. The standard B full-ramp high-pass RMS above 2 kHz is 0.0013006, versus 0.0011722 with standard residual texture muted; the remaining upper-band energy is spread across other source-guided contributions. The recordings alone cannot establish which part the listener perceives as breath noise.

Normalized six-second isolated Intake and Mechanics listening clips at 3,000 rpm / 65% load are in `output/b5-a-intake-review-3000-65/`; Cerberus was also rendered at 5,200 rpm / 65% load in `output/cerberus-load-noise-review-5200-65/`. Those isolated clips were deliberately boosted for inspection and do not represent the mixed loudness.

## Verification and boundary

`cargo test --release --all-targets` passed (all targets, including 49 library tests, 28 hybrid integration tests and 7 standalone tests). Strict release Clippy passed, and `target/release/bess.exe` was rebuilt. The ramp files have finite audio, mono 48 kHz PCM24 and peaks below 0.211. The exact audible result still needs b5_a listening in the GUI and, separately, BeamNG if the user wants to update installed add-ons. No in-game behavior is inferred from offline measurements.
