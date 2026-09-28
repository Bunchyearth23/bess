# engine-sim (AngeTheGreat / ange-yaghi) internals: a code-level blueprint for a real-time, physics-driven engine sound synthesizer

Source basis: a shallow clone of `github.com/ange-yaghi/engine-sim` master at commit `85f7c3b` (last commit 2023-01-22), plus the `simple-2d-constraint-solver` ("scs") submodule. All file paths below are relative to that repo. The README says the project "has moved" to `Engine-Simulator/engine-sim-community-edition`. This analysis covers only the original `ange-yaghi` master. Base URL for the citations: `https://github.com/ange-yaghi/engine-sim/blob/master/`.

## Architecture: the classes and how they connect

### Takeaway
engine-sim couples two solvers in one step loop. The mechanism (crank, rods, pistons, clutch, vehicle) is a 2D rigid-body constraint system (scs). The breathing and combustion side is a network of lumped 0-D `GasSystem` volumes linked by orifice flows. The two meet at `CombustionChamber`, which is an scs `ForceGenerator`: it pushes the piston with pressure force and reads the piston position back to set the chamber volume. Audio is a separate stage (`Synthesizer`) fed with one scalar per exhaust system per simulation step.

### Cited Findings
- `Engine` owns arrays of `Crankshaft`, `CylinderBank`, `CylinderHead` (one per bank), `Piston`, `ConnectingRod`, `ExhaustSystem`, `Intake` and `CombustionChamber` (one per cylinder), plus an `IgnitionModule`, a `Fuel` and a `Throttle*`. The output shaft is always `m_crankshafts[0]`. — [src/engine.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine.cpp)
- `Engine::createSimulator` builds a `PistonEngineSimulator` with `SystemType::NsvOptimized` and calls `setFluidSimulationSteps(8)`. — [src/engine.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine.cpp)
- `Simulator` (base class) owns the scs `RigidBodySystem`, the `Synthesizer`, a `Dynamometer`, a `StarterMotor`, the vehicle rotating mass and a `VehicleDragConstraint`. `PistonEngineSimulator` implements `simulateStep_()` and `writeToSynthesizer()`. — [include/simulator.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/simulator.h), [src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)
- `PistonEngineSimulator::loadSimulation` builds the constraint graph as follows:
  - each crankshaft: a `FixedPositionConstraint` to its world position and a `RotationFrictionConstraint` bounded by ±`frictionTorque`;
  - secondary cranks: a `ClutchConstraint` to the output shaft;
  - each piston: a `LineConstraint` along the bank axis `(cos(angle+π/2), sin(angle+π/2))`;
  - each cylinder: two `LinkConstraint`s (rod small end ↔ piston wrist pin; rod big end ↔ crank journal, or ↔ master-rod journal for radials/articulated rods);
  - each `CombustionChamber` is registered with `addForceGenerator`. Constraint gains are `ks = 5000` and `kd = 10`.
  - The dyno, the starter motor (`m_maxTorque = starterTorque`, `m_rotationSpeed = -starterSpeed`), the transmission clutch and the vehicle drag are also added as constraints.

  — [src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)
- `CylinderHead` holds the intake and exhaust port-flow `Function`s (flow constant vs. valve lift), a `Valvetrain` (`StandardValvetrain` = one intake camshaft plus one exhaust camshaft; `VtecValvetrain` also exists), the chamber volume and the runner volumes and cross-sections. Per cylinder it stores `{exhaustSystem, intake, soundAttenuation = 1.0, headerPrimaryLength = 0.0}`. — [include/cylinder_head.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/cylinder_head.h), [src/cylinder_head.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/cylinder_head.cpp)
- Each `CombustionChamber` holds three `GasSystem`s: `m_system` (the cylinder), `m_intakeRunnerAndManifold` and `m_exhaustRunnerAndPrimary`. Several cylinders share one `Intake` (plenum) and one `ExhaustSystem` (collector). — [include/combustion_chamber.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/combustion_chamber.h)
- `Transmission` is a `ClutchConstraint` between the output crank and a vehicle rotating mass, bounded by ±`maxClutchTorque * clutchPressure` (0 in neutral). `VehicleDragConstraint` applies aero drag using air density computed from 1 atm and 25 °C. — [src/transmission.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/transmission.cpp), [src/vehicle_drag_constraint.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/vehicle_drag_constraint.cpp)
- The README describes the project as "designed specifically to produce engine audio and simulate engine response characteristics. It is NOT a scientific tool". — [README.md](https://github.com/ange-yaghi/engine-sim/blob/master/README.md)

### Inferences
- A Rust port can keep the same split: a `Mechanism` (a closed-form slider-crank per cylinder would do for inline and V engines), `GasNode`s with `flow(a, b, k, dt)` edges, `Chamber`s acting as the coupling, and a `Synth` fed by a lock-free queue. The general 2D constraint solver is only needed for articulated or radial rods and for exotic layouts.

### Gaps
- `VtecValvetrain`, `Dynamometer`, `StarterMotor` and `Governor` were read only at a skim level. None of them is audio-critical.

## Gas model: state, ideal gas, γ, the flow function, choked vs subsonic, momentum and dynamic pressure

### Takeaway
Each `GasSystem` is a 0-D control volume with state `{n_mol, E_k (internal energy), V, momentum[2], mix{p_fuel, p_inert, p_o2}}`. It obeys the ideal-gas law in its energy form, with γ derived from the degrees of freedom (default 5, so γ = 1.4). Flow between two volumes uses the standard isentropic compressible orifice equation, with a lumped "flow constant" k calibrated from flow-bench CFM. There is a crude per-volume 2D momentum and dynamic-pressure term. Every volume is 0-D: there is no 1-D wave or pipe discretisation anywhere.

### Cited Findings
- The state struct is `State { double n_mol; double E_k; double V; double momentum[2]; Mix mix; }` with `Mix { p_fuel = 0, p_inert = 1, p_o2 = 0 }` (mole fractions). — [include/gas_system.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/gas_system.h)
- Thermodynamic relations (all in `include/gas_system.h`):
  - `pressure = E_k / (0.5 * dof * V)`
  - `temperature = E_k / (0.5 * dof * n * R)`
  - `kineticEnergyPerMol(T) = 0.5 * T * R * dof`
  - `heatCapacityRatio(dof) = 1 + 2/dof`
  - `mass = 0.02897 kg/mol * n`. Every gas uses the molar mass of air, `AirMolecularMass = 28.97 g/mol`.
  - `c = sqrt(P*γ/ρ)`
  - Constants: `R = 8.31446261815324`.

  — [include/gas_system.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/gas_system.h), [include/constants.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/constants.h), [include/units.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/units.h)
- `initialize(P, V, T, mix, dof = 5)` sets `n = PV/(RT)` and `E_k = T*0.5*dof*n*R`, and caches:
  - `chokedFlowLimit = (2/(γ+1))^(γ/(γ-1))`
  - `chokedFlowRate = sqrt(γ) * (2/(γ+1))^((γ+1)/(2(γ-1)))`

  — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)
- Volume change is compression work. `changeVolume(dV)` computes `L = (V+dV)^(1/3)`, `dL = -dV/L²`, `W = dL*P*L²` (which equals `-P*dV`), then `E_k += W`. The chamber calls `m_system.setVolume(getVolume())` once per simulation step, in `CombustionChamber::update`. — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp), [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)
- Flow function `GasSystem::flowRate(k, P0, P1, T0, T1, γ, chokedLimit, chokedRate)`:
  - The upstream side is whichever pressure is higher. `r = p_T/p_0`.
  - If `r <= chokedFlowLimit` (choked): `rate = chokedRate / sqrt(R*T0)`.
  - Otherwise (subsonic): `s = r^(1/γ)` and `rate = sqrt(max(0, (2γ/(γ-1)) * s*(s - r)) / (R*T0))`.
  - The result is `rate * direction * p0 * k`, in mol/s.
  - k therefore bundles C_d·A and the molar-mass factor; the gas `mass` is never used in this function.

  — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)
- Flow constants come from CFM ratings. `flowConstant(targetFlow, P, ΔP, T, γ)` evaluates the same equation and returns `targetFlow / rate`. Two helpers use it:
  - `k_28inH2O(cfm)` uses P = 1 atm, ΔP = 28 inH2O, T = 25 °C, γ(5) = 1.4. This is the flow-bench standard, used for port flow and blowby.
  - `k_carb(cfm)` uses ΔP = 1.5 inHg. This is the carburettor rating standard, used for the throttle, runners, primaries and outlet.
  - Unit constants: `scfm = 0.002641 lbmol/min`, `inHg = 3386.3887 Pa`, `inH2O = inHg*0.0734824`.

  — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp), [include/units.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/units.h)
- Two-volume flow `GasSystem::flow(FlowParameters)`:
  - Effective pressures are `P_0 = p0.pressure() + p0.dynamicPressure(dir)` and `P_1 = p1.pressure() + p1.dynamicPressure(-dir)`. The side with the higher value is the source.
  - Moles moved: `flow = dt * flowRate(...)`, clamped to `[0, 0.9 * source.n]`. `pressureEquilibriumMaxFlow` is computed but not used as a clamp in this overload.
  - Stage 1: the sink calls `gainN(flow, source E_k/mol, source.mix)`, which mixes by mole fractions, and the source calls `loseN`. Momentum moves in proportion to the moved fraction. Any change in bulk kinetic energy is subtracted from the sink's `E_k` so that energy is conserved.
  - Stage 2: the moved "slug" gets a velocity `clamp((fractionVolume / crossSection)/dt, 0, c)` along the flow direction. This momentum is added to both the source and the sink, and the matching bulk-KE change is again subtracted from each side's `E_k`.
  - `E_k` is clamped to ≥ 0.

  — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)
- `dynamicPressure(dx, dy)`: with `v = (momentum·dir)/mass` (0 if v ≤ 0) and `M² = v²/c²`, it returns `P_static * (sqrt(x^(dof+2)) - 1)` where `x = 1 + (γ-1)/2 * M²`. This is the isentropic stagnation formula `P((1+(γ-1)/2 M²)^(γ/(γ-1)) - 1)`, special-cased with `x^7` for dof = 5 and `x^5` for dof = 3. — [include/gas_system.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/gas_system.h)
- `updateVelocity(dt, beta)`:
  - Treats the volume as a box of `width × height × depth`, with `depth = V/(width*height)` and geometry set by `setGeometry(width, height, dx, dy)`.
  - Applies the dynamic pressure on the four faces as a force: `momentum -= ΣF * dt * beta`.
  - Adjusts `E_k` for the change in kinetic energy.
  - `beta` is the per-part "velocity_decay" parameter.

  Related helpers:
  - `dissipateExcessVelocity()` clamps |v| to the speed of sound c and returns the excess kinetic energy to `E_k`.
  - `dissipateVelocity(dt, τ)` applies exponential damping. Nothing in the files read here calls it.

  — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)
- `flow(k, dt, P_env, T_env, mix)` exchanges with an infinite reservoir (used for blowby). It clamps to the pressure-equilibrium maximum `-(P_env*(0.5*dof*V) - E_k)/E_k_per_mol`. — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)
- Chemistry in `react(n, mix)` assumes `25 O2 + 2 C8H16 → 16 CO2 + 18 H2O`. Consumed fuel and O2 are limited by the stoichiometric ratios. Products count as "inert", with a mole-count ratio of `(16+18)/(25+2)`. It returns the moles of fuel burned. — [src/gas_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/gas_system.cpp)

### Inferences
- The model is a "filling-and-emptying" (0-D) engine model with an ad-hoc momentum term. The momentum and dynamic-pressure terms plus the `velocity_decay` β are what give runners some inertia: a Helmholtz or ram-like effect without real wave propagation. A Rust port can copy this directly. It costs a handful of flops per edge, with one `powf` in subsonic flow (`r^(1/γ)`).
- All species share the molar mass and γ of air. The 0.9·n clamp and the ≥ 0 clamps on `E_k` make it robust but not conservative in extreme cases.

### Gaps
- No derivation or documentation of the momentum model from the author was found in the repository. Its physical validity is not established.

## Combustion chamber: volume vs crank angle, combustion, heat transfer, knock and misfire, randomness

### Takeaway
The chamber volume comes from the constraint solver's piston position, not from a closed-form slider-crank. Combustion is a geometric flame-front model: a growing cylinder of burned gas whose speed is turbulent flame speed = f(turbulence/laminar speed) × laminar speed. A Gülder-style laminar correlation feeds it. Burn "efficiency" is randomised per event, and that is the source of cycle-to-cycle variation. Heat loss is a fixed-coefficient convection term toward a 90 °C wall. No knock model exists.

### Cited Findings
- Volume: `V = A_bore * (deckHeight - s - compressionHeight) + chamberVolume - pistonDisplacement`. Here `s` is the piston's relative position projected on the bank axis, and `pistonDisplacement` is the dish/dome volume parameter. — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp) `getVolume()`
- The engine's displacement for the UI comes from numerically sweeping the crank angle at 1000 samples (`Engine::calculateDisplacement`). The author's comment: "There is a closed-form/correct way to do this which I really don't feel like deriving". — [src/engine.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine.cpp)
- Piston force (`CombustionChamber::apply`, an scs ForceGenerator): `F = -A*(P_cyl - P_crankcase)` along the bore axis, with the crankcase at 1 atm. A friction force is added:
  - `F_coul = 0.06 * |cylinder wall constraint force|`
  - a Stribeck breakaway term `sqrt(2)*e*(F_brk - F_coul) * exp(-(v/v_st)²) * (v/v_st)`, with `v_st = 0.1*sqrt(2)` m/s
  - `+ F_coul * tanh(v/(0.01))`
  - `+ 20 N/(m/s) * v`
  - The total is scaled by `min(|v|, 1e-3)/1e-3`.
  - Defaults are in `FrictionModelParams`: `frictionCoeff 0.06`, `breakawayFriction 50 N`, `breakawayFrictionVelocity 0.1`, `viscousFrictionCoefficient 20`.

  — [include/combustion_chamber.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/combustion_chamber.h), [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)
- The script exposes `cylinder_friction_parameters`, but in the files read here nothing copies them into `m_frictionModel`, so the C++ defaults apply. — [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr), [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)
- Ignition timing (`IgnitionModule::update`):
  - Each plug has a firing angle within the 4π cycle.
  - `advance = timingCurve.sampleTriangle(-ω)` (the timing map is indexed by crank speed).
  - A spark fires when `positiveMod(plugAngle - advance, 4π)` falls between the previous and current cycle angle.
  - Rev limiter: if `|ω| > revLimit`, ignition is cut for `limiterDuration`. Defaults are 6000 rpm and 0.5 s in C++, or 7000 rpm and 0.5 s in the script.

  — [src/ignition_module.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/ignition_module.cpp), [include/ignition_module.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/ignition_module.h)
- Ignition acceptance and randomness (`CombustionChamber::ignite`):
  - The spark is refused if `p_fuel == 0` or if `(p_o2/p_fuel)/molecularAfr` is outside `[0.5, 1.9]`.
  - `dilution = p_inert/(p_o2/0.7) - 1`.
  - `turbulence = meanPistonSpeedToTurbulence(mean |piston speed| over a 256-slot per-cycle table)`.
  - `mixingFactor = 1 - clamp(turb/maxTurbulenceEffect) * clamp(1 - dilution/maxDilutionEffect)`.
  - `rand_s = lowEfficiencyAttenuation * ((1-randomness) + randomness*U[0,1])`, using C `rand()`.
  - `efficiency = (mixingFactor*rand_s + (1-mixingFactor)) * maxBurningEfficiency`.
  - `flameSpeed = fuel.flameSpeed(turb, afr, T, P, peakPressure, 160 psi)`.

  — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)
- The turbulence map is hard-wired in `EngineNode::generate`: `Function(30 samples, filterRadius 1)`, `turb = 0.5 * meanPistonSpeed` for 0 to 29 m/s. — [scripting/include/engine_node.h](https://github.com/ange-yaghi/engine-sim/blob/master/scripting/include/engine_node.h)
- Fuel defaults. The C++ header and the script default differ on `max_dilution_effect`:

  | Parameter | Default |
  |---|---|
  | molecular mass | 100 g/mol |
  | energy density | 48.1 kJ/g |
  | density | 0.755 kg/L |
  | `molecularAfr` | 25/2 |
  | `burningEfficiencyRandomness` | 0.5 |
  | `lowEfficiencyAttenuation` | 0.6 |
  | `maxBurningEfficiency` | 0.8 |
  | `maxTurbulenceEffect` | 2.0 |
  | `maxDilutionEffect` | 50.0 in the C++ header; 10.0 in the script `fuel` node |

  — [include/fuel.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/fuel.h), [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr)
- Laminar burning velocity, "assuming fuel is gasoline" (`Fuel::laminarBurningVelocity`):
  - `er = afr/molecularAfr`
  - `S_L0 = 0.305 - 0.549*(er - 1.21)²` (m/s)
  - `α = 2.4 - 0.271*er^3.51`
  - `β = -0.357 + 0.14*er^2.77`
  - `S_L = S_L0 * (T/298)^α * (P/1 atm)^β`

  Turbulent speed: `S_T = turbulenceToFlameSpeedRatio.sampleTriangle(turb/S_L) * S_L`. The default ratio table is `(0 → 3), (x → 1.5x)` for x = 5 to 45 with filter radius 5. The EJ25 example scales it by 1.1 to 1.25 at high x. The `firingPressure` and `motoringPressure` arguments are unused (`p_adjustment = 1.0`). — [src/fuel.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/fuel.cpp), [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr), [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr)
- Flame propagation (`CombustionChamber::flow`, each fluid substep):
  - The burned region is a cylinder with radius `travel_x ≤ bore/2` and height `travel_y ≤ V/A_bore`.
  - `travel_y` is first scaled by the volume expansion `V/lastV`.
  - Both grow by `dt*flameSpeed`.
  - `litVolume = π·x²·y - π·x0²·y0` and `n = (litVolume/V) * n_cyl`.
  - `fuelBurned = react(n*efficiency, globalMix at ignition)`, then `E_k += fuelBurned * M_fuel * 48.1 MJ/kg`.
  - Combustion stops when the front stops growing, or when `|intakeFlow| > 1e-9` (the intake valve opens).

  — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)
- Heat transfer: each fluid substep applies `E_k += (363.15 K - T) * A_wall * 100 * dt`, where `A_wall = π·bore·h + 2·A_bore`. That is h = 100 W/m²K toward a fixed 90 °C wall. — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)
- Blowby: `m_system.flow(piston.blowby_k, dt, 1 atm, 25 °C)` each fluid substep. Examples use `blowby: k_28inH2O(0.001 to 0.002)`. — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp), [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr)
- Fuel mixing is carburettor-like, in `Intake::process`:
  - The atmosphere reservoir is reset each substep to a pre-mixed charge with `ideal_afr = 0.8 * molecularAfr * 4` (= 40 mol air per mol fuel by default).
  - Air is modelled as `p_o2 = 0.25`, `p_inert = 0.75`.
  - A separate idle circuit supplies a very rich mix (`idle_afr = 2.0`).
  - No port-injection or wall-film model exists.

  — [src/intake.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/intake.cpp)

### Inferences
- `er` as coded is the O2-to-fuel mole ratio divided by the stoichiometric ratio, which is the air-fuel equivalence ratio λ, not φ. The correlation constants (φ_m = 1.21, B_m = 30.5 cm/s, B_φ = -54.9 cm/s) are those usually quoted for gasoline in Metghalchi–Keck/Gülder-type forms. Feeding λ in where φ is expected looks like a naming or semantic inversion. It has little audible effect because the turbulence ratio table dominates.
- By default the intake charge is rich: 40 mol "air" at 25 % O2 gives 10 mol O2 per mol fuel, against a stoichiometric 12.5, so λ ≈ 0.8.
- Misfire happens only through the AFR window, empty cylinders or the rev limiter. Partial burns come from the efficiency randomness. Cyclic variability is therefore a uniform random number per ignition that scales the fraction of mixture that can react. That is cheap to reproduce in Rust (use a seeded RNG).

### Gaps
- No knock or auto-ignition model and no residual-gas-temperature ignition were found in the code.

## Valves: cam lobes, lift→flow tables, overlap

### Takeaway
Each camshaft samples one shared lobe `Function` (lift vs cam angle) at `camAngle + lobeCenterline/2`. Lift maps to an orifice flow constant through a per-head table of CFM@28"H2O vs lift, converted with `k_28inH2O`. Overlap is not special-cased: during overlap both valves flow through the cylinder volume simultaneously.

### Cited Findings
- Cam angle and lobe sampling:
  - `Camshaft::getAngle() = fmod((crank.getAngle() + advance) * 0.5, 2π)`.
  - `valveLift(lobe) = sampleLobe(getAngle() + m_lobeAngles[lobe])`.
  - `setLobeCenterline(lobe, crankAngle)` stores `crankAngle/2`.
  - `sampleLobe` wraps θ to [-π, π) and calls `lobeProfile.sampleTriangle(θ)`.
  - The default `baseRadius` is 600 thou (display only).

  — [src/camshaft.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/camshaft.cpp), [include/camshaft.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/camshaft.h)
- `harmonic_cam_lobe(duration_at_50_thou, gamma, lift, steps = 100)`, generated by `GenerateHarmonicCamLobeNode`:
  - `angle = dur50/4`
  - `s = (2*0.050in/lift)^(1/γ) - 1`
  - `k = acos(s)/angle` and `extents = π/k`
  - `step = extents/(steps - 5)`
  - Samples ±x of `lift(x) = L * (0.5 + 0.5*cos(k x))^γ`, which is 0 beyond `extents`.
  - Filter radius = step.

  — [scripting/include/actions.h](https://github.com/ange-yaghi/engine-sim/blob/master/scripting/include/actions.h), [es/actions/actions.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/actions/actions.mr)
- `Function::sampleTriangle(x)` is a normalised weighted average of the nearest samples within `filterRadius`, with weights `(r - |dx|)/r`. It clamps to the end values outside the range. — [src/function.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/function.cpp)
- Port flow:
  - `CylinderHead::intakeFlowRate(cyl) = intakePortFlow.sampleTriangle(intakeValveLift(cyl))` (exhaust is analogous).
  - Heads define `add_flow_sample(lift_thou, cfm)` as `add_sample(lift*thou, k_28inH2O(cfm))`, in a `function(50 thou)` (filter radius 50 thou).
  - EJ25 intake table: 0, 58, 103, 156, 214, 249, 268, 280, 280, 281 CFM at 0 to 450 thou in 50-thou steps.
  - EJ25 exhaust table: 0, 37, 72, 113, 160, 196, 222, 235, 245, 246 CFM.

  — [src/cylinder_head.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/cylinder_head.cpp), [es/part-library/parts/heads.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/part-library/parts/heads.mr), [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr)
- Lobe separation and timing are set in scripts as follows:
  - exhaust lobes at `360° - exhaust_lobe_center + i/N * 720°`;
  - intake lobes at `360° + intake_lobe_center + i/N * 720°`.
  - EJ25 values: intake centre 117°, exhaust centre 112°, harmonic lobes 232°/236° @ 0.050", γ = 2.0, 9.78 mm and 9.60 mm lift.
  - Part-library example: `stock_454_intake_lobe_profile` = 194° @ 0.050", γ 0.8, 390 thou.

  — [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr), [es/part-library/parts/cam_lobes.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/part-library/parts/cam_lobes.mr)
- The chamber's flow order per substep is: plenum→intake runner, intake runner→cylinder, cylinder→exhaust runner, exhaust runner→collector. Each edge is followed by `dissipateExcessVelocity`. Then `updateVelocity` runs for the intake runner (β = intake `velocity_decay`), the cylinder (β = 0.5) and the exhaust runner (β = exhaust `velocity_decay`). — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp)

### Inferences
- Because each edge is evaluated sequentially (a Gauss-Seidel-like operator split) inside 8 substeps, overlap scavenging and reversion emerge naturally from the pressure differences. Reproduce this in the same order to get similar behaviour.
- Reading the harmonic lobe code, `duration_at_50_thou` is given in crank degrees and `x` is in cam radians: `dur/4` is half the duration expressed in cam angle.

### Gaps
- Valve-lash, valve-dynamics (float) and ramp modelling are absent: lift is purely kinematic.

## Intake and exhaust: runners, headers, plenum and collector volumes, pipe model, throttle

### Takeaway
All intake and exhaust elements are lumped 0-D volumes in a chain:

atmosphere → throttle/idle orifice → plenum (shared) → per-cylinder "runner+manifold" volume → intake valve → cylinder → exhaust valve → per-cylinder "runner+primary" volume → primary orifice → collector (shared per exhaust system) → outlet orifice → atmosphere.

Pipe lengths only set volumes and box geometry for the momentum term. Header plus collector length also sets a pure audio delay line and a 1/L² gain. No 1-D pipe cells or wave solver exist.

### Cited Findings
- Intake runner volume = `head.intakeRunnerVolume + intakeRunnerCrossSection * intake.runnerLength`, with geometry `(overallLength, sqrt(A))`. Exhaust runner volume = `head.exhaustRunnerVolume + exhaustRunnerCrossSection * (exhaust.primaryTubeLength + head.headerPrimaryLength[cyl])`. Both are initialised at 1 atm and 25 °C. — [src/combustion_chamber.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/combustion_chamber.cpp) `initialize()`
- Intake plenum:
  - `m_system` is initialised with `volume` and `CrossSectionArea`.
  - The atmosphere is a 1000 m³ volume at 1 atm and 25 °C, reset every substep.
  - Throttle flow constant: `k = cos(throttlePlatePosition*π/2) * InputFlowK`, where `throttlePlatePosition = IdleThrottlePlatePosition * m_throttle`.
  - The idle circuit runs in parallel with `IdleFlowK`.
  - Then `dissipateExcessVelocity` and `updateVelocity(dt, VelocityDecay)`.

  — [src/intake.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/intake.cpp), [include/intake.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/intake.h)
- Throttle linkage: `DirectThrottleLinkage::setSpeedControl(s)` sets `m_throttlePosition = 1 - s^γ` (1 = closed). `Engine::setThrottle` writes it to every intake. A `Governor` alternative drives the throttle through a speed PD law. — [src/direct_throttle_linkage.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/direct_throttle_linkage.cpp), [src/governor.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/governor.cpp)
- Intake defaults (`intake_parameters`): plenum 2.0 L, cross-section 100 cm², `idle_throttle_plate_position` 0.975, runner length 4 in, `runner_flow_rate` k_carb(200), `velocity_decay` 0.25. `throttle_gamma` is read into `m_throttleGammaUnused`. — [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr), [scripting/include/intake_node.h](https://github.com/ange-yaghi/engine-sim/blob/master/scripting/include/intake_node.h)
- Exhaust system:
  - Collector volume = `collectorCrossSectionArea * length`, with geometry `(length, sqrt(A))`.
  - Each substep: flow to an atmosphere reset to 1 atm, 25 °C, `p_inert = 1`, with `k = outletFlowRate`; then `dissipateExcessVelocity` and `updateVelocity(dt, velocityDecay)`.
  - Defaults (`exhaust_system_parameters`): volume 100 L (length derived as volume/area), collector area = circle of 2 in, `outlet_flow_rate` k_carb(1000), `primary_tube_length` 10 in, `primary_flow_rate` k_carb(100), `audio_volume` 1.0, `velocity_decay` 1.0.

  — [src/exhaust_system.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/exhaust_system.cpp), [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr)
- Head defaults (`cylinder_head_parameters`): chamber 118 cc, intake runner 300 cc with area = circle(0.75 in), exhaust runner 300 cc with area = circle(0.85 in). — [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr)
- Example values:

  | Engine | Intake | Exhaust | Header primaries |
  |---|---|---|---|
  | EJ25 | plenum 1.325 L / 20 cm², k_carb(400) throttle, runner k_carb(100), runner length 12 in, idle plate 0.9978 | one system: primary tube 40 in, primary k_carb(400), outlet k_carb(1000), length 500 mm, `audio_volume` 0.5·0.02, IR `minimal_muffling_02` | unequal: 2, 3, 3, 5 in |
  | GM LS | — | two systems of lengths 100 in and 172 in, `audio_volume` 4.0, primary tube 29 in, primary k_carb(500) | per cylinder: 3·2 in + 2 cm, and so on |

  — [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr), [assets/engines/atg-video-2/07_gm_ls.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/07_gm_ls.mr)
- The exhaust "impulse response" is not computed from geometry. It is a WAV file per `ExhaustSystem`, convolved in the synthesizer (see the audio section). — [src/engine_sim_application.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine_sim_application.cpp)
- A third-party article describes engine-sim as "treating the exhaust geometry as a transmission line with reflections, standing waves, and resonant frequencies". — [Starlog](https://starlog.is/articles/developer-tools/ange-yaghi-engine-sim/). The source code contradicts this: every gas element is a 0-D `GasSystem` and the only length-dependent acoustics are `DelayFilter` plus a 1/L² gain ([src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)).

### Inferences
- A physically better Rust design could replace the runner and primary volumes with 1-D cells or waveguides. That would be a deliberate departure: engine-sim gets its "tuned header" character from a pure delay and a canned IR, not from wave dynamics in the pipes.

### Gaps
- No turbo or supercharger, intercooler or muffler-chamber model exists in this codebase.

## Time stepping, integrator, rpm coupling, CPU cost and threading

### Takeaway
Defaults:
- mechanical step at `simulation_frequency` (10 kHz default; examples use 5 to 40 kHz), one rigid-body substep;
- gas network at 8 substeps per step (80 kHz by default);
- semi-implicit Euler with a Gauss-Seidel constraint solve.

The number of steps per frame adapts to keep about 0.1 s of synthesizer input buffered. Physics runs on one thread and audio rendering on a second.

### Cited Findings
- `Simulator` defaults: `m_simulationFrequency = 10000`, `m_targetSynthesizerLatency = 0.1`, `m_simulationSpeed = 1.0`. The per-frame step count is `round(dt*speed/timestep)`. It becomes `(steps+1)*1.1` if the synthesizer latency is below target and `(steps-1)*0.9` if above. — [src/simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/simulator.cpp)
- Engine script default `simulation_frequency: 10000`. In the C++ `Engine` constructor the defaults are `m_initialSimulationFrequency = 10000.0`, `m_initialHighFrequencyGain = 0.01`, `m_initialJitter = 0.5`, `m_initialNoise = 1.0`. — [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr), [src/engine.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine.cpp)
- `simulation_frequency` per shipped engine:

  | Frequency | Engines |
  |---|---|
  | 40000 | TRX520 |
  | 35000 | Shovelhead |
  | 30000 | Kohler CH750 |
  | 20000 | Hayabusa, VTEC, EJ25 |
  | 17000 | Audi I5 |
  | 12000 | Radial-5 |
  | 10000 | 2JZ, LS, F136 |
  | 7500 | Radial-9 |
  | 7000 | Merlin V12 |
  | 6500 | LFA V10 |
  | 5000 | Ferrari 412 T2 |

  The UI "N + scroll" clamps it to [400, 400000]. — [assets/engines/](https://github.com/ange-yaghi/engine-sim/tree/master/assets/engines), [src/engine_sim_application.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine_sim_application.cpp)
- `Simulator::simulateStep()` order:
  1. `m_system->process(timestep, 1)` (rigid bodies, 1 substep)
  2. `engine->update` (throttle)
  3. `vehicle->update`
  4. `transmission->update`
  5. filtered speed
  6. drift correction: all crank θ forced equal to the output shaft ("temporary hack")
  7. dyno torque table
  8. `simulateStep_()`
  9. `writeToSynthesizer()`

  — [src/simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/simulator.cpp)
- `PistonEngineSimulator::simulateStep_()` order:
  1. ignition module update
  2. per cylinder: `ignite()` if a spark event, then `update()` (set volume, sample the valve-lift→k tables once per step)
  3. `m_fluidSimulationSteps = 8` substeps of `dt/8`, each running every `ExhaustSystem::process`, then every `Intake::process`, then every `CombustionChamber::flow`

  — [src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)
- scs integrator: `NsvOdeSolver::solve` does `v += a*dt; p += v*dt` (symplectic/semi-implicit Euler). `OptimizedNsvRigidBodySystem::process` does, per substep, `processForces()` (including the chamber gas forces), then `processConstraints` (a velocity-level solve with `q_dot' = q_dot + M⁻¹F dt` and Baumgarte-like `b_err = (biasFactor/dt)*C` with `m_biasFactor = 1.0`, with torque limits for friction and clutch constraints), then the ODE solve. The `GaussSeidelSleSolver` defaults are `m_maxIterations = 128`, `m_minDelta = 1E-1`. — [scs src/nsv_ode_solver.cpp](https://github.com/ange-yaghi/simple-2d-constraint-solver/blob/master/src/nsv_ode_solver.cpp), [scs src/optimized_nsv_rigid_body_system.cpp](https://github.com/ange-yaghi/simple-2d-constraint-solver/blob/master/src/optimized_nsv_rigid_body_system.cpp), [scs src/gauss_seidel_sle_solver.cpp](https://github.com/ange-yaghi/simple-2d-constraint-solver/blob/master/src/gauss_seidel_sle_solver.cpp)
- The application frame `dt` is clamped to [1/200, 1/30] s. Time-warp keys run at 1/10 down to 1/1000 speed. — [src/engine_sim_application.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine_sim_application.cpp)
- Threading: `Synthesizer::startAudioRenderingThread` spawns a `std::thread` running `renderAudio()` in a loop. It is synchronised with the physics thread by a mutex and condition variable and keeps at most about 2000 output samples buffered. — [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp)
- A contributor (not the author) in the issue tracker wrote: "The sim is using multiple threads, but the physics calculations are all on a single one, and splitting this work into multiple threads is non-trivial… Anything past [a V8] will probably require that you reduce the simulation frequency." — [issue #194](https://github.com/ange-yaghi/engine-sim/issues/194)
- In issue #111 a user reported that reducing the exhaust-system count from 7 to 2 on a radial turned "seconds-long lag" into smooth running. Another user reported V10/V12 latency rising above about 4000 rpm. — [issue #111](https://github.com/ange-yaghi/engine-sim/issues/111)
- `updateFilteredEngineSpeed` computes `alpha = dt/(100+dt)` and then `filtered = alpha*filtered + (1-alpha)*rpm`. — [src/simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/simulator.cpp)

### Inferences
- The exhaust-count sensitivity in #111 matches the synthesizer design: one direct-form convolution of up to 10000 taps per exhaust channel per 44.1 kHz output sample, which is up to about 441 M multiply-adds per second per channel. A Rust port should use partitioned FFT convolution (for example `rustfft`, or a uniform-partitioned overlap-save) or share one IR across channels.
- In `updateFilteredEngineSpeed` the weights look swapped: with alpha tiny, the "filtered" speed is essentially the raw rpm. Its only audio use is the fade-in `min(rpm, 40)/40`.
- The gas network costs about 4 flow edges per cylinder, times 8 substeps, times the step rate. At 10 kHz that is about 320k edge evaluations per second per cylinder: modest for Rust. The costly parts are the constraint solver and the convolution.

### Gaps
- No official CPU benchmark numbers from the author were found.

## Audio extraction: what signal, delays, jitter, noise, derivative mix, DC filter, leveler, convolution, resampling

### Takeaway
The audio source is not exhaust mass flow. It is the gauge pressure of each cylinder's exhaust runner+primary volume, plus 10 % of its dynamic pressure in each direction. Each cylinder's signal is:

1. delayed by header+collector length / 343 m/s;
2. scaled by `audio_volume * sound_attenuation / (cylinders * L²)`;
3. summed per exhaust system;
4. linearly upsampled to 44.1 kHz and low-passed at 1.9 kHz;
5. then passed through, in order: time jitter, DC removal, multiplicative low-passed "air noise", a small derivative mix, convolution with a WAV IR, a master 19.8 kHz low-pass, and an auto-leveler to int16 mono.

### Cited Findings
- Per-step input (`PistonEngineSimulator::writeToSynthesizer`), per cylinder:
  - `att = min(|filteredEngineSpeed|, 40)/40`
  - `x = att³ * 1600 * ( (P_exhRunner - 1 atm) + 0.1*dynP(+1,0) + 0.1*dynP(-1,0) )`
  - `y = delayFilter[i].fast_f(x)`
  - `channel[exhaustIndex] += soundAttenuation[cyl] * audioVolume * y / cylinderCount / (exhaustLength²)`, where `exhaustLength = headerPrimaryLength[cyl] + exhaust.length`.

  An unused `static double lastValveLift[8]` is also present. — [src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)
- Delay line: `m_delayFilters[i].initialize(exhaustLength / 343.0, 10000.0)`, a ring buffer that outputs 0 until filled. The sample rate is hard-coded to 10000 regardless of `simulation_frequency`. — [src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp), [include/delay_filter.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/delay_filter.h)
- The author said "This has been added in v0.1.11a" about per-cylinder exhaust pulse path length, in reply to a request to account for variable travel length. — [issue #272](https://github.com/ange-yaghi/engine-sim/issues/272)
- Synthesizer configuration (`Simulator::initializeSynthesizer`): `audioBufferSize = 44100`, `audioSampleRate = 44100`, `inputBufferSize = 44100`, `inputChannelCount = exhaustSystemCount`, `inputSampleRate = simulationFrequency`. `startFrame` also sets the input rate to `simFreq * simulationSpeed`. — [src/simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/simulator.cpp)
- Resampling (`Synthesizer::writeInput`):
  - Each simulation sample advances a fractional write offset by `audioRate/inputRate`.
  - Output samples between the last and current offsets are linearly interpolated `last*(1-f) + new*f`.
  - Each interpolated sample then passes through a per-channel 4th-order Butterworth low-pass at 1900 Hz (running at 44.1 kHz), called `antialiasing`, before entering the ring buffer.

  — [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp)
- `AudioParameters` defaults: `volume 1.0`, `convolution 1.0`, `dF_F_mix 0.01`, `inputSampleNoise 0.5`, `inputSampleNoiseFrequencyCutoff 10000`, `airNoise 1.0`, `airNoiseFrequencyCutoff 2000`, `levelerTarget 30000`, `levelerMaxGain 1.9`, `levelerMinGain 0.00001`. The engine script's `hf_gain`, `jitter` and `noise` overwrite `dF_F_mix`, `inputSampleNoise` and `airNoise`. — [include/synthesizer.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/synthesizer.h), [src/engine_sim_application.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine_sim_application.cpp)
- Per output sample (`Synthesizer::renderAudio(int)`), per channel:
  - `f_in = jitterFilter.fast_f(input)`
  - `f_dc = inputDcFilter.fast_f(f_in)` (a one-pole low-pass, `setCutoffFrequency(10.0)`, `m_dt = 1/44100`)
  - `f = f_in - f_dc` (a 10 Hz high-pass)
  - `f_p = derivative.f(f_in)` (`(x - x_prev)/dt`, with dt = 1/44100)
  - `r = airNoiseLowPass(uniform[-1,1])`, a 4-pole Butterworth at `airNoiseFrequencyCutoff`
  - `r_mixed = airNoise*r + (1 - airNoise)`
  - `v_in = f_p*dF_F_mix + f*r_mixed*(1 - dF_F_mix)`, with subnormals flushed to 0
  - `v = conv*convolution.f(v_in) + (1-conv)*v_in`
  - `signal += v`

  Then:
  - `signal = m_antialiasing.fast_f(signal)` (Butterworth at `0.45*44100` = 19845 Hz)
  - `levelingFilter.f(signal) * volume`
  - `lround`, clamp to int16

  — [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp)
- Two code oddities in the same function: `r` is computed through `m_filters->airNoiseLowPass` (channel 0's filter) for every channel, and a random `r_0` is computed but unused. — [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp)
- Jitter filter (`JitterFilter`):
  - `initialize(10, cutoff, 44100)` gives a 10-sample circular history.
  - Each sample, a random offset `U[0, 9] * jitterScale` is low-passed by a 4-pole Butterworth at `inputSampleNoiseFrequencyCutoff` (10 kHz).
  - The output is the history read at that fractional offset with linear interpolation, i.e. a randomly modulated delay of up to about 0.2 ms.

  — [include/jitter_filter.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/jitter_filter.h), [src/jitter_filter.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/jitter_filter.cpp)
- Butterworth design (`ButterworthLowPassFilter<T>::setCutoffFrequency`):
  - `f = tan(π f_c/fs)`
  - `m = -2cos(5π/8)`, `n = -2cos(7π/8)`
  - `a0 = 1 + (m+n)f + (2+nm)f² + (m+n)f³ + f⁴`
  - the other coefficients `a1` to `a4` are as coded
  - Filter: `y = f⁴/a0 * (x + 4x₁ + 6x₂ + 4x₃ + x₄) - Σ a_k y_k`

  — [include/butterworth_low_pass_filter.h](https://github.com/ange-yaghi/engine-sim/blob/master/include/butterworth_low_pass_filter.h)
- Leveler (`LevelingFilter::f`):
  - `peak *= 0.999` per sample, and is raised to `|x|` when exceeded (initial 30000)
  - `gain = clamp(target/peak, minLevel, maxLevel)`
  - `att = 0.9*att + 0.1*gain`
  - output = `x*att`

  — [src/leveling_filter.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/leveling_filter.cpp)
- Convolution:
  - `ConvolutionFilter::f` is a direct-form circular-buffer FIR.
  - `initializeImpulseResponse` trims the IR after the last sample with `|x| > 100` (int16), caps it at `min(10000, clippedLength)` samples (≤ 226.8 ms at 44.1 kHz), and scales it by `volume/INT16_MAX`.
  - WAVs are loaded per exhaust system with `ysWindowsAudioWaveFile`.

  — [src/convolution_filter.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/convolution_filter.cpp), [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp), [src/engine_sim_application.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine_sim_application.cpp)
- IR library (`es/sound-library/impulse_responses.mr`). All files are mono, 16-bit, 44.1 kHz; lengths below were measured from the WAV headers of the cloned files:

  | Name | File | Volume | Length |
  |---|---|---|---|
  | `default_0` | `smooth/smooth_39.wav` | 0.001 | 33705 samples, 764 ms |
  | `real_engine_0` | `archive/test_engine_14_eq_adjusted_16.wav` | 0.001 | 212 ms |
  | `real_engine_1` | `archive/test_engine_15_eq_adjusted_16.wav` | 0.001 | 293 ms |
  | `real_engine_2` | `archive/test_engine_16_eq_adjusted_16.wav` | 0.001 | 513 ms |
  | `sharp_0` | `sharp/sharp_01.wav` | 0.001 | 266 ms |
  | `mild_exhaust_0` | `new/mild_exhaust.wav` | 0.01 | 219 ms |
  | `mild_exhaust_0_reverb` | `new/mild_exhaust_reverb.wav` | 0.01 | 962 ms |
  | `minimal_muffling_01` | `new/minimal_muffling_01.wav` | 0.01 | 398 ms |
  | `minimal_muffling_02` | `new/minimal_muffling_02.wav` | 0.01 | 398 ms |
  | `minimal_muffling_03` | `new/minimal_muffling_03.wav` | 0.01 | 266 ms |

  Files longer than 10000 samples are truncated by the cap. — [es/sound-library/impulse_responses.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/sound-library/impulse_responses.mr), [es/sound-library/](https://github.com/ange-yaghi/engine-sim/tree/master/es/sound-library)
- Output path: the synthesizer produces int16 mono at 44.1 kHz. The application's audio device buffer is 44100 samples with a write lead of about 0.1 s, resynchronised if the lead exceeds 0.5 s. — [src/engine_sim_application.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/engine_sim_application.cpp)
- Per-engine tuning of `hf_gain`/`jitter`/`noise`: TRX520 0.00121/0.42/0.229, Hayabusa 0.00407/0.062/0.292, LS 0.01/0.6/1.0, Merlin 0.004/0.229/0.35, LFA 0.01/0.1/1.0. — [assets/engines/](https://github.com/ange-yaghi/engine-sim/tree/master/assets/engines)

### Inferences
- Because the delay filters are built at a hard-coded 10 kHz but fed at `simulation_frequency`, engines running at 20 kHz get half the intended header delay, and 40 kHz engines a quarter. A port should size delays with the actual step rate, or better, apply them at the output rate with fractional delay.
- The effective simulated bandwidth is about 1.9 kHz, set by the input low-pass. Brightness above that comes from three places only: the derivative term (`dF_F_mix` ≈ 0.01), the jitter (random micro-delay modulation) and the IR's own spectrum. The "physics" therefore supplies the pulse train and envelope; the timbre is largely IR plus noise shaping.
- `airNoise` multiplies the signal by (1 - a) + a·(band-limited noise). At `airNoise = 1` the pressure signal is fully amplitude-modulated by 2 kHz low-passed noise: a "turbulent flow" hiss that is proportional to the pulse amplitude.
- A Rust reimplementation should:
  - use a proper band-limited resampler, or run the synth at the simulation rate and resample once;
  - use FFT convolution;
  - fix the per-channel noise-filter bug;
  - make the delay rate-correct;
  - optionally replace the canned IR with a waveguide or tailpipe radiation model.

### Gaps
- The author does not document how the IR WAVs were produced (recorded vs synthesised). The names `*_eq_adjusted_16` suggest EQ'd recordings, but that is unconfirmed.

## Engine description language (.mr "piranha" scripts): user parameters and example defaults

### Takeaway
Engines are declared in Piranha (`.mr`) node scripts. `es/objects/objects.mr` defines the parameter nodes and defaults, and `scripting/include/*_node.h` map them onto the C++ `Parameters` structs. The user sets the items listed below. Pipe acoustics beyond lengths and volumes are not configurable except through the choice of IR.

### Cited Findings
- `engine(...)` inputs: name, redline (6000 rpm), `starter_speed` (200 rpm), `starter_torque` (200 lb·ft), `dyno_min_speed` (1000 rpm), `dyno_max_speed` (= redline), `dyno_hold_step` (100 rpm), fuel, `throttle_gamma` (1.0) or throttle (`direct_throttle_linkage` or `governor`), `simulation_frequency` (10000), `hf_gain` (0.01), `jitter` (0.5), `noise` (1.0). — [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr)
- Other nodes and their default values:

  | Node | Parameters (defaults) |
  |---|---|
  | `crankshaft` | throw, `flywheel_mass`, mass, `friction_torque`, `moment_of_inertia`, `position_x/y`, tdc (all 0) |
  | `rod_journal` | angle (0); the crank-pin angles define firing geometry |
  | `connecting_rod` | mass, `moment_of_inertia`, `center_of_mass`, length, `slave_throw` |
  | `piston` | mass, blowby, `compression_height`, `wrist_pin_position`, displacement (dish/dome volume) |
  | `cylinder_bank` | angle, bore, `deck_height`, `position_x/y`, `display_depth` 0.5 |
  | `camshaft` | advance, `base_radius`, `lobe_profile`, with `add_lobe(centreline)` |
  | `ignition_module` | `timing_curve` (Function of rpm), `rev_limit` 7000 rpm, `limiter_duration` 0.5 s, with `connect_wire(wire, angle)` |
  | `vehicle` | mass 1000 kg, Cd 0.25, area 72×72 in, diff 3.42, tire radius 10 in, rolling resistance 2000 |
  | `transmission` | `max_clutch_torque` 1000 lb·ft, with `add_gear(ratio)` |
  | `governor` | `k_s` 1.0, `k_d` 300, γ 0.1, `min_v`/`max_v` ±2 |

  — [es/objects/objects.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/objects/objects.mr)
- `add_cylinder(piston, connecting_rod, rod_journal, intake, exhaust_system, ignition_wire, sound_attenuation = 1.0, primary_length = 0.0)`. The firing order is set by `connect_wire(wireN, k/N * cycle)` with `cycle = 720°`, combined with the rod-journal angles and the lobe centrelines. — [es/actions/actions.mr](https://github.com/ange-yaghi/engine-sim/blob/master/es/actions/actions.mr), [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr)
- Full EJ25 example:
  - Geometry: bore 99.5 mm, stroke 79 mm, rod 5.142 in (535 g), compression height 1.0 in, `deck_height = stroke/2 + rod + compression_height`, piston 414 + 152 g, banks at ±90°, crank TDC 180°.
  - Crankshaft: crank 9.39 kg, flywheel 6.8 kg with r = 6 in, friction 1 lb·ft; rod journals 0/180/0/180°.
  - Head and cams: chamber 67 cc, intake runner 149.6 cc with area 1.75×1.75 in, exhaust runner 50 cc with area 1.25×1.25 in, cam base radius 17 mm.
  - Ignition: timing 25°@0–1000, 30°@2000, 40°@3000–4000 rpm; rev limit 6800 rpm, limiter 0.16 s; firing 1-3-2-4 at 0/180/360/540°.
  - Engine and transmission: starter 70 lb·ft at 500 rpm, redline 6500, `max_burning_efficiency` 0.9, `throttle_gamma` 2.0, `simulation_frequency` 20000, `hf_gain` 0.01, noise 1.0, jitter 0.5; six gears 3.636 … 0.756.

  — [assets/engines/atg-video-2/01_subaru_ej25_eh.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/01_subaru_ej25_eh.mr)
- Shipped engines:
  - `atg-video-1`: Honda TRX520, Kohler CH750, Shovelhead, Hayabusa, Honda VTEC, EJ25, Audi I5, Radial-5.
  - `atg-video-2`: EJ25 with equal- and unequal-length headers, 2JZ, 60° V6, odd- and even-fire 90° V6, GM LS, Ferrari F136, Radial-9, LFA V10, Merlin V12, Ferrari 412 T2.
  - Others: Audi i5, BMW M52B28, Chevrolet 454, Kohler.

  `atg-video-2` corresponds to the video "Simulating an F1 V12, cross-plane and flat-plane V8s, unequal length headers and more". — [assets/engines/atg-video-2/README.md](https://github.com/ange-yaghi/engine-sim/blob/master/assets/engines/atg-video-2/README.md)
- `assets/main.mr` imports `engines/atg-video-2/01_subaru_ej25_eh.mr` by default. — [assets/main.mr](https://github.com/ange-yaghi/engine-sim/blob/master/assets/main.mr)

### Inferences
- For a Rust port, a TOML or RON schema mirroring these nodes is enough. The Piranha graph language adds no physics.

### Gaps
- The full `rod_moment_of_inertia` and `disk_moment_of_inertia` helper formulas in `es/utilities` were not inspected.

## Known limitations and criticisms

### Takeaway
The main complaints in the issue tracker are about CPU, latency and stutter: physics is single-threaded, and cost scales with cylinders, exhaust-system count and simulation frequency. The code itself shows several simplifications and bugs that limit realism. The acoustics are 0-D plus canned IRs, and the simulated bandwidth is about 1.9 kHz.

### Cited Findings
- The author says the codebase is sloppy and was made "to demo in a YouTube video, not as a real product". Windows-only build. — [README.md](https://github.com/ange-yaghi/engine-sim/blob/master/README.md)
- Performance reports:
  - [issue #61](https://github.com/ange-yaghi/engine-sim/issues/61): a user was told to lower the simulation frequency with N + scroll.
  - [issue #111](https://github.com/ange-yaghi/engine-sim/issues/111): multi-cylinder lag; reducing exhaust systems helps.
  - [issue #381](https://github.com/ange-yaghi/engine-sim/issues/381): with an i9-7960X, "needles are bouncing all over the place, until I drop the frequency < 6210… reverbing echos, glitching noises, clipping".
  - [issue #307](https://github.com/ange-yaghi/engine-sim/issues/307) (title only): latency spikes from about 100 to 750.
- Issue #6 reported a high-pitched buzz at start-up. The author: "A bit of random noise plays when the simulation first opens which is not intentional". The issue was later marked fixed. — [issue #6](https://github.com/ange-yaghi/engine-sim/issues/6)
- In-code hacks visible in the source:
  - "Correct drift (temporary hack)": all crankshaft angles are forced to the output shaft each step — [src/simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/simulator.cpp)
  - the 10 kHz hard-coded delay rate — [src/piston_engine_simulator.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/piston_engine_simulator.cpp)
  - the channel-0 noise filter shared across channels — [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp)
  - the unused intake `throttle_gamma` — [scripting/include/intake_node.h](https://github.com/ange-yaghi/engine-sim/blob/master/scripting/include/intake_node.h)
  - fuel-model arguments that are ignored (`firingPressure`, `motoringPressure`) — [src/fuel.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/fuel.cpp)
  - C `rand()` in the audio thread — [src/synthesizer.cpp](https://github.com/ange-yaghi/engine-sim/blob/master/src/synthesizer.cpp)
- The project moved to a community edition (`Engine-Simulator/engine-sim-community-edition`). This original repo's master was last committed on 2023-01-22. — [README.md](https://github.com/ange-yaghi/engine-sim/blob/master/README.md)

### Inferences
- Criticisms a Rust redesign should address:
  1. No 1-D pipe acoustics: tuned-length resonance comes only from a delay and an IR.
  2. Input band-limited to 1.9 kHz, with brightness faked by a derivative term and jitter.
  3. Direct-form convolution cost.
  4. A single physics thread. Cylinders could be parallelised only with care, because they share the plenum and collector within each substep.
  5. The adaptive step count ties audio continuity to frame timing, so a real-time audio callback design should pull samples, not push frames.
  6. Uniform-random cyclic variation with no temporal correlation.

### Gaps
- The author's YouTube video descriptions and Discord notes could not be retrieved. No first-party written explanation of the audio design beyond the code was found.
- GitHub issues were reviewed only through the search API. Titles and bodies were read for about 8 issues, not exhaustively.
