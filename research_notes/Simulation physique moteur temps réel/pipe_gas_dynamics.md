# 1D unsteady gas dynamics of intake and exhaust systems for a real-time engine sound simulator

Scope: methods that let the gas dynamics of runners, headers, collectors, plenums, mufflers, catalysts, tailpipes, throttle and turbocharger produce the sound at audio rate (48–96 kHz) on a CPU budget, with stability limits and costs.

Note on sourcing: facts fetched in this session carry a link. Textbook-standard equations quoted from memory, and my own sizing calculations, go under **Inferences** and are labelled as such. The report writer should keep them apart from the cited facts.

---

## 1. Filling-and-emptying vs method of characteristics vs finite-volume Euler vs digital waveguides: accuracy, cost and real-time feasibility (8 cylinders, 48–96 kHz)

### Takeaway
No single method does everything. Filling-and-emptying (0D) is cheap but has no wave action. Conservative finite-volume 1D Euler (two-step Lax-Wendroff with a TVD limiter, or MUSCL-HLL/HLLC) is the reference for nonlinear wave action, but at audio-rate CFL steps it costs tens of millions of cell-updates per second for a V8 and resolves waves accurately only up to a few kHz. Linear digital waveguides cost almost nothing per sample and carry the high-frequency acoustics well, but they miss finite amplitude, entropy convection and mean flow. The realistic design that has shipped is a hybrid: a nonlinear low-band quasi-1D solver (or 0D) plus audio-rate linear characteristic waveguides driven by the solver's valve-boundary state (EngineLab), or 0D plus delay lines plus convolution (AngeTheGreat's engine-sim).

### Cited Findings
**Industrial and academic 1D codes (numerics)**
- OpenWAM (CMT-Motores Térmicos, UPV) is an open-source (GPL) 1D gas-dynamics code for ICEs. It was first written in 1984 with the Method of Characteristics and now lets the user choose among schemes "to obtain fast or accurate results". Its main scheme for manifold wave action is the **two-step (Richtmyer) Lax-Wendroff**, second order in space and time, with an **adaptation of Sweby's TVD flux limiter (by Gascón)** to damp the oscillations typical of second-order schemes. FCT and other high-resolution schemes were also tested. — [OpenWAM numerical methods](https://openwam.webs.upv.es/docs/?p=180); [OpenWAM home](https://openwam.webs.upv.es/docs/); [GitHub CMT-UPV/OpenWAM](https://github.com/CMT-UPV/OpenWAM)
- OpenWAM's governing system is the conservation form ∂W/∂t + ∂F/∂x + C₁ + C₂ = 0, with W = (ρA, ρuA, ρe₀A), C₁ = area-change terms and C₂ = friction and heat-transfer terms. Source terms are incorporated into the TVD flux calculation. — [OpenWAM numerical methods](https://openwam.webs.upv.es/docs/?p=180)
- Galindo, Serrano, Arnau and Piqueras (J. Eng. Gas Turbines Power, 2009): conventional 1D models use one global time step limited by the smallest cell ("the use of small ducts in some parts of the engine reduces calculation speed"). Their **independent time discretization** lets each element (duct, turbocharger, intercooler, catalyst, cylinder, DPF) run at its own stability limit under a global manager, to improve the accuracy/cost trade-off. — [ResearchGate abstract](https://www.researchgate.net/publication/267577863_Description_and_Analysis_of_a_One-Dimensional_Gas-Dynamic_Model_With_Independent_Time_Discretization); [ASME semi-independent TD](https://asmedigitalcollection.asme.org/gasturbinespower/article-abstract/131/3/034504/466305/Description-of-a-Semi-Independent-Time?redirectedFrom=fulltext). A companion paper studies the **high-frequency response** of this method. — [ScienceDirect](https://www.sciencedirect.com/science/article/pii/S0895717709001435)
- GT-Power (as described in an SAE paper that uses it) solves the nonlinear mass, momentum and energy balances plus the equation of state with **explicit time integration on spatially discrete control volumes** (staggered finite volume). Its momentum equation includes wall friction f and a lumped pressure-loss coefficient K. — [Dehner, Selamet et al., SAE 2010-01-2142](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)
- Onorati, Winterbone & Pearson compared Lax-Wendroff and MoC for engine gas dynamics using FFT spectral analysis (SAE 930428, 1993). I could not retrieve the abstract. — [SAE 930428](https://www.sae.org/publications/technical-papers/content/930428/)
- For non-homentropic flow, the **two-step Lax-Wendroff is faster than MoC and easier to implement**. The non-homentropic MoC is less accurate with large entropy changes (exhaust flows) and unsuitable in steep diffusers, where **Blair's GPB finite-system method** was the most accurate. All schemes struggle in tapered pipes: mass/energy conservation, overshoot, stability. — [OSTI 282269 comparison study](https://www.osti.gov/biblio/282269), [search-snippet summary of related papers](https://www.researchgate.net/publication/223592313_1D_gas_dynamic_modelling_of_mass_conservation_in_engine_duct_systems_with_thermal_contact_discontinuities). These are secondary snippets; the full papers were not read.
- MoC in non-homentropic, viscous flow needs modification because "the Riemann invariants are not constant anymore". A known MoC problem is **mass conservation at thermal contact discontinuities** (hot/cold gas interfaces). — [ResearchGate: 1D gas dynamic modelling of mass conservation … thermal contact discontinuities](https://www.researchgate.net/publication/223592313_1D_gas_dynamic_modelling_of_mass_conservation_in_engine_duct_systems_with_thermal_contact_discontinuities)

**Real-time / HiL evidence**
- Detailed 1D gas-dynamics design models run **20–30× slower than real time**, so HiL use needs simplification. — [Crank-Angle Resolved Real-Time Engine Modelling, SAE 2018-01-1245 / ResearchGate](https://www.researchgate.net/publication/324170677_Crank-Angle_Resolved_Real-Time_Engine_Modelling_A_Seamless_Transfer_from_Concept_Design_to_HiL_Testing)
- Crank-angle resolved real-time 1D models for HiL compute in-cylinder pressure and intake/exhaust manifold pressures and temperatures crank-resolved. — [ResearchGate: Crank Angle Resolved Real-Time Engine Modeling for HiL](https://www.researchgate.net/publication/320830724_Crank_Angle_Resolved_Real-Time_Engine_Modeling_for_HiL_Based_Component_Testing); [Academia: 1D engine model for HiL, SAE 2018-01-0874](https://www.academia.edu/38171242/Development_and_Calibration_of_One_Dimensional_Engine_Model_for_Hardware_In_The_Loop_Applications_2018_01_0874)
- Gamma Technologies' **Fast Running Models (FRM)**, which are coarsened and lumped GT models: on 10 real test cases, average turnaround time ≈ **0.4× real time**, max ≈ 0.5×, "using high frequency sampling common in today's ECUs". ECU rates are ms-scale, not audio rate. — [GT news: Realtime engine models 2x faster](https://www.gtisoft.com/news-archives/realtime-gt-power-engine-models-now-run-2x-faster/); [ARAI FRM white paper](https://www.gtisoft.com/wp-content/uploads/2019/07/ARAI-White-Paper-Developing-a-Real-Time-Heavy-Duty-Engine-Model-Using-GT-SUITEs-Fast-Running-Model-FRM-Builder-and-Running-on-Virtual-Test-Bed.pdf)

**Sound-oriented implementations**
- **AngeTheGreat's engine-sim** (source read directly):
  - Architecture: 0D "GasSystem" volumes (cylinder, intake, exhaust runner/primary, exhaust system) joined by orifice-flow restrictions with a flow constant `k_flow`. The flows use an isentropic nozzle equation with a choked branch (`p_ratio <= chokedFlowLimit` → choked).
  - Default `m_simulationFrequency = 10000` Hz, with `m_fluidSimulationSteps` substeps per step for the fluid.
  - Audio: per cylinder, `exhaustFlow = 1600·[(p_runner − 1 atm) + 0.1·dynamicPressure(±x)]`, passed through a `DelayFilter` of delay `exhaustLength / 343 m/s` (header primary + exhaust length; note the constant cold-air speed of sound), scaled by `1/exhaustLength²`, summed per exhaust system, then sent to a synthesizer with a user "convolution level" (impulse-response convolution).
  - So it is a filling-and-emptying model with a phenomenological delay and IR, **not** a wave-action model.
  - — [engine-sim src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp), [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp), [src/simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/simulator.cpp), [README](https://github.com/ange-yaghi/engine-sim)
- **EngineLab** (C++20/JUCE, open source):
  - Physics: "conservative quasi-1-D intake and exhaust" in which every duct is meshed and solved, with signed SI mass flow at the valves, "characteristic waveguides", a thermal wall model and a passive radiation load at the outlet. Elements include primaries, collectors, junctions, resonators, expansion chambers (Munjal), porous packing (Delany-Bazley), catalysts and multiple outlets.
  - Stated limitation: the quasi-1D networks are "low-band", and **audible propagation is linear, through characteristics aggregated per path**. Transverse modes, 3D bends and the mean-flow radiation correction are not resolved.
  - Cost: "large engines (V8, V12) remain expensive for the physics thread". The app displays its realtime factor.
  - — [EngineLab README](https://github.com/zolaski333/EngineLab)
- EngineLab's audio coupling, which is the key hybrid recipe:
  - A slow mean separates operating state from acoustic perturbation. Then Zc = ρc/A, U′ = ṁ′/ρ, p⁺ = (p′ + Zc·U′)/2, p⁻ = (p′ − Zc·U′)/2.
  - The **valve's differential resistance, linearised from the orifice law around the current flow, sets the port reflection coefficient**.
  - Characteristics propagate in forward and backward delay lines whose delays come from physical lengths and local sound speed. Collector junctions use admittances A/(ρc).
  - — [EngineLab docs/realtime-audio.md](https://github.com/zolaski333/EngineLab/blob/HEAD/docs/realtime-audio.md)
- Baldan, Lachambre, Delle Monache & Boussard (IEEE SIVE 2015), "Physically informed car engine sound synthesis": multiple digital waveguides, with the muffler as four independent, partially reflecting waveguides. — [ResearchGate](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments) (403 on fetch; details from search snippet). A patent (US5835605A) also describes an exhaust pipe as a waveguide-type circuit of delays plus scattering junctions. — [Google Patents US5835605A](https://patents.google.com/patent/US5835605A/en)

### Inferences
**Sizing (my calculations, standard CFL analysis)**
- **CFL constraint.** Explicit schemes need Δt ≤ CFL·Δx / max(|u|+a), with CFL ≲ 1 for LW/MUSCL-Hancock and ≲ 0.5–0.9 in practice.
  - Hot exhaust at ~900 K (γ≈1.35, R=287): a ≈ √(1.35·287·900) ≈ 590 m/s. With peak |u| ≈ 150–200 m/s, |u|+a ≈ 800 m/s.
  - Locking Δt to the audio period gives Δx_min ≈ 800/(CFL·fs):
    - 48 kHz, CFL 0.9: Δx ≳ **18–19 mm** (≈ 55 cells/m).
    - 96 kHz: Δx ≳ **9 mm** (≈ 110 cells/m).
  - Cold intake (a ≈ 347 m/s, |u| up to ~100 m/s): Δx ≳ 10 mm at 48 kHz.
  - Rule of thumb: pick Δx at the CFL limit for the hottest pipe. Smaller cells force sub-stepping. Local/independent time stepping (Galindo) avoids penalising the whole network.
- **Resolved bandwidth.** Second-order schemes need roughly 10–20 cells per wavelength for low dispersion/dissipation.
  - With Δx = 20 mm in 590 m/s gas: λ_min ≈ 0.2–0.4 m → **f_accurate ≈ 1.5–3 kHz**. Doubling fs to 96 kHz halves Δx and roughly doubles the accurate band (≈3–6 kHz) at **4× cost** (2× cells × 2× steps).
  - This is why a pure FV solver at audio-rate CFL is "low-band" (as EngineLab states), and HF content should come from the linear waveguide layer.
- **Cell counts for a V8** (illustrative geometry):
  - 8 primaries × 0.8 m + 2 collectors × 0.3 m + 2 × 2 m intermediate + muffler internals ~1 m + 2 × 0.5 m tail ≈ 13 m exhaust.
  - 8 runners × 0.3 m ≈ 2.4 m intake, with the plenum as 0D.
  - Total ≈ 15 m → **≈ 750 cells at 48 kHz** (Δx = 20 mm), ≈ 1500 cells at 96 kHz.
  - Cell-updates/s: 750 × 48 000 ≈ **36 M/s** at 48 kHz; 1500 × 96 000 ≈ **144 M/s** at 96 kHz.
- **CPU per cell-update** (estimates; no benchmark found):
  - Two-step LW (+ TVD limiter): roughly 60–120 flops incl. one sqrt/div, i.e. ~10–30 ns scalar. First-order HLL: similar. MUSCL + HLLC + 2-stage RK/Hancock: ~200–400 flops, ~40–100 ns scalar.
  - Resulting load at 48 kHz: LW ≈ 0.4–1.1 core-s per second of audio (one core, borderline to OK); MUSCL-HLLC ≈ 1.5–3.6 core-s/s (needs SIMD across cells/pipes, or 2–4 threads).
  - At 96 kHz, full FV is 4× worse and not realistic on one core for a V8.
  - SIMD over pipes (8 primaries in AVX lanes) is natural because the primaries are identical.
- **Recommended hybrid:**
  - (a) Run the nonlinear quasi-1D FV solver (LW+TVD or MUSCL-HLL) at a **reduced rate**: 20–40 kHz, Δx ≈ 25–50 mm, i.e. ~300–600 cells for a V8. This covers blowdown, finite amplitude, entropy slugs, tuning/scavenging and anything below ~1–2 kHz.
  - (b) Superimpose **audio-rate linear waveguides** (fractional-delay lines, O(1) per sample regardless of length, plus scattering at area changes and junctions) driven by the perturbation at the valve boundary. This is EngineLab's choice.
  - (c) Or skip FV entirely: 0D filling-emptying + waveguides with nonlinear valve terminations. This is the cheapest option that still produces tuned-length resonances and reflected-pulse interaction.
  - Waveguide network cost for a V8 (~30–50 junctions/filters): ~10⁴ flops/sample ≈ 0.5 GFLOP/s at 48 kHz, negligible.
- **Method choice:**
  - MoC (Benson mesh method): characteristics and path lines are interpolated on a fixed mesh. It is non-conservative, so it leaks mass at temperature discontinuities.
  - Two-step LW with a flux limiter is the pragmatic industrial standard (OpenWAM, and historically WAVE/GT-type codes).
  - Godunov-type HLL/HLLC/Roe with MUSCL (Toro, *Riemann Solvers and Numerical Methods for Fluid Dynamics*) are more robust at strong shocks and contact discontinuities. HLLC preserves contact (entropy) waves, which matters for exhaust temperature slugs, at roughly 1.5–3× LW cost.
  - For an audio engine, **robustness (no NaN, positivity of ρ and p) matters more than formal order**. HLL/HLLC with a minmod limiter and a positivity fallback to first order is the safest.
- A filling-and-emptying model at 10 kHz (engine-sim) shows that plausible engine sound can come from 0D + delay + IR. But engine-sim uses c = 343 m/s for the exhaust delay regardless of gas temperature, so tuned-length pitch is physically off by a factor ~1.7 for hot gas.

### Gaps
- No published benchmark of ns per cell-update for engine-type 1D solvers at audio rate was found. The CPU figures above are estimates to be measured.
- The Onorati/Winterbone/Pearson SAE 930428 findings (accuracy of LW vs MoC vs FFT) were not retrieved.
- Blair's GPB method details (Blair, *Design and Simulation of Four-Stroke Engines*, SAE 1999) were not accessed. The claim that it is the most accurate in steep diffusers is secondary.
- Lotus Engine Simulation and Ricardo WAVE numerics docs were not accessed.
- DAFx-specific papers coupling a nonlinear 1D Euler solver to waveguides for engines were not found. The closest are EngineLab (code, not peer-reviewed) and Baldan et al. 2015 (waveguides without a nonlinear gas solver).

---

## 2. Boundary conditions: valve/port, open end with radiation, closed end, junctions, area changes, plenum/Helmholtz, perforates and packing, catalyst

### Takeaway
Every boundary reduces to "incoming characteristic(s) from the pipe + a quasi-steady algebraic law at the boundary":
- valve/throttle: compressible orifice law with Cd·A(lift), subsonic or choked, inflow or outflow;
- open end: p = p_atm, plus a Levine-Schwinger frequency-dependent reflection for acoustics;
- junctions: constant-pressure or pressure-loss (Bassett–Winterbone–Pearson);
- catalyst: Darcy–Forchheimer resistance;
- packing: Delany-Bazley.
In a waveguide layer, each linearises into a (frequency-dependent) reflection/scattering coefficient.

### Cited Findings
- **Compressible orifice law** (MathWorks Flow Restriction block, the standard form): ṁ = Γ·Ψ(P_ratio), with Γ = A_eff·P_up/√(R·T_up).
  - Choked when P_ratio ≤ P_cr = (2/(γ+1))^(γ/(γ−1)), where Ψ = √γ·(2/(γ+1))^((γ+1)/(2(γ−1))).
  - Otherwise Ψ = √[(2γ/(γ−1))·(P_ratio^(2/γ) − P_ratio^((γ+1)/γ))].
  - Near P_ratio → 1 the block switches to a **linearised branch** to avoid the infinite slope (numerical stiffness).
  - — [MathWorks Flow Restriction](https://www.mathworks.com/help/autoblks/ref/flowrestriction.html)
- engine-sim implements the same subsonic/choked formula with a cached choked-flow factor and γ derived from the degrees of freedom. — [engine-sim gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)
- OpenWAM separately studied an **inflow boundary condition** for 1D manifold codes. — [ResearchGate: Inflow boundary condition for 1D gas dynamics simulation code of IC engine manifolds](https://www.researchgate.net/publication/239407115_Inflow_boundary_condition_for_one-dimensional_gas_dynamics_simulation_code_of_internal_combustion_engine_manifolds)
- **Open-end radiation (Levine–Schwinger 1948)**: rigorous solution for an unflanged circular pipe, valid for plane-wave propagation, ka < 3.832. — [Phys. Rev. 73, 383](https://journals.aps.org/pr/abstract/10.1103/PhysRev.73.383)
  - Low-frequency limits: |R| = 1 − β(ka)² + o(ka²), with β = ½ unflanged and 1 flanged. End correction L/a → η = **0.6133** unflanged, **0.8216** flanged.
  - Silva, Guillemain, Kergomard, Mallaroni (2008) give Padé fits (<2% error for ka < 3, non-causal):
    - |R| = (1 + a₁(ka)²) / (1 + (β + a₁)(ka)² + a₂(ka)⁴ + a₃(ka)⁶)
    - L/a = η·(1 + b₁(ka)²) / (1 + b₂(ka)² + b₃(ka)⁴ + b₄(ka)⁶)
    - Unflanged: a₁ = 0.800, a₂ = 0.266, a₃ = 0.0263; b₁ = 0.0599, b₂ = 0.238, b₃ = −0.0153, b₄ = 0.00150.
    - Flanged: a₁ = 0.730, a₂ = 0.372, a₃ = 0.0231; b₁ = 0.244, b₂ = 0.723, b₃ = −0.0198, b₄ = 0.00366.
  - The same paper gives a **causal time-domain reflection function**: r(t) = −A·(ct/a)^ν·exp(−α·ct/a) for t > 0. Unflanged: ν = 0.504, α = 1.2266, A = 1.534·(c/a). Flanged: ν = 0.350, α = 0.8216, A = 0.861·(c/a). A rational (d₁ = 1.393, d₂ = 0.457 unflanged) causal model is also given.
  - The authors note the difficulty of making a digital (z-domain) filter because the coefficients depend on radius a.
  - — [Silva et al., arXiv:0811.3625](https://arxiv.org/pdf/0811.3625)
- The reflection coefficient at an open pipe changes with mean flow at low Mach and low Helmholtz number (measured). — [J. Fluid Mech.: damping and reflection coefficient at an open pipe at low Mach](https://www.cambridge.org/core/journals/journal-of-fluid-mechanics/article/abs/damping-and-reflection-coefficient-measurements-for-an-open-pipe-at-low-mach-and-low-helmholtz-numbers/176F64F8EDC5D7B2E8C2777D6FAA6FE2)
- **Junctions**:
  - Bassett, Winterbone & Pearson (Proc. IMechE C, 2001) give simple expressions for all loss coefficients of a three-pipe T-junction at any branch angle and area ratio, from steady-flow data. — [SAGE](https://journals.sagepub.com/doi/abs/10.1177/095440620121500801)
  - Bassett, Pearson, Fleming & Winterbone (SAE 2003-01-0370) give a **multi-pipe pressure-loss junction model** that captures directionality imposed by pipe angles. It needs empirical steady-flow loss data. — [SAE 2003-01-0370](https://saemobilus.sae.org/papers/a-multi-pipe-junction-model-one-dimensional-gas-dynamic-simulations-2003-01-0370)
  - Related work: [Modified pressure loss model for T-junctions of exhaust manifolds (CJME 2014)](https://link.springer.com/article/10.3901/CJME.2014.0904.143); [Pulse-converter junction loss coefficients (OSTI)](https://www.osti.gov/etdeweb/biblio/20064549); [Fluid dynamic modelling of junctions in IC engine systems (J. Thermal Sci. 2010)](https://link.springer.com/article/10.1007/s11630-010-0402-0); [Exhaust manifold junction loss experiment (IJRM 2014)](https://onlinelibrary.wiley.com/doi/10.1155/2014/316498)
- **Catalyst monolith**:
  - Pressure drop across catalytic converters is well described by **Darcy–Forchheimer**: a viscous term ∝ u plus an inertial term ∝ ρu².
  - The monolith is treated as a porous zone, and the flow inside is **laminar** because of the very small channel hydraulic diameter.
  - Multi-scale approaches pre-compute small-scale effects into look-up tables for the macro-scale model.
  - — [Understanding flow through catalytic converters (FFHMT 2017)](https://www.avestia.com/FFHMT2017_Proceedings/files/paper/FFHMT_135.pdf); [Multi-zone permeability approach (ScienceDirect)](https://www.sciencedirect.com/science/article/abs/pii/S0263876219302278); [LES of monolith catalytic converter (Entropy 2022)](https://pmc.ncbi.nlm.nih.gov/articles/PMC9141327/)
- **Packing/mufflers**: EngineLab models expansion chambers per Munjal and porous packing per **Delany-Bazley**. — [EngineLab README](https://github.com/zolaski333/EngineLab)
- **Helmholtz behaviour of a duct + plenum + restriction**: oscillations occur at the Helmholtz frequency even without a compressor. In GT-Power, 7.0 Hz was simulated vs a 7.3 Hz theoretical value on a test-bench system. — [Dehner et al., SAE 2010-01-2142](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)

### Inferences
Standard formulations below are from textbooks (Benson; Winterbone & Pearson; Toro), written from memory and to be verified against those sources.

- **Valve/port boundary (partially open end, Benson type)**:
  - Given the incoming Riemann variable from the pipe and the cylinder state (p_cyl, T_cyl), solve iteratively for the boundary state such that the pipe-side mass flux equals the orifice law with A_eff = Cd(L/D)·π·D_v·L (curtain area at low lift, capped by the port area). Outflow (cylinder → pipe) is isentropic expansion from cylinder stagnation to the throat pressure, then either choked or with sudden expansion into the pipe (entropy rises). Inflow (pipe → cylinder) uses throat p = p_cyl with isentropic flow from pipe stagnation.
  - In an FV code, this is a ghost-cell state or a direct boundary flux.
  - Cheap real-time version: compute ṁ from the orifice law with Newton (2–3 iterations) or a precomputed Ψ table, and impose it as the boundary mass flux with the enthalpy of the upstream side.
  - Use the MathWorks-style linearisation near P_ratio = 1 to avoid stiffness when valve Δp → 0.
- **Waveguide version of the valve** (EngineLab recipe): linearise ṁ(Δp) around the operating point, R_valve = ∂Δp/∂ṁ. The reflection coefficient is then r = (R_valve·A/(ρc) − 1)/(R_valve·A/(ρc) + 1): closed valve → r = +1 (rigid), wide open → r → (A_port/A_pipe-dependent).
  - This is a time-varying scattering junction updated every sample. It is the "nonlinear termination" in waveguide form.
  - For true nonlinearity, keep the full orifice law and solve p⁻ = f(p⁺, p_cyl) per sample (1D Newton), as for clarinet reed–bore coupling.
- **Closed end**: u = 0 → mirror ghost cell (ρ, −u, p). Waveguide r = +1.
- **Open end, nonlinear FV**:
  - Subsonic outflow: impose p = p_atm and extrapolate ρ and u (characteristic compatibility).
  - Inflow: isentropic acceleration from ambient stagnation (p₀, T₀), with a loss coefficient if desired.
  - Choked outflow (rare at tailpipes): u = a.
  - Acoustic layer: replace r = −1 by the Levine-Schwinger reflection. A cheap implementation is a first- or second-order IIR lowpass fitted to |R|(ka) plus a fractional delay 2·0.6133·a/c for the end correction. Example: for a 60 mm tailpipe (a = 30 mm) at c = 450 m/s, ka = 1 at f ≈ 2.4 kHz. The tailpipe becomes a strong radiator (low reflection) above ~2–3 kHz.
- **Constant-pressure junction** (Benson): all branch-end static pressures equal (Σ ṁᵢ = 0; energy mixed at the junction). In waveguide form: p_J = 2·Σ(Yᵢ·pᵢ⁺)/ΣYᵢ with Yᵢ = Aᵢ/(ρᵢcᵢ), and pᵢ⁻ = p_J − pᵢ⁺. This is the standard N-port parallel scattering junction.
  - The pressure-loss junction adds Δp_ij = K_ij·½ρu² per path, with K from Bassett et al. It yields directional (pulse-converter) behaviour and less cross-talk. It needs an iterative solve per step (small nonlinear system, N ≤ 5).
- **Area change**: the same junction with two ports (Y₁, Y₂). For expansions with flow, add Borda–Carnot loss Δp = ½ρu₁²(1 − A₁/A₂)² in the FV version.
- **Plenum/muffler chamber**: 0D volume (mass and energy ODE) connected to pipe ends by constant-pressure or orifice boundaries.
  - Valid when its length is ≪ λ/4. Above that, model the chamber as a short fat pipe (1D) so that its axial modes appear. Munjal's expansion-chamber transmission loss has notches at k·L_chamber = nπ.
  - Helmholtz frequency f = (c/2π)·√(A_neck/(V·L_eff_neck)).
- **Perforated pipes and absorptive packing**:
  - Perforate: a distributed transfer impedance per unit length of the form Z_p = ρc·(R + jωm)/σ_porosity (e.g. Sullivan/Crocker type correlations) that couples the inner pipe to the outer chamber. Cheap real-time option: lumped series leak per segment, or a 2-duct waveguide with coupling junctions every Δx.
  - Packing: Delany-Bazley complex wavenumber k = (ω/c)·[1 + 0.0978X^−0.700 − j·0.189X^−0.595] and impedance Zc = ρc·[1 + 0.0571X^−0.754 − j·0.087X^−0.732], with X = ρf/σ (σ = flow resistivity). Implement as a frequency-dependent lossy delay (a lowpass per delay segment). From memory of Delany & Bazley 1970; verify coefficients.
- **Catalyst**:
  - Treat as a pipe of open frontal area (porosity ε ≈ 0.7–0.9) with a volumetric source term −(μ/K·u + β·ρ·u²).
  - Laminar-channel friction: Hagen–Poiseuille in square channels, f·Re ≈ 56.9 (Darcy), 64 for circular. From memory; verify.
  - Acoustically the catalyst is a mostly resistive element (roughly frequency-independent attenuation plus a small delay). In waveguide form: a 2-port with transmission ≈ 1/(1 + R_cat·A/(2ρc)) and small reflection.
- **Stability of boundaries**: implicit/iterative boundaries (orifice Newton, junction solve) do not constrain the explicit CFL. But they must be solved to convergence, or clamped, to avoid mass creation. Use a first-order (non-limited) update in the cells adjacent to boundaries for robustness.

### Gaps
- Exact Benson valve/partially-open-end equations and iteration scheme (Benson, *The Thermodynamics and Gas Dynamics of IC Engines*, 1982) were not retrieved online. The form above is from memory.
- The Bassett et al. loss-coefficient formulas themselves (paywalled) were not retrieved.
- Perforate impedance correlations (Sullivan & Crocker, Bauer, Lee & Ih) and Munjal's formulas were not retrieved in this session.
- Delany-Bazley and catalyst channel-friction coefficients above are from memory and unverified here.

---

## 3. Nonlinear effects: wave steepening, finite amplitude, entropy (temperature) waves, mean-flow convection

### Takeaway
Exhaust blowdown pulses are finite-amplitude (p′/p₀ of order 0.2–1). They steepen over metre-scale distances, carry entropy (hot-gas) slugs that convect at u rather than u ± a, and ride on mean flow of Mach 0.1–0.4. A linear waveguide misses all three. A conservative FV solver (ideally HLLC, to keep contacts sharp) captures them. Entropy slugs matter for sound mainly through (i) local sound speed and hence pipe tuning and (ii) indirect noise when they are accelerated through a restriction (tailpipe exit, turbine, catalyst, throttle).

### Cited Findings
- **Indirect (entropy) noise**: temperature fluctuations (entropy waves) produce sound when they are accelerated through a nozzle or mean-flow gradient. Entropy noise can exceed direct noise in reactive systems. The compact-nozzle theory (Marble & Candel) is valid only at low frequency, and conversion depends strongly on Mach number. — [Morgans & Duran, "Entropy noise: A review", 2016](https://journals.sagepub.com/doi/full/10.1177/1756827716651791); [arXiv 2106.10469, indirect noise in non-isentropic nozzles](https://arxiv.org/pdf/2106.10469)
- MoC codes have trouble conserving mass across thermal contact discontinuities, which are exactly the hot/cold interfaces in engine ducts. — [ResearchGate: mass conservation with thermal contact discontinuities](https://www.researchgate.net/publication/223592313_1D_gas_dynamic_modelling_of_mass_conservation_in_engine_duct_systems_with_thermal_contact_discontinuities)
- EngineLab explicitly acknowledges that its audible propagation is linear, i.e. it drops nonlinear steepening in the audio path and does not apply the "mean-flow radiation correction". — [EngineLab README](https://github.com/zolaski333/EngineLab)
- The GT-Power surge study notes that nonlinear formulation with spatial distribution captures wave dynamics that lumped (0D) models cannot. Its surge waveforms were "somewhat steepened", producing harmonics of the Helmholtz frequency. — [Dehner et al., SAE 2010-01-2142](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)

### Inferences
- **Shock-formation distance** for a plane wave (standard nonlinear acoustics, from memory): x_s ≈ c²/(β·ω·u′), with β = (γ+1)/2.
  - My calculation for an exhaust pulse: p′ = 0.3 bar in 1 bar gas at 900 K. ρ ≈ 0.39 kg/m³, c ≈ 590 m/s, ρc ≈ 228 → u′ ≈ 130 m/s. At a 200 Hz fundamental (V8 at 3000 rpm), β ≈ 1.175 → x_s ≈ **1.8 m**.
  - So steepening is audible within a typical header plus downpipe. It generates high harmonics (the "rasp") that a linear model must fake.
  - At 1 bar overpressure (WOT, high rpm), x_s drops below 1 m. Pulses from each cylinder can become weak shocks in the primaries.
- **Convection and Doppler of wave speeds**: forward waves travel at u + a and backward at u − a. At M = 0.3 this shifts tuned-pipe round-trip times by ~10% (factor 1/(1 − M²)). A waveguide can include this with separate forward/backward delay lengths L/(c+u) and L/(c−u), updated slowly from the mean-flow solver. This is a cheap, significant improvement.
- **Temperature gradient**: exhaust gas cools from ~900–1100 °C at the port to ~200–500 °C at the tailpipe, so c varies by ~1.5× along the system. The waveguide delay per segment must use the local mean temperature, which the low-rate thermal model provides.
- **What a hybrid loses**: indirect noise at the tailpipe exit and at the turbine/catalyst, and nonlinear steepening in the high band. Mitigations:
  - (a) run FV up to the tailpipe at a reduced rate (captures < ~2 kHz nonlinearity);
  - (b) apply a memoryless or slew-dependent waveshaper to the forward wave in long hot segments as a steepening surrogate (heuristic, not physical);
  - (c) add an entropy-noise source term ∝ d(T′/T̄)/dt · Mach at restrictions.
- **Scheme implication**: HLLC or Roe keep contact discontinuities sharper than HLL or LW. That matters if entropy slugs are to arrive at the tailpipe with realistic timing and sharpness. HLL smears contacts over many cells.

### Gaps
- No engine-specific quantitative study was found of how much entropy (indirect) noise contributes to exhaust tailpipe sound. The reviews found are aero-engine/combustor focused.
- The shock-formation formula and the numbers above are my calculations from textbook nonlinear acoustics, not from a fetched source.

---

## 4. Heat transfer, friction and temperature gradient along the exhaust

### Takeaway
Use a friction factor (Blasius/Colebrook-type, or laminar 64/Re, 56.9/Re for catalyst channels) for the momentum source term. Get the wall heat-transfer coefficient from the Reynolds/Chilton-Colburn analogy (St·Pr^(2/3) = Cf/2). Integrate a lumped wall temperature per pipe segment at low rate. These terms mainly set damping and the mean temperature (hence sound speed) profile.

### Cited Findings
- In quasi-1D exhaust-pipe models, convective heat-transfer coefficients on pipe walls can be evaluated from skin-friction correlations using the **Chilton-Colburn analogy**. External-wall radiation strongly affects pipe temperature: emissivity variation changes dimensionless pipe temperature by over 40%. — [Quasi-1D model for exhaust gas and pipe with convective and radiative losses (ScienceDirect)](https://www.sciencedirect.com/science/article/abs/pii/S2451904920300214)
- Chilton-Colburn j-factor analogy: valid for fully developed turbulent flow in conduits with Re > 10 000, 0.7 < Pr < 160, L/d > 60. More accurate than the plain Reynolds analogy. — [Wikipedia: Chilton and Colburn J-factor analogy](https://en.wikipedia.org/wiki/Chilton_and_Colburn_J-factor_analogy); [Reynolds analogy](https://en.wikipedia.org/wiki/Reynolds_analogy)
- OpenWAM's C₂ source term covers friction and heat transfer, and gravity as well. — [OpenWAM numerical methods](https://openwam.webs.upv.es/docs/?p=180)
- GT-Power's momentum equation carries a friction factor f (with equivalent diameter D) and a local loss coefficient K. — [Dehner et al.](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)
- EngineLab includes a "thermal wall model". — [EngineLab README](https://github.com/zolaski333/EngineLab)

### Inferences
- **Source terms** (standard form, from memory):
  - Momentum: −(f/D)·½ρu|u|·A per unit length (Darcy f), or −(4Cf/D)·½ρu|u|·A (Fanning Cf).
  - Energy: q = h·(4/D)·(T_w − T)·A, with h = St·ρ·c_p·|u| and St = (Cf/2)·Pr^(−2/3) (Pr ≈ 0.7).
- **Wall ODE**: m_w·c_w·dT_w/dt = h·A_wet·(T_gas − T_w) − h_ext·A_ext·(T_w − T_amb) − ε·σ·A_ext·(T_w⁴ − T_amb⁴). The time constant is seconds to minutes, so update at 100 Hz–1 kHz. It gives realistic warm-up drift in pitch and timbre.
- **Friction time scale for acoustics**: at pipe acoustic velocities, friction damping of the fundamental is small compared with radiation and valve losses. Friction evaluated on the instantaneous u (quadratic) mostly damps the large blowdown pulses. In the linear waveguide layer, add viscothermal boundary-layer losses as a per-segment lowpass (attenuation ∝ √f/a). These control HF decay and ring length.
- **Stiffness**: explicit integration of friction/heat sources is stable when h·Δt/(ρ·c_v·D) ≪ 1. At audio-rate Δt ≈ 20 µs this is satisfied for normal exhaust conditions. Operator splitting (source step after flux step) is sufficient.

### Gaps
- Specific friction and heat-transfer correlations used by Winterbone & Pearson, Blair, WAVE or GT-Power for pulsating exhaust flow (which enhances h beyond steady correlations) were not retrieved.
- No quantitative data found on how much heat-transfer modelling changes the audible spectrum.

---

## 5. Turbocharger: turbine as restriction with pressure-ratio map, compressor map with surge line, shaft dynamics, wastegate and blow-off; minimal real-time models

### Takeaway
The standard 1D approach treats compressor and turbine as zero-length actuator-disk boundaries between pipes, driven by steady maps: compressor (ṁ, PR) = f(N) plus efficiency; turbine reduced mass flow vs PR at N, often as an equivalent nozzle/orifice. Add shaft inertia and a lag on compressor mass flow for stability. Surge emerges as a Helmholtz-type limit cycle of the compressor + duct + plenum + throttle system (Greitzer B-parameter). A real-time sound model can do the same with ~10 state variables plus map look-ups. The audible turbo "whine" and blow-off "psssh" are source models on top.

### Cited Findings
- In GT-Power, the compressor map (a `.cmp` text file) is looked up with speed and pressure ratio as inputs, giving mass flow and efficiency. The preprocessor extrapolates it to choke, zero flow and low speeds. The compressor is modelled as an **actuator disk** (pressure/density discontinuity, no gas angular momentum). — [Dehner, Selamet, Keller, Becker, SAE 2010-01-2142](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)
- GT-Power offers a **time constant to damp compressor mass-flow changes**, proportional to n rotor revolutions (n = 2 used). Without it, "a small change in pressure ratio at the peak of the characteristic can cause an unrealistically large change in mass flow rate in a single time-step". It also lets the compressor leave the steady map during surge. — [same](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)
- The turbine was replaced by a **drive torque on the shaft**, and the rotor inertia multiplier must stay at 1 near surge. — [same](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf)
- Mild surge occurs at the system's **Helmholtz resonance**: 7.3 Hz predicted and matched in that rig. Harmonics above 20 Hz become audible. Greitzer's critical B was around 0.8. The Greitzer model was extended to centrifugal compressors (Hansen) and to shaft-speed dynamics (Fink, Cumpsty & Greitzer). — [same](https://mae.osu.edu/sites/default/files/2021-11/J82.pdf); deep surge: [Dehner et al., J. Turbomach. 2016](https://asmedigitalcollection.asme.org/turbomachinery/article-abstract/138/11/111002/378647/Simulation-of-Deep-Surge-in-a-Turbocharger); [OSU J96 deep surge PDF](https://mae.osu.edu/sites/default/files/2021-11/J96.pdf); [OSU J98 surge in turbocharged engine intake](https://mae.osu.edu/sites/default/files/2021-11/J98.pdf)
- 1D compressor models under deep-surge operation, and an enhanced Greitzer model for a numerical surge limit. — [Energy 2017 (ScienceDirect)](https://www.sciencedirect.com/science/article/abs/pii/S0360544217317140); [IJTPP 2023](https://doi.org/10.3390/ijtpp8040048)
- UPV/CMT (OpenWAM group) validated a 1D twin-entry radial turbine model under nonlinear pulse conditions. — [Serrano et al., IJER 2021](https://journals.sagepub.com/doi/abs/10.1177/1468087419869157); [A procedure to achieve 1D predictive modelling of turbochargers](https://www.academia.edu/29747288/A_Procedure_to_Achieve_1D_Predictive_Modeling_of_Turbochargers)
- EngineLab drives boost with shaft power, blade/lobe orders and wastegate/dump-valve flows. It conserves total flow split between turbine and wastegate, but its **acoustic levels for boost remain semi-empirical**. — [EngineLab realtime-audio.md](https://github.com/zolaski333/EngineLab/blob/HEAD/docs/realtime-audio.md)

### Inferences
Minimal real-time turbo model (equations from standard turbomachinery practice, from memory):
- **Turbine** = orifice between exhaust manifold pipe end and downpipe: ṁ_t = A_eq(N, VGT)·p₀₃/√(R·T₀₃)·Ψ(p₄/p₀₃) (same Ψ as §2), fitted to the turbine map's reduced flow ṁ√T/p vs PR.
  - Power: P_t = ṁ_t·c_p·T₀₃·η_t·[1 − (p₄/p₀₃)^((γ−1)/γ)].
  - Treating it as an orifice makes the turbine a partially reflecting boundary for exhaust pulses. This is physically correct: turbos strongly muffle exhaust sound. It is also cheap.
- **Wastegate**: a parallel orifice with A_wg(duty), using the same law.
- **Compressor** = map lookup ṁ_c(N, PR) or PR(N, ṁ) with a first-order lag τ ≈ 2 revolutions (per GT practice): dṁ_c/dt = (ṁ_map − ṁ_c)/τ.
  - Power P_c = ṁ_c·c_p·T₀₁·[PR^((γ−1)/γ) − 1]/η_c.
  - Left of the surge line, extrapolate the characteristic into negative flow (cubic Moore-Greitzer shape) so the system can limit-cycle.
- **Greitzer lumped surge model**:
  - dṁ_c/dt = (A_c/L_c)·(Ψ_c(ṁ_c, N)·p₀₁ − p_plenum)
  - dp_plenum/dt = (a²/V_p)·(ṁ_c − ṁ_throttle)
  - B = U/(2a)·√(V_p/(A_c·L_c)); deep surge above B ≈ 0.8 (per the source above).
  - Surge frequency ≈ Helmholtz f = (a/2π)·√(A_c/(V_p·L_c)), typically a few to ~20 Hz: the "flutter/chatter" when lifting off without a blow-off valve.
- **Shaft**: J·dω/dt = (P_t·η_mech − P_c)/ω. A typical small turbo has J ~ 10⁻⁵–10⁻⁴ kg·m², so spool time constants are ~0.2–1 s.
- **Blow-off/dump valve**: a pressure-actuated orifice (spring + manifold vacuum reference) venting the compressor outlet to atmosphere or to the compressor inlet. The "psssh" is the radiated d(ṁ)/dt of this jet (monopole) plus broadband jet noise. Jet noise must be a separate noise source (shaped by Strouhal), since 1D gas dynamics produces no turbulence noise.
- **Whine**: blade-passing frequency f_BPF = N_blades·N/60 (compressor ~8–12 blades at 100–250 krpm → 15–50 kHz fundamental, often aliasing into audible subharmonics/orders). It is not produced by 1D gas dynamics. Add it as an order-tracked tonal source scaled by compressor power.
- **Cost**: ~10 states plus 2–3 bilinear map look-ups per step. Negligible, and runnable at audio rate or at the FV rate.

### Gaps
- No published "minimal real-time turbocharger model for sound synthesis" paper was found.
- Typical turbo inertia and blade counts above are from general knowledge, not fetched sources.
- Turbine attenuation of exhaust sound (transmission loss) was not quantified by any fetched source.

---

## 6. Throttle body: flow area vs angle, choked flow at small angles, whistle

### Takeaway
Model the throttle as a compressible orifice with A_eff = Cd(θ)·A_geom(θ). The geometric area of a thin elliptical plate is A = (πD²/4)·(1 − cos θ/cos θ₀). At small openings and idle/overrun manifold vacuum (p_man/p_amb < 0.528) the throttle is choked. Upstream (air box) acoustics are then decoupled from the manifold, and the throttle is a strong sound source (sonic jet) and a near-rigid reflector for intake waves. Whistle is an aeroacoustic (shear-layer/edge) phenomenon outside 1D gas dynamics and needs an explicit source model.

### Cited Findings
- Projected area of an infinitely thin elliptical plate closing at θ_CIB: **A_throttle = (πD²/4)·(1 − cos θ / cos θ_CIB)**. — [US patent (throttle flow, USPTO)](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5526787) (search-snippet derivation; the patent text was not fully read)
- Effective area is Cd·A_th, with Cd from a regression on flow and geometry. The standard reference is Heywood, *Internal Combustion Engine Fundamentals* (1988). — [same search set](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5526787)
- MathWorks throttle model: A_eff = (π/4)·D²·Cd(θ), with Cd tabulated vs throttle angle (0–90° breakpoints), fed into the isentropic orifice law with choked/subsonic/linearised branches. — [MathWorks Flow Restriction](https://www.mathworks.com/help/autoblks/ref/flowrestriction.html)
- A thesis compares heat transfer and flow through a throttle body. — [DiVA thesis](https://www.diva-portal.org/smash/get/diva2:23184/fulltext01)
- engine-sim models throttle as `flowAttenuation = cos(throttle·π/2)` multiplying an intake orifice flow constant, plus a parallel idle-circuit orifice. — [engine-sim src/intake.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/intake.cpp)

### Inferences
- **Implementation**: A(θ) from the formula above plus a leakage area A_leak (plate-to-bore clearance, typically 0.5–2% of bore area) so that A never reaches 0. Cd(θ) ≈ 0.6–0.85 via a short table. Then apply the orifice law.
  - Choked when p_man/p_up ≤ 0.528 (γ = 1.4). Idle manifold pressure ~0.3–0.4 bar, so the throttle is **choked at idle and on overrun**.
  - Choked flow is a one-way acoustic valve: downstream waves cannot propagate upstream through the sonic throat. Make the manifold-side boundary behave as a mass-flow source, i.e. near-rigid reflection.
- **Stiffness**: near P_ratio → 1 (WOT) use the linearised branch. With a 0D plenum after the throttle and a tiny plenum volume, the plenum ODE can be stiff at audio Δt. Check the stability limit Δt < V·p/(γ·RT·∂ṁ/∂p). Usually fine for litres of plenum.
- **Whistle / hiss**: at small openings a high-speed jet forms at the plate edges. Model it as band-limited noise or an edge-tone oscillator. Centre frequency f ≈ St·U_jet/h, with St ~0.2–0.5, U_jet up to sonic, and h the gap height. Amplitude scales with ṁ·U² (dipole-like). This is my heuristic proposal; I found no source quantifying throttle whistle.
- The throttle angle-to-area law is strongly nonlinear. Most of the airflow authority is in the first ~20–30° of opening, which drives the characteristic "tip-in" sound change.

### Gaps
- No source found on throttle-body whistle mechanisms or frequencies.
- Cd(θ) tables for real throttle bodies were not retrieved.

---

## 7. How acoustic radiation is extracted (tailpipe and intake orifice)

### Takeaway
At each open end, take the unsteady volume (or mass) flow leaving the pipe and radiate it as a monopole: p(r, t) = ρ₀·Q̇(t − r/c)/(4πr) = ṁ̇(t − r/c)/(4πr) (compact source, ka ≪ 1). Using the pressure at the pipe end directly is wrong, since p ≈ p_atm at an open end. Add the Levine-Schwinger reflection so the energy that is radiated leaves the pipe, and add ka-dependent directivity above ka ~ 1. Sum tailpipes and intake mouth with their own delays and 1/r.

### Cited Findings
- EngineLab: each outlet ends in a "causal passive load" of a free or flanged circular opening. The reflected wave returns to the collector. **"the volume-velocity acceleration gives a monopole reference pressure"**, propagated to left/right microphones with exact distance, 1/r decay, r/c delay and ka-dependent directivity. Stereo comes from physical path differences. — [EngineLab realtime-audio.md](https://github.com/zolaski333/EngineLab/blob/HEAD/docs/realtime-audio.md)
- EngineLab's intake network (`AcousticIntakeNetwork`) propagates valve flows through runners, plenum, throttle and air inlet. It also has a structural modal radiator driven by gas forces. — [same](https://github.com/zolaski333/EngineLab/blob/HEAD/docs/realtime-audio.md)
- engine-sim does **not** use monopole radiation. It uses the exhaust-runner gauge pressure (+0.1 × dynamic pressure) delayed by L/343 and scaled by 1/L², then convolved with an IR. — [engine-sim piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)
- The Levine–Schwinger reflection and causal reflection-function fits for the open end are in §2. — [Silva et al. 2008](https://arxiv.org/pdf/0811.3625)

### Inferences
- **Formula** (standard compact monopole): p_far(r, t) = [dṁ_exit/dt](t − r/c₀)/(4πr), where ṁ_exit = ρ_e·u_e·A_e from the last FV cell face (or u⁺/u⁻ from waveguide characteristics: U = (p⁺ − p⁻)/Zc).
  - Discretise with a first difference at the audio rate. Differentiation is +6 dB/oct, so high-pass the mean flow (the DC ṁ radiates nothing) with a ~5–10 Hz DC blocker before or after.
  - With ṁ in kg/s: at r = 1 m, a 0.05 kg/s peak-to-peak pulse at 200 Hz gives |dṁ/dt| ≈ 2π·200·0.025 ≈ 31 kg/s² → p ≈ 2.5 Pa ≈ 102 dB SPL. That is a plausible order of magnitude for a car exhaust at 1 m (my calculation).
- **Hot jet correction**: use ρ₀ = ambient density if radiating Q̇ (volume velocity) into ambient air. Using ṁ̇ with hot-gas density underestimates by T_exit/T_amb. Pick one convention consistently. The ṁ̇ version is the one requested; ṁ̇/(4πr) is exact for a compact mass source.
- **Beyond compact**: for ka > ~0.5, apply the unflanged-pipe directivity (forward-beaming) as a first-order shelf/lowpass for off-axis listener positions.
- **Intake**: identical treatment at the air-filter mouth. With a choked throttle the intake-mouth radiation is weak and dominated by the air-box Helmholtz resonance and throttle hiss.
- **Double counting**: if a measured IR (car body, room) is applied after the monopole, it must not include the pipe resonances already simulated (EngineLab makes this point explicitly).
- **Mean-flow radiation correction** (Munt 1977/1990 theory for an open pipe with flow) is neglected by EngineLab. Its main effect is to raise the reflection-coefficient modulus above 1 at low Strouhal number (vortex shedding) and to modify directivity. It is probably secondary for sound design.

### Gaps
- No peer-reviewed engine-sound paper was found that validates ṁ̇/(4πr) tailpipe radiation against measured drive-by or near-field microphone data. The approach is standard acoustics, but validation for engine sound specifically was not found in this session.

---

## 8. Existing implementations and resources

### Takeaway
The best open code to study numerics and boundaries is OpenWAM (C++, GPL, LW + TVD, full engine BC library including turbo, junctions, catalysts and DPF). The most directly relevant sound-oriented references are EngineLab (quasi-1D + characteristic waveguides + monopole radiation) and engine-sim (0D + delay + convolution). The authoritative textbooks are Winterbone & Pearson (MoC and LW, junctions), Benson (MoC, boundary conditions), Blair (GPB) and Toro (Riemann solvers).

### Cited Findings
- **OpenWAM** (C++/GPL, CMT-UPV). Repos: [CMT-UPV/OpenWAM](https://github.com/CMT-UPV/OpenWAM), [releases](https://github.com/CMT-UPV/OpenWAM/releases), [SourceForge](https://sourceforge.net/projects/openwam/), [refactored fork](https://github.com/EliDeCo/OpenWAM-Refactored). Docs: [openwam.webs.upv.es/docs](https://openwam.webs.upv.es/docs/). It includes external-control classes (e.g. `TTGV.h`, [link](https://github.com/CMT-UPV/OpenWAM/blob/master/Source/Extern/TTGV.h)).
- **Winterbone & Pearson**, *Theory of Engine Manifold Design — Wave Action Methods for IC Engines* (2000), co-authors of the pressure-loss junction work. — [SAE 2003-01-0370 summary](https://saemobilus.sae.org/papers/a-multi-pipe-junction-model-one-dimensional-gas-dynamic-simulations-2003-01-0370)
- **Blair's GPB finite system method**: most accurate in steep diffusers vs non-homentropic MoC (secondary). — [OSTI 282269](https://www.osti.gov/biblio/282269)
- **Galindo et al. independent-time-discretization 1D model** (OpenWAM group). — [ResearchGate](https://www.researchgate.net/publication/267577863_Description_and_Analysis_of_a_One-Dimensional_Gas-Dynamic_Model_With_Independent_Time_Discretization)
- **Simulink manifold gas dynamics coupled to single-cylinder models** (a possible simple reference implementation). — [ResearchGate](https://www.researchgate.net/publication/237900266_Manifold_Gas_Dynamics_Modeling_and_Its_Coupling_With_Single-Cylinder_Engine_Models_Using_Simulink)
- **CE–SE scheme** (space-time conservation element) validated for tapered engine ducts. — [ResearchGate](https://www.researchgate.net/publication/230250568_Experimental_validation_of_a_new_semi-implicit_CE-SE_scheme_for_the_calculation_of_unsteady_one-dimensional_flow_in_tapered_ducts)
- **Real-time/HiL 1D**:
  - [SAE 2018-01-1245](https://www.sae.org/publications/technical-papers/content/2018-01-1245/)
  - [MTZ: crank-angle resolved realtime engine simulation](https://link.springer.com/article/10.1007/BF03227905)
  - [Engine control using a real-time 1D engine model (Springer)](https://link.springer.com/chapter/10.1007/978-3-658-20736-6_20)
  - [GT FRM](https://www.gtisoft.com/news-archives/realtime-gt-power-engine-models-now-run-2x-faster/)
- **Lagrange-Eulerian GPU gas-dynamics integration** (general, not engine-specific). — [arXiv 1912.04855](https://arxiv.org/pdf/1912.04855)
- **Sound-oriented**:
  - [engine-sim](https://github.com/ange-yaghi/engine-sim), [community edition](https://github.com/Engine-Simulator/engine-sim-community-edition), [EngineLab](https://github.com/zolaski333/EngineLab)
  - [Antonio-R1 WebAudio engine sound generator](https://github.com/Antonio-R1/engine-sound-generator)
  - Baldan et al. SIVE 2015 ([ResearchGate](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments))
  - [Sound Design Toolkit (Baldan, Delle Monache)](https://www.sciencedirect.com/science/article/pii/S2352711017300195)
  - [Four Decades of Digital Waveguides (arXiv 2604.12878, 2026 review)](https://arxiv.org/pdf/2604.12878) (too large to fetch; not read)
  - [DAFx08: direct simulation for wind instrument synthesis](http://legacy.spa.aalto.fi/dafx08/papers/dafx08_26.pdf): the reed/bore nonlinear-termination analogue
  - [DAFx-14 sample-based car engine noise](https://www.researchgate.net/publication/345253375_Sample_Based_Synthesis_of_Car_Engine_Noise)
- **Open-end radiation formulas**: [Levine & Schwinger 1948](https://journals.aps.org/pr/abstract/10.1103/PhysRev.73.383); [Silva et al. 2008 fits](https://arxiv.org/pdf/0811.3625)

### Inferences
Suggested implementation spec for BESS (synthesis of the above; my recommendation):
1. **Engine core**: 0D cylinders plus valve orifice boundaries (§2), evaluated at the gas-solver rate.
2. **Gas network**: quasi-1D FV, HLLC or LW+TVD, with source terms for area, friction and heat.
   - Run at f_gas = 24–48 kHz, Δx chosen for CFL 0.8 at the hottest pipe (≈ 20–40 mm), with local time stepping for short/cold pipes.
   - Plenums, muffler chambers and the catalyst front volume are 0D, or 1D with Darcy–Forchheimer sources.
   - Junctions use constant-pressure (cheap) or Bassett pressure-loss (directional) models.
   - Turbo as orifice/map boundaries plus the shaft ODE (§5).
3. **Audio layer at fs**: linear characteristic waveguides per pipe, with fractional delays L/(c ± u) from the slow solver's mean state, viscothermal and packing lowpass filters, and scattering junctions with admittance A/(ρc). They are driven by the perturbation at the valve boundaries (EngineLab p± decomposition) or by band-splitting the FV output: FV below ~1.5 kHz, waveguide above, with a Linkwitz-Riley crossover.
4. **Termination**: Levine–Schwinger open-end filters, and a monopole ṁ̇/(4πr) with delay r/c to each listener position.
5. **Add-on sources outside 1D physics**: throttle and blow-off jet hiss, turbo blade-pass whine, and mechanical/structural radiation.

Stability checklist:
- CFL ≤ 0.8–0.9 at max(|u|+a), recomputed each block, with automatic sub-stepping.
- Positivity guards on ρ and p, with fallback to a first-order flux on the offending cell.
- Linearised orifice law near Δp = 0.
- Mass-flow lag (≈ 2 revolutions) on the compressor map.
- DC blocker before the radiation differentiator.
- Denormal flushing in the waveguide filters.

### Gaps
- Lotus Engine Simulation, Ricardo WAVE and GT-Power theory manuals (numerics, recommended Δx) were not accessible. Typical industry discretisation guidance (for example, discretisation length relative to bore) could not be sourced.
- No paper was found that reports audio-rate (≥ 44.1 kHz) execution of a full nonlinear 1D engine gas-dynamics model with measured CPU cost. Evidence for feasibility is indirect: EngineLab's self-reported realtime factor and GT FRM's ms-rate HiL.
