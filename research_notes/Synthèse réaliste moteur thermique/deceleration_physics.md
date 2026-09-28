# Engine deceleration physics and sound (lift-off, overrun, DFCO, return to idle): quantitative fact base for procedural synthesis

Scope note for the report writer: the web sources I could reach (ECU documentation, tuning guides, patent snippets, paper abstracts) give strong **structural** facts: which conditions, which ramps, which mechanisms. They give few **published numbers** for sound levels. Where no source gave a number, I derived a first-principles estimate and kept it in the *Inferences* subsections, with the arithmetic shown so it can be checked. Nothing in *Cited Findings* is invented. Some findings come from search-engine summaries of pages I could not fetch in full; those are marked "(search summary)".

---

## 1. Throttle closure: MAP drop, time constant, pulse amplitude vs trapped mass, pumping loss

### Takeaway
When the throttle closes, the manifold empties in about 2–3 engine revolutions. MAP falls to its lowest value of the whole operating map (below idle MAP). During motoring (fuel cut), the exhaust-valve-opening "blowdown" pulse loses its combustion pressure. It becomes weak, and it can even reverse, because cylinder pressure at EVO falls below exhaust back-pressure. This is the main physical reason a firing-pulse synth sounds wrong on overrun.

### Cited Findings
- Closed-throttle deceleration gives the highest manifold vacuum of any operating state, "typically four to five inches Hg higher than at idle". As the engine slows, the vacuum decays back toward the idle value. — [Manifold vacuum, Autopedia](https://automobile.fandom.com/wiki/Manifold_vacuum) (search summary); decay behaviour also described in [US 4675135, Engine intake system with deceleration valve](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/4675135) (search summary)
- In a modern pressure-control patent, a target manifold pressure (e.g. 75 kPa) "may be reached as the throttle closes in four cylinder intake valve opening events". In other words, the manifold settles in about 4 intake events (= 2 crank revolutions on an I4). — [US 10024256, System and method for intake manifold pressure control](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10024256) (search summary)
- Speeduino community guidance for a MAP-based DFCO threshold is "hard vacuum perhaps 5 kPa below lowest idle". This confirms that overrun MAP sits clearly below idle MAP. — [Speeduino forum, Deceleration settings](https://speeduino.com/forum/viewtopic.php?t=3595) (search summary)
- Some ECUs (rusEFI `useIacTableForCoasting`) open the idle air valve during overrun "to help reduce engine braking … for large engines in light weight cars or engines that have trouble returning to idle". Overrun MAP (and therefore pumping loss and pulse strength) is therefore partly a calibration choice. — [rusEFI rusefi_config.txt](https://github.com/rusefi/rusefi/blob/master/firmware/integration/rusefi_config.txt)

### Inferences
- **Manifold emptying time constant.** From the filling/emptying model: volumetric flow drawn by a 4-stroke is Q = ηv·Vd·N/120, so τ = V_man/Q. Expressed in crank revolutions, τ_rev = 2·V_man/(ηv·Vd). With the typical V_man ≈ 1–1.5·Vd and ηv ≈ 0.7–0.9, τ ≈ 2.2–4.3 revolutions. At 3000 rpm that is ~45–85 ms, and at 6000 rpm ~22–43 ms. This matches the patent's "four intake events". **Synth rule:** after the throttle closes, apply a one-pole low-pass on MAP with τ ≈ 3 revolutions (convert to ms at the current rpm), not a fixed ms value.
- **Overrun MAP numbers.** At sea level, 1 inHg ≈ 3.39 kPa. Assuming a typical warm idle vacuum of ~18–20 inHg (MAP ≈ 34–40 kPa; this idle figure is my assumption, not from the sources above), adding the cited +4–5 inHg gives an overrun MAP of roughly **17–24 kPa absolute**. A synth can use a target of ~20–25 kPa at high rpm, relaxing toward idle MAP (~35 kPa) as rpm falls toward idle.
- **Trapped mass ∝ MAP.** At fixed temperature, charge mass per cycle scales ~linearly with MAP. On overrun it is about 20–25 % of WOT trapped mass.
- **Blowdown pulse amplitude.** The exhaust pulse at EVO is driven by (p_cyl,EVO − p_exhaust).
  - Firing at WOT: p_cyl,EVO is typically several bar (order 3–5 bar abs; textbook range, not sourced here), giving a strong positive blowdown pulse.
  - Firing at light load with lean or retarded combustion: EVO pressure is lower, but late-burning gas makes it hotter.
  - Motoring at 20–25 kPa MAP: after compression and expansion with no heat release, p_cyl,EVO ends up at roughly MAP level (~20–30 kPa abs, minus heat-transfer losses). That is **below** exhaust back-pressure (~100–105 kPa). So at EVO, exhaust gas flows *back into* the cylinder: a negative (rarefaction) pulse. The exhaust-stroke displacement then pushes out a low-density charge.
  - Order of magnitude: a firing blowdown Δp of +200…+400 kPa against a motoring Δp of about −70…−85 kPa puts the motoring fundamental about **10–15 dB** lower than full-load firing, with fewer high harmonics. The sharp blowdown front is gone and the waveform is dominated by the smoother piston-displacement pulse.
  - **Synth rule:** on fuel cut, drop the firing-pulse gain by 10–20 dB relative to its WOT level, flip or soften the pulse sign/shape, and low-pass the pulse (lengthen its rise time). Do not simply scale the firing pulse down.
- **Pumping loss.** The pumping mean effective pressure is roughly (p_exh − p_int). With p_int ≈ 20–25 kPa and p_exh ≈ 105 kPa, PMEP ≈ 0.8 bar, added to friction (FMEP of about 1–2.5 bar, rising with rpm; textbook range). This is the engine-braking torque used in section 3.

### Gaps
- I found no published measured comparison of fired vs motored exhaust pulse amplitude or tailpipe SPL for the same engine and rpm. The 10–15 dB figure is a first-principles estimate.
- I found no source giving measured MAP-vs-time traces at throttle closure (time constant in ms). The τ above is derived from the filling/emptying model plus one patent statement.

---

## 2. Deceleration fuel cut-off (DFCO): conditions, delays, torque/spark ramps, exit, and what motoring sounds like

### Takeaway
DFCO is a state machine gated by rpm, TPS, MAP, coolant temperature, vehicle speed and a delay. Before the cut, OEM strategies first ramp spark out (torque reduction), and they ramp it back in on exit. The exact numbers are calibration-specific, and open-source ECUs expose them as user tables with community starting values.

### Cited Findings
- **Speeduino DFCO logic:** injection stops when RPM > cutoff RPM, TPS < TPS threshold, and coolant > minimum temperature, and all of these have held for "Cutoff delay" seconds. An RPM hysteresis prevents on/off chattering. — [Speeduino forum/manual](https://speeduino.com/forum/viewtopic.php?t=3595) (search summary); the config struct is at [Speeduino config4 doxygen](https://speeduino.github.io/speeduino-doxygen/structconfig4.html)
- Speeduino TPS threshold typical values: "5 %–10 %". — [Speeduino Manual (pdfcoffee mirror)](https://pdfcoffee.com/speeduino-manual-3-pdf-free.html) (search summary)
- Community starting values: TPS ≤ ~3 % ("to avoid adding sudden braking sensation"), MAP ~5 kPa below lowest idle MAP, **RPM low limit ≈ 2× idle rpm**. A very low RPM threshold with narrow hysteresis causes on/off looping. — [Speeduino forum t=3595](https://speeduino.com/forum/viewtopic.php?t=3595), [Speeduino forum t=3724, DFCO map based](https://speeduino.com/forum/viewtopic.php?t=3724) (search summary)
- Speeduino disables DFCO during warm-up enrichment and for at least **30 s after engine start**. — [speeduino issue #213, DFCO after start delay](https://github.com/noisymime/speeduino/issues/213) (search summary)
- **rusEFI** DFCO ("coasting fuel cut") is gated by CLT, RPM, MAP and TPS thresholds. Its config comment says DFCO is also used "to prevent back firing". Vehicle-speed gates `coastingFuelCutVssLow/High` exist "to prevent jerkiness from fuel enable/disable in low gears". `noFuelTrimAfterDfcoTime` (0–10 s, 0.1 s resolution) pauses closed-loop fuelling after DFCO. — [rusEFI rusefi_config.txt](https://github.com/rusefi/rusefi/blob/master/firmware/integration/rusefi_config.txt)
- rusEFI tuning notes: a DFCO delay of 0 "cuts immediately with no delay, which may cause rumbles and pops out of your exhaust". Timing can be retarded by a set amount during DFCO "to smooth the transition back from fuel cut", and after fuel is restored "the timing ramps back in over a specified period". — [rusEFI Configuration Guide](https://rusefi.com/docs/guide/) / [rusEFI wiki](https://github.com/rusefi/rusefi/wiki/Fuel-Overview) (search summary; exact page not confirmed)
- **GM OEM strategy (HP Tuners parameter docs):**
  - Entry requires ECT, vehicle speed, rpm (table vs gear), TPS (vs rpm) and MAP (vs baro) conditions.
  - A "Spark Threshold": "the degree spark advance that must be reached to turn the fuel off". "Spark will ramp down to this value before DFCO enables."
  - Spark is removed and re-added in steps "at a rate of 12.5 msec at the Entry/Exit Blending" multiplier.
  - Separate "Active Spark Advance" tables apply while fuel is off.
  - There are separate exit rpm, TPS and MAP tables (hysteresis), and a "Stall RPM Limit" as the lower rpm limit for DFCO.
  - [HP Tuners GM Fuel Cutoff, DFCO](https://www.hptuners.eu/help/vcm_editor_parameters_gm_eng_fuel_cutoff.htm)
- **Ford OEM strategy:**
  - Enable/disable rpm and enable/disable vehicle speed (hysteresis pairs).
  - A max-load gate.
  - A "Closed Throttle Delay" (the time conditions must hold before injectors are disabled), plus a separate "Extended" delay used when re-entering DFCO.
  - "DFCO Torque Ramp On Rate / Off Rate".
  - [HP Tuners Ford Fuel Cutoff, DFCO](https://www.hptuners.eu/help/vcm_editor_parameters_ford_eng_fuel_cutoff.htm)
- **Return-to-idle helpers (rusEFI):**
  - `idlePidRpmUpperLimit`: "if target = 800, this param = 200, then anything below 1000 RPM is considered idle", i.e. the coasting-detection threshold.
  - `iacByTpsTaper`: an extra idle-air "added when coasting and transitioning into idle".
  - `idleReturnTargetRamp`: on return from coasting, "start the closed-loop RPM target elevated by the 'RPM upper limit' … then ramp it down … Helps prevent RPM from dipping below the idle target".
  - `useIdleAdvanceWhileCoasting` and a separate idle timing table use "extra advance at low idle speeds [to] prevent stalling and extra retard at high idle speeds".
  - [rusEFI rusefi_config.txt](https://github.com/rusefi/rusefi/blob/master/firmware/integration/rusefi_config.txt)

### Inferences
- **Re-enable rpm.** The "≈2× idle" community rule gives ~1400–1700 rpm for a 700–850 rpm idle. This agrees with the commonly quoted ~1200–1500 rpm OEM re-enable, but I found no OEM number to confirm it (see Gaps).
- **Synth state machine**, as consistent with the sources:
  1. **Lift (t = 0):** MAP falls with τ ≈ 3 revs, and combustion continues at reduced load.
  2. **Torque-reduction phase:** spark is retarded step-wise (GM: 12.5 ms steps) toward the "entry spark". Combustion becomes late, so pulses get weaker, hotter and later. This is the window where occasional pops can occur (rusEFI: "0 delay → rumbles and pops").
  3. **Cut:** after the delay (a separate calibration value; Speeduino expresses it in seconds), all cylinders stop firing and the motoring sound of section 1 takes over. Model a short cross-fade of a few cycles, not an instantaneous switch; some OEMs ramp torque (Ford "Torque Ramp On/Off Rate").
  4. **Exit:** when rpm falls below the exit threshold (hysteresis below entry), or on throttle tip-in, fuel returns, spark ramps back in, and the idle target is elevated then ramped down (rusEFI `idleReturnTargetRamp`).
  - Audibly, step 4 is a soft "re-light": firing pulses return at low load with retarded timing, and a slight rpm "catch" is heard.
- **Low-gear VSS gating** (rusEFI VssLow) means that in low gears or at low speed, many cars **never cut fuel**. They stay at lean/retarded firing through the whole decel, which sounds different from high-gear coasting.

### Gaps
- No OEM-published default numbers were found for the entry delay, rpm entry/exit, ECT minimum or ramp rates. HP Tuners pages list the parameter names only. The specific "~1200–1500 rpm re-enable" and "~1–3 s entry delay" figures are not confirmed by a fetched source.
- The rusEFI wiki DFCO page URL I tried returned 404, so default values in the rusEFI TunerStudio project were not verified.

---

## 3. Rev-drop dynamics: free-rev vs in-gear deceleration, undershoot and catch at idle

### Takeaway
Free-revving (neutral) rpm falls fast, roughly 1000–3000 rpm/s at high rpm (a first-principles estimate), set by friction + pumping torque divided by crank/flywheel inertia. In gear, rpm falls one to two orders of magnitude more slowly, set by vehicle decel × overall ratio. Return to idle is shaped by the dashpot/idle-air strategy: a 3–4 s decay from high rpm is a cited calibration goal, and poor calibrations undershoot by 200–300 rpm before recovering.

### Cited Findings
- Engine deceleration obeys T = I·α. Standard inertia tests run an unloaded engine to e.g. 2500 rpm, cut it, and time the fall to 1000 rpm to obtain the average deceleration rate. — [US 5620392, Engine accessory torque and deceleration rate …](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5620392) (search summary)
- The ISC "dashpot" function "causes the RPMs to drop slowly to a stabilized idle" to avoid driveline bucking, stabilise the idle return and reduce emissions. "Typically, decay from high rpm should last **3–4 seconds** before reaching idle rpm." The difficulty is balancing too much dashpot (hanging idle) against too little (stall). — [EFIDynoTuning, Dashpot](http://www.efidynotuning.com/dashpot.htm) (search summary; the site's TLS certificate had expired, so the page was not fetched directly); [HP Tuners forum, Dashpot Tuning Guide](https://forum.hptuners.com/showthread.php?72079-Dashpot-Tuning-Guide=&p=561867#post561867)
- Reported undershoot on Ford EEC: rpm "drop well into the low 500s then revive back to stabilize near the target idle". On tip-out, rpm can drop to ~1100, "hang briefly, and then stall". — [EECTuning, Dashpot issue: rpm dip prior to hanging idle](https://eectuning.org/forums/viewtopic.php?t=21156); [EECTuning, RPM dip/stall after neutral throttle snap](https://eectuning.org/forums/viewtopic.php?t=23104) (search summaries)
- A Chrysler/FCA MultiAir patent describes a "stepped idle return" strategy, i.e. the return to idle is deliberately staged rather than a free fall. — [US 9121359, Stepped idle return](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9121359) (search result title only)

### Inferences
- **Free-rev decel estimate.** Take a 2.0 L engine on overrun with FMEP + PMEP ≈ 2–3 bar at 5000–6000 rpm. Braking torque = MEP·Vd/(4π) ≈ 2.5e5 × 2e-3 / 12.57 ≈ 40 N·m. With crank + flywheel + clutch inertia of 0.12–0.25 kg·m², α ≈ 160–330 rad/s², which is **≈1500–3200 rpm/s**. At 1500 rpm, FMEP + PMEP drops (to perhaps ~1.2–1.5 bar), so decel slows to ~700–1500 rpm/s. Light-flywheel performance engines sit at the high end. The drop is therefore **not linear**: it is fast at the top and slower near idle, and the dashpot then adds a deliberate slow tail.
- **In-gear estimate.** rpm-dot = (a_vehicle / r_wheel)·i_total·60/(2π). With r = 0.31 m and engine-braking vehicle decel of 0.5–1.5 m/s²:
  - 1st gear (i ≈ 13–15): ≈ 200–650 rpm/s.
  - 3rd gear (i ≈ 5.5): ≈ 85–250 rpm/s.
  - 5th/6th gear (i ≈ 2.5–3.2): ≈ 40–150 rpm/s.
  - Scale by gear, not by engine.
- **Synth idle-catch model.** Approach idle target N_i (plus elevated ramp target N_i + ΔN, where ΔN ≈ 150–250 rpm per the rusEFI example) with a second-order response:
  - Well-calibrated engine: undershoot ~20–80 rpm, recovery ~0.5–1.5 s.
  - Sloppy or modded engine: undershoot up to 200–300 rpm (down to "low 500s" from a ~750–800 target), recovery ~1–2 s.
  - These damping values are my suggestion for a plausible default, not measured data.
- **Audible consequence.** Because the idle controller also moves spark (the rusEFI idle timing table), firing pulses near idle show amplitude and timing wobble during the catch. A small correlated amplitude/phase modulation at ~1–3 Hz during the catch adds realism.

### Gaps
- No published free-rev deceleration curves (rpm vs time) for specific production engines were found. The rpm/s figures are estimates.
- No measured undershoot statistics for OEM calibrations were found; only forum anecdotes.

---

## 4. Overrun noises: afterfire/pops/crackle, pop & bang maps; turbo BOV/surge flutter; supercharger whine

### Takeaway
Overrun pops need fuel *and* ignition energy reaching the hot exhaust. That happens either by accident (delayed DFCO, rich tip-out, misfire into a hot header) or deliberately: "pop & bang" maps keep injecting on overrun with ignition retarded by 10–15° (up to ~30° for loud maps), so combustion completes in the exhaust. Turbo surge "flutter" is a compression-system instability at a few tens of Hz (Helmholtz-type, mild surge typically 30–85 Hz, deep surge lower). Blow-off valves replace it with a broadband "whoosh".

### Cited Findings
- **Pop & bang mechanism:** the map modifies the overrun fuel-cut and ignition tables so unburnt fuel ignites in the exhaust. "The heavily retarded ignition means much of the combustion happens within the exhaust system, with fuel ignited by the hot exhaust or catalytic converter." — [Fast Car, Pop and Bang Maps Guide](https://www.fastcar.co.uk/tuning-tech-guides/pop-and-bang-maps/) (search summary)
- Typical values quoted by tuners:
  - Retard of 10–15° on overrun, with ~30° of retard and rich fuelling for "very loud" pops; some use ~10° BTDC absolute during overrun.
  - Some add enrichment of ~11.5–12.5:1 AFR. Others run lean "so too much heat isn't created".
  - [Fast Car](https://www.fastcar.co.uk/tuning-tech-guides/pop-and-bang-maps/); [PistonHeads thread, How do manufacturers map in exhaust bangs and crackles?](https://www.pistonheads.com/gassing/topic.asp?h=0&f=66&t=1521874); [ChipMotorSports, Pop & Bang Map Explained 2026](https://chipmotorsports.com/en/blog/pop-and-bang-map-explained-2026) (search summaries; these are tuner/enthusiast sources, not primary engineering)
  - Values vary by tuner and conflict (rich vs lean).
- Risks: "really high EGTs during the 'crackle' period". Exhaust valves, turbine, flex joints, catalysts and GPF/DPF see temperature spikes. Crackle maps are expressed as "intensity maps [that] define how rich, how retarded and for how long, often per drive mode", with limited duration per event and disabled when cold. — [TuningBot, Pops and Bangs](https://tuningbot.com/kb-articles/pops-and-bangs-overrun/) (fetched; no numeric values given)
- DFCO prevents backfire, and an immediate (zero-delay) cut can cause "rumbles and pops". — [rusEFI config](https://github.com/rusefi/rusefi/blob/master/firmware/integration/rusefi_config.txt); [rusEFI guide](https://rusefi.com/docs/guide/) (search summary)
- **Compressor surge on lift-off:** with the throttle closed, high boost and low flow, blades "lose their grip". Flow separates and reverses, the pressure drops, the blades re-grip, and the cycle repeats while the turbo spins down. This is heard as "a fluttering or repeated 'choofing' sound, typically when closing the throttle". A diverter valve opens to relieve pressure and provide a flow path, which prevents surge. — [GFB, The truth about compressor surge](https://gfb.com.au/tech/tech-articles/11-the-truth-about-compressor-surge/) (fetched; no numeric frequency given)
- Blow-off/dump valves produce a "sharp 'whoosh' or rhythmic 'flutter'". Valve opening "can generate broadband acoustic pressure waves in a backflow direction". — [Wikipedia, Blowoff valve](https://en.wikipedia.org/wiki/Blowoff_valve) (search summary)
- **Surge frequencies (academic):**
  - Mild surge frequency is set by the upstream/downstream duct geometry and lies "well below 100 Hz". It occurs at the theoretical Helmholtz-resonator frequency.
  - As flow drops further, the system enters deep surge at a frequency **below** the Helmholtz value; deep-surge frequencies were measured at 63 % and 50 % of the mild-surge frequency.
  - Surge inception was identified acoustically in the **30–85 Hz** band.
  - Sources: [Simulation of Surge in Turbocharger Compression Systems](https://www.researchgate.net/publication/278403864_Simulation_of_Surge_in_Turbocharger_Compression_Systems); [Simulation of Deep Surge in a Turbocharger Compression System](https://www.researchgate.net/publication/267503667_Simulation_of_Deep_Surge_in_a_Turbocharger_Compression_System); [Experimental Investigations of Centrifugal Compressor Surge Noise, J. Turbomach. 145(8) 081014](https://asmedigitalcollection.asme.org/turbomachinery/article-abstract/145/8/081014/1160437/Experimental-Investigations-of-Centrifugal); [Acoustic and pressure characteristics of a ported shroud turbocompressor near surge, Applied Acoustics](https://www.sciencedirect.com/science/article/abs/pii/S0003682X18305978) (abstract-level search summaries)
  - The exact attribution of each number to each paper was made by the search engine and was not verified in full text.

### Inferences
- **Pop timing and rate.** Pops come from individual cylinders' late-burning charges igniting in the header or collector, so they are **quasi-synchronous with firing events but sparse and random**. Not every event pops. In the pop window, a probability per exhaust event of ~5–30 % gives the typical irregular "crackle". Mostly-every-event pops sound like a "burble" (retarded but combusting).
  - Pops mostly occur **in the first ~1–3 s after lift-off** and at mid-high rpm (roughly 2500–5000 rpm), where the exhaust is hottest and fuel is still being delivered. Pop & bang maps typically define this rpm/time window.
  - The duration is my synthesis of the qualitative sources ("limits its duration per event", "crackle period"), not a measured value.
- **Pop spectrum.** Each pop is an impulsive combustion event in a pipe, a broadband transient. Model it as a short (~2–10 ms) decaying noise burst with a sharp attack, injected at the exhaust-waveguide input (header end), plus a low-frequency "thump" from the pipe's resonances. Big "bangs" (rich + ~30° retard) carry more low-frequency energy and a longer tail.
- **Surge frequency check.** Treat the intercooler piping as a Helmholtz resonator: f = (c/2π)·√(A/(V·L)). With V ≈ 5 L, pipe length L ≈ 0.5 m and diameter 50 mm (A ≈ 1.96e-3 m²), f ≈ 54.6 × 0.886 ≈ **48 Hz**, consistent with the 30–85 Hz literature band. Because the turbo spins down during the event, the flutter should slow and fade over ~0.3–1.5 s (estimate). Model it as amplitude-modulated broadband compressor noise with a modulation rate of ~20–60 Hz decaying toward ~10–20 Hz, not as a pitched tone.
- **BOV "pssh".** A broadband burst (valve opening, choked jet into atmosphere or recirculation), with an envelope of fast attack (<20 ms) and exponential decay tracking the boost-pressure decay (~0.2–0.8 s). Recirculating (diverter) valves are much quieter than atmospheric ones.
- **Supercharger whine.** No fetched source; see Gaps. On lift-off, a Roots or screw blower's whine tracks rotor order and rpm and drops in level with pressure ratio. Many installs have a bypass valve that opens on vacuum.

### Gaps
- No primary measurement of pop rate (pops/s), pop SPL or pop spectrum was found.
- No quantitative BOV or surge-flutter SPL data from a production car was found. Supercharger whine on overrun (rotor-lobe order, level change with bypass opening) was not covered by any fetched source.

---

## 5. Intake side on closed throttle: throttle hiss/whistle, induction noise reduction

### Takeaway
With the throttle closed, the pressure ratio across the plate is far below critical (~0.53), so flow through the gap and idle-air passages is choked. The resulting jet produces high-frequency hiss or whistle, but the mass flow is small. Low-frequency induction (intake pulsation) noise collapses on overrun because the pulsation amplitude scales with MAP. The loud "whoosh" OEMs fight is mostly a *tip-in* phenomenon.

### Cited Findings
- High-frequency flow noise arises when the throttle plate opens from fully closed to partially open. "The convergence of turbulent air streams through the openings … on either side of the throttle plate" makes a "whoosh", at tip-in or at steady part throttle. — [US 6824119, Throttle plate having reduced air rush noise](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6824119) (search summary; the PDF is a scanned image and its text could not be extracted); also [US 5970963, Apparatus for preventing flow noise in throttle valve](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5970963)
- Owner reports describe throttle whistling at idle or with the throttle "just cracked open", and a "hissing noise (almost a whistle)" above ~15 % throttle under load. — [Holley EFI forum](https://forums.holley.com/forum/holley-efi/hp-efi/2684-hissing-whistling-noise-from-throttle-body); [GR86 forum, Throttle closing sound](https://www.gr86.org/threads/induction-noise%E2%80%A6-throttle-closing-sound.6880/) (anecdotal)

### Inferences
- **Choking.** For air, critical pressure ratio ≈ 0.528. With overrun MAP ≈ 20–25 kPa and a pre-throttle pressure ≈ 100 kPa, the ratio is ≈ 0.2–0.25, so the gap flow is sonic (~310–340 m/s local). The jet noise is broadband and high-frequency. Because the gap is sub-millimetre, the Strouhal peak (f ≈ 0.2·U/d) sits largely ultrasonic, so the *audible* part is the low-frequency tail plus any whistle tones from edge/cavity interaction (reported by owners, typically perceived as a few kHz; not measured by any fetched source).
- **Synth rule.** Add a quiet high-passed (~2–8 kHz) noise "hiss" whose gain follows √(Δp) or the idle-air mass flow, present during overrun and idle. Its level should be well below firing induction noise. Fade low-frequency intake pulsation noise with MAP (∝ MAP/MAP_WOT).
- For a turbo car, the hiss component is replaced or dominated by BOV and compressor noise (section 4).

### Gaps
- No measured frequency (Hz) or SPL for closed-throttle whistle was found. The frequency band above is an estimate.

---

## 6. Exhaust flow reversal, low-flow resonances, exhaust cooling and sound-speed shift

### Takeaway
On fuel cut, exhaust "gas" becomes cool air and exhaust temperature drops abruptly. Speed of sound (∝ √T) falls, so every pipe resonance in the waveguide shifts **down** in frequency, by about 15–25 % as gas cools from ~800–900 K toward ~400–550 K. At low flow, exhaust-valve-opening flow reversal occurs (section 1).

### Cited Findings
- During fuel cut, "the combustion heat of the fuel disappears, causing the exhaust temperature to drop abruptly". With fuel cut, "exhaust gas equals air", producing a cooling action. Air flowing into the catalyst makes its temperature drop "relatively quickly". — Patent snippets surfaced by search, e.g. [US 10563604, Control apparatus for internal combustion engine](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10563604), [US 6073440, catalyst deterioration detection](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6073440) (search summary; which quote belongs to which patent was not verified; no cooling rate numbers given)
- Exhaust noise is driven by pressure pulsations whose spectral amplitudes are "strongly influenced by standing pressure waves caused by reflections". Exhaust noise level peaks where pulsation frequency equals a natural frequency of the system. — [OSTI/ETDEWEB, A study on the growth of new frequency due to high pressure pulsation in exhaust pipe of engine](https://www.osti.gov/etdeweb/biblio/671813) (search summary)

### Inferences
- **Sound speed.** c = √(γRT).
  - Hot exhaust at ~900 K (γ ≈ 1.33–1.35): c ≈ 585 m/s.
  - Air at 500 K (γ ≈ 1.39): c ≈ 450 m/s.
  - Air at 400 K: c ≈ 400 m/s.
  - Frequency ratio 450/585 ≈ 0.77 (−23 %, about −4.5 semitones); for 400 K, 0.68.
  - Temperatures also fall along the pipe even at steady state: header ~700–900 °C at load; tailpipe gas typically a few hundred °C (textbook ranges, not from a fetched source).
- **How fast.** Two time scales:
  - (a) Gas replacement: the exhaust system volume (order 10–30 L) is flushed by the engine's overrun air flow in about 1 s. Overrun volume flow at 3000 rpm for 2 L at ~0.2 ηv-equivalent is ≈ 10 L/s, so gas replacement takes ~1–3 s.
  - (b) Once flushed, the gas temperature is pinned by **wall temperature**. Header and cat walls are heavy and cool over tens of seconds to minutes.
  - Net effect: fast partial drop (~1–3 s) to a gas temperature set by wall heat transfer, then slow drift.
  - **Synth rule:** on DFCO, drive the waveguide's c (or delay length) with a two-pole model: ~50–70 % of the eventual shift within 1–3 s, the rest over 20–60 s. Reverse quickly (≤1 s) when fuel returns. At a realistic, subtle level, the audible effect is a slow downward drift of the pipe "formants" during long coasts. The downward drift of the firing fundamental with rpm dominates the first seconds anyway.
- **Flow reversal.** With the EVO pressure below back-pressure (section 1), each EVO event momentarily draws exhaust back toward the cylinder. Mean flow is low and the net DC flow noise (turbulent "flow noise" in a synth) should drop sharply with mass flow. Scale it ~∝ ṁ^2–ṁ^3 as a rule of thumb for turbulent/jet noise (general aeroacoustic scaling, estimate).

### Gaps
- No measured EGT-vs-time trace at DFCO entry was found in accessible sources. No measured tailpipe resonance frequency shift during coast-down was found.

---

## 7. Driveline and transmission contributions on overrun

### Takeaway
No source fetched in this session quantified driveline noise on overrun. Only well-known mechanisms can be listed, as inferences.

### Cited Findings
- Engine braking/overrun is defined as the vehicle decelerating solely due to release of the accelerator. — [Wikipedia, Engine braking](https://en.wikipedia.org/wiki/Engine_braking) (search summary)
- The ISC dashpot is calibrated partly "to prevent bucking" of the driveline on tip-out. rusEFI's VSS-gated DFCO exists "to prevent jerkiness from fuel enable/disable in low gears". These are driveline-shuffle phenomena around the fuel-cut and re-enable transitions. — [EFIDynoTuning Dashpot](http://www.efidynotuning.com/dashpot.htm) (search summary); [rusEFI config](https://github.com/rusefi/rusefi/blob/master/firmware/integration/rusefi_config.txt)

### Inferences (not source-backed; general NVH knowledge)
- **Torque reversal (tip-out).** Drive-to-coast torque reversal takes up backlash, causing "clunk". Driveline shuffle is a low-frequency (~2–8 Hz) fore-aft oscillation that modulates rpm, audible as a few-Hz modulation of engine pulses. The DFCO torque ramps exist to suppress it.
- **Gear whine on the coast flank.** Tooth-mesh order = teeth × shaft order. Coast-side whine can be louder than drive-side in some gearboxes and final drives because the coast flank is less optimised.
- **Relative level.** Driveline whine usually sits well below engine noise on power, but on overrun, when engine pulses drop by 10+ dB (section 1), it can become audible inside the cabin. Treat it as a minor optional layer.

### Gaps
- No measured SPL, order or level ratio for coast-side gear whine or tip-out clunk was found in this session.

---

## 8. Diesel differences (brief)

### Takeaway
Only first-principles notes are possible here; no diesel-specific source was fetched.

### Cited Findings
- None fetched.

### Inferences (not source-backed)
- Most diesels are unthrottled or only lightly throttled. On overrun the injected quantity simply goes to zero (fuel cut is inherent), and intake pressure stays near atmospheric (or boost decaying to atmospheric).
- Motoring cylinder pressures therefore stay high (compression ratio 15–18:1 gives tens of bar at TDC), and the EVO pressure is near or somewhat above atmospheric.
- So the diesel overrun exhaust pulse **does not collapse or reverse the way a throttled SI engine's does**. The characteristic diesel combustion "knock" (high-frequency, from rapid premixed burn) disappears entirely, leaving a smoother, low-harmonic pulsation.
- Engine braking is weaker (no pumping loss) unless an exhaust brake or throttle flap is used.
- No afterfire pops (no premixed fuel on overrun). Turbo surge and BOV behaviour differ: diesels usually lack a BOV because there is no throttle, except models with an intake throttle.

### Gaps
- No quantitative diesel overrun acoustics source was found.

---

## 9. Published run-down/coast-down sound measurements (order tracking)

### Takeaway
Run-up and run-down with order tracking is the standard NVH method for separating rpm-locked orders from fixed resonances. No openly accessible spectrogram of a production car's overrun or coast-down tailpipe sound with numeric order levels was retrieved.

### Cited Findings
- Engine run-up/coast-down tests split measured signals (sound, vibration) into orders against a tacho reference across the whole rpm range. Coast-down is the standard way to sweep through structural and acoustic resonances. — [Dewesoft, Order Tracking training](https://training.dewesoft.com/online/course/order-tracking); [Dewesoft, What is Order Analysis](https://dewesoft.com/blog/what-is-order-analysis)
- Run-up/run-down covers ranges "from a few RPM to 10,000 RPM" on automotive engines, and faithful results during fast speed changes need specific processing: synchronous order analysis / constant-band tracking. — [OROS, Order tracking analysis](https://oros.com/solutions/rotating-analysis/order-tracking-analysis)
- Order tracking separates rpm-related frequencies from "spurious" fixed-frequency ones. — [Crystal Instruments, Order Tracking Analysis](https://www.crystalinstruments.com/order-tracking-analysis)

### Inferences
- **What a coast-down spectrogram of a throttled SI engine should show,** based on sections 1–6:
  - Firing-order lines (e.g. order 2 for an I4, order 3 for a V6/I6, order 4 for a V8) falling with rpm, with a level step down of roughly 10–15 dB at the DFCO instant.
  - Fewer high orders after the cut (loss of the sharp blowdown).
  - Fixed-frequency bands (pipe and muffler resonances) that light up as the order lines sweep through them, and that themselves drift slowly downward as the exhaust cools.
  - Broadband noise, for pops, surge and BOV, only in the first seconds.
  - A level step *up* when fuel is re-enabled near 1200–1600 rpm before idle.
- **Synth validation.** The team can record its own car on a coast-down with a phone microphone plus an OBD rpm log. A simple order-tracking resample, e.g. at fixed samples per revolution, would validate all the above cheaply. This is the most reliable route given the lack of open published data.

### Gaps
- No open-access paper with a tailpipe or cabin coast-down spectrogram showing fired-vs-DFCO order levels was retrieved. SAE papers likely exist but are paywalled and were not located in this session.
