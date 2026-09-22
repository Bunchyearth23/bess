# Audio realism — steps and demonstrations

Plan requested on September 22, 2026, after user feedback that the sound was still artificial. Existing foundations: Rust + BDSP, imported Automation audio, driving separated from sound settings, and rpm limited to the exported bank range.

## Validation rule

The user subsequently requested that we execute the [work sequence](STEPLIST.md) without waiting for intermediate feedback. User listening tests are grouped at the end. Each step prepares a technical demonstration, which does not establish sound preference.

A step is implemented when its functional demonstration has been delivered and verified. Written code, compilation or tests alone are insufficient. Each delivery includes an identifiable executable, the vehicle and settings, a reproducible procedure and observed results. A sound improvement also needs a level-comparable A/B; preference and realism still require user listening.

Keep checkboxes open until that evidence exists. Reports distinguish automated checks, observed operation, user listening and BeamNG tests. Functions dependent on later steps are not described as available.

## Ordered steps

### 1. Build a corpus and listening reference

- Inventory every ZIP under `cars`: banks, audio duplicates, load layers, rpm ranges and metadata actually available.
- Verify import with BESS's reader, handle errors explicitly and select several genuinely different banks for later tests.
- Prepare Automation and current BESS references without modifying archives.
- **Functional demo:** open the selected vehicles in BESS, play their source and current treatment, export comparisons and trace provenance. Publish the import result for every ZIP.

### 2. Stabilize cycles and rpm transitions

- Measure the samples' actual periodicity and confidence instead of relying only on nominal rpm.
- Prepare synchronized overlapping segments; align neighboring rpm and load samples; use a safe fallback when analysis is ambiguous.
- Preserve recorded natural variations and exported rpm limits.
- **Functional demo:** slow and fast sweeps, fixed rpm and crossings between samples on the selected banks; A/B with current playback to assess clicks, beating and loop effects. Check bank endpoints too.

### 3. Separate periodic content and texture

- Approximately decompose WAVs into a cycle-related component and residual; measure reconstruction before applying transformations.
- Prefer source-derived texture to generic added noise for variation.
- Expose pulses and texture separately without presenting them as isolated intake or mechanical recordings.
- **Functional demo:** listen to each component, their reconstruction and live modulation; idle and loaded A/B across several banks, without obvious identity loss or separation artifacts.

### 4. Build rpm × load maps

- Drive body, texture, intake, mechanical sound and acoustic response by rpm and load with smooth transitions.
- Keep tone settings separate from driving controls; save and restore the maps.
- Intermediate states derived from two exported load layers remain a model, not new engine measurements.
- **Functional demo:** compare low and high load at equal rpm; change one map region and observe the local effect, then accelerate, lift off, shift and reopen the project. Confirm the corresponding WAV render.

### 5. Calibrate intake and exhaust

- Account for coloration already present in the WAVs to avoid stacking artificial resonances.
- Calibrate treatment by bank with a direct/treated control; add per-bank branches only if data support them.
- Make changes to length, diameter, chamber and absorption stable and audible without claiming sound alone identifies real geometry.
- **Functional demo:** propose a default on import, compared with Automation at similar level; manipulate each acoustic group and listen through acceleration and lift-off. “Better immediately after import” requires user listening validation.

### 6. Add engine event-based excitation

- Identify reliable export facts: cylinders, layout, firing order and active parts when available. Request explicit values for missing information; do not infer them solely from spectral peaks.
- Prototype combustion and valve event excitation with correlated variation and acoustic interaction; combine it with the source without doubling pulses.
- Retain a useful hybrid fallback when physical data are insufficient. Neural model training is not a prerequisite.
- **Functional demo:** a documented or explicitly configured engine, with/without-excitation comparison, audible parameter effects and stability through driving. Document remaining approximations.

### 7. Deliver and validate BeamNG audio

- Produce rpm/load loops, configuration and necessary events as a separate package, preserving the original and provenance.
- Distinguish settings rendered into sound from behaviors requiring an in-game controller; identify anything that cannot transfer.
- **Final functional demo:** import a vehicle, obtain improved sound without initial editing, customize it, export it and actually drive it in BeamNG. Check idle, acceleration, full load, lift-off, gear changes and inside/outside cameras. A WAV or compilation does not validate this step.

## Evidence to retain for each step

Report under `docs/reports`: version, banks and hashes, procedure, settings, demo files, observations, comparison with the preceding step and remaining validation. Use the same scenarios and reference banks to detect regressions, then expand coverage with new banks.
