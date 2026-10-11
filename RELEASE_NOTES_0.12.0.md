# BESS 0.12.0

Released 2026-10-11. One common physical engine continues to power new engines, Automation resynthesis, listening and sound exports. Original source A and selectable original/BESS vehicle configurations remain preserved.

## Simulation additions

- Provenanced public-engine reference import and reproducible torque/power comparison, starting with Honda GX160 published ratings. This is comparison tooling; measured calibration has not passed.
- Explicit intake/exhaust wave feedback and a qualification bench separating delay propagation, port budgets and whole-engine resonance. The existing strict 10% resonance criterion remains unchanged.
- Optional fuel film, evaporation and reduced state-dependent combustion, saved with the common engine definition. Cold/hot stopped initialization and free-crank starter, tip-in, fuel cut, stall and restart demonstrations expose gas, fuel and thermal budgets, including numerical corrections.
- Optional compressor/turbine maps with conservative shaft budgets. Corrected turbine discharge area and gas composition; zero mapped flow retains a closed turbine path.
- Optional crank torsion and structural response. Static stops prevent backward-angle steps and negative drag work; radiation gain remains separate from mechanical energy.
- Offline finite-volume exhaust with the cylinder mixture equation of state and distributed air/fuel species. Shared conservative fluxes, refinement diagnostics and valid partial WAV output on cancellation are provided. This coupled 1D bench is not yet the production engine solver.
- Physical options remain explicit and disabled by default. Existing projects remain readable, and the fresh-engine mechanical level remains zero.

## Workshop and vehicle workflow since 0.11.1

- Shared editable Automation/free-engine workshop, engine-data provenance and saved sound settings, including independent exhaust decay and intake airflow noise.
- Complete vehicle exports retain the original sound/configuration and add a separately selectable BESS variant. BABM exchanges carry the selected folders and source-bound variant identity.
- Visible, cancellable export progress and bounded parallel sound rendering; saved engine sound gain applies to listening and generated exports.

## Validation and remaining gates

The frozen simulation sources passed **483 release tests, zero failures and six ignored diagnostics**, strict all-target Clippy and formatting checks before the version bump. The release build and desktop-copy verification are recorded separately in the [publication report](docs/reports/BESS-0.12.0-PUBLICATION-2026-10-11.md).

The explicitly rerun strict resonance diagnostic still fails. Cold numerical heat corrections, restart step convergence and non-isothermal 1D material-contact pressure accuracy remain open. Real-engine calibration, full production 1D integration, device endurance, listening and BeamNG acceptance remain separate gates. The detailed [simulation report](docs/reports/BESS-SIMULATION-SECOND-BATCH-2026-10-11.md) records the actual results and limitations.

Windows builds require an AVX2/FMA-capable processor (`x86-64-v3`). Building from source requires access to the pinned BDSP dependency; the executable runs without repository access. No vehicle archive, game installation or listening corpus is bundled with this release.
