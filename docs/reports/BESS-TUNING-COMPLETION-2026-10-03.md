# Independent cam tuning and intake mapping

The common engine definition now carries separate intake and exhaust duration, lift and static phase controls. The physical cylinder builds one harmonic profile per valve, uses each profile for its flow area and valve-contact events, and ends combustion at the actual exhaust opening. The offline cylinder prototype consumes the same configuration.

Old `duration_deg` and `lift_mm` values remain a shared compatibility baseline. New optional intake/exhaust overrides are independent of one another; absent overrides preserve the prior resolved quantities and arithmetic. Project v4 requires no destructive migration. Default tuning remains omitted; an active tuning block includes the optional fields so per-parameter provenance also covers future overrides. The shared legacy controls are not duplicated in the interface.

The workshop exposes intake/exhaust duration at 0.050 inch, lift, independent advance and shared base lobe separation. Per-field, per-cam and complete-cam resets retain their stated scopes. The overlap display uses both durations and both phases and labels dynamic intake VVT separately from static timing.

Geometry controls now carry an “acoustic geometry” tag. Its explanation makes the optional pressure-wave feedback explicit: geometry can also affect torque with that coupling enabled, so these controls are not labelled as universally sound-only.

## ITB and inherited gains review

The throttle override remains the diameter of one equivalent total flow area. The interface now displays the equivalent diameter and the corresponding diameter of each individual body. The stock 1.35 area factor is applied only when deriving ITB sizing, never on top of an explicit total-area override. The ITB volume is explicitly labelled an equivalent shared gas-supply volume; physical runners and acoustic trumpets remain per cylinder. This remains a bounded mean-flow approximation, not identified per-cylinder manifold geometry from Automation.

Dormant event-renderer timbre/level fields do not excite or scale the physical stems. User intake/mechanical layer levels and the engine-layer gain remain explicit listening/export mix settings; they are preserved rather than recalibrated silently. Their preferred balance still requires listening. No new acoustic calibration against a real engine is claimed.

## Verification and demonstration

Added regressions cover legacy JSON neutrality, independent override resolution and resets, separate real valve schedules, the exhaust duration-to-seat conversion, actual changed cylinder output, project round trips, total ITB area conservation and absence of dormant legacy timbre gains in the physical output. UI range tests cover every new override and the overlap/ITB display arithmetic.

`examples/tuning_completion.rs` creates four reloadable v4 projects, four three-second PCM24 WAVs and seven-point computed dyno curves: baseline, intake-only, exhaust-only and individual intakes. It checks project identity, finite non-silent audio and changed output under identical user mix levels, then writes `validation.json`. Run the example with an output-directory argument.

Release verification: all nine `tests/tuning.rs` tests passed, including the four added regressions. The targeted physical valve-schedule test also passed. Both demonstration examples compiled; `tuning_completion` completed all four project reloads, finite non-silent WAVs and computed curves. The artifacts and their hashes are in `output/final-readiness-20261003/tuning/validation.json` alongside the four projects and WAVs. The full integrated/UI suite is coordinated separately by the main task.

These checks do not establish real-engine calibration, subjective sound acceptance or BeamNG behavior.
