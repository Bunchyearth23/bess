# RPM stability and grouped simulation controls — 2026-10-11

The user reports unstable acceleration in driving mode and difficulty holding idle with the optional models. Back pressure also needs to be grouped with the other simulation choices. The earlier fuel/flame dyno corrections remain in place.

## Reproduction and causes

`examples/rpm_stability.rs` reproduces idle and a first-gear acceleration with the UI's 1 ms drivetrain cadence. Idle preparation holds 1,200 rpm, closed accelerator, for one second; the crank then runs freely for nine seconds. Statistics cover seconds 8–10. Synthetic default hardware and estimated coefficients are used, not a saved user project or measured engine.

1. Liquid fuel's retained film/droplets could deliver vapor into the cylinder throughout its exhaust stroke. That admission model puts fuel into the wrong gas phase: outgoing exhaust carries fuel away before ignition, and its reaspiration can over-enrich later cycles. Default fuel/both/all idle cases stop completely. Before correction the failed trace crosses lambda 0.55. Merely adding exhaust reaspiration to the metering history changes the failure to lean combustion and still stalls; adding residual-charge compensation also fails. Neither trial is retained.
2. The clutch calculation reads crank RPM and total inertia although torsion applies the resulting load to the separate flywheel. These are different speeds and inertias. This delayed feedback drives large slow oscillations. The corrected fuel model can be replayed with either clutch input, isolating this fault without comparing different throttle schedules.

## Changes and boundaries

Default `FuelDelivery::Admission` retains liquid until incoming intake gas can deliver vapor to the cylinder. The existing mass, sensible energy and latent-heat budgets are preserved. The old metering formula is retained. An explicit `FuelDelivery::Cylinder` mode allows evaporation with closed intake valves, including later injection experiments; timing alone does not select the delivery location. Older saved configs missing the field default to admission. Older deliberate in-cylinder experiments must explicitly choose cylinder delivery.

This remains a reduced admitted-charge model: evaporation and port transport are lumped and no separate port vapor reservoir is solved. The correction is functional, not a measured wall-film calibration. In particular, retained liquid at rest does not resolve actual closed-port evaporation. No torque multiplier, RPM restoration, artificial inertia increase or display smoothing is introduced.

`Engine::drive_shaft()` supplies flywheel RPM and flywheel inertia with torsion; otherwise it supplies the ordinary crank RPM and total inertia. Both designed and Automation-imported listening paths use it for the clutch. Combustion, crank phase, engine controls and RPM telemetry continue to follow the actual crank.

Back pressure, intake feedback, fuel, flame, torsion, mapped turbo, structural response, VVT alternative, afterfire, knock and acoustic solver choices share the **Simulation models** panel. Acoustic exhaust decay and observation-level calibration remain in **Exhaust sound**.

## Observed synthetic results

Final admission-model reproduction at 8 kHz output (gas solver still at least 96 kHz):

| Options | Idle mean / min–max, rpm | End-window fuel-cut frames |
| --- | --- | --- |
| Baseline | 844.0 / 829.3–855.0 | 0 |
| Fuel | 845.2 / 820.0–883.1 | 0 |
| Flame | 843.6 / 826.3–855.1 | 0 |
| Fuel + flame | 841.4 / 816.7–883.9 | 0 |
| Four visible models | 837.7 / 813.0–879.5 | 0 |

With four models during first-gear acceleration near the limiter, the old clutch observation gives 5,766–7,197 rpm; the flywheel observation gives 6,766–7,054 rpm (1,431 → 288 rpm peak-to-peak). Torsion alone similarly gives 5,925–7,200 versus 6,786–7,045 rpm. Limiter cuts here are expected and are distinct from idle instability. These ranges include instantaneous combustion/torsion ripple; they are not claims of constant RPM at the limiter.

Evidence: `output/rpm-before/`, `output/rpm-fixed/`, `output/rpm-idle-before.log`, `output/rpm-idle-exhaust-meter.log`, `output/rpm-idle-residual-budget.log`, `output/rpm-idle-gate-only.log`. Misfire fields in the CSVs are cumulative counters, not per-window totals. The earlier before-driving run begins tip-in later; only idle is compared directly across those runs. The shaft comparison within `rpm-fixed` uses identical preparation and commands.

## Validation and delivery

The focused two-test suite passes in the debug profile on final source (`output/rpm-fix-focused-debug.log`):

- 48 kHz free idle: fuel 845.17 rpm (820.01–883.09), four models 837.68 (812.86–879.35), five including exhaust feedback 837.76 (815.20–872.55); no end-window fuel cuts or stalls.
- Actual `Bench` driving, first gear at 65% accelerator, with torsion: 6,785–7,045 rpm; all five models: 6,772–7,057 rpm. Both reach about 68 km/h. This path starts freely at nominal idle, unlike the independently prepared diagnostic.

The final locked all-target release suite passes: **493 tests, zero failures, six excluded diagnostics, 65 suites** (`output/rpm-fix-all-tests-final.log`), after a 12 min 12 s optimized rebuild. Strict all-target Clippy passes on final source (`output/rpm-fix-clippy-final.log`), as do formatting and diff whitespace checks. New regressions exercise fuel/energy accounts during admission and explicit closed-valve cylinder delivery, and delivery-field migration/roundtrip. User listening/driving reception and authentic engine calibration remain open.

The first full release run builds in 26 min 21 s and reports 329 library tests passing, one failure and four excluded diagnostics. The failure is the old 0.75 s cold-charge heat deadline: initial 280 K admission produces no combustion yet (lambda 2.1–3.0 at failed sparks), despite closed inventories and positive evaporation/DFCO tail. The diagnostic finds first burning at 0.95995 s with admission, versus 0.16040 s with cylinder delivery. The historical cylinder cases retain their original 0.75 s deadline and unchanged heat/ledger gates; admission cases additionally run for 3 s with the same gates. All four pass in the focused test (`output/rpm-cold-delivery-modes.log`). No production equation is changed to force this deadline. This standalone charge fixture has a default coolant boundary and is not evidence of an actual fully cold free-crank start.

The newly rebuilt executable's default `--render` completes successfully and produces `output/rpm-fixed-default.wav`, SHA-256 `50EF7089BEFAA804DA8DC40389FCF685FD5C4757ECC7E68357AEC19E4EC95F51`, identical to the previous default render. This is default-render invariance, not listening acceptance.

`output/rpm-stability-comparison.png` and its SVG show the unsmoothed 10 ms traces. The corrected idle still dips to about 590 rpm before recovery and briefly overshoots; the final-window stability figures do not qualify every transition, engine or fully cold start.

The ordinary release binary build succeeds (`output/rpm-fix-release-build.log`). At 21:31 CEST, `D:/Desktop/BESS.exe` is replaced with the checked 0.12.0 build; SHA-256 `606B5D4C711185CE8CE6DE0769C1DC0BF4787E60A02A2EE40D155A9896AFC40C` matches `target/release/bess.exe`. The prior `A3374E84…857F485A` binary and receipt are retained in `output/rpm-fixed-desktop-20261011-213118/`. A default render executed through the delivered Desktop binary succeeds (`output/rpm-fixed-delivered-default.wav`) and retains the identical `50EF7089…EC95F51` hash above. No new commit or push is performed.
