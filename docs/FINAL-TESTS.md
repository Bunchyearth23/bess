# BESS 0.8.4 — Final test guide

`dist/BESS-0.8.4-public` contains the application, twelve source vehicles, projects, comparisons and vehicle copies for BeamNG. Open `START.html` to reach each item. Keep the complete folder together: delivered projects use relative paths to `sources`.

## 1. Sound identity and naturalness

Open the comparisons and try at least Genesis, B5 A, Thunderhawk Zero and Volk Icarus II, then the remaining vehicles. A is the Automation bank replayed by BESS, B is the automatic 0.8.4 setting, and C is the previous 0.8.3 treatment. The three tracks for each vehicle have equal integrated RMS.

Also open `CERBERUS-5200.html`. Six eight-second clips compare A, B and C at a steady 5,200 rpm, at full and low load. Cerberus should remain stable at that rpm and pass through 4,989–5,338 rpm without pumping or artificial jumps. A level dip already present in A between those samples does not, by itself, establish a new B defect.

- [ ] Engine character remains recognizable.
- [ ] Idle has no added whistle, distracting beating or obvious repetition.
- [ ] Slow and fast acceleration have no clicks between samples or sudden timbre jumps.
- [ ] Cerberus held at 5,200 rpm has no distracting level beating at low or full load; also check a slow sweep through that range.
- [ ] Full load and lift-off transition convincingly, without distracting artificial background sound.
- [ ] Low and high load at the same rpm have credible textures.
- [ ] B's pressure front sounds natural, especially on already bright engines.
- [ ] Record A/B/C preference for each vehicle; B is not assumed to be better.

## 2. Application settings and driving

Launch `BESS.exe`, open a B project under `listening`, and listen.

- [ ] The mouse wheel adjusts a hovered slider; Shift makes finer changes. Outside a slider, it scrolls the panel.
- [ ] **At high rpm** and **At full load** affect the expected sound regions.
- [ ] **Added color** adjusts additions without losing the source.
- [ ] **Cycle variation**, **Pressure front** and **Pulse-linked texture** make useful changes without beating or whistling.
- [ ] Intake, exhaust path, chamber and absorption make useful changes.
- [ ] Throttle, brake, gearbox and load remain in the Driving block.
- [ ] Rpm stays within each vehicle's bank range, including after opening a project.
- [ ] Save and reopen a project to restore its settings and correct bank.
- [ ] Ten minutes of listening and editing produce no interruption or crackling.
- [ ] Audio status reports 48 kHz on the Sound BlasterX; another device shows its actual format, including a fallback when needed.

## 3. Optional combustion

The Automation sheet provides layout and cylinder count when the files agree. It provides neither firing order nor angles. Combustion events are disabled on import because the WAVs already contain their own pulses.

- [ ] Confirm that the displayed sheet matches the imported vehicle.
- [ ] If reliable data exist, enable **Engine and combustion** and enter known angles; the proposed even spacing is not a manufacturer's firing order.
- [ ] Check force, duration and angles; lower **Added color** if the effect dominates the source.
- [ ] Leave this option off if firing order or angles are unknown.

## 4. Export and BeamNG test

Choose an existing copy under `beamng`, or use **Create BeamNG vehicle** after customization. Each folder has installation instructions, a full ZIP, settings and a provenance manifest.

1. Keep the original ZIP and **disable it** in the game's mod manager.
2. Install the `bess-… .zip` listed in that vehicle folder. The names are already distinct. Locate the active BeamNG user mods folder for your installation.
3. Enable only the BESS copy and reload the vehicle.
4. Test idle, acceleration, load, lift-off, gear changes and inside/outside cameras. Check again after saving and reloading.
5. To compare with or return to the original, disable BESS and re-enable the original.

- [ ] The game recognizes the mod and loads the vehicle without audio errors.
- [ ] All rpm points and both load states remain audible.
- [ ] No loop clicks or gaps occur during transitions.
- [ ] Level is balanced with the vehicle's other sounds.
- [ ] Vehicle pops, turbo and starting still work.

**Transferred:** stable tone, source, curves evaluated at both original loads, acoustics and explicitly configured combustion, encoded into WAVs. **Still handled by the game:** physics and driving controls, start, stop, turbo and pops. BESS transients do not become a BeamNG controller; a middle load map is not reproduced exactly by interpolation between two layers. Playback volume and A/B compensation are not applied to the mod. A common safety gain prevents clipping.

## Feedback to retain

For each issue, record the vehicle, project, rpm/load, affected setting, A/B comparison, application or BeamNG, camera view and a short recording if possible. S-10 remains open until these real tests are complete.
