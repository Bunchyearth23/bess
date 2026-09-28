# 0D (single-zone) cylinder thermodynamics and combustion for a real-time, sound-producing engine simulator

Scope note: equations marked **[textbook]** are the standard forms from Heywood, *Internal Combustion Engine Fundamentals* (McGraw-Hill 1988) and similar texts. They were written from domain knowledge and were **not** re-checked against an online copy in this session. Numbers marked **[estimate]** are engineering rules of thumb with no source found here. Everything else has an inline source. Worked numbers (Wiebe fractions, knock frequencies, step sizes) were computed with a short Python check in this session.

---

## 1. Slider-crank kinematics: V(θ), piston velocity and acceleration

### Takeaway
The volume comes straight from the exact slider-crank position x(θ). With crank radius r = S/2, rod length l, bore B, clearance volume Vc = Vd/(CR−1) and piston area Ap = πB²/4, you get V(θ) = Vc + Ap·(l + r − x(θ)). dV/dθ comes analytically from x′(θ), so it never needs numerical differentiation.

### Cited Findings
- Piston pin position measured from the crank axis: x = r·cos A + √(l² − r²·sin²A). Its first derivative is x′ = −r·sin A − r²·sin A·cos A / √(l² − r²·sin²A). The second derivative is x″ = −r·cos A − r²(cos²A − sin²A)/√(l² − r² sin²A) − r⁴ sin²A cos²A/(l² − r² sin²A)^{3/2}. Piston motion is *not* simple harmonic. — [Wikipedia: Piston motion equations](https://en.wikipedia.org/wiki/Piston_motion_equations)

### Inferences
- **[textbook]** Implementation set:
  - Vd = Ap·S, Vc = Vd/(CR − 1), R_rod = l/r (typical 3–4).
  - V(θ) = Vc + Ap·(l + r − x(θ)), with θ = 0 at TDC. Equivalent Heywood form: V/Vc = 1 + ½(CR−1)[R_rod + 1 − cos θ − √(R_rod² − sin²θ)].
  - dV/dθ = −Ap·x′(θ) (per radian).
  - Instantaneous piston speed is v_p = ω·(−x′(θ)). Mean piston speed is S̄p = 2·S·N (N in rev/s). Real engines stay at or below about 20–25 m/s **[estimate]**. The simulator must not crash when a user goes beyond that.
- For badly designed engines, clamp R_rod > 1 plus a margin. If l ≤ r the square root goes imaginary, which means the geometry cannot exist. Also reject CR ≤ 1.
- The wrist-pin offset (desaxe) can be ignored for sound.
- The same x′(θ) is the lever arm for gas torque (see §8): T_gas = −(p − p_crankcase)·Ap·x′(θ).
- Per-cylinder phase for a 4-stroke is θ_i = θ_crank − φ_i (mod 720°), with φ_i taken from the firing order. Uneven firing, such as a cross-plane V8 on each bank or a big-bang layout, falls out of this for free.

### Gaps
- None substantive. The kinematics are exact geometry.

---

## 2. Single-zone first-law energy equation (open system) and variable γ / specific heats

### Takeaway
Integrate mass m and temperature T (or internal energy U) of the cylinder charge per step, and derive p = m·R·T/V from them. Include the enthalpy flows through the valves. Use a temperature-dependent γ(T) or c_v(T) rather than a constant. A constant γ of about 1.3 visibly distorts the pressure trace and the heat release, and it breaks energy conservation over many cycles.

### Cited Findings
- A crank-angle-resolved first-law balance per cylinder, integrated over 720° with a 0.5–1° step, has the form du/dθ = (−p dV/dθ + dQ_comb/dθ − dQ_wall/dθ + Σ ṁ_i h_i/ω)/m, with valve flows from compressible-flow equations and lift given on a crank-angle basis. — [search summary of jenilclaudeai/diesel-simulator (GitHub) and related papers](https://github.com/jenilclaudeai/diesel-simulator) (low-authority source, but the form matches the textbook).
- Zero-dimensional or single-zone models use a predefined mass burning rate, most commonly the Vibe (Wiebe) law. — [ScienceDirect, multi-zone SI modelling overview](https://sciencedirect.com/science/article/abs/pii/S0196890409000132)
- Brunt et al. temperature-dependent γ: γ = 1.338 − 6.0×10⁻⁵·T + 1.0×10⁻⁸·T² (T in K). The source page prints "60×10⁻⁵", but that coefficient gives γ ≈ 0.75 at 1000 K, which is physically impossible, so it is a typo for 6.0×10⁻⁵. With the corrected coefficient, γ(300 K) ≈ 1.321, γ(1000 K) ≈ 1.288 and γ(2500 K) ≈ 1.25. — [xarin catoolRT: Ratio of specific heats](https://www.xarin.com/cylinder-pressure-analysis/ratio-of-specific-heats.html)
- A γ that is too low makes calculated heat release too high and makes the heat-release rate negative after combustion ends. Ignoring the dependence on φ causes errors of up to ±0.015 in γ for 0.8 < φ < 1.2. — [xarin catoolRT](https://www.xarin.com/cylinder-pressure-analysis/ratio-of-specific-heats.html)
- Zucrow–Hoffman polynomial c̄p/R̄ (air-like gas):
  - T < 1000 K: 3.6359 − 1.33736e-3·T + 3.29421e-6·T² − 1.91142e-9·T³ + 0.275462e-12·T⁴.
  - T > 1000 K: 3.04473 − 1.33805e-3·T (sign as quoted) − 0.488256e-6·T² + 0.0855475e-9·T³ − 0.00570132e-12·T⁴.
  - γ = c̄p/(c̄p − R̄).
  - — [xarin catoolRT](https://www.xarin.com/cylinder-pressure-analysis/ratio-of-specific-heats.html). Caution: the sign on the T > 1000 K linear term looks suspicious (NASA-style fits usually have a positive linear coefficient there). Verify before use.
- Temperature- and AFR-dependent γ functions exist for lean burned and unburned mixtures. — [ScienceDirect: T- and AFR-dependent specific heat ratio functions](https://www.sciencedirect.com/science/article/abs/pii/S019689040500018X)

### Inferences
- **[textbook]** Open-system single zone. State (m, T, composition scalar x_b or residual fraction), ideal gas p = m·R_mix·T/V:
  - Mass: dm/dt = Σ ṁ_in − Σ ṁ_out (valve flows, signed; see §5). Optionally add blow-by and crevice terms. They are negligible for sound.
  - Energy: m·c_v·dT/dt = dQ_comb/dt − dQ_wall/dt − p·dV/dt + Σ_in ṁ_in·(h_in − u) − Σ_out ṁ_out·(h − u) − m·(∂u/∂x_b)·dx_b/dt (last term only if composition-dependent u is tracked).
  - For outflow, h − u = R·T, so the term becomes −ṁ_out·R·T. For inflow, h_in is evaluated at the upstream (port or manifold) stagnation temperature. **Backflow (reversion) enters with cylinder-gas enthalpy into the port, then re-enters later with port-gas enthalpy.** Track the port or runner gas temperature and composition if reversion matters for the sound (idle with big overlap).
- **[textbook]** Closed-period shortcut (IVC → EVO), no mass flow: dp/dθ = [(γ−1)/V]·(dQ_comb/dθ − dQ_wall/dθ) − γ·(p/V)·dV/dθ. With variable γ, integrating T via energy is cleaner than integrating p directly, because it avoids the dγ/dθ correction term.
- Practical two-species single zone for real time:
  - Treat the gas as a blend of "unburned" (air + fuel vapour + residual) and "burned".
  - Use R ≈ 287 J/kg/K for both **[estimate]**: burned gas is about 285–295, and the fuel changes molar mass slightly.
  - c_v(T) from γ(T) = Brunt, or a linear γ(T) = γ₀ − k·T with separate coefficients for unburned and burned gas. Mix with x_b.
  - This captures the effect that matters for sound: expansion and blowdown pressure are lower than a constant-γ = 1.4 model would give.
- Fuel energy release: Q_total = η_comb · m_fuel · Q_LHV. Gasoline Q_LHV ≈ 43–44 MJ/kg and stoichiometric AFR ≈ 14.6–14.7 **[estimate/textbook]**. Rich mixtures cap burned fuel by available O₂: effective m_fuel,burnable = m_air/AFR_st, so the extra fuel does not add energy.
- Numerical-robustness guards for "bad" engines: floor T (for example 200 K), m and p. Cap T at about 3500 K (dissociation, which a single zone does not model). Limit per-step |ΔT|.

### Gaps
- No online source verified the Gatowski linear γ(T) coefficients. (The Linköping thesis "A specific heat ratio model and compression ratio estimation" by M. Klein, 2004, contains them, but the fetch failed.)
- Frozen-composition burned-gas properties with dissociation (for example, Heywood's charts) were not retrieved.

---

## 3. Combustion: Wiebe, spark timing / MBT, retard/advance effects, lean/rich limits, misfire, knock (Livengood–Wu) and knock sound

### Takeaway
Use the Wiebe function x_b(θ) = 1 − exp[−a((θ − θ_soc)/Δθ)^(m+1)] with a = 5 and m = 2. Place the start of combustion after the spark via an ignition delay. Scale Δθ with rpm, load, AFR and dilution. MBT is reached when CA50 is about 8–10° ATDC, with peak pressure about 15° ATDC. Knock is predicted with a Livengood–Wu integral of the Douaud–Eyzat delay applied to an end-gas temperature. When knock fires, superimpose damped cylinder acoustic modes f_mn = α_mn·c/(π·B). The **first circumferential mode** (α = 1.841) is the dominant one, at about 5–8 kHz for car bores. The first *radial* mode is α = 3.832, not 1.84.

### Cited Findings
- Typical Wiebe constants are a ≈ 5 ("efficiency parameter") and m ≈ 2 ("form factor"). The function depends on crank angle, start of combustion and combustion duration Δφ_com. — [ScienceDirect Topics: Wiebe function](https://www.sciencedirect.com/topics/engineering/wiebe-function); [Wiebe parameter determination, ethanol-gasoline SI engine (ResearchGate)](https://www.researchgate.net/publication/292793252_Wiebe_function_parameter_determination_for_mass_fraction_burn_calculation_in_an_ethanol-gasoline_fuelled_SI_engine)
- Double-Wiebe fits are used across compression ratio and EGR levels, and for HCCI. — [ScienceDirect: double-Wiebe, ethanol-gasoline blends, CR & EGR](https://www.sciencedirect.com/science/article/abs/pii/S1359431111000640); [Double-Wiebe single-zone HCCI](https://www.researchgate.net/publication/245213448_Double-Wiebe_function_An_approach_for_single-zone_HCCI_engine_modeling)
- At MBT timing, CA50 is about 8–10° ATDC and peak cylinder pressure location is about 15° ATDC. Across "massive testing data", CA50 is mostly 7–9° ATDC. — [US patent 7086382 (MBT estimation by ionization)](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7086382)
- Livengood–Wu: integrate 1/τ(p, T) of the end gas from IVC. Knock occurs when the integral reaches 1. — [ScienceDirect: predictive knock-onset model with cooled EGR](https://www.sciencedirect.com/science/article/abs/pii/S0196890414007262)
- Douaud–Eyzat ignition delay: τ = C1·(ON/100)^C2·p^−C3·exp(C4/T), with C1 = 17.68 (quoted as 17.69 in one source), C2 = 3.402, C3 = 1.7 and C4 = 3800. It was fitted on primary reference fuels (iso-octane/n-heptane). — [same source](https://www.sciencedirect.com/science/article/abs/pii/S0196890414007262); [core.ac.uk quasi-dimensional SI model, gasoline-alcohol](https://fileserver-az.core.ac.uk/download/55710732.pdf). Units are usually τ in ms, p in atm and T in K **[textbook, verify]**.
- Knock pressure oscillations match the cylinder "drum modes" described by Draper (1938). Frequencies come from Bessel-derived mode factors α_m,n, speed of sound and bore. Peaks lie in the 6–20 kHz range depending on mode. The first radial mode (0,1) has α = 3.832. The value 1.841 belongs to the first *circumferential* mode (1,0). — [Knocking and combustion noise analysis (Springer)](https://link.springer.com/chapter/10.1007/978-3-030-11954-6_9); [Knocking combustion in SI engines, PECS 2017](https://www.sciencedirect.com/science/article/pii/S0360128516300764)
- Engine-sim (a real-time, audio-oriented simulator) only lets the charge ignite for equivalence ratios between 0.5 and 1.9. Its flame speed depends on turbulence, AFR, temperature and pressure. Turbulence comes from mean piston speed. A mixing factor of the form 1 − clamp(turb/maxTurbEffect)·clamp(1 − dilution/maxDilutionEffect) is used, plus a uniform random term on burning efficiency. — [engine-sim src/combustion_chamber.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/combustion_chamber.cpp)

### Inferences
- **Wiebe convention trap.** Heywood writes the exponent as (m+1) with m = 2, so the exponent is 3. Some texts (for example Ferguson/Kirkpatrick) write exponent n with n = 3. It is the same curve, so check which convention a source uses. With a = 5 and m = 2 (computed):
  - x_b reaches 99.3% at θ_soc + Δθ.
  - CA10 = θ_soc + 0.276Δθ, CA50 = θ_soc + 0.518Δθ, CA90 = θ_soc + 0.772Δθ.
  - The 10–90% duration is 0.496·Δθ.
  - Burn rate: dx_b/dθ = a(m+1)/Δθ·((θ−θ_soc)/Δθ)^m·exp[−a((θ−θ_soc)/Δθ)^(m+1)].
- **Burn duration model for arbitrary engines [estimate]:**
  - Δθ(0–100%) of about 40–70°CA is typical: shorter at part-to-high load near stoichiometry, longer at idle, lean or with high EGR.
  - Duration in *crank degrees* grows only weakly with rpm (turbulent flame speed scales roughly with piston speed). A usable form is Δθ = Δθ_ref·(N/N_ref)^k with k ≈ 0.3–0.5.
  - Multiply by an AFR factor that is minimal near φ ≈ 1.1 (λ ≈ 0.9) and rises steeply lean.
  - Multiply by a dilution factor of roughly (1 + c·x_residual) with c ≈ 2–3.
  - Multiply by a bore factor (bigger bore gives longer flame travel), for example ∝ (B/B_ref)^~0.5–1.
  - Flame-development delay from spark to about 1–2% burned: roughly 10–25°CA **[estimate]**.
- **MBT for user-designed engines.** Solve for the spark advance that puts CA50 at about 8° ATDC given the current Δθ, since CA50 = spark + delay + 0.518·Δθ. This gives the engine a sensible default map automatically. The user's own spark setting then deviates from it.
- **Retarded spark [estimate/textbook]:**
  - Torque falls slowly near MBT: a few degrees costs about 1–2%. Far retard loses much more.
  - Combustion runs into the expansion stroke, so EVO pressure and temperature rise. The exhaust gets hotter and the blowdown pulse louder and "crackly"; late burning in the exhaust gives pops and bangs.
  - Peak pressure falls, so the combustion "bark" in the cylinder is softer.
- **Over-advanced spark:** peak pressure rises and moves toward TDC, negative work grows during compression, torque drops, and knock onset becomes likely because the end gas stays hot and pressurised longer.
- **Knock implementation:**
  - Unburned end-gas temperature: from IVC, use polytropic compression of the unburned gas, T_u = T_IVC·(p/p_IVC)^((γ_u−1)/γ_u), with γ_u ≈ 1.35 **[estimate]**.
  - Integrate I += dt/τ_DE(p, T_u) from IVC. If I ≥ 1 while 0 < x_b < ~0.9, knock occurs. Burn the remaining (1 − x_b) fuel almost instantly (a spike of heat release).
  - Trigger a sum of exponentially damped sinusoids for the pressure acoustic modes. Amplitude ∝ unburned mass fraction at onset; decay about 1–3 ms **[estimate]**.
  - Hot burned gas has c ≈ 950–1000 m/s (√(1.3·287·2500) ≈ 966 m/s). Computed frequencies:

    | Bore | f(1,0) | f(2,0) | f(0,1) | f(3,0) |
    |---|---|---|---|---|
    | 86 mm | 6.6 kHz | 10.9 kHz | 13.7 kHz | 15.0 kHz |
    | 70 mm | 8.1 kHz | 13.4 kHz | 16.8 kHz | 18.5 kHz |
    | 100 mm | 5.7 kHz | 9.4 kHz | 11.8 kHz | 12.9 kHz |
    | 130 mm | 4.4 kHz | 7.2 kHz | 9.1 kHz | 9.9 kHz |

  - Because T falls during expansion, c and the frequency drift down during the ring-down.
  - At 48 kHz sample rate everything below 24 kHz is representable. Sub-stepping (§9) is needed anyway for the ODE.
- **Misfire and lean/rich limits [estimate]:**
  - Flame-kernel failure likely below φ ≈ 0.6–0.7 (λ > 1.4–1.6) for homogeneous gasoline, or above about 25–30% total dilution (residual + EGR).
  - Rich limit about φ ≈ 1.6–1.9. Engine-sim's hard gate is 0.5–1.9 (cited above).
  - Model it as an ignition probability that goes smoothly from 1 to 0 across the limit band. Also allow partial burns: the Wiebe duration stretches so far that the burn is frozen at EVO, and the unburned fuel goes out the exhaust (afterfire/backfire source).

### Gaps
- No primary source was retrieved for quantitative torque-loss-vs-retard curves, the exhaust-temperature rise per degree of retard, or a validated Δθ(rpm, load, φ, dilution) correlation. The numbers above are estimates.
- Knock ring-down damping and amplitude scaling vs unburned mass fraction were not sourced. The PECS review (403 on fetch) likely covers them.
- The Douaud–Eyzat unit conventions (ms / atm) are from memory.

---

## 4. Heat transfer (Woschni / Hohenberg) and whether it matters for sound

### Takeaway
Woschni with C1 = 2.28 (compression/combustion/expansion) or 6.18 (gas exchange) and C2 = 3.24×10⁻³ (combustion/expansion only) is the standard choice. Hohenberg (130·V^−0.06·p^0.8·T^−0.4·(S̄p + 1.4)^0.8) is simpler and does not need a motored reference pressure. For sound, heat transfer is a second-order amplitude and brightness effect. It is cheap to add and it improves motoring and engine-braking realism, so it is worth a coarse implementation.

### Cited Findings
- Woschni gas-velocity coefficients by phase: combustion/expansion C1 = 2.28, C2 = 0.00324; intake/exhaust C1 = 6.18, C2 = 0; compression C1 = 2.28, C2 = 0. The swirl-corrected version uses C1 = 6.18 + 0.417·c_u/c_m for gas exchange and 2.28 + 0.308·c_u/c_m for the rest of the cycle. — [Effect of different heat transfer models on a diesel HCCI engine (ResearchGate)](https://www.researchgate.net/publication/260134113_Effect_of_Different_Heat_Transfer_Models_on_A_Diesel_Homogeneous_Charge_Compression_Ignition_Engine); [Annals FIH, "Revising engine heat transfer" 2008](https://annals.fih.upt.ro/pdf-full/2008/ANNALS-2008-3-45.pdf)
- Hohenberg: h = C1·V^−0.06·p^0.8·T^−0.4·(C2 + S̄p)^0.8 with C1 = 130 and C2 = 1.4. It uses instantaneous volume instead of bore. Hohenberg argued Woschni under-predicts during compression and over-predicts during combustion. — [search summary citing Hohenberg comparison literature](https://www.researchgate.net/figure/Comparisons-of-Han-Woschni-and-Hohenberg-heat-transfer-correlations-for-the-spark_fig2_228855636); [techno-office: Heat transfer in IC engines](http://www.techno-office.com/file/TCS_Paper1.pdf)
- Engine-sim uses a much cruder wall cooling term, roughly ∝ (90 °C − T_gas)·A_surface·100·dt. — [engine-sim combustion_chamber.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/combustion_chamber.cpp)

### Inferences
- **[textbook]** Woschni full form: h [W/m²K] = 3.26·B^−0.2·p^0.8·T^−0.55·w^0.8, with B in m, p in kPa, T in K. Here w = C1·S̄p + C2·(V_d·T_r/(p_r·V_r))·(p − p_mot). r = reference state (IVC), and p_mot is the motored pressure, which can be approximated as p_r·(V_r/V)^γ.
- Wall heat loss: dQ_w/dt = h·A_wall·(T − T_wall). A_wall = 2·(πB²/4)·(head + piston, or more for a pent-roof) + πB·(stroke-exposed height). T_wall ≈ 400–450 K for liner and head, and about 500–600 K for the piston crown **[estimate]**.
- Magnitude: in-cylinder heat loss is roughly 15–30% of fuel energy at low load, less at high load **[estimate/textbook]**. For sound it slightly lowers peak and expansion pressure and the blowdown pulse. Its most audible role is during **motoring/overrun (fuel cut)**, where it produces the compression-expansion hysteresis (a loss loop). Without it, a fuel-cut engine is a lossless gas spring.
- Hohenberg is preferable for a real-time loop. It needs no motored-pressure bookkeeping, which gets ambiguous with arbitrary cam timing and backflow. Evaluate h once per sub-step. It is a cheap power-law.

### Gaps
- No quantitative study was found on how much heat-transfer fidelity changes perceived engine sound. The claim that it is second-order is an inference.

---

## 5. Valve gas exchange: orifice flow, Cd vs L/D, lift profiles, overlap, reversion, throttle area

### Takeaway
Each valve and the throttle is a quasi-steady compressible orifice: ṁ = Cd·A_ref·f(p₀, T₀, p_T). It is choked when p_T/p₀ ≤ (2/(γ+1))^(γ/(γ−1)), which is 0.528 at γ = 1.4 and 0.546 at γ = 1.3. The reference area is the curtain area π·Dv·L up to about L/Dv ≈ 0.25, then the port/throat area. Cd is about 0.55–0.7, falling with lift. Flow direction comes from the pressure sign, which gives reversion automatically.

### Cited Findings
- Poppet-valve mass flow is usually described with the Heywood (1988) compressible flow-restriction equation. The flow area depends on lift, seat angle and inner/outer diameters (Blair 1999). — [ENCIT 2012, steady discharge coefficient in ICE](https://abcm.org.br/anais/encit/2012/links/pdf/ENCIT2012-158.pdf)
- Cd is larger at low lift than at high lift. At low lift Cd depends on pressure drop (Reynolds number): a 4.6% change at 1 mm lift, 0.9% at 2.5 mm. For L/Dv ≈ 0.12–0.15, Cd is practically independent of Reynolds number. At high lift Cd decreases roughly linearly with lift (Weclas et al. 1998). — [ENCIT 2012](https://abcm.org.br/anais/encit/2012/links/pdf/ENCIT2012-158.pdf)
- Measured Cd for a two-stroke poppet-valve engine: 0.32 and 0.26 at L/D = 0; 0.54, 0.61 and 0.57 at L/D = 0.05, 0.10 and 0.15. — [IOP Conf. Ser., the two-stroke poppet valve engine, Part 1](https://iopscience.iop.org/article/10.1088/1757-899X/257/1/012023/pdf)
- Valve models often take Cd = 0.7 as a constant. A square-edged orifice in diesel exhaust showed Cd = 0.60–0.90 for critical and near-critical flow. The quasi-steady constant-Cd assumption is inaccurate for strongly pulsating flow at large amplitude and low flow rate. — [Frontiers Mech. Eng. 2019, discharge coefficients in pulsating flows](https://www.frontiersin.org/journals/mechanical-engineering/articles/10.3389/fmech.2019.00025/full)
- At low lift (about 10% of head diameter), curtain area is the bottleneck and Cd is near 0.6. At about 25% of head diameter the curtain area equals the port area (the textbook design point). — [firgelliauto poppet valve explainer](https://www.firgelliauto.com/blogs/mechanisms/poppet-valve) (low-authority source, consistent with Heywood)
- Simplified throttle projected area for a thin elliptical plate: A = (πD²/4)·(1 − cos θ·(…)θ_closed), plus a more complex form accounting for shaft blockage with a = shaft diameter / bore. — [US patent 6591667 (throttle flow)](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6591667) (the search summary rendering of the formula looks garbled; see Inferences)

### Inferences
- **[textbook, Heywood App. C]** Orifice flow from upstream stagnation state (p₀, T₀) to downstream static p_T:
  - Subsonic (p_T/p₀ > crit): ṁ = Cd·A_R·p₀/√(R·T₀)·(p_T/p₀)^(1/γ)·√{2γ/(γ−1)·[1 − (p_T/p₀)^((γ−1)/γ)]}
  - Choked: ṁ = Cd·A_R·p₀/√(R·T₀)·√γ·(2/(γ+1))^((γ+1)/(2(γ−1)))
  - Take "upstream" as whichever side has the higher pressure. The sign gives the flow direction, so reversion and backflow are automatic.
  - For numerical smoothness near p_T ≈ p₀, the square-root form has infinite slope at zero Δp. Linearise for |1 − p_T/p₀| < ~1e-3 **[estimate]** to avoid chatter or limit cycles at the audio rate.
- **[textbook/estimate] Effective area vs lift:**
  - A_curtain = π·Dv·L. For L/Dv < ~0.125, a seat-angle refinement is A = π·L·cos β·(Dv + (L/2)·sin 2β) with β = 45°.
  - Cap at A_port = π/4·(Dp² − Dstem²).
  - Cd(L/Dv) table referenced to curtain area: 0 → 0.3; 0.05 → 0.55; 0.10 → 0.62; 0.15 → 0.60; 0.25 → 0.55; ≥0.3 → 0.5, with the port area cap then active.
  - This is consistent with the cited measurements, and it gives "≈0.6–0.7 at moderate L/D". Exhaust valves have slightly lower Cd at low lift, especially in the reverse direction.
- **Cam lift profiles [estimate; no source retrieved]:**
  - Simple smooth lobe: L(φ) = L_max·½·(1 − cos(2π·(φ − φ_open)/D_adv)) over the advertised duration D_adv, where φ is cam-referenced crank angle. Better: a 3-4-5 or 4-5-6-7 polynomial (C²/C³ continuous) plus a ramp (lash) of about 0.1–0.3 mm.
  - Typical street cam: 200–230° duration at 0.050" (1.27 mm) lift, advertised duration about 40–60° longer, L_max ≈ 9–13 mm, lobe separation angle 106–116° (cam degrees), intake centreline about 100–110° ATDC.
  - Overlap = (IVO before TDC + EVC after TDC). It is about 0–20° on economy engines, 40–80°+ on race cams, and gives lumpy idle through reversion and residual.
  - Derive IVO, IVC, EVO and EVC from centreline ± duration/2 so users can set LSA and advance directly.
- **Throttle [textbook, Heywood eq. 7.x, from memory]:**
  - ψ is the plate angle from the plane perpendicular to the bore, ψ₀ the closed angle (about 5–10°), a = d_shaft/D.
  - A_th/(πD²/4) = 1 − cos ψ/cos ψ₀ + (2/π)·[ (a/cos ψ)·√(cos²ψ − a²cos²ψ₀) + (cos ψ/cos ψ₀)·asin(a·cos ψ₀/cos ψ) − a·√(1 − a²) − asin a ].
  - Valid while a·cos ψ₀/cos ψ ≤ 1.
  - Add a small constant leak/idle-bypass area so idle is possible at "closed" throttle. Cd at small opening is about 0.6–0.8 **[estimate]**.
- For sound, the valve equations matter more than the combustion details. The EVO blowdown (typically choked initially, cylinder at 3–6 bar vs a port at about 1 bar) is the main acoustic excitation of the exhaust. The IVC/IVO events and reversion shape the intake sound. Make EVO timing, the early lift slope and Cd at low lift accurate.

### Gaps
- No primary source was retrieved for typical cam lobe polynomials, duration at 0.050" or LSA ranges. These are estimates from general automotive knowledge.
- The exact Heywood throttle-area formula was not verified online; the patent summary was garbled.
- There is no source for reverse-flow Cd values.

---

## 6. Cyclic variability: randomness injection to reproduce COV_IMEP 2–12%, residual coupling, partial burns and misfire

### Takeaway
Physical cyclic variability comes from random early-flame-kernel conditions (local turbulence, AFR, spark energy, residual mixing) plus deterministic coupling through residual gas. Residual variation alone accounts for about a third of IMEP variance in one study. In a Wiebe model:
- jitter the ignition/flame-development delay and the burn duration per cycle (log-normal or Gaussian);
- make the jitter grow with dilution and leanness;
- carry the previous cycle's unburned fuel and residual into the next cycle, so a weak cycle is followed by a richer, less diluted one.

This reproduces the characteristic non-random "stumble, then strong cycle" pattern near the lean or dilute limit.

### Cited Findings
- Cycle-to-cycle variability in normal SI operation is mainly caused by random perturbations of the flow velocity field, air-fuel homogeneity, spark energy discharge and turbulence intensity at the flame front. — [Duan et al. 2024, Energy Sci. & Eng., CCV mechanisms](https://scijournals.onlinelibrary.wiley.com/doi/10.1002/ese3.1879)
- Residual gas variations contribute about 1/3 of IMEP variations. Air and fuel variations contribute 7.5% and 4.6%, and about 54% is attributed to flow-field inhomogeneity. — [search summary of Cycle-to-cycle variations under cylinder-pressure-based combustion analysis, J. Mech. Sci. Tech.](https://link.springer.com/article/10.1007/BF03185069) (attribution between this and neighbouring results is uncertain; treat as indicative)
- In normal operation CCV of energy release behaves like uncorrelated random noise. Near dilute or lean limits, prior-cycle coupling through residual gas creates deterministic structure. — [ORNL: Evaluation of residual gas fraction estimation methods for CCV](https://www.osti.gov/pages/servlets/purl/1756265); [ORNL: Daw et al., A simple model for cyclic variations in an SI engine](https://impact.ornl.gov/en/publications/a-simple-model-for-cyclic-variations-in-a-spark-ignition-engine/)
- Daw–Finney model: small-scale stochastic parameter fluctuations interact with nonlinear deterministic coupling between cycles. The coupling works because the residual gas alters the in-cylinder equivalence ratio and so the next cycle's combustion efficiency. The model is simple enough to simulate thousands of cycles. — [ORNL Daw et al.](https://impact.ornl.gov/en/publications/a-simple-model-for-cyclic-variations-in-a-spark-ignition-engine/)
- A related stochastic map (Wendeker/Litak type) tracks per-cycle fuel and air masses with residual fraction α:
  - Rich of stoichiometric: m_f(i+1) = α·(m_f(i) − m_a(i)/s) + δm_f.
  - Lean: m_a(i+1) = α·(m_a(i) − s·m_f(i)) + δm_a.
  - Fresh-fuel noise σ = 0.1·δm_f was used. Residual fractions α = 0.16 were studied. Lean combustion is the least stable.
  - — [arXiv nlin/0405054, stochastic/deterministic model of CCV in SI engines](https://arxiv.org/pdf/nlin/0405054)
- Engine-sim adds a uniform random term to burning efficiency and attenuates flame speed with dilution. — [engine-sim combustion_chamber.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/combustion_chamber.cpp)

### Inferences
- **Recommended per-cycle recipe [estimate]**, drawn at spark time for each cylinder:
  1. Effective φ and dilution: φ_eff from trapped fresh air, fuel and carried-over unburned fuel. x_r = residual fraction, taken from the gas-exchange simulation itself (it emerges from valve overlap and backflow, so bad cam designs automatically give high x_r).
  2. Flame-kernel delay: θ_delay = θ_delay,0·f(φ, x_r, T, p)·exp(σ_d·ξ₁), with ξ₁ ~ N(0,1) and σ_d ≈ 0.1–0.2 at nominal conditions, growing to 0.3–0.5 near the limit.
  3. Burn duration: Δθ = Δθ_0·g(...)·exp(σ_b·ξ₂), with σ_b ≈ 0.05–0.15, and ξ₂ partially correlated with ξ₁ (ρ ≈ 0.5), since a slow kernel tends to give a slow burn.
  4. Ignition probability: P_ign = 1/(1 + exp((D − D_lim)/w)), where D is a dilution/leanness index. Misfire gives no heat release, the fuel stays and goes out with the exhaust or into the residual.
  5. Partial burn: if the Wiebe curve has not reached about 90% by EVO, or if Δθ exceeds a threshold, cut the burn at a random x_b,final. The unburned remainder goes into the residual and exhaust accounting. Unburned fuel reaching a hot exhaust is the hook for afterfire and pops.
  6. Carry-over: the next cycle's charge includes α·(unburned fuel and O₂), producing the Daw-type deterministic coupling.
- Tuning targets [estimate/textbook]: COV_IMEP about 1–3% at part load for a healthy engine, about 3–5% at idle, and more than 10% is considered drivability-limited with audible roughness. A lumpy race cam at idle, lean misfire or high EGR can reach 10–20%+. Start with σ_d = 0.15 and σ_b = 0.08, then check COV_IMEP over about 300 simulated cycles. IMEP COV is roughly proportional to the CA50 scatter away from MBT, because the torque-vs-phasing curve is flat near MBT and steep far from it.
- The sound benefit: CA50 jitter of ±2–4° modulates each firing pulse's amplitude and timing at sub-cycle scale. This removes the "machine-perfect" periodicity of synthetic engines and is arguably the single most important realism knob.

### Gaps
- The exact Daw et al. combustion-efficiency sigmoid was not retrieved. From memory it is CE = CE_max/(1 + 100^−(φ−φ_m)/(φ_u−φ_l)), with φ_m = (φ_u + φ_l)/2. Verify in SAE 962086 / Phys. Rev. E 1996 before use.
- There is no source for the specific σ values that map to given COV_IMEP. They must be calibrated in-sim.

---

## 7. Diesel (compression ignition) differences

### Takeaway
There is no spark. Ignition happens after an ignition delay (for example Hardenberg–Hase, using activation energy E_a = 618840/(CN + 25) J/mol) following start of injection. The fuel injected during the delay burns as a sharp premixed spike, which is the source of the diesel "knock" or clatter. The rest burns as a slower mixing-controlled diffusion phase. Model the heat release as a double Wiebe (premixed plus diffusion). The premixed fraction grows with longer delay, which happens at cold, idle, low CR or low cetane.

### Cited Findings
- Hardenberg–Hase (1979) ignition delay uses the apparent activation energy E_a = 618840/(CN + 25). It was derived for DI diesel engines. The constant is often tuned (for example raised for biodiesel). Delay falls with rising cetane number and compression ratio. — [Ngayihi Abbe et al. 2014, Int. Sch. Res. Notices](https://onlinelibrary.wiley.com/doi/full/10.1155/2014/534953); [Investigation on sensitivity of ignition delay and activation energy (ResearchGate)](https://www.researchgate.net/publication/268224101_An_Investigation_on_Sensitivity_of_Ignition_Delay_and_Activation_Energy_in_Diesel_Combustion)
- Woschni 1978 is the heat-transfer model most frequently used for diesels. Single-zone CI models use a Wiebe-based burned mass fraction plus Woschni. — [search summary, single-zone CI model literature](https://www.sciencedirect.com/science/article/abs/pii/S0016236122001545)
- Multiple-Vibe 2-zone models are used for diesel combustion phases. — [ScienceDirect 2025, Vibe 2-zone vs multiple Vibe](https://www.sciencedirect.com/science/article/pii/S2666691X25000478)

### Inferences
- **[textbook, from memory]** Full Hardenberg–Hase: τ_id [°CA] = (0.36 + 0.22·S̄p)·exp[E_a·(1/(R̃·T) − 1/17190) + (21.2/(p − 12.4))^0.63], with p in bar, T in K, R̃ = 8.314 J/mol/K and S̄p in m/s. p and T are evaluated at TDC or SOI. Typical delay is 0.5–2 ms (about 5–15°CA). Verify before use.
- Heat release: double Wiebe with a premixed part (fraction β ≈ 0.1–0.5 **[estimate]**, Δθ_p ≈ 5–10°CA, m_p ≈ 2–3) and a diffusion part (Δθ_d ≈ 40–80°CA, m_d ≈ 0.5–1.5). β grows with delay: β ≈ 1 − c₁·φ^c₂/τ_id^c₃ (Watson correlation, constants not retrieved).
- For sound, the premixed dp/dθ spike is what counts: about 5–10+ bar/°CA **[estimate]**, a broadband clatter spanning roughly 1–5 kHz and exciting the cylinder modes. Longer delay gives a larger spike and louder clatter. This explains cold-idle rattle and quieter behaviour with pilot injection.
- No throttle, so load is set by fuel mass. Overall φ at idle is about 0.1–0.3, and lean misfire logic does not apply the same way.

### Gaps
- The Watson premixed-fraction constants and the complete Hardenberg–Hase expression were not verified online.

---

## 8. Friction (FMEP, Chen–Flynn) and crankshaft dynamics for instantaneous rpm

### Takeaway
Use Chen–Flynn FMEP = C + PF·p_max + MPSF·S̄p + MPSSF·S̄p², for example 0.25 bar + 0.005·p_max + 0.1·S̄p. Apply it as a smoothed per-cycle friction torque, or split it into a constant plus a crank-angle-dependent part. Integrate a single rigid crank: J·dω/dt = Σ T_gas,i + Σ T_recip,i − T_fric − T_load. The per-cylinder gas torque comes from pressure times the kinematic lever arm, and it drives the instantaneous rpm ripple that frequency-modulates the sound.

### Cited Findings
- Chen–Flynn: FMEP = C + PF·P_max + MPSF·S̄p + MPSSF·S̄p². Example coefficients: C = 0.25 bar, PF = 0.005, MPSF = 0.1, MPSSF = 0. It is widely used (for example in GT-Power) for its simplicity. It can err by up to 35% on modern common-rail diesels. — [Chen–Flynn correlation coefficients table (ResearchGate)](https://www.researchgate.net/figure/Chen-Flynn-Correlation-Coefficients_tbl1_339069198); [Correlation of FMEP by Chen–Flynn (ResearchGate)](https://www.researchgate.net/figure/Correlation-of-FMEP-exp-and-predicted-FMEP-by-Chen-Flynn-model_fig3_386859816)
- In engine-sim, piston force = A·(p_cylinder − p_crankcase). — [engine-sim combustion_chamber.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/combustion_chamber.cpp)

### Inferences
- **[textbook]** Units: FMEP in bar, p_max in bar, S̄p in m/s, so MPSF is in bar/(m/s). Example: S̄p = 15 m/s and p_max = 60 bar give FMEP = 0.25 + 0.30 + 1.5 = 2.05 bar, plausible for a high-revving SI engine. Mean friction torque for a 4-stroke is T_f = FMEP·V_d,total/(4π). Add pumping losses explicitly: they come out of the gas-exchange simulation, so do not include PMEP in FMEP.
- Low-speed stability: make friction torque smooth through ω = 0, for example T_f·tanh(ω/ω_ε) with a Coulomb/Stribeck-like breakaway, so the engine can stall and stop without chattering.
- **[textbook]** Per-cylinder gas torque: T_gas = (p − p_cc)·Ap·(−dx/dθ). Reciprocating inertia torque: T_recip = −m_rec·ẍ·(−dx/dθ), with ẍ = ω²·x″(θ) + α·x′(θ). m_rec ≈ piston + pin + rings + about ⅓ of the rod mass **[estimate]**.
- Crank ODE: J_eff(θ)·dω/dt = Σ(T_gas + T_recip) − T_fric − T_load. J includes flywheel, crank, the rotating ⅔ of the rod and the driveline if clutched. J_eff varies slightly with θ because of reciprocating mass; the constant J plus the T_recip term is adequate. Typical flywheel plus crank J is about 0.1–0.3 kg·m² for car engines and 0.02–0.05 for motorcycles **[estimate]**. The small J of bikes gives large rpm ripple (lumpy singles and V-twins).
- Torsional vibration (crank twist, dual-mass flywheel) can be added as a 2-DOF spring-damper, with the crank nose inertia coupled to the flywheel through k_t (first torsional mode about 200–600 Hz for inline engines **[estimate]**). It is optional for sound.

### Gaps
- No source was retrieved for crank-angle-resolved friction distribution (for example the Patton–Nitschke–Heywood component model).
- Rotating inertia values are estimates.

---

## 9. Numerics: step size, stiffness, integrator choice

### Takeaway
Crank-angle-resolved single-zone models are normally integrated at 0.1–1°CA. Audio rate itself gives 0.1°CA per sample at 800 rpm and 0.75° at 6000 rpm at 48 kHz, so stepping the cylinder ODE at audio rate with 2–4 sub-steps is accurate enough. Use an explicit RK2/RK4, or a semi-implicit / exponential update for the valve flows. The stiff parts are small-volume/large-orifice flow (blowdown into small ports) and knock-rate heat release. Sub-cycle adaptively on those rather than globally.

### Cited Findings
- A per-cylinder first-law model integrated over 720° with 0.5–1° steps is common practice. — [search summary, jenilclaudeai/diesel-simulator](https://github.com/jenilclaudeai/diesel-simulator) (low authority)
- For *IMEP computation from measured data*, 3–6° sampling stays within 2% of 1° results. This is a data-analysis result, not ODE integration, so do not use it to justify coarse ODE steps. — [search summary, US patents 8725385 / 8700287](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8725385)
- RK2 has local error O(h³) and global error O(h²). RK4 has local error O(h⁵) and global error O(h⁴). — [Vensim: Runge-Kutta integration](https://www.vensim.com/documentation/rungekutta.html)
- Engine-sim runs its physics at an update rate equivalent to about 80,000 steps per second to produce realistic audio. — [The Drive: homebrew engine sim](https://www.thedrive.com/news/hear-this-absurdly-accurate-homebrew-engine-sim-rev-a-chevy-big-block)

### Inferences
- **Step-size arithmetic (computed):** Δθ_per_sample = 6·rpm/f_s degrees.
  - At 48 kHz: 800 rpm gives 0.10°, 3000 rpm 0.375°, 6000 rpm 0.75°, 9000 rpm 1.125°.
  - At 96 kHz: half of those values.
  - A burn Δθ of 40–70° gives more than 50 steps even at 9000 rpm with 1 sub-step. The Wiebe burn is well resolved.
  - Valve events (early lift, where Cd·A changes fast) and knock onset need finer steps. Use 2–4 fixed sub-steps per audio sample, which is about 100–200 kHz, the same order as engine-sim's ~80 kHz.
- **Stiffness estimate [estimate]:** the flow relaxation time of a cylinder through an open valve is τ ≈ V/(Cd·A·c). Examples:
  - V = 50 cm³ (near TDC), Cd·A = 3 cm², c = 800 m/s: τ ≈ 0.2 ms, about 10 samples at 48 kHz. Explicit is fine.
  - A tiny runner node (for example 5 cm³) gives τ ≈ 20 µs, about one sample. Explicit Euler becomes unstable here. This is where sub-stepping or a semi-implicit linearisation of ṁ(Δp) is needed.
  - Rule: sub-steps n ≥ 2·dt/τ_min, evaluated per step (adaptive), or clamp |Δm| per step to a fraction of the mass available to equalise pressure.
- **Integrator choice:**
  - Cylinder: RK2 (Heun/midpoint) at 2–4 sub-steps per sample gives about the accuracy of RK4 at 1 sub-step for similar cost, and it is simpler with discontinuous events (spark, valve open/close, knock).
  - Handle discrete events (spark, IVC/EVO, misfire decision, knock trigger) at step boundaries, with sub-sample timing interpolation for the acoustic output to avoid 1-sample jitter aliasing at high rpm.
  - Mass-conserving update: compute all orifice flows first, then update every volume with the same fluxes (finite-volume style), so total mass and energy are conserved to round-off.
  - Use double precision for crank angle accumulation. For pressure and temperature states, f32 is usually fine with renormalisation, but f64 is safer for long runs.
- **Robustness for bad designs:** enforce physical floors and ceilings on m, T and p. Treat CR < 3, CR > 25, absurd rod ratios, zero-overlap valve clashes (lift beyond piston-to-valve clearance is a mechanical, not thermodynamic, issue), and throttle ≈ 0 with huge cams as valid but extreme inputs that must degrade gracefully (misfire, stall) instead of producing NaN.

### Gaps
- No peer-reviewed benchmark of step size vs error for crank-angle single-zone models was retrieved. The 0.1–1° guidance is established practice, not quantified here.
- There is no source on audio-rate specific integration schemes beyond engine-sim's reported update rate.
