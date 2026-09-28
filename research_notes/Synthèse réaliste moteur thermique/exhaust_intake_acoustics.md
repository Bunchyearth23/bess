# Acoustics of automotive exhaust and intake systems for physically based real-time engine sound synthesis

Scope: how the exhaust/intake path turns cylinder blowdown pulses into the sound that is heard, with numbers and modeling guidance for a 1D digital-waveguide (DWG) synthesizer (header → chamber → tailpipe, junction scattering, lossy lowpass per tube). Current model: 3 linear tubes with fixed reflection coefficients, which sounds artificial.

Note on sourcing: the session could fetch only a few primary sources in full: Alfredson & Davies 1970 (JSV, ISVR), Selamet et al. 1998 (JASA, catalysts), Gordon/NASA 1969 (flow noise), ITU engine lecture notes (Heywood-style numbers), and the PTR arXiv paper 2026. Munjal's book, SAE papers and GT-Power docs were **not** accessible. Standard textbook relations (sound speed vs T, expansion-chamber TL formula, Helmholtz formula, Doppler, cut-on frequencies) are written under **Inferences** and marked "textbook". They were not re-verified against a fetched source in this session. The worked numbers come from my own arithmetic.

---

## 1. Blowdown pulse: waveform at the exhaust port, scaling with load/rpm, nonlinear propagation

### Takeaway
The exhaust valve opens 40–60° BBDC. Cylinder gas is then at about 4–5 bar and up to about 1000 K, against a manifold at about 1 atm, so valve flow starts choked (sonic). The port pressure pulse grows with load, and its crank-angle duration grows with rpm. Pulses of about 1 bar and more steepen noticeably within tens of cm to about 1 m of hot pipe. By the tailpipe, levels (≤155–160 dB in-pipe) are low enough that linear acoustics suffices (Alfredson & Davies). So the nonlinearity matters in the header and source region, not in the tailpipe.

### Cited Findings
- At exhaust valve opening (60–40° BBDC), cylinder pressure is about 0.4–0.5 MPa and temperature up to 1000 K, while exhaust-system pressure is about 1 atm. The flow is at first choked and sonic ("blowdown phase"). With increasing engine speed, the **crank-angle duration of blowdown increases**. — [ITU MAK 493E lecture notes, Exhaust process](https://web.itu.edu.tr/~sorusbay/ICE/index_files/LN09.pdf)
- Exhaust valve closes 8–50° ATDC. Intake opens 10–25° BTDC. Valve overlap is 15–50° CA, with reverse flow worst at idle/low speed. — [ITU LN09](https://web.itu.edu.tr/~sorusbay/ICE/index_files/LN09.pdf)
- During blowdown the flow is choked at the valve/seat gap (cylinder pressure > 2× manifold pressure), not in the port. This is practitioner-forum knowledge, not peer-reviewed. — [Speed-Talk forum](https://www.speed-talk.com/forum/viewtopic.php?t=34736&start=30)
- Exhaust port pressure pulse amplitude rises substantially with engine speed. In a load sweep the main waveform change is in the blowdown phase, and pulse magnitude increases with load. As speed rises, the share of exhaust mass leaving during blowdown decreases. One study measured a pressure-wave amplitude of about 170 kPa in exhaust piping. (Search-snippet level; figure pages were not readable.) — [ResearchGate figure: instantaneous exhaust port pressure vs crank angle](https://www.researchgate.net/figure/nstantaneous-exhaust-port-pressure-distributions-with-crank-angle-for-various-engine_fig2_26624827); [Applied Thermal Eng. 2023, crank-angle-resolved exhaust mass flow](https://www.sciencedirect.com/science/article/pii/S1359431123017544)
- The exhaust pulse consists of a high-pressure head, a medium-pressure body and a low-pressure tail. Runner length, cross-section and shape set scavenging and the rpm range where it works. — [Wikipedia: Exhaust manifold](https://en.wikipedia.org/wiki/Exhaust_manifold)
- Alfredson & Davies used the method of characteristics to estimate the time for a sinusoid to steepen into a shock: **T ≈ p0 / (5·f·Δp)**. The distance to shock is T·(c ± u0), with + for waves travelling with the mean flow. Example: ±1 psi (±6.9 kPa, 167.7 dB) at 300 Hz with 100 ft/s mean flow shocks after about 15 ft (4.6 m). Real dissipation always lengthens this distance. — [Alfredson & Davies 1970, "The radiation of sound from an engine exhaust", JSV 13(4)](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Measured tailpipe in-pipe levels peak around **155 dB at 300 Hz and about 135 dB at 1 kHz**. The authors conclude that below about 160 dB (at about 300 Hz) finite-amplitude effects can be neglected, so the linear theory is adequate in the tailpipe. Higher frequencies radiate more efficiently at the outlet, so reflected waves are less steep than incident ones. — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Large-amplitude waves in exhaust pipes can grow through interference between reflected waves and subsequent pulses, and turn into shock waves during propagation ("abnormal exhaust noise"). A weak shock through a silencer has also been studied. — [SAE 871924, Generation mechanism for abnormal exhaust noise](https://www.sae.org/publications/technical-papers/content/871924/); [JSV: weak shock wave through an engine exhaust silencer](https://www.sciencedirect.com/science/article/abs/pii/S0022460X03007946); [NASA NTRS: two-stroke exhaust noise by method of characteristics](https://ntrs.nasa.gov/citations/19820005954)
- PTR model (2026) models each cylinder's pressure-release envelope as E(φ) = (1 − e^(−αφ))·e^(−βφ), with learnable attack α and decay β for the asymmetric fast-rise/slow-decay transient. It uses a derivative-of-cosine (bipolar) pulse and a harmonic series with exponential roll-off ∝ e^(−0.5kλ). It also models "thermodynamic pitch modulation": hot gas (about 800–1000 °C) moves faster than cooler residual gas, so the pulse's leading edge outruns its tail and the pitch glides downward within a pulse. — [Doerfler et al., arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)

### Inferences
- Worked example with the Alfredson formula for the header: Δp = 100 kPa (a 2-bar-absolute port pulse), f = 300 Hz, p0 = 100 kPa gives T ≈ 0.67 ms. With c + u ≈ 650 + 100 m/s, the distance to shock is about 0.5 m. **Steepening therefore plausibly happens inside a typical 0.5–1 m primary at WOT/high load, but not at idle/part load** (Δp of a few tens of kPa gives several metres). This supports load-dependent brightening of the pulse front.
- Cheap way to get the audible part of the nonlinearity:
  - (a) Make the excitation pulse rise time load-dependent: sharper at high load.
  - (b) Or insert a mild amplitude-dependent "steepening" stage in the header only. Options are a memoryless waveshaper on the forward wave, or amplitude-dependent fractional delay (local speed c + (γ+1)/2·u′), which is the standard Burgers/"sharpening" trick used in brass-instrument DWG synthesis.
  - The effect audibly adds high-frequency energy (the "rasp") that rises faster than linearly with throttle. A fixed linear DWG cannot do this, which is one likely cause of the "artificial" sound.
- Pulse shape guidance for the source:
  - Port pressure is roughly a one-sided pulse: fast rise (valve opening while choked), then decay over blowdown, then a plateau/sub-atmospheric tail during displacement and overlap.
  - The width grows with rpm in crank degrees (ITU), i.e. it stays closer to constant in **milliseconds** at low rpm.
  - Model the source as a **mass-flow (volume-velocity) source with its own impedance**, not an ideal pressure source. Choked valve flow is nearly independent of downstream pressure during early blowdown. This means the port end should present a high-impedance (≈ rigid, R ≈ +1) reflection while the valve is closed and a finite one while it is open. A time-varying reflection at the port end is cheap and adds realism.
- The PTR envelope form (1 − e^(−αφ))e^(−βφ) is a directly usable, cheap excitation shape. Map α to load (sharper rise at high load) and β to rpm.

### Gaps
- No primary measured port-pressure traces with numbers (kPa, rise time in µs or °CA) for a modern gasoline engine could be read. Heywood ch. 6/7 and GT-Power validation papers would give them. The 170 kPa figure is snippet-level.
- No source found that quantifies the audibility threshold of header steepening for road cars.

---

## 2. Headers/manifold: runner lengths, collectors, V8 cross-plane burble, boxer rumble, junction modeling

### Takeaway
Engine "character" (burble, rumble) comes largely from **pulse arrival times at the collector and tailpipe**. Those times are set by the firing order grouped per bank, plus runner-length delays L_i/c. Cross-plane V8 banks each see uneven pulse spacing, which produces strong half-order (camshaft-rate) content. Unequal-length runners on a flat-4 stagger arrival times the same way. Junctions must be modeled as proper N-port scattering (pressure continuity, volume-velocity conservation), with branch admittances that depend on local temperature.

### Cited Findings
- Cross-plane V8: the usual firing order L-R-L-L-R-L-R-R gives **uneven exhaust pulse spacing within each bank**, which causes the characteristic rumble. — [Wikipedia: Crossplane](https://en.wikipedia.org/wiki/Crossplane); [Wikipedia: V8 engine](https://en.wikipedia.org/wiki/V8_engine)
- Two same-bank cylinders firing consecutively create a high-pressure region in that bank's pipe. X- or H-pipes let one bank's pulse interact with the other (the X more strongly); the X helps scavenge. — [LSX Mag: Flat-plane vs cross-plane](https://www.lsxmag.com/tech-stories/flat-plane-vs-cross-plane-the-gemini-v8-legacy/); [Autosport forum: X-pipes and H-pipes](https://forums.autosport.com/topic/215212-x-pipes-and-h-pipes-for-v8s/); [Wikipedia: Exhaust manifold](https://en.wikipedia.org/wiki/Exhaust_manifold)
- With dual banks, the first bank's sound is delayed relative to the second by the different path length to the tailpipe end. This gives **low-frequency beating/modulation** that occupants can find annoying (patent literature). — search result from [Keysight/patent cluster; claim appears in US patent text](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10024218) (attribution to the specific patent is uncertain)
- Subaru boxer unequal-length (UEL) headers: runners of different length deliver pulses to the collector at staggered times. The result is a "strong pulse (two merging) followed by a mellower note", perceived as rumble. UEL comes from the direct routing to a turbo on one side. — [SlashGear](https://www.slashgear.com/1970710/subaru-unequal-length-headers-rumble-sound/); [MAPerformance UEL vs EL](https://www.maperformance.com/pages/equal-length-vs-unequal-length-subaru-headers) (enthusiast/commercial sources)
- Long primaries resonate (tune) at lower rpm and short ones at higher rpm. Oversized tubes slow the gas and weaken scavenging. — [Wikipedia: Exhaust manifold](https://en.wikipedia.org/wiki/Exhaust_manifold)
- A Yamaha patent models the exhaust as a waveguide-type circuit. Delay circuits represent pipe propagation and "junctions" represent scattering at pipe connections. — [US5835605A Engine exhaust sound synthesizer](https://patents.google.com/patent/US5835605A/en)
- PTR model: two independent resonators process the two cylinder-bank outputs (separate manifold paths), then a final shared resonator (common pipe). — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)

### Inferences
- **Worked cross-plane example** (GM LS order 1-8-4-3-6-5-7-2; odd cylinders = left bank). Pulses every 90° of crank:
  - Left bank fires at 0, 270, 450, 540° → intervals **270/180/90/180°**.
  - Right bank: 90, 180, 360, 630° → **90/180/270/180°**.
  - Each bank is therefore a 4-pulse pattern repeating every 720°. Its spectrum has strong lines at all multiples of **rpm/120 (order 0.5)**, instead of only order 2 as an even 4-pulse bank would give. With true duals and no crossover, a listener hears both banks. An X-pipe mixes them toward the even order-4 sound, making it "smoother, raspier". An H-pipe mixes partially.
  - Model: 8 per-cylinder excitations → 2 bank waveguide trees (runner delays) → optional crossover junction (X = 4-port junction; H = two 3-port junctions joined by a short balance tube) → two tailpipes, panned or summed by listener position.
- **Arrival-time rule**: t_i = θ_fire,i/ω + L_runner,i/c(T). With c ≈ 600 m/s in a hot header, ΔL = 0.3 m gives 0.5 ms, i.e. 18° CA at 6000 rpm but only 2.7° at 900 rpm. So runner-length staggering matters more at high rpm, while firing-order grouping dominates at idle. Subaru EJ: cylinders 1&3 are on one side and 2&4 on the other, with firing order 1-3-2-4. Per side the pulses come in pairs (180° then 540°), and UEL adds a further offset before the merge. **Do not collapse all cylinders into one excitation feeding one tube**. The per-cylinder/per-bank topology creates these sub-harmonic orders.
- **Junction scattering (textbook DWG / Kelly-Lochbaum, generalized)**: for N branches with areas S_i, gas density ρ_i and sound speed c_i, use admittance Y_i = S_i/(ρ_i c_i). Junction pressure is p_J = 2·Σ Y_i p_i⁺ / Σ Y_i, and each outgoing wave is p_i⁻ = p_J − p_i⁺. Two-port special case: R = (Y1 − Y2)/(Y1 + Y2) = (S1 − S2)/(S1 + S2) for equal gas properties.
  - Example: 50 mm pipe into a 200 mm chamber (area ratio m = 16) gives R ≈ −0.88 entering the chamber and +0.88 inside the chamber at its exit.
  - Using per-branch ρc means temperature steps (hot header → cooler tailpipe) create their own small reflections automatically.
  - A 4-into-1 collector is a 5-port junction. Pulses from one runner partly go back up the other three runners and reflect off closed valves. This is a large part of the "collector" colour and is missing from a 3-tube chain.
- Cheap enough for real time: 8 runners + 2 collectors + crossover + 2 mufflers + 2 tailpipes is about 20 delay lines with 1 one-pole loss filter each, which is trivial on a CPU.

### Gaps
- No peer-reviewed measurement of cross-plane V8 per-bank or tailpipe order spectra with/without X/H-pipe was retrieved.
- Typical primary/secondary lengths (e.g. 4-2-1 dimensions) were not sourced here. SAE header-tuning papers or Blair's "Design and Simulation of Four-Stroke Engines" would give them.

---

## 3. Temperature gradient along the exhaust and its effect on sound speed/resonances

### Takeaway
Gas cools from about 800–1000 °C at the port/manifold to roughly 150–300 °C at the tailpipe outlet under load. Idle is cooler throughout, and overrun/fuel cut cools the system further. Because c ∝ √T, the header "acoustic length" is almost half its physical length compared with the tailpipe. All resonances move with load and thermal history (seconds-to-minutes time constant), which a fixed-delay model cannot reproduce.

### Cited Findings
- SI engines: exhaust temperature 400–600 °C average, **300–400 °C at idle, about 900 °C at max power**. In-cylinder gas at EVO is 200–300 °C hotter. CI engines: 200–500 °C. — [ITU LN09](https://web.itu.edu.tr/~sorusbay/ICE/index_files/LN09.pdf)
- Manifold entry temperatures up to 1000 °C. Catalyst inlet typically 650–850 °C. One study found about 310 °C after the catalyst. Another had 475 °C at manifold entry and 272 °C leaving the muffler. Pipe-end temperature was < 160 °C in one scenario. (Aggregated search results, mixed engines.) — [ScienceDirect topic: Cool exhaust gas](https://www.sciencedirect.com/topics/engineering/cool-exhaust-gas); [ResearchGate: exhaust gas temperature vs tailpipe length](https://researchgate.net/figure/Illustration-of-the-exhaust-gas-temperature-versus-tailpipe-length-9_fig4_271643836); [ResearchGate Q&A catalytic converter temperature](https://www.researchgate.net/post/Catalytic-converter-exhaust-temperature-anyone-know)
- Selamet et al.: the frequency range at ambient conditions "needs to be stretched proportional to the square root of the temperature in the hot exhaust system. **A multiplying factor of 1.5 or slightly higher is not atypical**." The first catalyst TL dome is reactive, i.e. set by the expansion ratio, so it simply shifts with temperature. — [Selamet, Easwaran, Novak, Kach 1998, JASA 103(2)](https://mae.osu.edu/sites/default/files/2021-12/J23.pdf)
- PTR model uses c ∝ √T with hot gas about 800–1000 °C and explicitly models a downward pitch trajectory within a pulse, plus DFCO (deceleration fuel cut-off) as a distinct operating mode. — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)
- Alfredson & Davies note that cooling the gas at a given mass flow lowers the Mach number and raises ka, so it changes the outlet reflection coefficient. Temperature and flow effects on the boundary are coupled. — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)

### Inferences
- Textbook: c = √(γRT). For exhaust gas (γ ≈ 1.33–1.35, R ≈ 287–290 J/kg·K), c ≈ 19.7–20.0·√T[K]:

  | Location / state | T | c (m/s) |
  |---|---|---|
  | Port/header, WOT | 900 °C | ≈ 680 |
  | Header, part load | 700 °C | ≈ 620 |
  | Mid-system | 400 °C | ≈ 515 |
  | Tailpipe, load | 250 °C | ≈ 455 |
  | Tailpipe, idle/cold | 100 °C | ≈ 385 |
  | Ambient | 20 °C | 343 |

  Example: a 2 m open-open tailpipe at 250 °C has f1 ≈ 114 Hz. At 20 °C it would be 86 Hz. That +33% is about 5 semitones.
- Implementation:
  - (a) Give every tube section its own temperature state T_k(t). Drive it with first-order lag filters from a target set by load × rpm. Use fast time constants (about 1–3 s) in the header and slow ones (about 10–60 s) for the muffler/tailpipe, since the thermal mass is larger downstream.
  - (b) Derive delay length N_k = L_k·fs/c(T_k) with **fractional, smoothly interpolated delays** (Lagrange/allpass/Thiran). The ringing then glides instead of stepping.
  - (c) On overrun/DFCO, drop header T toward about 300–400 °C over seconds (the engine pumps air). Tailpipe resonances then sag audibly downward. This is a key cue, and a fixed-tube model has none of it.
  - (d) Discretize the gradient: split long pipes into 2–4 segments with decreasing T. Per-segment ρc changes give weak distributed reflections.
- Within-pulse "pitch bend" (PTR): during blowdown the hot, fast gas front leads. It can be approximated by a slight temporal compression of the excitation onset, or by modulating header delay per pulse by a few percent.

### Gaps
- No time-resolved data on exhaust temperature transients during decel/fuel cut (time constants) was found. The values above are engineering estimates.
- No direct measurement of tailpipe resonance frequency drift vs warm-up was retrieved.

---

## 4. Catalytic converter, mufflers, resonators: transmission loss and efficient approximation

### Takeaway
Reactive elements (expansion chambers, the catalyst housing's expansion ratio) produce **domed TL curves**. Peaks depend on area ratio (roughly 10–25 dB for practical ratios) and drop to ≈ 0 dB at chamber pass-band frequencies (kL = nπ). Dissipative elements (catalyst capillaries, glass-wool packing, perforates) add broadband attenuation that rises with frequency above the first dome. Perforated or micro-perforated inserts can add 15–60 dB in the 100–1000 Hz range. In a DWG these map to: area-step junctions + chamber delay (reactive), per-segment lowpass/shelving loss (dissipative), and side-branch delay loops (Helmholtz/quarter-wave).

### Cited Findings
- Catalytic converter (Selamet et al., JASA 1998):
  - The **first attenuation dome is dictated by reactive effects**, i.e. wave reflections from the cross-sectional area changes of the housing.
  - Adding the monolith (i) slightly reduces the effective expansion ratio, which can reduce first-dome TL, and (ii) adds significant capillary (viscous/thermal) dissipation beyond the first dome.
  - Tested monoliths: circular with open-area ratio φ = 0.8 (square pore 1.214 mm), oval 400 cpsi with φ = 0.75.
  - Wave propagation is predominantly 1D in the automotive range, so "a simple analytical treatment based on 1D approach appears to be rather useful, if not sufficient".
  - A higher open-area ratio exposes more pore fluid to viscous loss, so TL increases.
  - The effect of flow on attenuation is mainly **convective** (wavenumber k/(1±M)).
  - Coating raises flow resistance and therefore dissipation.
  - — [Selamet et al. 1998](https://mae.osu.edu/sites/default/files/2021-12/J23.pdf)
- Two-port (transfer-matrix) elements for monolith honeycomb capillaries and ordinary pipes are standard for catalysts. FEM, mode matching and point collocation cover 3D effects in the expansion/contraction cones. — [Multidimensional acoustic modelling of catalytic converters](https://www.researchgate.net/publication/266469190_Multidimensional_acoustic_modelling_of_catalytic_converters); [The passive acoustic effect of automotive catalytic converters](https://www.researchgate.net/publication/289649603_The_Passive_Acoustic_Effect_of_Automotive_Catalytic_Converters)
- Simple expansion chamber designs reach TL maxima around **22 dB** in one study. Micro-perforated panels plus backing-wall absorptivity raised muffler TL by **15–60 dB over 100–1000 Hz**. Inhomogeneous MPPs in an expansion chamber added **25–30 dB** at low-to-mid frequency. — [Prediction of TL on a simple expansion chamber muffler](https://www.academia.edu/75195284/Prediction_of_Transmission_Loss_on_a_Simple_Expansion_Chamber_Muffler); [Applied Acoustics 2017, multi-chamber MPP muffler](https://www.sciencedirect.com/science/article/abs/pii/S0003682X17301081); [TL analysis of muffler with MPP](https://www.researchgate.net/publication/261252698_Transmission_loss_analysis_of_muffler_with_micro-perforated_panel)
- Absorptive material type changes muffler TL (study on varying absorption material). — [ResearchGate 347950364](https://www.researchgate.net/publication/347950364_Investigation_of_transmission_loss_in_muffler_by_varying_absorption_material)
- Whole-system resonances: with reactive silencers, overall level vs rpm shows **resonance peaks** (e.g. louder at 1150 and 1650 rpm at full load on the test engine). These come from resonances of the **entire exhaust system**, which "are often not simply related to resonances of individual components". — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Baldan et al. 2015 render the muffler with **four independent, partially reflecting waveguides**. Zero reflection gives almost no silencing, and a feedback factor of 1 gives a perfectly silent muffler. — [Baldan, Lachambre, Delle Monache, Boussard 2015 (ResearchGate)](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments); open-source JS implementation: [Antonio-R1/engine-sound-generator](https://github.com/Antonio-R1/engine-sound-generator)
- Wave-1D + transfer-matrix TL evaluation is used industrially for perforated plug mufflers with absorptive material. — [Springer chapter: integrated TMM with Wave 1D](https://link.springer.com/chapter/10.1007/978-981-92-0694-0_46)

### Inferences
- Textbook (Munjal/Kinsler) simple expansion chamber, no flow: **TL = 10·log10[1 + ¼(m − 1/m)²·sin²(kL)]**, with m = chamber/pipe area ratio and L = chamber length:
  - m = 9 → max 13.2 dB.
  - m = 16 → max 18.1 dB.
  - m = 25 → max 21.9 dB (matches the "22 dB" cited above).
  - TL = 0 at kL = nπ, i.e. f = n·c/(2L). A 0.4 m chamber at 400 °C (c ≈ 515 m/s) has pass bands at about 645, 1290, … Hz. These pass bands are what gives each muffler its "colour", and they also move with temperature.
- Textbook (plane-wave validity): the first non-planar mode cuts on at f ≈ 1.84·c/(π·D) for a circular section.
  - 250 mm round muffler at 400 °C: ≈ 1.2 kHz.
  - 55 mm pipe at 250 °C: ≈ 4.8 kHz.
  - So 1D DWG is physically valid up to about 1 kHz inside large mufflers. Above that, a lumped lowpass/"diffuse" treatment is honest. Oval mufflers are 1D-equivalent at low f per Selamet.
- Textbook Helmholtz resonator: f_H = (c/2π)·√(S_neck/(V·L_eff)), where L_eff = neck length + about 1.7·r_neck (flanged/unflanged end corrections). Quarter-wave side branch: f = c/(4(L + 0.6r)), with notches at odd multiples. In DWG, a side branch is a 3-port junction feeding a delay line with a closed end (R = +1, plus loss). This is exact in 1D and costs one delay line.
- Efficient DWG realisation of a muffler:
  - (1) Inlet area step as a junction.
  - (2) Chamber as a bidirectional delay with length set by L/c(T).
  - (3) Outlet area step.
  - (4) Extended inlet/outlet tubes as quarter-wave side branches at the chamber junctions (these produce the characteristic notches).
  - (5) Perforate/packing as a **per-round-trip loss filter**, e.g. a one-pole lowpass/high-shelf with 3–10 dB/round trip attenuation above about 300–500 Hz. This mimics the "rising with frequency" absorptive TL.
  - (6) Catalyst as area step + short delay + gentle lowpass (capillary viscous loss grows ∝ √f).
  - Fixed scalar reflection coefficients (the current model) give only one comb-like response. It is the **frequency-dependent loss** and **multiple incommensurate delay loops** that avoid the metallic "tube" sound.
- Alternative for very low cost: precompute a muffler's TL/transfer function offline (TMM at a few temperatures). Then fit it with a low-order IIR or a small FIR and crossfade across temperature. You lose the interaction with upstream reflections, so it is less "alive".

### Gaps
- No full Munjal TL curves for a production 3-pass or glass-packed muffler were retrieved. Typical production muffler TL (probably about 20–40 dB mid band) is **unverified**.
- No numerical catalyst TL in dB was extracted (figures only). The qualitative dome structure is solid.

---

## 5. Mean flow effects: convection, flow-generated noise (jet, U^6/U^8), whoosh; idle vs load

### Takeaway
Mean flow Mach in tailpipes is about 0.05–0.2. It shifts wave speeds to c ± u, raises the open-end reflection magnitude by about ≤ (1 + 2M), and reduces the end correction at low Strouhal number. Flow noise comes from two sources. Obstruction/perforate/lip **dipole noise scales ∝ U^6** (power ∝ Δp³). Free-jet **quadrupole noise scales ∝ U^8** at the outlet. At idle it is negligible. In one classic engine study, broadband jet noise was insignificant against the pulse harmonics. It matters at high flow, on overrun (whoosh with no pulses) and for high-frequency hiss.

### Cited Findings
- Tailpipe mean flow in a real engine exhaust (6-cyl 2-stroke diesel, about 200 bhp at 2200 rpm): Mach 0.078 and 0.171 measured. Mean velocity is about 85% of the midstream value (1/7–1/8.5 turbulent profile). — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Cold-flow measurements showed the open-end reflection coefficient increased by a factor of about **1 + 2M**, with the phase insensitive to flow. In hot exhaust, the measured increase was somewhat less. Radiated SPL depends on R², so neglecting mean flow "will lead to a considerable underestimation of the level of the radiated sound". — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- With mean flow, the unflanged-pipe end correction varies from δ/a = 0.61 (high Strouhal) to **0.19 (low Strouhal)**. Vortex shedding at the lip absorbs acoustic energy (a 2D theory underestimates it by a factor of 2.5). |R| follows Munt/Cargill theory with a full Kutta condition. — [Peters, Hirschberg et al., JFM "Damping and reflection coefficient measurements for an open pipe at low Mach and low Helmholtz numbers"](https://www.cambridge.org/core/journals/journal-of-fluid-mechanics/article/abs/damping-and-reflection-coefficient-measurements-for-an-open-pipe-at-low-mach-and-low-helmholtz-numbers/176F64F8EDC5D7B2E8C2777D6FAA6FE2); [Howe/ADS 1980 JSV 72, "Reflection coefficients for an unflanged pipe with flow"](https://ui.adsabs.harvard.edu/abs/1980JSV....72..543H/abstract)
- Gordon (NASA 1969):
  - Lighthill: free-jet noise power **I ∝ ρU⁸/c⁵** (quadrupole). Aerodynamic dipole **I ∝ ρU⁶/c³**.
  - A plain pipe outlet shows U⁸ at higher velocities, tending to **U⁶ at lower velocities** (dipole at the exit lip).
  - With any in-pipe obstruction, a U⁶ law held over the whole range.
  - Obstruction noise power **W = k·Δp³·D²/(ρ²c³)**, with Δp the obstruction's pressure drop (spectra collapse within ±2 dB with a Strouhal/cut-off correction).
  - Below the pipe's plane-wave cut-off, in-pipe sources radiate with lowered multipole order. The exit-plane transmissibility ∝ f² below cut-off.
  - — [Gordon, "A study of exhaust noise as it relates to…", NASA NTRS 19690002231](https://ntrs.nasa.gov/api/citations/19690002231/downloads/19690002231.pdf)
- Jet noise at the tailpipe arises from mixing of the exhaust jet with ambient air: a potential core plus a turbulent mixing region. Small eddies make high-frequency noise and large eddies make low-frequency noise, giving a broadband spectrum. — [ScienceDirect topic: Flow noise](https://www.sciencedirect.com/topics/physics-and-astronomy/flow-noise)
- Engine measurement: tailpipe harmonics of firing frequency extend into the kHz range, and "the contribution from the broad-band jet noise of the gas flow was insignificant" (diesel, simple expansion-chamber silencer). — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Flow-generated noise in perforated mufflers scales with perforation rate. Perforate **whistle** frequency ∝ flow velocity / hole diameter. Flow regeneration noise must be included at higher flow velocities. — [Oxford J. Mechanics: flow-induced noise of perforated tube mufflers](https://academic.oup.com/jom/article/29/2/225/5948233); [PLOS One 2025, airflow and muffler acoustic performance](https://journals.plos.org/plosone/article?id=10.1371%2Fjournal.pone.0318210); [SAE 2017-01-1795 flow-generated noise inside mufflers](https://saemobilus.sae.org/papers/measurement-flow-generated-noise-inside-mufflers-2017-01-1795); [Experiments on flow noise generation in simple exhaust geometries](https://www.researchgate.net/publication/233552204_Experiments_on_Flow_Noise_Generation_in_Simple_Exhaust_Geometries)
- 1D CFD (GT-Power-type) matches measured engine orders below about 1 kHz, but **flow noise is not captured**, which is a known weakness. (Search snippet; exact source attribution uncertain.) — [Frontiers in Energy Research 2022](https://www.frontiersin.org/journals/energy-research/articles/10.3389/fenrg.2022.873749/xml)
- PTR model adds three noise components: turbulent exhaust flow with throttle-controlled stochastic AM, intake pulsation noise, and steady aeroacoustic flow noise during DFCO (engine as air pump). They are implemented as an ERB-spaced filtered-noise bank with time-varying gains. — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)

### Inferences
- Scaling for synthesis. Tailpipe velocity U ≈ ṁ/(ρ_gas·A_pipe):
  - Idle: a few m/s (M < 0.02).
  - WOT, about 2 L engine at 6000 rpm (ṁ ≈ 0.1 kg/s, ρ ≈ 0.6 kg/m³ at 300 °C, single 55 mm pipe): about 70 m/s (M ≈ 0.15). This is consistent with the measured 0.08–0.17.
  - With U⁸, idle → WOT (×10–20 in U) raises jet-noise power by **80–100 dB**. With U⁶ (lip/obstruction dipoles) the rise is 60–80 dB.
  - So: flow noise ≈ absent at idle, a hissy broadband layer at WOT high rpm, and prominent on high-rpm overrun when pulses vanish but flow remains.
- Jet-noise spectral peak (textbook, cold subsonic jets): Strouhal St = fD/U ≈ 0.2–0.3. For 55 mm and 70 m/s this puts the peak at roughly 250–400 Hz, a broad hump of about ±2 octaves. Obstruction/perforate dipole noise peaks at St ≈ U_c/δ (Gordon), usually higher. Implement as noise → bandpass with centre ∝ U/D, gain ∝ U^6..U^8 (in linear power), **amplitude-modulated by the instantaneous pulse flow**. Exhaust flow is strongly pulsating, so turbulence noise is gated at firing rate, which gives the "chuff" texture.
- Convective effects in DWG: use different delays for forward and backward waves in each tube, N± = L·fs/(c ± u). The round-trip resonance shifts by the factor (1 − M²), which is small (< 3% at M = 0.17). A **tone-quality cue is the flow-dependent outlet |R|** (more low-frequency ringing at high flow) and the extra lip damping at low Strouhal.

### Gaps
- No published gasoline passenger-car tailpipe flow-noise spectra in dB(A) vs velocity were retrieved (SAE papers by Davies/Holland likely have them).

---

## 6. Tailpipe radiation, open-end reflection, directivity, listener position, cabin, ground, Doppler

### Takeaway
The open end is a strongly frequency-dependent boundary. At low ka it reflects almost everything (R ≈ −1, end correction 0.6133a) and radiates like a monopole whose far-field pressure rises about 6 dB/octave for a given in-pipe volume velocity (a high-pass). Reflection falls and radiation rises until ka ≈ 1–2. With flow, |R| is somewhat higher (≈ 1 + 2M) and the end correction is shorter. Assumptions of R = 0 or R = 1 are both "grossly in error" for exhaust prediction. Listener position then applies a further chain: directivity (above ka ≈ 1), ground comb filtering, distance/air absorption, Doppler for drive-by, and for the cabin, a strong low-pass body/firewall transfer plus structure-borne paths.

### Cited Findings
- Levine & Schwinger (1948): the exact unflanged-pipe end correction is **0.6133a**. This is the reference solution for |R|(ka) and the end correction. — [Phys. Rev. 73, 383](https://journals.aps.org/pr/abstract/10.1103/PhysRev.73.383); [Wikipedia: End correction](https://en.wikipedia.org/wiki/End_correction); [Silva et al., approximation formulae for radiation (arXiv 0811.3625)](https://arxiv.org/pdf/0811.3625)
- Alfredson & Davies compared four outlet boundary assumptions: (i) R = 0, (ii) R = 1 with 0 phase, (iii) Levine–Schwinger zero-flow R and phase, (iv) infinite tube. R = 0 is "grossly in error". R = 1 ignores radiation efficiency and gives large errors at high frequency. (iii) is the most appropriate, but slightly underestimates R with flow, so radiated SPL (∝ R²-dependent energy flux) is underestimated. The measured outlet phase angle stays close to the zero-flow prediction. — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- End correction with flow: 0.61a (high Strouhal) → 0.19a (low Strouhal). Vortex-shedding absorption at the lip. — [JFM open-pipe damping paper](https://www.cambridge.org/core/journals/journal-of-fluid-mechanics/article/abs/damping-and-reflection-coefficient-measurements-for-an-open-pipe-at-low-mach-and-low-helmholtz-numbers/176F64F8EDC5D7B2E8C2777D6FAA6FE2)
- Below cut-off, exit-plane acoustic transmissibility ∝ f² (for in-pipe sources); at high exit velocities this reflection/transmission influence weakens. — [Gordon NASA 1969](https://ntrs.nasa.gov/api/citations/19690002231/downloads/19690002231.pdf)
- Measurement setup in which overall radiated level was about 120 dB at 3 ft (0.9 m) from the outlet, with the outlet 6 ft above ground radiating into near-free space. A tailpipe measurement standard (orifice noise) uses 0.25 m at 45° from the orifice, with order tracking (2nd, 4th order for an I4). — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf); [flow-noise topic page incl. tailpipe measurement description](https://www.sciencedirect.com/topics/physics-and-astronomy/flow-noise)
- Interior noise: combustion and mechanical noise reach the cabin mostly as structure-borne noise. Intake/exhaust aerodynamic noise radiates outside and transmits into the cabin. One study found the structure-borne contribution significantly greater than airborne over run-up/run-down. — [ScienceDirect topic: Interior noise](https://www.sciencedirect.com/topics/engineering/interior-noise); [MSSP 2018: interior noise source identification via wavelet + partial coherence](https://www.sciencedirect.com/science/article/abs/pii/S0888327018301006); [Springer: Vehicle interior noise mechanism and prediction](https://link.springer.com/chapter/10.1007/978-981-19-5579-2_2)

### Inferences
- Textbook low-ka unflanged approximations (Levine–Schwinger fits): |R| ≈ 1 − ½(ka)² (roughly e^(−(ka)²/2)), phase ≈ π − 2k·(0.6133a). Examples:
  - 55 mm pipe (a = 27.5 mm) at 250 °C (c ≈ 455 m/s): ka = 1 at about 2.6 kHz. |R| ≈ 0.9 at ka ≈ 0.45 (≈ 1.2 kHz).
  - 100 mm "trumpet" tip at ambient: ka = 1 at about 1.1 kHz.
- DWG implementation of the open end:
  - Reflection filter H_R(z): a first- or second-order lowpass with DC gain −1 (+ M-dependent boost ≤ 1 + 2M), rolling off toward |R| ≈ 0.1–0.3 near ka ≈ 2, plus the end-correction delay 0.6a/c.
  - Radiated output = complementary filter 1 + H_R (the transmitted pressure). This is automatically a **first-order high-pass** at low ka and gives the classic "tailpipe = HPF" behaviour.
  - The standard Smith/CCRMA DWG approach for wind instruments applies unchanged. Using a scalar R here (current model) is a major cause of artificial sound: low partials decay too fast or too slowly, and the spectrum lacks the radiation tilt.
- Listener chain (textbook):
  - (1) Directivity: omni below ka ≈ 0.5, forward-lobed above ka ≈ 1. Off-axis listeners (in front of the car, in-cabin) lose high frequencies. Implement as a one-pole shelf whose corner depends on angle.
  - (2) Ground reflection: for a tailpipe at 0.3 m and mic at 1.2 m / 7.5 m (drive-by geometry), the path difference is ≈ 0.095 m, giving the first comb notch (hard ground) at about 1.8 kHz. Implement as one delayed copy with reflection factor 0.7–0.95. Asphalt ≈ rigid; grass low-passes the reflection.
  - (3) Distance: 1/r level, plus air absorption (a few dB/100 m at 4 kHz; minor for < 50 m).
  - (4) Doppler: f′ = f / (1 − (v/c)·cosθ). At 100 km/h the approach→recede swing is ×1.088 → ×0.925, about 2.8 semitones total. Implement with a variable fractional delay line driven by the source–listener distance r(t)/c. This gives both Doppler and correct propagation delay, plus the level "swoosh".
  - (5) Cabin:
    - Exhaust enters the cabin through rear floor/trunk/rear glass (mass-law transmission loss, rising about 6 dB/octave per panel, i.e. strongly low-pass) and through structure-borne paths (hangers, body).
    - Model as exhaust-radiated sound → low-pass (−3 dB around 150–400 Hz), plus cabin modes (first longitudinal mode about 50–70 Hz for a 2.5–3 m cabin), plus a "boom" resonance.
    - Engine block/intake reach the cabin through the firewall, again low-passed.
    - The cabin mix is much more dominated by low orders and by the engine block/intake than the exterior view is.
  - The exhaust and intake orifices are separate point sources at different positions, so they must be spatialized separately (differential delays → comb/phase effects that change with listener position).

### Gaps
- No measured car body transfer functions (exhaust orifice → driver ear, dB vs frequency) were retrieved. The numbers above are engineering estimates. SAE NVH papers on "exhaust tailpipe to interior noise transfer function" would provide them.
- No measured drive-by spectra (ISO 362) with order content retrieved.

---

## 7. Intake: airbox/snorkel resonances, throttle, intake orifice noise vs exhaust

### Takeaway
Intake noise comes from intake valve opening and closing. These create pressure waves in the runners, which travel to the airbox and snorkel and radiate from the snorkel orifice. Tuned Helmholtz side-branches (1–3 L) and quarter-wave tubes are used to kill low-frequency intake booms (< 120 Hz). The intake path is colder (c ≈ 343–360 m/s), its source is a suction pulse, and it is heavily throttled at part load. It is usually quieter than the exhaust at the tailpipe. It is relatively louder in the cabin/engine bay, and at WOT (throttle open) it gives the "induction roar".

### Cited Findings
- Pressure variation in the fresh charge of the induction system produces airborne noise that propagates to ambient through the inlet snorkel. The main source is the sudden opening and closing of intake valves, which launches waves through gas-column inertia. — [Applied Acoustics: insertion loss of a Helmholtz resonator in the intake system](https://www.sciencedirect.com/science/article/abs/pii/S0003682X00000426); [ScienceDirect topic: exhaust system/engine noise](https://www.sciencedirect.com/topics/physics-and-astronomy/exhaust-system)
- The Helmholtz resonator is a reliable, often the only practical, way to raise intake attenuation **below about 120 Hz**. Typical volume is **1–3 L** as a side branch on the snorkel, connected via a tuned pipe. — [ScienceDirect topic: Helmholtz resonator](https://www.sciencedirect.com/topics/engineering/helmholtz-resonator) (snippet; also consistent with the Applied Acoustics intake HR paper)
- Folded quarter-wave tubes can replace Helmholtz resonators in intake systems. Airbox volume, ducts and resonators are tuned with 1D and FEM models. — [Design of quarter wave tube for engine intake noise attenuation (FEA)](https://www.academia.edu/18452723/Design_of_Quarter_Wave_Tube_for_Engine_Intake_Noise_Attenuation_by_Finite_Element_Analysis); [1D acoustic study of an SI engine air intake](https://www.academia.edu/106169940/Acoustic_Study_of_an_Air_Intake_System_of_SI_Engine_using_1_Dimensional_Approach)
- PTR model includes intake pulsation noise ("valve dynamics and air column reversals") as a separate source. — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)
- Baldan et al. model intake, exhaust and engine-block vibration as separate waveguide/resonator branches. — [Baldan et al. 2015](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments); [Antonio-R1 implementation](https://github.com/Antonio-R1/engine-sound-generator)

### Inferences
- Intake DWG chain: per-cylinder runner (closed end when the valve is shut, R = +1) → plenum (a large volume, i.e. a Helmholtz-like junction) → throttle → airbox (expansion chamber, typically 5–10 L) → snorkel (tube, open end) + HR side branch.
  - Model the **throttle** as a variable area/loss junction. The small area at part load gives a high reflection and strong attenuation, so intake is nearly inaudible at cruise. WOT opens it and brings in induction roar.
  - Excitation is a negative (suction) pulse at IVO/IVC, weaker than blowdown.
- Runner/snorkel resonance numbers (textbook quarter-wave, c ≈ 350 m/s): 0.3 m runner → about 290 Hz. 0.5 m snorkel open-open → about 350 Hz. Plenum-runner Helmholtz for tuned intake is typically 60–150 Hz.
- Relative level: intake orifice is typically well below exhaust at the rear and comparable in the engine bay/front. Use a mix gain that rises with throttle opening.

### Gaps
- No measured intake orifice SPL vs tailpipe SPL for the same car was found. No KTH thesis was reached (search did not surface one).

---

## 8. Engine block / structure-borne radiation (combustion noise) for gasoline engines

### Takeaway
Combustion pressure-rise excitation and mechanical sources (piston slap, valvetrain, gears/belts) radiate from the block/covers mainly in the roughly 500 Hz–4 kHz range. They dominate the cabin via structure-borne paths. In gasoline engines this is weaker than in diesels because the pressure rise is gentler. Knock is the exception: cylinder pressure oscillation at the chamber modes (typically 5–10 kHz) makes a sharp "ping".

### Cited Findings
- Engine noise sources include exhaust, intake, fan, combustion, piston slap, accessories/belt and valvetrain. Combustion pressure changes load the cylinder head, liner and crank train, producing structural vibration. — [ResearchGate: Analyzing the sources of noise in IC engines](https://www.researchgate.net/publication/389432008_Analyzing_the_Sources_of_Noise_in_Internal_Combustion_Engines); [MSC: Understanding automotive engine noise](https://simulatemore.mscsoftware.com/understanding-automotive-engine-noise/)
- Combustion and mechanical noises are mostly structure-borne. Structure-borne contribution to interior noise exceeded airborne across run-up/run-down in one study. — [ScienceDirect topic: Interior noise](https://www.sciencedirect.com/topics/engineering/interior-noise)
- Exhaust noise at low frequency is governed by the working cycle, with harmonics of firing frequency dominating up to about 1 kHz. — [ScienceDirect topic: Exhaust system](https://www.sciencedirect.com/topics/physics-and-astronomy/exhaust-system)
- Diesel power-group source identification: [ICA 2019, dominant noise sources in a diesel power group](https://pub.dega-akustik.de/ICA2019/data/articles/000158.pdf) (not read in detail)

### Inferences
- For synthesis: add a "block" branch in which each combustion event excites a bank of modal resonators (engine-structure modes, roughly 0.5–4 kHz, Q 10–50). Scale gain with load (combustion pressure-rise rate) and add a small per-event random variation (cycle-to-cycle variability). This supplies the "mechanical" content that exhaust-only DWG lacks, especially for engine-bay and cabin perspectives. Baldan et al. use a similar engine-block vibration branch.
- Mechanical ticks (valvetrain at camshaft rate, injector clicks for GDI engines) are short high-frequency transients. They are cheap (noise bursts through bandpass) and add much realism at idle.
- Level: exterior at the rear is exhaust-dominated. Beside/in front of the car and in the cabin, block + intake become comparable or dominant. Gasoline block noise is typically not dominant at the tailpipe. (Engineering judgement; not sourced.)

### Gaps
- No quantitative gasoline block sound-power vs exhaust orifice level comparison (dB) was found. The Priede / Anderton combustion-noise scaling laws (diesel) were not retrieved.

---

## 9. Published measured tailpipe spectra for calibration

### Takeaway
The one fully readable classic dataset (Alfredson & Davies, 6-cyl 2-stroke diesel, expansion-chamber silencer) shows discrete firing harmonics from very low frequency up to a few kHz. The largest amplitudes are in **100–1000 Hz**, and broadband jet noise is insignificant. Overall level is about 120 dB at 0.9 m. In-pipe levels are about 155 dB at 300 Hz and 135 dB at 1 kHz. Overall level rises with speed at both no-load and full-load, with whole-system resonance peaks vs rpm.

### Cited Findings
- Spectrum at 1880 rpm (fundamental 31.4 Hz) at 3 ft from outlet: extends from very low frequencies to a few kHz, with the largest amplitudes in the 100–1000 Hz region. Narrow-band (3 Hz) analysis shows all components are harmonics of the fundamental (crank rate for 2-stroke), up to kHz. "Measurements of a number of other systems confirm that this behaviour is typical." — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Overall and A-weighted SPL increase with speed at both no load and full load. At full load there are louder resonances at 1150 and 1650 rpm, typical of reactive silencers. — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- In-pipe maximum about 155 dB at 300 Hz and about 135 dB at 1 kHz. Radiated about 120 dB at 3 ft. — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- For a 4-cyl 4-stroke engine, orifice noise is typically reported as overall + **2nd and 4th order** at 0.25 m, 45°, A-weighted and unweighted. 1D CFD matches orders below 1 kHz but misses flow noise. — [ScienceDirect topic: Flow noise](https://www.sciencedirect.com/topics/physics-and-astronomy/flow-noise); [Frontiers Energy Research 2022](https://www.frontiersin.org/journals/energy-research/articles/10.3389/fenrg.2022.873749/xml)
- PTR dataset: 7.5 h of audio over three engine types; rpm range 600–8000+; fundamentals 10–133 Hz (crank rate), down to 5 Hz (half-order); pulse intervals below 2 ms at high rpm. Code and model weights are public. — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1); [project page](https://rdoerfler.github.io/ptr-model-page/)

### Inferences
- Calibration targets for a 4-stroke:
  - Firing frequency f_fire = N_cyl·rpm/120. I4 idle at 800 rpm → 26.7 Hz (order 2). V8 idle at 700 rpm → 46.7 Hz (order 4), with half-orders at 5.8 Hz spacing for cross-plane per-bank.
  - Exterior tailpipe spectrum: firing-order harmonics dominant to about 1 kHz. Envelope peaks around 100–500 Hz shaped by muffler domes/pass-bands. Roll-off above about 1–2 kHz from dissipative elements, partly offset by radiation high-pass. Broadband floor rising strongly with load/rpm.
  - A practical calibration: record the target car at 0.5 m/45° (orifice) at idle and at 3–4 steady rpm/load points. Fit (a) per-order level vs rpm (order-tracking) and (b) the 1/3-octave broadband residual.
- The PTR public model/data could serve as a reference for per-order envelopes.

### Gaps
- No gasoline passenger-car tailpipe order spectra with numbers (e.g. dB per order vs rpm) were readable in this session. SAE papers on "exhaust orifice noise" and "sound quality of exhaust" by OEMs/Tenneco/Eberspächer are the likely sources.

---

## 10. Established modeling tools and methods; what is cheap enough for real time

### Takeaway
Industry uses 1D nonlinear gas dynamics (GT-Power, Ricardo WAVE; method of characteristics or finite-volume). These handle blowdown, steepening, temperature and flow, and match measured engine orders below about 1 kHz. They are far too expensive for real time and miss flow noise. Linear transfer-matrix (two-port) muffler models are cheap offline, and 1D suffices for catalysts and most silencers. For real time, a **linear DWG network with the right topology and frequency-dependent/time-varying elements** is the practical sweet spot. Add small targeted nonlinear or flow-noise modules on top.

### Cited Findings
- Method of characteristics computes finite-amplitude exhaust wave action (gas exchange, turbocharger matching). It was "expensive, even for simplified systems" in 1970. — [Alfredson & Davies 1970](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf); [NASA NTRS: two-stroke exhaust noise by MoC](https://ntrs.nasa.gov/citations/19820005954)
- 1D CFD (GT-Power/WAVE class) agrees with measured engine orders below 1 kHz, but flow noise is not included. — [Frontiers Energy Research 2022](https://www.frontiersin.org/journals/energy-research/articles/10.3389/fenrg.2022.873749/xml)
- Transfer-matrix + Wave 1D is used for muffler TL/perforate optimization. Catalyst two-ports (honeycomb capillary) are established. A 1D analytical approach is sufficient for catalysts in the automotive range. — [Springer TMM + Wave 1D](https://link.springer.com/chapter/10.1007/978-981-92-0694-0_46); [Selamet et al. 1998](https://mae.osu.edu/sites/default/files/2021-12/J23.pdf)
- Baldan et al. 2015: physically informed real-time car engine synthesis for virtual/augmented environments. It uses a DWG for the exhaust (muffler = 4 partially reflecting waveguides), plus intake and engine-block branches, and was evaluated with users. — [ResearchGate](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments); [block diagram figure](https://www.researchgate.net/figure/Block-diagram-of-the-digital-waveguide-implemented-in-the-current-version-of-the-engine_fig4_280086598)
- Yamaha patent (US5835605, 1998): exhaust DWG with delay circuits and scattering junctions. — [Google Patents](https://patents.google.com/patent/US5835605A/en)
- Differentiable Pulse-Train-Resonator (2026): pulses per cylinder → two bank Karplus-Strong resonators → shared resonator. The loop filter is h = α·y[n−L] + β·y[n−L−1]; reflection coefficients are tanh-constrained. It adds DFCO/throttle modes and a noise bank, and reports +21% harmonic reconstruction over baselines. — [arXiv 2603.09391](https://arxiv.org/abs/2603.09391)
- Other related work: Farnell "Designing Sound" (2010), Lundberg 2020 (neural procedural engine), Motor2Synth (DDSP, AES 2025), Heitbrink & Cable 2007 (driving-simulator sound engine). — cited in [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)
- Survey of DWG methods: [Four Decades of Digital Waveguides, arXiv 2604.12878](https://arxiv.org/pdf/2604.12878)

### Inferences — prioritized upgrade list for the current 3-tube fixed-R model (ranked by expected perceptual gain per CPU)
1. **Frequency-dependent open-end filter** (Levine–Schwinger fit, DC −1, lowpass magnitude, end-correction delay) with radiated output = transmitted part (high-pass). Replace scalar R.
2. **Per-cylinder / per-bank topology**: separate runners with individual lengths feeding N-port collector junctions; dual-bank trees with optional X/H junction. This supplies half-orders, burble and rumble for free.
3. **Temperature-driven fractional delays** per segment with thermal lag (fast header, slow tailpipe), plus overrun cooling. Resonances then glide with load and history.
4. **Proper muffler sub-network**: area-step junctions + chamber delay + quarter-wave extended tubes/side branches + per-round-trip dissipative lowpass (perforate/packing), plus a catalyst lowpass. Use at least 2–3 incommensurate loops so there are no obvious comb pitches.
5. **Load-dependent excitation**: a PTR-style envelope; rise time sharper with load; volume-velocity source with valve-state-dependent port reflection. Optional header steepening nonlinearity at high load.
6. **Flow-noise layer**: bandpass noise (centre ∝ U/D), power ∝ U^6–U^8, AM-gated by the pulse flow. Keep it active on overrun without combustion.
7. **Cycle-to-cycle variation**: a few % random per-event amplitude and timing jitter (combustion variability). A fixed periodic source is a major "synthetic" giveaway. (Engineering judgement.)
8. **Listener chain**: directivity shelf, ground reflection, Doppler via variable delay, separate intake/exhaust/block source positions, and a cabin low-pass + modal boom for the interior view.
9. **Loss filters**: wall/visco-thermal loss scales ∝ √f per metre (textbook Kirchhoff). Commute the loss of each tube into one filter per delay line with a √f-like slope (a one-pole shelf is enough). Wall loss is minor compared with mufflers and the open end, so don't over-invest.

- CPU estimate (my own): about 20–40 fractional delay lines + about 40 one/two-pole filters + a noise bank of about 8 bands per engine, at 48 kHz. That is well under 1% of one modern core. A per-cylinder MoC/finite-volume solver at audio rate would be orders of magnitude more expensive. A hybrid is possible: run a coarse 1D gas-dynamics model at control rate (e.g. 1 kHz) for T, u and mean pressures, and drive the DWG parameters from it.

### Gaps
- The Baldan et al. full text (exact filter designs and reflection values) could not be fetched (403).
- No accessible DAFx paper specifically on a physically modelled car exhaust with nonlinear propagation was found. The DAFx 2013 jackhammer paper surfaced but is off-topic.
- GT-Power/WAVE solver specifics (time step, cost) were not retrieved.
