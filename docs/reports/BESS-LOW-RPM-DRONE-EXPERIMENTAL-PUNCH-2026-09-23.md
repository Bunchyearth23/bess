# Low-RPM standard exhaust drone and experimental pressure response — 2026-09-23

## Listening observations

The user reports that experimental B lacks engine "oomph", then that standard resynthesis has an extremely regular exhaust drone at low RPM. This pass uses b5_a as the main comparison and Cerberus A as a second vehicle check. The edits apply to the live B modes; the Automation A branch is unchanged.

## Diagnosis

- At b5_a 1,052 rpm / 12% load, a five-second standard B render put 59.1% of its 20–2,000 Hz power in 150–300 Hz, with a dominant 176 Hz line. The source A dominated at 35 Hz. A zero-pressure-edge diagnostic moved standard B's dominant line back to 35 Hz. The normalized derivative of the pressure pulse was therefore the main cause of the artificial upper firing harmonic at idle.
- At b5_a 3,000 rpm / 65% load, experimental B's adjacent cycle similarity was 0.983 versus 0.928 in A. Its 50 ms RMS p95–p5 span was 0.705 dB versus 2.043 dB in A. The broad bass share was already strong, so raising bass EQ alone would not address the missing pressure movement.

## Changes

- Standard B's pressure-edge blend is now reduced below roughly 2,500 rpm and reaches its previous value at higher RPM. On banks with an existing abnormal-idle balance correction, a bounded level factor retains the earlier idle-level fix after this timbre change.
- The descriptor-driven pressure event varies more with measured source cycle motion, within an 18% event bound. This also affects the generated contribution inside standard B, while preserving deterministic playback.
- Experimental B now gives the pressure pulse a brief load-rise boost. This follows the existing fast-versus-slow Engine Load difference; it does not add a sustained oscillator or more airflow noise.

## Measured results

At b5_a 1,052 rpm / 12% load, standard B's dominant line moved from 176 to 35 Hz. The 150–300 Hz share of 20–2,000 Hz power fell from 59.1% to 28.8%; the 20–80 Hz share rose from 20.4% to 66.8%. Its raw settled RMS went from 0.0058276 to 0.0054244, avoiding a renewed idle level hump. Source A's before/after SHA-256 is identical at both 1,052 and 3,000 rpm.

At b5_a 3,000 rpm / 65% load, experimental B's adjacent cycle similarity changed from 0.9834 to 0.9791, cycle-level coefficient of variation from 0.0290 to 0.0481, and 50 ms level span from 0.705 to 1.192 dB. The source spans 2.043 dB in that held condition. On a 12%→85% load step at 3,000 rpm, experimental B's early loaded 200 ms RMS is 2.56 dB above its later settled loaded RMS; the absolute peak is 0.0875. Cerberus A's matching step is +1.79 dB with peak 0.1433. Both are below the 0.95 safety ceiling. This verifies an attack response, not perceived punch.

The raw b5_a files are under `output/b5-a-punch-before/` and `output/b5-a-punch-after/`. `idle-standard.wav` is the low-RPM comparison; `step-experimental.wav` is a six-second held-RPM load step. The Cerberus check is under `output/cerberus-punch-after/`. These files are not separately level-normalized. `examples/punch_review.rs` creates the comparisons, and `tools/engine_punch_probe.py` measures cycle repeatability, broad bands and step levels.

## Validation boundary

`cargo test --release --all-targets` and strict release Clippy passed, and the GUI executable was rebuilt. The new sound still requires the user's listening judgment. Installed BeamNG add-ons have not been regenerated, so this pass makes no claim about their current in-game sound.
