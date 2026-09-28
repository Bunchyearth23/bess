# Idle physics: what makes a real IC engine sound "alive" at idle (numbers for a procedural synth)

Scope note: about 30 search/fetch calls. Several primary sources (SAE papers, Heywood, the Ozdor/Dulger/Sher review, the Finney 2015 review, ScienceDirect topic pages) were paywalled or blocked (HTTP 403), so some numbers come from abstracts, patents or open PDFs. Every number below has its source next to it. Anything not verified in this session is in the Gaps sections or marked as an inference.

---

## 1. Cycle-to-cycle combustion variability at idle (COV IMEP, peak pressure, CA50, distribution, correlation)

### Takeaway
Measured on production 4-cylinder cars at hot idle, the COV of IMEP per cylinder is about **6–12%**. That is far above the 1–3% seen at part/full load, and even healthy cars reach that range. Controlled research-engine idle points can be below 2%. Successive cycles are mostly random at stoichiometric idle. Deterministic cycle-to-cycle coupling through residual gas only becomes important with dilute/lean or high-EGR mixtures.

### Cited Findings
- **Production 4-cyl, 4-stroke SI engines at hot idle (700 rpm), with a dual-clutch transmission; 300 cycles per cylinder measured with spark-plug pressure sensors and a Kistler KiBox:**
  - Vehicle A (the one with the vibration complaint): COV IMEP per cylinder = **6.4 / 7.5 / 7.0 / 8.2 %**. Inter-cylinder imbalance δ = **4.5 %**.
  - Competitor C1 (no complaint): COV = **10.5 / 11.1 / 12.0 / 11.8 %**, δ = **7 %**.
  - Competitor C2 (no complaint): COV = **8.4 / 8.4 / 7.1 / 8.2 %**, δ = **5 %**.
  - δ is defined as (IMEPmax − IMEPmin)/IMEPmean across the four cylinders. The authors treat all three vehicles as having acceptable combustion stability. — [Shen et al., "An Experimental Study of Vehicle 1st Order Vibration Improvement at Engine Idle", Int. J. Acoustics & Vibration 25(4), 2020, Table 1](https://iiav.org/ijav/content/volumes/25_2020_36521584896523/vol_4/1708_fullpaper_568371609345225.pdf)
- Han (2001, KSME Int. J. 15(1)) studied COV IMEP at idle in an SI engine under several spark-timing, A/F and other conditions. It reports "COVs for IMEP under each experimental condition are all below 2.0%, which means combustion is very stable." (This comes from the abstract; the engine type was not visible.) — [Han, "Investigation of cyclic variations of IMEP under idling operation in SI engines"](https://link.springer.com/article/10.1007/BF03184801)
- **Contradiction to note:** the <2% figure (Han, lab idle) and the 6–12% figures (Shen, production cars at 700 rpm hot idle) differ by up to 5×. The likely reasons are the definition used (gross vs net IMEP; net IMEP at idle is very small because pumping work is subtracted, which inflates COV), the idle speed and the calibration. The Shen paper does not say whether its IMEP is gross or net. — sources as above.
- Driveability problems usually appear when COV IMEP exceeds **about 10%**. (Heywood's criterion, quoted in the Han abstract and search listing.) — [Han 2001 abstract/ResearchGate listing](https://www.researchgate.net/publication/225996888_Investigation_of_cyclic_variations_of_IMEP_under_idling_operation_in_spark_ignition_engines)
- Dependence on speed and load, for a 2.2 L SI engine (8.85:1 CR, 85 kW at 5000 rpm), 300 cycles per point over 1000–3000 rpm and 0–~110 Nm: COV IMEP is "almost constant at loads higher than 30 Nm irrespective of engine speed". At lower loads it "progressively increases, especially at engine speeds below 1500 rpm". This is attributed to low flame speed and to **valve overlap / residual gas at low speed**. COV of Pmax follows the same trend. The authors conclude that cyclic variability is "practically experienced at speeds below 1500 rpm and loads below ~30 Nm", i.e. near idle. (The numeric values are only in the figures, which could not be extracted.) — [Cyclic Variability Correlations to Operating Conditions for SI Engines, Communications – Scientific Letters of Univ. Žilina, 2023](https://komunikacie.uniza.sk/pdfs/csl/2023/02/11.pdf)
- According to Sztenderowicz & Heywood (as summarised in the search listing), IMEP fluctuations are controlled mainly by cycle-to-cycle variation in the **amount of fuel burned**. Contributing sources are fuel quantity variation, variation in the intake flow field, fresh-charge variation driven by residual gas temperature, and valve leakage. — [search listing summarising literature; ScienceDirect ATE 2004 correlations paper](https://www.sciencedirect.com/science/article/abs/pii/S1359431104000614)
- **Deterministic coupling between cycles:** Daw/Finney et al. describe cyclic variability as small-scale stochastic fluctuations interacting with a nonlinear deterministic coupling between successive cycles. The residual gas from cycle *n* changes the charge composition, and so the combustion efficiency, of cycle *n+1*. — [Daw et al., "A simple model for cyclic variations in a SI engine", SAE 962086 (ORNL record)](https://impact.ornl.gov/en/publications/a-simple-model-for-cyclic-variations-in-a-spark-ignition-engine/); [Finney et al., "A review of deterministic effects in cyclic variability of IC engines", IJER 2015](https://journals.sagepub.com/doi/10.1177/1468087415572033)
- This deterministic structure (visible in return maps and bifurcations) is strongest in **dilute/lean** operation, with an "intermittent route to chaos at highly lean and diluted charge". At normal stoichiometric operation the variations look mostly random. — [Maldonado & Kaul, IJER 2022](https://journals.sagepub.com/doi/10.1177/1468087420983087); [Finney 2015](https://journals.sagepub.com/doi/10.1177/1468087415572033)

### Inferences
- For the synth: draw a per-event "burned-fuel fraction" (or IMEP) with **σ ≈ 5–10% of mean** at idle, and about 1–3% at cruise/WOT. The 1–3% is an inference from the "flat above 30 Nm" statement; the actual value was not extracted.
- Since fuel-burned variation drives the IMEP variation, amplitude jitter and timing jitter should be **correlated**. A slow, weak burn gives both a lower pressure peak and a later peak. Later blowdown (§8) means a weaker, later exhaust pulse.
- A low-tail skew is the right shape. Partial burns pull the distribution down; nothing pushes it above the "complete burn" ceiling. A truncated/skewed distribution (for example 1 − |N(0,σ)| or a beta distribution) is physically better justified than a symmetric Gaussian. The skew is an inference from the physics; I found no verified skewness value.
- Add optional AR(1) coupling (ρ ≈ 0 to −0.3, with a weak burn followed by a slightly stronger one through residual gas) only for "lean/high-EGR/cam-overlap" presets. At stoichiometric idle, independent draws are defensible.

### Gaps
- No verified numeric **std of peak pressure** or **CA50/burn-duration spread (CAD)** at idle was found. Standard textbook values (Heywood's Pmax scatter of roughly ±10% or more, CA50 std of a few CAD) could not be checked because Heywood and the Ozdor/Dulger/Sher SAE 940987 review are paywalled.
- No verified skewness/kurtosis figures, and no lag-1 autocorrelation value at stoichiometric idle.
- The ESE 2024 review on CCV mechanisms (Duan et al.) was blocked (403).

---

## 2. Partial burns and misfires at idle

### Takeaway
At idle, healthy production engines do not misfire. Occasional partial or slow burns are what make up the low tail of the IMEP distribution. When a real misfire happens, the crank decelerates clearly (easy to detect on 4-cylinder engines, harder on 6/8-cylinder engines where power strokes overlap). The exhaust pulse from that cylinder changes shape because unburned charge is blown down at much lower pressure.

### Cited Findings
- With more than four cylinders, power strokes overlap, so "associating crankshaft acceleration fluctuations with any particular cylinder becomes more difficult". Torsional vibration and reciprocating inertia torque add noise to the acceleration signal. On a 4-cylinder the firing interval equals the power stroke. — [US patent 6112149, misfire detection with recursive median filtering](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6112149)
- Misfire detection studies use gasoline engines idling around **960 rpm** and report >99% detection precision, which shows that a single misfire is a distinct event in the crank-speed/vibration signal. — [MDPI Electronics 13(14):2688, 2024](https://www.mdpi.com/2079-9292/13/14/2688)
- Idle-speed feedback is usually **suspended or re-gained during misfire** so that the controller does not start hunting. So a misfire is followed by a short open-loop period, not an immediate correction. — [US patent 5333585, idle rotation control](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5333585)
- In the Shen 2020 idle data, even the best vehicle had per-cylinder COV of 6–8%. That spread is only consistent with occasional clearly weak cycles, not with a narrow Gaussian. — [Shen et al. 2020](https://iiav.org/ijav/content/volumes/25_2020_36521584896523/vol_4/1708_fullpaper_568371609345225.pdf)

### Inferences
- **Misfire model:** the cylinder's pulse has only the compression/expansion work (roughly zero net) and a very small blowdown. The exhaust pulse amplitude falls to perhaps 10–30% of normal (the cylinder pressure at EVO is close to motored pressure), and the crank decelerates, so the next firing arrives a little late (see §4).
- **Partial burn:** it releases heat late, so the cylinder pressure at EVO is *higher* than usual. The blowdown pulse can therefore be of similar or larger amplitude but come with slower combustion. This is why a "weak" cycle is not simply a quieter exhaust pop. This is an inference from blowdown physics and was not measured in these sources.
- Suggested healthy-idle rates for the synth: true misfire ≲ 1 per 10,000 events (effectively 0). Mild partial burns come from the low tail of the §1 distribution. For "rough/cammy/cold" presets, add an explicit partial-burn probability of 0.5–3%/event. These are tuning suggestions, not sourced values.

### Gaps
- No published misfire/partial-burn **rate** for healthy engines at idle was found. The OBD-II thresholds (percent misfire per 200/1000 revolutions) were not retrieved.
- No measured exhaust pulse after a partial burn was found.

---

## 3. Cylinder-to-cylinder imbalance

### Takeaway
Production engines at idle show about **4.5–7%** spread in mean IMEP between their best and worst cylinders. Injector flow tolerances of ±1.5–4% are one contributor. Intake runner and manifold distribution and valve lash/deposits are others.

### Cited Findings
- Inter-cylinder IMEP imbalance δ = (IMEPmax − IMEPmin)/IMEPmean = **4.5%, 5%, 7%** on three production 4-cylinder cars at hot idle (700 rpm). Per-cylinder COV also differs between cylinders (for example 6.4% vs 8.2% in the same engine). — [Shen et al. 2020, Table 1](https://iiav.org/ijav/content/volumes/25_2020_36521584896523/vol_4/1708_fullpaper_568371609345225.pdf)
- Gasoline injector specifications typically allow about **±3.2% static flow and ±3% dynamic flow**. The industry target is about **±1.5%**. Older production reached ±4%. These figures are from injector-calibration patent text surfaced by search; the exact patent could not be pinned down. — [US patent 6085142 (calibration method for fuel injection system) and related results](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6085142)
- OEMs monitor "air-fuel ratio imbalance" with an imbalance ratio IB = (Q_imbalanced − Q_balanced)/Q_balanced. As imbalance grows, **the amplitude of A/F fluctuation in the exhaust within one engine cycle grows**, i.e. a per-cycle periodic signature. — [US patent 9043121](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9043121); [US 8510017](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8510017)
- Half-order exhaust content (0.5, 1.5 … orders) is "caused by factors such as changes in exhaust timing or imbalance in intake or exhaust runner length". Such imbalance is what makes the tone. — [search listing, exhaust patents, e.g. US 10161287](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10161287)

### Inferences
- For the synth: give each cylinder a **fixed** amplitude offset drawn once per engine instance (σ ≈ 2–3%, so max − min ≈ 5–7% on a 4-cylinder). Also give each cylinder a fixed timing offset of a fraction of a CAD, plus its own COV scaler (0.85–1.15×). This makes the pattern repeat every 720° and produces the **0.5-order family**, which a perfectly even synth lacks.
- Low-frequency modulation at 1/2 engine order (one cylinder slightly weak) is the main difference between "machine" and "engine" in the idle spectrum.

### Gaps
- No quantitative data on runner-to-runner air distribution (% VE spread) for a production intake manifold.
- The injector-tolerance attribution to one specific patent is unverified; the figures are consistent across the search snippets.

---

## 4. Instantaneous crankshaft speed fluctuation (firing intervals are not constant in time)

### Takeaway
Crank speed rises after each firing TDC and falls during the next compression. The ripple is at firing frequency (4-cyl at 750 rpm → 25 Hz) and is strongest at idle, where inertial smoothing is weakest. The minimum speed ("valley") in each firing interval tracks that cylinder's strength. Because of this, **each event's timing depends on the previous events' torque**.

### Cited Findings
- For a 4-stroke engine with N cylinders, the speed variation repeats every **720/N crank degrees**. Speed rises in the expansion stroke after each compression TDC and then falls. — [US patent 4697561, torque from crankshaft speed fluctuations](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/4697561); [US 7133766](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7133766)
- "During idle and low-speed operation, the contribution of inertial smoothing is reduced, and the instantaneous torque variation has a stronger effect on crankshaft angular velocity." — [MDPI Energies 19(16):3724, balancing cam profiles](https://www.mdpi.com/1996-1073/19/16/3724)
- Idle-roughness metric: on a 4-cylinder, five consecutive speed minima ("valleys") are stored. The cycle-to-cycle change in minimum rpm gives each cylinder's relative strength: "the stronger the firing … the greater the minimum rpm value". — [US patent 7110874, method for evaluating engine idle roughness](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7110874)
- For a 4-stroke 4-cylinder diesel at 750 rpm, the speed fluctuation is at **25 Hz superimposed on a lower ~5 Hz fluctuation**. (This is from a speed-fluctuation patent in the search results; the exact document is unclear, probably US 5040412 or 4697561.) — [US patent 5040412, evaluation of a fluctuating variable](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5040412)
- The amplitude of speed fluctuation over one firing interval relates to that cylinder's mean torque, and the cycle-averaged amplitude relates to mean engine torque. — [US 4697561](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/4697561)
- Instantaneous angular speed is shaped by gas-pressure tangential force **and** by reciprocating-mass inertia forces. At idle the gas force dominates; at high rpm inertia dominates. — [Charchalis 2013, Math. Problems in Engineering](https://onlinelibrary.wiley.com/doi/10.1155/2013/659243); [Hasegawa & Aoyama, IJER 2024](https://journals.sagepub.com/doi/10.1177/14680874241261419)

### Inferences
- **The mechanism the synth should reproduce:** schedule event *k+1* by integrating a crank-speed state, not with a fixed interval.
  ω̇ = (T_gas(θ) − T_load − T_friction)/J. Each event's T_gas is scaled by that event's IMEP draw (§1) and the cylinder's offset (§3). A weak cycle then slows the crank, and the next pulse comes slightly *late*. That is exactly the "valley" signature in the idle-roughness patent.
- Order of magnitude: the peak-to-peak speed ripple within a firing interval at idle is in the **tens of rpm on a 4-cylinder** (a few % of 750 rpm). It is much smaller on a 6 or 8-cylinder because the power strokes overlap. This is an inference; no source number was verified. A useful synth range is ±1–4% of mean speed on 4-cyl, ±0.5–1.5% on V8, and larger on singles/V-twins.
- The "5 Hz under 25 Hz" component on the 4-cyl diesel matches a **1/2 or 1/4-order-type, per-cylinder repeating pattern** (at 750 rpm, 0.5 order = 6.25 Hz). That is consistent with a fixed cylinder imbalance (§3).

### Gaps
- No verified rpm peak-to-peak value per cylinder count at idle (the ScienceDirect topic page with such data returned 403).
- No flywheel inertia values (kg·m²) were retrieved. Dual-mass flywheels (mentioned by Shen 2020) filter this further toward the transmission, but no numbers were found.

---

## 5. Idle speed control: hunting, spark reserve, load steps, the slow "breathing" of idle

### Takeaway
Idle is a closed loop with two actuators: slow airflow (throttle/IAC, with about one engine cycle of delay) and fast spark (a retard "torque reserve"). Load steps (AC compressor, power steering, electrical loads, shift into Drive) cause speed dips that recover within a second or so. A poorly tuned or delay-dominated loop hunts at about 0.5–2 Hz. This is the "breathing" of idle.

### Cited Findings
- **Ford 4.6 L V8, production vehicle, electronic throttle, 4-speed automatic:** nominal idle is about **600 rpm** in neutral and **~525 rpm in Drive**. The lower speed in Drive means a longer delay. The intake-to-torque delay is about **360 crank degrees**, which is about **100 ms at 600 rpm**. The controller samples every **30 ms**. — [Di Cairano, Yanakiev, Bemporad, Kolmanovsky, Hrovat, "Model Predictive Idle Speed Control", IEEE TCST 20(1), 2012](http://cse.lab.imtlucca.it/~bemporad/publications/papers/ieeecst-idlespeed.pdf)
- In the same study, the disturbances were: power steering at full lock ≈ **22 Nm** step; air-conditioning compressor ≈ **16 Nm** extra load. Both are "larger than the disturbances encountered in normal conditions". The simulated test step was 20 Nm, held for several seconds. The main metric is the speed **dip**, because a dip below the "fishhook point" (where friction rises steeply) risks a stall. At cold start, idle starts near **1000 rpm** and drops as coolant warms from about 60 °F to 192 °F. — [Di Cairano et al. 2012](http://cse.lab.imtlucca.it/~bemporad/publications/papers/ieeecst-idlespeed.pdf)
- Spark reserve: moving the nominal idle spark **5° closer to MBT** improved idle fuel economy by about **6.5% in neutral and 4.5% in Drive**. This shows that production idle runs noticeably retarded from MBT, which gives spark authority for fast torque corrections. — [Di Cairano et al. 2012](http://cse.lab.imtlucca.it/~bemporad/publications/papers/ieeecst-idlespeed.pdf)
- Spark timing is set below optimum to create a torque reserve. When torque is needed it is advanced toward MBT, and the spark path acts faster than the air path. — [US 7418943B2, spark advance for idle speed control](https://patents.google.com/patent/US7418943B2/en)
- Too much spark advance or too high a feedback gain causes idle "hunting". Patents reduce the feedback constant, or freeze feedback during misfire, to avoid it. — [US 5333585](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5333585); [HP Academy forum, idle hunting](https://www.hpacademy.com/forum/reflashing/show/idle-oscillationhunting/)
- A conventional system "cannot avoid suffering from a temporary decrease of the idle speed due to a delay in response". Supplying extra air immediately on AC engagement prevents a large dip, rough feel or stall. — [search listing summarising idle-control patents, e.g. US 5265571](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5265571)
- Idle-speed control models are made of manifold filling dynamics, induction-to-power delay and rotational dynamics. "One of the main difficulties … lies with the induction-to-torque delay." — [Bemporad group, MPC design flow for idle speed (CDC 2008)](http://cse.lab.imtlucca.it/~bemporad/publications/papers/cdc08-idle-speed.pdf)
- Passenger-car idle is typically **600–1000 rpm**. — [Wikipedia, Idle (engine)](https://en.wikipedia.org/wiki/Idle_(engine))

### Inferences
- Synth model: a PI loop on rpm with a **~1 engine cycle (≈100 ms at 600–800 rpm) air-path delay** and a first-order manifold lag (τ ≈ 0.1–0.3 s, assumed). Add a fast spark path that can change the amplitude of the next few pulses by ±5–10% (from roughly 5° of retard around a torque curve that is flat near MBT).
- With that delay, a moderately aggressive gain gives a lightly damped oscillation of **±5–30 rpm at 0.5–2 Hz**: the breathing. Well-calibrated modern idle: ±5–15 rpm. Old or hot-cam engines: ±30–100 rpm. These ranges are inferred from the delay and loop structure, not measured in a retrieved source.
- Load-step events for realism: AC clutch engagement (~15 Nm) causes a dip of a few tens of rpm, recovering in about 0.5–1.5 s. Every pulse also gets louder (more air, and the spark advances), which is audible. The same happens briefly on radiator-fan or electrical-load switching and on the N→D shift (setpoint about 75 rpm lower on the Ford V8).

### Gaps
- No retrieved paper gave measured dip size in rpm (the Ford Table I values are embedded images). No measured hunting frequency/amplitude from a production calibration was found.

---

## 6. Engine mounts, block vibration and torsional effects at idle

### Takeaway
Idle vibration felt in a 4-cylinder car is dominated by **2nd order (firing), about 23–27 Hz**. **1st order (about 12 Hz)** appears from rotating imbalance (clutch/DMF) and combustion imbalance, and can be amplified by mount-system resonance. Hydraulic mounts are tuned with a dynamic-stiffness dip at the idle firing frequency.

### Cited Findings
- 4-cylinder car (700 rpm hot idle, DCT): the malfunctioning vehicle showed seat-rail peaks at **11.7 Hz (1st order)** and **23.3 Hz (2nd order)**, with the 1st order much larger. The root cause was clutch imbalance beyond specification, amplified by mount-system resonance. Fixing it cut 1st-order vibration by about **87%**. Re-clocking the clutch by 120° alone gave 25–35%. The vibration was not noticeable at cold idle (1200 → 900 rpm) but appeared when idle fell to 700 rpm (subjective score 5/10). — [Shen et al., IJAV 2020](https://iiav.org/ijav/content/volumes/25_2020_36521584896523/vol_4/1708_fullpaper_568371609345225.pdf)
- The same paper cites a prior vibration absorber that reduced idle vibrations at **20 Hz and 27 Hz** by 48% and 57% respectively. It lists the paths as powertrain mounts, exhaust hangers, drive half-shafts and powertrain pipes. — [Shen et al. 2020](https://iiav.org/ijav/content/volumes/25_2020_36521584896523/vol_4/1708_fullpaper_568371609345225.pdf)
- Hydraulic engine mounts are designed with **"idle rate dip" frequencies**, i.e. low dynamic stiffness around the idle firing frequency. — [US patent 8157250, hydraulic mount with double idle-rate dip](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8157250)

### Inferences
- For the sound: at idle the block and mount path mostly adds **sub-audible/low-audible (10–30 Hz) structure-borne modulation**, which you feel more than hear in the cabin. In the synth this is best modeled as a slow (1st/2nd order) amplitude and pitch modulation of the mechanical/structure-borne layer, and as a small modulation of pipe position/radiation. It is not a separate tonal source.
- Idle speed lowered toward about 600–700 rpm pushes the 2nd order toward the body/mount resonance region. That is why idle "shudder" grows at low idle.

### Gaps
- No crankshaft torsional natural frequencies (typically hundreds of Hz for passenger cars) were retrieved, and no mount transmissibility curves.

---

## 7. Mechanical noises at idle (injectors, HP pump, valve train, drives)

### Takeaway
At idle, especially on GDI engines, the high-pressure fuel system ticking is **more noticeable than combustion and valve-train noise**. Injector clicks are broadband impacts above about 5 kHz. The HP-pump pressurisation causes 1–3 kHz impact noise once per pump stroke (cam-driven). These are the "clatter" layer that makes an idle sound mechanical and alive.

### Cited Findings
- "At idle running speed, noise related to the high-pressure fuel system is more noticeable than camshaft/crankshaft-related mechanical noise and combustion noise due to lower engine speed and lower power output." At idle, the engine is the **only** source of cabin noise (no wind, tyre or transmission noise). Occupants also hear it directly with a window open. — [Watanabe et al. (Hitachi), "Noise Reduction in Gasoline DI Engines by Isolating the Fuel System", ISMA 2010](https://past.isma-isaac.be/downloads/isma2010/papers/isma2010_0514.pdf)
- Sources: fuel pulsation from the plunger-type HP pump (intermittent pressurisation), and the solenoid injector's armature striking its stopper at opening and the needle/pin hitting the seat at closing. Both are transmitted to the cylinder head through the rail and injector bodies and radiated by the block. — [Watanabe et al. 2010](https://past.isma-isaac.be/downloads/isma2010/papers/isma2010_0514.pdf); [US 9512798, DI noise mitigation](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9512798)
- Spectral/level data (V6 DI spin rig at idle: cam 375 rpm = crank 750 rpm, fuel pressure **5 MPa**, injector pulse width **0.8 ms**, 4 mics at 1 m, 1/3-octave):
  - **1–3 kHz** "strong impact noise", which lasts long during the **pump pressurisation event**. Isolating the rail reduced it by about **5 dB(A)**; with a rubber isolator the reduction was 5.5 dB(A) at 1.6 kHz.
  - **Injector ticking > 5 kHz**, reduced by about 3 dB(A) by suspending the injector.
  - Background was ≥10 dB below the engine. (No absolute SPL was extracted.) — [Watanabe et al. 2010](https://past.isma-isaac.be/downloads/isma2010/papers/isma2010_0514.pdf)
- System-level GDI work reported reductions of **2–6 dB(A)** in the characteristic frequency bands of the GDI system at idle. — [Mitigation of Noise and Vibration in the HP Fuel System of a GDI Engine, Procedia 2012](https://www.sciencedirect.com/science/article/pii/S1877042812030261)
- OEM patents specifically target "idle tick" from the HP pump by changing pump control at idle. — [US 8091530, HP fuel pump control for idle tick reduction](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8091530)
- GDI engines are noisier than PFI engines because higher fuel pressure acts on the valves and plungers. — [Watanabe et al. 2010](https://past.isma-isaac.be/downloads/isma2010/papers/isma2010_0514.pdf)

### Inferences
- Timing for the synth:
  - **Injector ticks:** two impulses per injection (opening and closing, separated by the pulse width, about 0.5–1 ms at idle), once per cylinder per cycle. On GDI they happen during intake/early compression, i.e. **offset from that cylinder's exhaust pulse by roughly 180–360 CAD**. On PFI they are much quieter.
  - **HP pump:** 1 stroke per cam lobe. Pumps typically have 2–4 lobes on the camshaft (not verified in this research), so the pump event rate is a small integer multiple of cam speed. Each event is a 1–3 kHz burst lasting a few ms.
  - Give each tick small random amplitude (±1–2 dB) and timing (±0.1 ms) jitter, driven by the crank-speed state (§4).
- These ticks are not attenuated by the exhaust system, so their level relative to the exhaust depends on listener position (engine bay/hood open > cabin > tailpipe).

### Gaps
- No absolute dB(A) numbers were found for injector tick vs exhaust at tailpipe or cabin.
- Nothing quantitative was retrieved on hydraulic-lifter/valve-lash tick, timing-chain whine (at chain-mesh order = sprocket teeth × cam or crank order) or alternator whine (at pulley ratio × poles order). These are well-known order sources, but no levels were found.

---

## 8. Exhaust blowdown pulse at idle

### Takeaway
Each exhaust pulse at idle is a sharp rise when the exhaust valve opens (blowdown), then a plateau/decline while the piston displaces the gas, then a trough, often **below atmospheric**, before the next cylinder. At idle the pulse amplitudes in the manifold/pipe are only a few to about 10 kPa about ambient. There is also a disputed claim that there is no real blowdown at throttled idle.

### Cited Findings
- 2.5 L 4-cylinder at about idle: exhaust gas pressure pulsations vary between **~95 and 110 kPa (absolute)**, at a frequency of about **two cycles per 0.05 s** (≈40 Hz, which suggests a measurement at about 1200 rpm or at a tapping that sees two pulses). Part of each cycle is **sub-atmospheric**, and this sub-atmospheric part shrinks as speed rises to about 1800 rpm. — [US patent 4502848, exhaust-gas-operated vacuum pump](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/4502848)
- Waveform features at idle (Pico automotive diagnostics):
  1. "an initial pulse peak formed by the high-pressure shot of gases escaping from the cylinder toward the end of the power stroke";
  2. "a slight drop … as the main slug of gas is pushed out during the exhaust stroke";
  3. "a significant drop … between pulsations as each piston decelerates towards the end of its exhaust stroke".
  - Peaks are 180° apart on a 4-cylinder. At idle, propagation delay down the pipe can reach "just under half a pulse width in a 12-cylinder engine".
  - Pico states that healthy cylinders should produce uniform pulses. — [Pico AGT-130 exhaust vs ignition, idling](https://www.picoauto.com/library/automotive-guided-tests/pressure-transducers/firstlook%E2%84%A2-pressure-sensor/AGT-130-exhaust-vs-ignition-idling/); [Pico AGT-893](https://www.picoauto.com/library/automotive-guided-tests/pressure-transducers/wps500x-pressure-transducer/AGT-893-exhaust-pressure-running/)
- Modelling practice: the in-cylinder blowdown pressure drop from EVO pressure to exhaust pressure is approximated with a **cosine over a fixed duration**, for example **28 CAD**. — [US patent 7201140 / related engine-braking patents, via search listing](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7201140)
- GM's steady exhaust backpressure limit at idle is ≤ **8.62 kPa (1.25 psi)**. This is a mean, not a pulse amplitude. — [search listing, aa1car / Vehicle Service Pros](https://www.aa1car.com/library/exhaust_backpressure.htm)
- **Disputed:** a forum claim (Eng-Tips "blowdown cycle duration") says that at throttled idle the cylinder/exhaust pressure ratio at EVO is below 1, so there is no blowdown and the piston just pushes gas out. The page could not be fetched (403) to verify context or author. It conflicts with the Pico description of a distinct initial blowdown peak at idle. — [Eng-Tips thread](https://www.eng-tips.com/threads/blowdown-cycle-duration.24870/) vs [Pico AGT-130](https://www.picoauto.com/library/automotive-guided-tests/pressure-transducers/firstlook%E2%84%A2-pressure-sensor/AGT-130-exhaust-vs-ignition-idling/)

### Inferences
- Resolving the dispute (physics, not verified): at idle, manifold pressure is about 0.3–0.4 bar, so cylinder pressure at EVO is low (perhaps 1–2 bar absolute) and the blowdown is weak and short. A distinct initial spike still exists (Pico), but the displacement ("piston push") phase is a larger share of the pulse than at WOT. So the idle pulse should be **lower, broader, more rounded and followed by a deeper sub-atmospheric trough** than a WOT pulse. The 95–110 kPa swing (about +10/−5 kPa about ambient) fits this.
- Synth pulse at idle: fast rise over about 20–40 CAD (≈4–8 ms at 750 rpm), then decay/plateau over about 100–180 CAD, then an undershoot. Scale the peak with the event's "EVO pressure", which rises for late/partial burns and falls to nearly zero for a misfire (§2).

### Gaps
- No measured in-cylinder pressure at EVO for idle, and no port-pressure trace with kPa and CAD axes, was retrieved (Heywood figures not accessible).

---

## 9. Published idle sound measurements (spectra, orders, dB(A)); motorcycles/V-twins; diesel

### Takeaway
Order content at idle is dominated by firing order and its harmonics. Half orders come from cylinder imbalance and uneven firing (V-twins and singles are dominated by 0.5 order). Exhaust and intake orifice noise dominate below ~250 Hz; engine-radiated noise (including the mechanical ticks) dominates above ~125–250 Hz. Few absolute idle dB(A) figures were retrievable.

### Cited Findings
- Motorcycle study (hemi-anechoic, rolling road; idle, snaps, run-ups; tailpipe mic at **50 cm / 45°**):
  - The single-cylinder-character bike's orders were dominated by **0.5, 1, 1.5, 2**, with 0.5 order present throughout. The benchmark bike was dominated by integer orders 1, 2, 3.
  - Exhaust and intake are "inherently the major contributors for the harmonics of ½ crank order". Exhaust orifice noise matters below 250 Hz, intake below 125 Hz, and engine-radiated noise above 125 Hz.
  - Listeners rated integer orders "sporty/powerful" and strong half orders "unpleasant/rough". — [Inter-noise 2014 p70, "Target setting and source contribution for sound quality of a motorcycle"](https://www.acoustics.asn.au/conference_proceedings/INTERNOISE2014/papers/p70.pdf)
- Exhaust noise spectra at typical speeds are dominated by components up to about **1000 Hz**. — [ScienceDirect topic "Flow noise" (search listing)](https://www.sciencedirect.com/topics/physics-and-astronomy/flow-noise)
- Half-order exhaust noise differs by **10–25 dB(A)** between muffler designs over 1500–3300 rpm, which shows how much the exhaust system shapes half-order content. — [US patent 11391195, exhaust system and muffler](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/11391195)
- Diesel: knock/clatter is most prevalent at very light load such as idle, and pilot injection is the standard fix. Pilot-injection studies report COV of gross IMEP < 2.5% in the conditions studied. — [ResearchGate, "Effect of pilot injection on combustion noise of diesel engines"](https://www.researchgate.net/publication/285956666_Effect_of_pilot_injection_on_combustion_noise_of_diesel_engines); [Applied Sciences 9(9):1875, PPCI pilot injection noise](https://doi.org/10.3390/app9091875)
- Diesel IAS work: the 4-cylinder diesel at 750 rpm shows 25 Hz firing ripple plus a ~5 Hz component (see §4). — [US 5040412](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5040412)

### Inferences
- Firing frequencies for reference: f_fire = rpm/60 × N/2. At 750 rpm this gives 4-cyl 25 Hz, 6-cyl 37.5 Hz, 8-cyl 50 Hz, V-twin 12.5 Hz (with uneven spacing on a 90°/45° twin). The 0.5 order at 750 rpm is 6.25 Hz. It is not heard as a tone but as a **rhythmic "lope"/amplitude pattern** repeating each 720°, which the ear reads as "alive".
- Diesel idle differs from SI: combustion is more repeatable (COV often ≤2–3%) but has a sharp pressure-rise "clatter" (impulsive, 1–4 kHz structure-borne) and loud injector/pump noise. The randomness therefore comes mostly from mechanical/injection ticks and the clatter envelope, not from IMEP scatter.
- Suggested per-event parameter set for the synth, all derived from the above:
  - `amp = A0 · cyl_offset[c] · (1 − |N(0, σ_idle≈0.06–0.10)|-skew) · spark_reserve_gain(t)`
  - `t_next` from integrating ω with torque ∝ amp (§4)
  - `pulse_shape_width` +5–15% for weak/late burns
  - idle-control loop at 0.5–2 Hz (±5–30 rpm)
  - discrete events for AC/fan load steps
  - HP-pump bursts at 1–3 kHz and injector double-clicks >5 kHz, synced to cam angle with small jitter
  - optional AR(1) residual coupling for lean/cam presets.

### Gaps
- No absolute idle SPL in dB(A) at tailpipe or cabin for passenger cars was retrieved from a primary source. Commonly quoted cabin idle levels (roughly 35–45 dB(A) for modern cars) and tailpipe near-field levels are unverified here.
- Nothing retrieved on Brandl et al. idle sound-quality work or on published idle order spectra for 4/6/8-cylinder cars.
- Only secondary coverage of V-twin/motorcycle idle. No quantitative COV or crank-speed data for motorcycles.
