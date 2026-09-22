# BESS 0.4 — Simulated driving

## Delivered

`dist/BESS-0.4.exe`, SHA256 `E262B138A858A354D452D41382C36D6D08F4637940C19C6D17E90279BF2C6BC0`.

Throttle and brake, automatic or manual N/1–6 gearbox, added wheel resistance (Nm after gearbox and final drive), and positive uphill grade. Mass, peak torque, engine inertia, wheel radius, final drive and ratios are editable. The display shows sound rpm, engine load, speed, engaged gear and transmitted/resisting torque. There is an automatic launch clutch, shift load reduction, stall prevention and pending downshift when the result would exceed the sound range. Turning off automatic mode retains the engaged gear. The 803–4,989 rpm bounds remain.

Direct and comparison-cycle modes remain available. UI projects use version 3 with driving; versions 1/2 can be read, and older projects without driving use direct mode. Stopping playback pauses motion. Reset stops the vehicle without allocation in the callback.

## Export

Selected-mode WAVs use the same `Bench` as playback. Simulated renders start at standstill with current controls held; automatic shifts occur during rendering. This does not capture earlier user input. Direct mode holds rpm/load; comparison mode adapts its 16 seconds to selected duration. A/B and character exports use the comparison cycle. No BeamNG mod or physics parameters are exported by this version.

## Checks

- 26 tests: 8 unit, 5 driving, 11 hybrid, 2 MVP. Strict linting and formatting passed. Tests covered acceleration, gears, neutral, resistance, braking, automatic shifts, out-of-range downshift, reset, pause, extremes, compatibility and project saving.
- Sample-by-sample 48 kHz comparison between simulated rendering and playback transport, including the same end fade. The audio effect of resistance was measured. Sound quality has no automatic subjective validation.
- Zero allocation in `Bench::next/set` over two seconds of throttle, resistance and gear changes, outside CPAL/crossbeam.
- Three 16-second renders from the real 56-WAV bank, mono 48 kHz / PCM24, with projects and CSV under `output/drive-0.4`. `tools/check_drive.py` independently checked them; historic measurements were stored in `drive-metrics.json`.
- At 85% throttle: open road, four shifts, final gear 5 and observed final speed 128.593 km/h; 900 Nm extra resistance, two shifts, final gear 3 and 73.409 km/h. In neutral, speed stayed zero and maximum rpm was 4,957.037. These are results of the generic bench, not predicted real-vehicle performance. PCM peaks were 0.12729 / 0.12279 / 0.11873 respectively.
- Sound BlasterX G6 at 192 kHz stereo with real bank, 80% throttle and automatic shifting: 30 seconds, 2,999 callbacks, 3.495 ms CPU maximum, no overruns. Output was silent (volume zero); historic report `drive-audio-check.txt` records it.
- Final executable opened with vehicle imported and simulation selected, playback stopped. The inspected egui capture showed controls, telemetry and WAV-render rules. Complete remote interactive use was not verified. The French-interface capture is excluded from the public package.

## Model and limits

Longitudinal motion includes rolling resistance, drag and grade. A bounded impulse couples engine and wheel inertia through a slipping launch clutch. These principles appear in [Vehicle Body](https://www.mathworks.com/help/sdl/ref/vehiclebody.html) and [How a Clutch Works](https://www.mathworks.com/help/sdl/ug/how-a-clutch-works.html); BESS uses its own listening-oriented approximation and does not copy the Simulink model.

Engine torque, gear ratios and other values remain generic editable settings, not identified from the ZIP. The torque curve is stylized; transmission is idealized; there is no tire model, reverse or realistic stalling. The acoustic ceiling also bounds speed for each gear and is not the actual rev limiter. Live geometry/inertia edits are bench controls, not simulated mechanical modifications while driving.

Usability and perceived sound response still required validation, followed by identification of actual characteristics and BeamNG export.
