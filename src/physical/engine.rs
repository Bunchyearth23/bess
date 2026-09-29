//! Coupled engine: physical cylinders, manifold states, shaft and acoustic ports.
//! All evolving gas/shaft state is f64. Audio is an explicitly calibrated f32
//! observation of mass-flow waves and mechanical impacts, never a torque curve.
use super::mechanical::{Knock, Slap};
use super::radiation::Modes;
use super::{
    acoustic::Acoustic,
    config::CylinderConfig,
    controller::Controller,
    crank::Crank,
    cycle::{CycleCylinder, CycleInput},
    induction::Induction,
    intake_acoustic::IntakeAcoustic,
    manifolds::{AfterTreatment, Exchange, Flows, Manifolds},
    tone::Tone,
};
use crate::{
    engine_build::{Aspiration, Fuel},
    scratch::Scratch,
};
use bdsp::svf::{StateVariableFilter, SvfMode};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub struct Commands {
    /// A test stand can impose RPM; None lets the physical crank accelerate.
    pub imposed_rpm: Option<f64>,
    pub throttle: f64,
    pub load_nm: f64,
    pub overrun: f64,
    pub starter: bool,
    pub ac: bool,
    pub steering: bool,
}
impl Default for Commands {
    fn default() -> Self {
        Self {
            imposed_rpm: Some(850.),
            throttle: 0.1,
            load_nm: 0.,
            overrun: 1.,
            starter: false,
            ac: false,
            steering: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub exhaust: f32,
    pub intake: f32,
    pub mechanical: f32,
    pub bank_pressure: [f32; 2],
    pub rpm: f64,
    pub map_pa: f64,
    pub torque_nm: f64,
    pub heat_j: f64,
    pub correction_j: f64,
    pub fuel_cut: bool,
    pub misfires: u64,
    pub turbo_rpm: f64,
    /// Acoustic afterfire excitation energy: 0.8 * reacted chemical heat.
    /// Not total chemical heat, nor net heat transferred to the gas after cooling.
    pub afterfire_heat_j: f64,
    pub idle_bypass: f64,
    pub fresh_supply_kg_s: f64,
    pub fresh_tailpipe_in_kg_s: f64,
    pub fuel_injected_kg: f64,
}

pub struct Engine {
    scratch: Scratch,
    config: CylinderConfig,
    cylinders: Vec<CycleCylinder>,
    phases: [f64; 12],
    spark_shift: [f64; 12],
    spark_enabled: [bool; 12],
    banks: [usize; 12],
    bank_counts: [usize; 2],
    manifolds: Manifolds,
    induction: Option<Induction>,
    aftertreatment: [AfterTreatment; 2],
    crank: Crank,
    controller: Controller,
    acoustic: Acoustic,
    rate: f64,
    substeps: usize,
    dt: f64,
    bank_gain: f32,
    angle: f64,
    rpm: f64,
    was_imposed: bool,
    failed: bool,
    radiation: super::radiation::Radiation,
    radiation_seed: u64,
    intake_acoustic: IntakeAcoustic,
    intake_phase: f64,
    afterfire_armed: bool,
    afterfire_remaining_s: f64,
    /// Summed cylinder pressure at the previous output sample; NaN before any.
    pressure_previous: f64,
    combustion_highpass: [StateVariableFilter; 2],
    slap: Slap,
    knock: Knock,
    /// Last block/head excitations (contact, combustion, slap) for calibration.
    excitation: [f32; 3],
    mechanics: Modes,
    tone: Tone,
    previous_mechanics: Option<Modes>,
    previous_tone: Option<Tone>,
    sound_fade: f32,
    last: Sample,
}

impl Engine {
    pub fn new(scratch: &Scratch, rate: u32) -> Result<Self, String> {
        scratch.validate()?;
        if !(8000..=384000).contains(&rate) {
            return Err("Physical sample rate: 8–384 kHz".into());
        }
        for (name, value) in [
            ("bore", scratch.build.bore_mm),
            ("stroke", scratch.build.stroke_mm),
        ] {
            if !(5.0..=500.0).contains(&value) {
                return Err(format!(
                    "Physical engine {name}: {value} mm is outside the current supported range of 5–500 mm"
                ));
            }
        }
        let mut scratch = scratch.clone();
        scratch.design.resolve_cam_revolutions();
        let tuning = scratch
            .tuning
            .resolve(&scratch.build, scratch.design.cylinders);
        let config = CylinderConfig::from_tuning(&scratch.build, &tuning)?;
        let n = scratch.design.cylinders as usize;
        let displacement = config.displacement_m3() * n as f64;
        if !(1e-6..=0.5).contains(&displacement) {
            return Err(format!(
                "Physical engine total displacement: {:.6} L is outside the current supported range of 1 cm³–500 L",
                displacement * 1000.
            ));
        }
        let firing = scratch.design.firing();
        let mut phases = [0.; 12];
        let mut spark_shift = [0.; 12];
        let mut spark_enabled = [false; 12];
        // Mechanical geometry uses a fixed cylinder-one reference. Rewiring
        // spark order must never rotate selected cam revolutions independently.
        let start = scratch.design.tdc(0);
        let revolutions = scratch.design.cam_revolutions.expect("resolved cams");
        for i in 0..n {
            let tdc = (scratch.design.tdc(i) - start).rem_euclid(360.);
            phases[i] = f64::from(tdc + if revolutions[i] { 360. } else { 0. }).to_radians();
        }
        let spark_origin = phases[scratch.design.order()[0] as usize - 1];
        for k in 0..firing.events {
            let i = firing.cylinder[k] as usize;
            if !spark_enabled[i] {
                spark_shift[i] =
                    (f64::from(firing.angles[k]).to_radians() + spark_origin - phases[i] + 2. * PI)
                        .rem_euclid(2. * TAU)
                        - 2. * PI;
                spark_enabled[i] = true;
            }
        }
        let seed = scratch.life().seed;
        let cylinders = (0..n)
            .map(|i| {
                CycleCylinder::new(config, phases[i], seed.wrapping_add(i as u64 * 7919))
                    .map_err(|e| format!("Cylinder {}: {e:?}", i + 1))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let banks = std::array::from_fn(|i| scratch.design.banks[i] as usize);
        let mut bank_counts = [0; 2];
        for &b in &banks[..n] {
            bank_counts[b] += 1;
        }
        let mut manifolds = Manifolds::new(&scratch.build, &tuning, n as u32, displacement)?;
        manifolds
            .set_active_bank_mask(bank_counts.map(|count| count > 0))
            .map_err(|e| format!("Manifolds: {e:?}"))?;
        let rpm = f64::from(scratch.idle_rpm);
        let crank = Crank::new(
            f64::from(scratch.inertia),
            config.stroke_m,
            displacement,
            rpm,
        )
        .map_err(|e| e.to_string())?;
        let controller = Controller::new(rpm, f64::from(scratch.redline_rpm), scratch.build.fuel)
            .map_err(|e| e.to_string())?;
        let acoustic = Acoustic::new(rate, &scratch.design, &scratch.build, &scratch.sound);
        let intake_acoustic =
            IntakeAcoustic::new(rate, n, &scratch.build, displacement, &scratch.sound);
        let tone = Tone::new(rate, &scratch.sound);
        let mechanics = Modes::new(
            rate as f32,
            scratch.sound.mechanical_pitch_hz,
            scratch.sound.mechanical_resonance,
            scratch.build.block,
            scratch.build.bore_mm,
            seed,
        );
        let induction = if scratch.build.aspiration == Aspiration::Natural {
            None
        } else {
            Some(Induction::new(&scratch.build, &tuning, displacement)?)
        };
        let aftertreatment = std::array::from_fn(|_| AfterTreatment::new(scratch.build.catalyst));
        let substeps = (96000. / f64::from(rate)).ceil().max(1.) as usize;
        let (slap, knock) = mechanical_events(&scratch, &config, rate, substeps, seed);
        let bank_gain = 10_f32.powf(scratch.design.bank_gain_db / 20.);
        Ok(Self {
            scratch,
            config,
            cylinders,
            phases,
            spark_shift,
            spark_enabled,
            banks,
            bank_counts,
            manifolds,
            induction,
            aftertreatment,
            crank,
            controller,
            acoustic,
            rate: f64::from(rate),
            angle: 0.,
            rpm,
            substeps,
            dt: 1. / (f64::from(rate) * substeps as f64),
            bank_gain,
            was_imposed: true,
            failed: false,
            radiation: super::radiation::Radiation::new(rate, seed),
            radiation_seed: seed,
            intake_acoustic,
            intake_phase: 0.,
            afterfire_armed: false,
            afterfire_remaining_s: 0.,
            pressure_previous: f64::NAN,
            // Fourth-order Butterworth high-pass at 500 Hz.
            combustion_highpass: [0.5412, 1.3066]
                .map(|q| StateVariableFilter::new(rate as f32, 500., q, SvfMode::Highpass)),
            slap,
            knock,
            excitation: [0.; 3],
            mechanics,
            tone,
            previous_mechanics: None,
            previous_tone: None,
            sound_fade: 1.,
            last: Sample {
                rpm,
                ..Default::default()
            },
        })
    }
    pub fn inertia(&self) -> f64 {
        f64::from(self.scratch.inertia)
    }
    pub fn state(&self) -> Sample {
        self.last
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    /// Last mechanical excitations (valve/DI contact, combustion, piston slap)
    /// before the modal bank; a calibration diagnostic.
    pub fn mechanical_excitation(&self) -> [f32; 3] {
        self.excitation
    }
    /// Crank friction torque at the current speed and cylinder peak pressure.
    pub fn friction_nm(&self) -> f64 {
        self.crank.friction_nm(self.rpm)
    }

    /// Adopt prepared sound controls without replacing the running engine.
    /// Gas, crank, thermal/controller state, noise phase and cylinders survive.
    /// The caller must retire `prepared` off the rendering thread afterwards.
    pub fn apply_sound_tuning(&mut self, prepared: &mut Self) -> bool {
        if self.rate != prepared.rate
            || self.failed
            || prepared.failed
            || !self.scratch.same_engine_except_sound(&prepared.scratch)
        {
            return false;
        }
        let sound = prepared.scratch.sound;
        if sound == self.scratch.sound {
            return true;
        }
        // Move the prepared BDSP filters. The neutral placeholder has only
        // inline state; the prepared engine and any previous fade go to trash.
        let incoming = std::mem::replace(
            &mut prepared.tone,
            Tone::new(self.rate as u32, &Default::default()),
        );
        prepared.previous_tone = self.previous_tone.take();
        prepared.previous_mechanics = self.previous_mechanics.take();
        self.previous_tone = Some(std::mem::replace(&mut self.tone, incoming));
        self.previous_mechanics = Some(self.mechanics.clone());
        self.mechanics
            .retune(sound.mechanical_pitch_hz, sound.mechanical_resonance);
        self.sound_fade = 0.;
        self.acoustic.retune(&sound);
        self.intake_acoustic.retune(&sound);
        // Cycle inputs read new ignition/variation/duration settings; existing
        // burn, trapped charge and angular phase are not recreated or cleared.
        self.scratch.sound = sound;
        true
    }
    /// Live sound-only retune for an imported engine. All DSP state is inline:
    /// valid controls allocate no memory and preserve crank, gas and thermals.
    /// Identical settings are a no-op, including an already running crossfade.
    pub fn set_sound_tuning(&mut self, sound: &crate::scratch::SoundTuning) -> bool {
        if self.failed || sound.validate().is_err() {
            return false;
        }
        if *sound == self.scratch.sound {
            return true;
        }
        self.previous_tone = Some(std::mem::replace(
            &mut self.tone,
            Tone::new(self.rate as u32, sound),
        ));
        self.previous_mechanics = Some(self.mechanics.clone());
        self.mechanics
            .retune(sound.mechanical_pitch_hz, sound.mechanical_resonance);
        self.sound_fade = 0.;
        self.acoustic.retune(sound);
        self.intake_acoustic.retune(sound);
        self.scratch.sound = *sound;
        true
    }

    /// Restore the gas/control state without allocating on the producer.
    /// Existing acoustic tails decay naturally across the restart boundary.
    pub fn reset(&mut self) {
        for cylinder in &mut self.cylinders {
            cylinder.reset();
        }
        let n = self.cylinders.len();
        let displacement = self.config.displacement_m3() * n as f64;
        self.rpm = f64::from(self.scratch.idle_rpm);
        self.crank = Crank::new(self.inertia(), self.config.stroke_m, displacement, self.rpm)
            .expect("validated crank");
        let tuning = self.scratch.tuning.resolve(&self.scratch.build, n as u32);
        self.manifolds = Manifolds::new(&self.scratch.build, &tuning, n as u32, displacement)
            .expect("validated manifolds");
        self.manifolds
            .set_active_bank_mask(self.bank_counts.map(|count| count > 0))
            .expect("valid banks");
        if self.induction.is_some() {
            self.induction = Some(
                Induction::new(&self.scratch.build, &tuning, displacement)
                    .expect("validated induction"),
            );
        }
        self.aftertreatment =
            std::array::from_fn(|_| AfterTreatment::new(self.scratch.build.catalyst));
        self.controller = Controller::new(
            self.rpm,
            f64::from(self.scratch.redline_rpm),
            self.scratch.build.fuel,
        )
        .expect("validated controls");
        self.angle = 0.;
        self.was_imposed = true;
        self.failed = false;
        self.last = Sample {
            rpm: self.rpm,
            ..Default::default()
        };
        self.radiation = super::radiation::Radiation::new(self.rate as u32, self.radiation_seed);
        self.intake_phase = 0.;
        self.afterfire_armed = false;
        self.afterfire_remaining_s = 0.;
        self.pressure_previous = f64::NAN;
        self.combustion_highpass
            .iter_mut()
            .for_each(StateVariableFilter::reset);
        (self.slap, self.knock) = mechanical_events(
            &self.scratch,
            &self.config,
            self.rate as u32,
            self.substeps,
            self.radiation_seed,
        );
    }

    pub fn next(&mut self, commands: Commands) -> Sample {
        if self.failed {
            return Sample::default();
        }
        match self.advance(commands) {
            Ok(sample) => {
                self.last = sample;
                sample
            }
            Err(_) => {
                self.failed = true;
                Sample::default()
            }
        }
    }
    fn advance(&mut self, commands: Commands) -> Result<Sample, ()> {
        if !commands.throttle.is_finite()
            || !(0.0..=1.).contains(&commands.throttle)
            || !commands.load_nm.is_finite()
            || commands.load_nm.abs() > 1e6
            || commands
                .imposed_rpm
                .is_some_and(|r| !(0.0..=12000.).contains(&r))
        {
            return Err(());
        }
        if self.was_imposed && commands.imposed_rpm.is_none() {
            self.crank = Crank::new(
                self.inertia(),
                self.config.stroke_m,
                self.config.displacement_m3() * self.cylinders.len() as f64,
                self.rpm,
            )
            .map_err(|_| ())?;
            self.crank.set_angle_unwrapped(self.angle).map_err(|_| ())?;
        }
        self.was_imposed = commands.imposed_rpm.is_some();
        self.crank.set_accessories(commands.ac, commands.steering);
        let substeps = self.substeps;
        let dt = self.dt;
        let n = self.cylinders.len();
        let mut flow = [0.; 12];
        let mut intake_flows = [0.; 12];
        let mut heat = 0.;
        let mut correction = 0.;
        let mut torque = 0.;
        let mut impact = 0.;
        let mut pressure = 0.;
        let mut fuel_cut = false;
        let mut misfires = self.last.misfires;
        let mut afterfire = [0.; 2];
        let (mut idle_bypass, mut fresh_supply, mut fresh_tailpipe, mut injected) =
            (0., 0., 0., 0.);
        // Opt-in: at zero the knock state is never touched (bit-identical).
        let knock = self.scratch.experimental.knock > 0.;
        let (mut throttle_flow, mut throttle_gap, mut compressor_flow) = (0., 0., 0.);
        for _ in 0..substeps {
            self.rpm = commands.imposed_rpm.unwrap_or(self.crank.state().rpm);
            let mut controller = self
                .controller
                .step(
                    dt,
                    self.rpm,
                    commands.throttle,
                    commands.overrun,
                    commands.starter,
                )
                .map_err(|_| ())?;
            // Explicit lift-off tune: retain actual injected fuel briefly and
            // alter ignition. Unburnt species still need oxygen and sufficient
            // gas/catalyst temperature in AfterTreatment to release any heat.
            // At zero the old arithmetic/control path is left untouched.
            let afterfire_intensity = f64::from(self.scratch.experimental.afterfire);
            let mut afterfire_active = false;
            if afterfire_intensity > 0. {
                if commands.throttle > 0.2 {
                    self.afterfire_armed = true;
                }
                let eligible = self.rpm > f64::from(self.scratch.idle_rpm) + 500.
                    && !controller.rev_limited
                    && !commands.starter;
                if commands.throttle <= 0.03 && self.afterfire_armed {
                    self.afterfire_armed = false;
                    if eligible {
                        self.afterfire_remaining_s = 0.2 + 0.1 * afterfire_intensity;
                    }
                }
                if commands.throttle > 0.08 || !eligible {
                    self.afterfire_remaining_s = 0.;
                }
                if self.afterfire_remaining_s > 0. {
                    afterfire_active = true;
                    self.afterfire_remaining_s = (self.afterfire_remaining_s - dt).max(0.);
                    controller.fuel_multiplier = controller
                        .fuel_multiplier
                        .max(0.25 + 0.75 * afterfire_intensity);
                    // DFCO disabled sparks together with its fuel command;
                    // restore ignition eligibility before the per-cycle cuts.
                    controller.spark_enabled = true;
                    // Positive shift is retard: 20–60 crank degrees. The
                    // cylinder model decides whether a late charge can ignite.
                    controller.spark_shift_rad += (20. + 40. * afterfire_intensity).to_radians();
                }
            }
            fuel_cut = controller.dfco || controller.rev_limited;
            if afterfire_active {
                fuel_cut = false;
            }
            idle_bypass = controller.bypass;
            let angle = self.angle + self.rpm * TAU / 60. * dt;
            if let Some(induction) = &self.induction {
                self.manifolds
                    .set_supply(induction.supply())
                    .map_err(|_| ())?;
                self.manifolds
                    .set_supply_composition(induction.supply_composition())
                    .map_err(|_| ())?;
                self.manifolds
                    .set_supply_limit(induction.outgoing_budget_kg())
                    .map_err(|_| ())?;
            }
            let exhaust_composition = [
                self.manifolds.exhaust_composition(0),
                self.manifolds.exhaust_composition(1),
            ];
            let exhaust = [
                self.manifolds.exhaust_bank(0),
                self.manifolds.exhaust_bank(1),
            ];
            let budgets = self.manifolds.outgoing_budgets();
            let mut flows = Flows::default();
            torque = 0.;
            pressure = 0.;
            let mut peak: f64 = 0.;
            let mut reciprocating = 0.;
            // Retard the inlet at closed-throttle idle to reduce overlap/EGR;
            // under load follow the schedule below. This changes
            // valve flow, never combustion gain or an imposed idle speed.
            let idle_vvt = (1. - controller.throttle / 0.2).clamp(0., 1.)
                * ((2000. - self.rpm) / 800.).clamp(0., 1.);
            // Under load, advance (earlier IVC, better trapping) through the
            // mid range and return to the late-closing base cam towards
            // redline, where runner ram fills the cylinder after BDC; the
            // usual intake-phaser full-load schedule (Heywood §6.8, VANOS/VVT-i).
            let redline = f64::from(self.scratch.redline_rpm);
            let load_advance = (self.rpm / 2000.).clamp(0., 1.)
                * ((0.9 * redline - self.rpm) / (0.35 * redline)).clamp(0., 1.);
            let vvt_target = if self.scratch.build.vvt {
                (20. * idle_vvt - 15. * load_advance * (1. - idle_vvt)).to_radians()
            } else {
                0.
            };
            self.intake_phase += (vvt_target - self.intake_phase) * (dt / 0.05).min(1.);
            for (i, cylinder_flow) in flow.iter_mut().enumerate().take(n) {
                let bank = self.banks[i];
                // The conservative 0D system supplies the valve mass flow.
                // Acoustic waves observe that prescribed flow. Feeding a
                // delayed pressure wave straight into the nonlinear orifice
                // creates an active numerical loop near pressure equilibrium
                // (a sustained idle whistle despite finite thermal states).
                // Two-way wave/valve coupling needs a passive joint boundary
                // solve; a pressure clamp does not provide that guarantee.
                let boundary = exhaust[bank];
                let mut spark_enabled = controller.spark_enabled && self.spark_enabled[i];
                if afterfire_active {
                    // A deterministic 720-degree schedule holds each cylinder's
                    // cut decision across its spark, with 1–4 cuts in 8 cycles.
                    // No audio event/noise is generated by this control law.
                    let cycle = ((angle - self.phases[i] + PI) / (2. * TAU)).floor() as i64;
                    let cut_slots = (4. * afterfire_intensity).ceil() as i64;
                    spark_enabled &= (cycle + i as i64).rem_euclid(8) >= cut_slots;
                }
                let output = self.cylinders[i]
                    .step(CycleInput {
                        angle_rad: angle,
                        rpm: self.rpm,
                        dt_s: dt,
                        intake: self.manifolds.runner(i),
                        exhaust: boundary,
                        intake_fresh_air_fraction: self
                            .manifolds
                            .runner_composition(i)
                            .fresh_air_fraction,
                        intake_fuel_fraction: self.manifolds.runner_composition(i).fuel_fraction,
                        exhaust_fresh_air_fraction: exhaust_composition[bank].fresh_air_fraction,
                        exhaust_fuel_fraction: exhaust_composition[bank].fuel_fraction,
                        intake_mass_limit_kg: self.manifolds.runner_budget_kg(i),
                        exhaust_mass_limit_kg: budgets.exhaust_kg[bank]
                            / self.bank_counts[bank].max(1) as f64,
                        fuel_multiplier: controller.fuel_multiplier,
                        spark_enabled,
                        spark_shift_rad: controller.spark_shift_rad
                            + self.spark_shift[i]
                            + f64::from(self.scratch.sound.ignition_retard_deg).to_radians(),
                        intake_phase_rad: self.intake_phase,
                        variation: f64::from(self.scratch.build.cam)
                            * 0.4
                            * f64::from(self.scratch.sound.cycle_variation),
                        burn_duration_scale: f64::from(self.scratch.sound.combustion_duration),
                    })
                    .map_err(|_| ())?;
                add_exchange(
                    &mut flows.runner[i],
                    -output.intake_mass_kg,
                    -output.intake_enthalpy_j,
                );
                add_exchange(
                    &mut flows.exhaust[bank],
                    output.exhaust_mass_kg,
                    output.exhaust_enthalpy_j,
                );
                flows.runner_species[i].fresh_air_kg += (-output.intake_fresh_air_kg).max(0.);
                flows.runner_species[i].fuel_kg += (-output.intake_fuel_kg).max(0.);
                flows.exhaust_species[bank].fresh_air_kg += output.exhaust_fresh_air_kg.max(0.);
                flows.exhaust_species[bank].fuel_kg += output.exhaust_fuel_kg.max(0.);
                *cylinder_flow += output.exhaust_mass_flow_kg_s / substeps as f64;
                intake_flows[i] += output.intake_mass_kg * self.rate;
                torque += output.gas_torque_nm;
                heat += output.heat_j;
                injected += output.injected_fuel_kg;
                correction += output.ledger.numerical_correction_j.abs()
                    + output.wall_numerical_correction_j.abs();
                peak = peak.max(output.pressure_pa);
                pressure += output.pressure_pa;
                misfires += u64::from(output.misfired);
                let phase = angle - self.phases[i];
                self.slap.step(i, phase, output.pressure_pa, self.rpm, dt);
                if knock {
                    self.knock.step(
                        i,
                        output.burn_fraction,
                        output.pressure_pa,
                        output.temperature_k,
                        dt,
                    );
                }
                // First two slider-crank acceleration terms, estimated moving mass.
                if commands.imposed_rpm.is_none() {
                    let (sine, cosine) = phase.sin_cos();
                    let radius = self.config.stroke_m * 0.5;
                    let ratio = radius / self.config.rod_m;
                    let omega = self.rpm * TAU / 60.;
                    let mass = 0.45 * (self.config.bore_m / 0.086).powi(2);
                    let acceleration =
                        radius * omega * omega * (cosine + ratio * (cosine * cosine - sine * sine));
                    let derivative = radius * (sine + ratio * sine * cosine);
                    reciprocating -= mass * acceleration * derivative;
                }
                for closure in [2. * PI - 1.6, 3. * PI + 0.7] {
                    if crossed(self.angle - self.phases[i], angle - self.phases[i], closure) {
                        impact += 0.015 * (self.rpm / 3000.).sqrt();
                    }
                }
                if self.scratch.build.fuel == Fuel::DirectInjection && output.injected_fuel_kg > 0.
                {
                    impact += (output.injected_fuel_kg * 250.).min(0.05);
                }
            }
            let manifold = self
                .manifolds
                .step(dt, controller.throttle, controller.bypass, flows)
                .map_err(|_| ())?;
            let mut reaction_heat = [0.; 2];
            fresh_supply += manifold.intake_mass_flow_kg_s.max(0.) / substeps as f64;
            throttle_flow += manifold.intake_mass_flow_kg_s / substeps as f64;
            throttle_gap = self
                .manifolds
                .throttle_area_m2(controller.throttle, controller.bypass);
            fresh_tailpipe += manifold
                .external_tailpipe_mass_flow_kg_s
                .iter()
                .map(|f| (-f).max(0.))
                .sum::<f64>()
                / substeps as f64;
            for bank in 0..2 {
                let species = self.manifolds.exhaust_species(bank);
                let treatment = self.aftertreatment[bank]
                    .step_resident(
                        dt,
                        self.manifolds.exhaust_bank(bank).temperature_k,
                        self.manifolds.exhaust_mass_kg(bank),
                        species.fuel_kg,
                        species.fresh_air_kg * 0.233,
                    )
                    .map_err(|_| ())?;
                self.manifolds
                    .consume_exhaust_reactants(
                        bank,
                        treatment.fuel_burned_kg,
                        treatment.fuel_burned_kg * 3.5,
                    )
                    .map_err(|_| ())?;
                reaction_heat[bank] = treatment.gas_heat_j;
                afterfire[bank] += treatment.chemical_heat_j * 0.8;
            }
            // The mass constituents above are authoritative; chemical heat is
            // deposited by the next explicit gas step, never counted twice.
            self.manifolds
                .set_exhaust_heat_j(reaction_heat)
                .map_err(|_| ())?;
            if let Some(induction) = &mut self.induction {
                let total = manifold
                    .exhaust_mass_flow_kg_s
                    .iter()
                    .map(|f| f.max(0.))
                    .sum::<f64>();
                // A design may put every cylinder on bank two. Weight the
                // turbine boundary by actual delivered flow, not bank one.
                let turbine_inlet = if total > 0. {
                    let weights = manifold.exhaust_mass_flow_kg_s.map(|f| f.max(0.) / total);
                    super::cylinder::Reservoir {
                        pressure_pa: exhaust[0].pressure_pa * weights[0]
                            + exhaust[1].pressure_pa * weights[1],
                        temperature_k: exhaust[0].temperature_k * weights[0]
                            + exhaust[1].temperature_k * weights[1],
                    }
                } else {
                    exhaust[0]
                };
                let outlet = if total > 0. {
                    (0..2)
                        .map(|b| {
                            self.manifolds.tailpipe_pressure_pa(b)
                                * manifold.exhaust_mass_flow_kg_s[b].max(0.)
                        })
                        .sum::<f64>()
                        / total
                } else {
                    self.manifolds.tailpipe_pressure_pa(0)
                };
                induction.set_turbine_outlet_pa(outlet).map_err(|_| ())?;
                let turbo = induction
                    .step_with_species(
                        dt,
                        self.rpm,
                        controller.throttle,
                        turbine_inlet,
                        total,
                        (manifold.supply_exchange, manifold.supply_species),
                    )
                    .map_err(|_| ())?;
                correction += turbo.charge_ledger.numerical_correction_j.abs();
                self.manifolds
                    .set_wastegate(induction.wastegate())
                    .map_err(|_| ())?;
                compressor_flow += turbo.compressor_mass_flow_kg_s / substeps as f64;
                let removed_heat = manifold
                    .exhaust_mass_flow_kg_s
                    .map(|flow| -turbo.turbine_energy_j * flow.max(0.) / total.max(1e-20));
                self.manifolds
                    .set_tailpipe_heat_j(removed_heat)
                    .map_err(|_| ())?;
            }
            correction += manifold.intake_ledger.numerical_correction_j.abs()
                + manifold.runner_correction_j.abs()
                + manifold
                    .exhaust_ledger
                    .iter()
                    .map(|l| l.numerical_correction_j.abs())
                    .sum::<f64>()
                + manifold
                    .tailpipe_ledger
                    .iter()
                    .map(|l| l.numerical_correction_j.abs())
                    .sum::<f64>();
            self.crank
                .set_peak_pressure_pa(peak.min(1e9))
                .map_err(|_| ())?;
            if commands.imposed_rpm.is_none() {
                let state = self
                    .crank
                    .step(
                        dt,
                        torque,
                        reciprocating,
                        commands.load_nm,
                        commands.starter,
                    )
                    .map_err(|_| ())?;
                self.rpm = state.rpm;
                // Cylinders were fed the start-RPM prediction. When compression
                // stops the crank mid-step (cranking, stall) the integrated angle
                // falls short of it; never step the cylinders backwards.
                self.angle = state.angle_unwrapped.max(angle);
            } else {
                self.angle = angle;
            }
        }
        let exhaust = [
            self.manifolds.exhaust_bank(0),
            self.manifolds.exhaust_bank(1),
        ];
        let bank_pressure = self.acoustic.next(
            &flow,
            exhaust.map(|r| r.temperature_k),
            exhaust.map(|r| r.pressure_pa),
            afterfire.map(|q| q * self.rate),
        );
        // A fixed acoustic calibration, independent of RPM/load/observed RMS.
        const PA_TO_SAMPLE: f32 = 1. / 3000.;
        let exhaust_audio = (bank_pressure[0] + bank_pressure[1] * self.bank_gain) * PA_TO_SAMPLE;
        let supply = self
            .induction
            .as_ref()
            .map_or(super::manifolds::ATMOSPHERE, Induction::supply);
        let manifold = self.manifolds.intake();
        let air = super::radiation::Air {
            throttle_kg_s: throttle_flow as f32,
            gap_m2: throttle_gap as f32,
            bore_m2: self.manifolds.throttle_area_m2(1., 0.) as f32,
            supply_pa: supply.pressure_pa as f32,
            supply_k: supply.temperature_k as f32,
            manifold_pa: manifold.pressure_pa as f32,
            manifold_k: manifold.temperature_k as f32,
            shaft_rpm: self.induction.as_ref().map_or(0., Induction::shaft_rpm) as f32,
            compressor_kg_s: compressor_flow as f32,
        };
        let (intake_audio, contact) =
            self.radiation
                .next(self.intake_acoustic.next(&intake_flows), impact as f32, air);
        // Structure-borne combustion noise: summed cylinder dp/dt (Pa/s),
        // high-passed so only the fast pressure-rise content excites the block.
        // Fixed calibration; load and spark timing scale it physically.
        const PA_S_TO_SAMPLE: f64 = 2e-12;
        // Knock rings the chamber gas only in this observation, never the solver.
        if knock {
            pressure += self.knock.pressure();
        }
        let dp_dt = if self.pressure_previous.is_nan() {
            0.
        } else {
            (pressure - self.pressure_previous) * self.rate
        };
        self.pressure_previous = pressure;
        let combustion = self
            .combustion_highpass
            .iter_mut()
            .fold((dp_dt * PA_S_TO_SAMPLE) as f32, |x, f| f.next_sample(x));
        // Piston slap is structure-borne: block input, beside combustion.
        let slap = self.slap.next();
        self.excitation = [contact, combustion, slap];
        let block = combustion + slap;
        let mut mechanical = self.mechanics.next(contact, block);
        let normalized_rpm = ((self.rpm as f32 - self.scratch.idle_rpm)
            / (self.scratch.redline_rpm - self.scratch.idle_rpm))
            .clamp(0., 1.);
        let (mut shaped_exhaust, mut shaped_intake) = self.tone.process(
            exhaust_audio,
            intake_audio,
            normalized_rpm,
            commands.throttle as f32,
        );
        if let Some(previous) = &mut self.previous_tone {
            let old = previous.process(
                exhaust_audio,
                intake_audio,
                normalized_rpm,
                commands.throttle as f32,
            );
            shaped_exhaust = old.0 + (shaped_exhaust - old.0) * self.sound_fade;
            shaped_intake = old.1 + (shaped_intake - old.1) * self.sound_fade;
            if let Some(previous) = &mut self.previous_mechanics {
                let old = previous.next(contact, block);
                mechanical = old + (mechanical - old) * self.sound_fade;
            }
            self.sound_fade = (self.sound_fade + 1. / (self.rate as f32 * 0.03)).min(1.);
            if self.sound_fade >= 1. {
                // These DSP states are inline and own no heap allocations.
                self.previous_tone = None;
                self.previous_mechanics = None;
            }
        }
        let sample = Sample {
            exhaust: shaped_exhaust,
            intake: shaped_intake,
            mechanical,
            bank_pressure,
            rpm: self.rpm,
            map_pa: self.manifolds.intake().pressure_pa,
            torque_nm: torque,
            heat_j: heat,
            correction_j: correction,
            fuel_cut,
            misfires,
            turbo_rpm: self.induction.as_ref().map_or(0., Induction::shaft_rpm),
            afterfire_heat_j: afterfire.iter().sum(),
            idle_bypass,
            fresh_supply_kg_s: fresh_supply,
            fresh_tailpipe_in_kg_s: fresh_tailpipe,
            fuel_injected_kg: injected,
        };
        if !sample.exhaust.is_finite()
            || !sample.intake.is_finite()
            || !sample.mechanical.is_finite()
            || self.rpm > 30000.
        {
            return Err(());
        }
        Ok(sample)
    }
}

fn add_exchange(exchange: &mut Exchange, mass: f64, enthalpy: f64) {
    if mass > 0. {
        exchange.mass_in_kg += mass;
        exchange.enthalpy_in_j += enthalpy;
    } else {
        exchange.mass_out_kg -= mass;
    }
}
fn mechanical_events(
    scratch: &Scratch,
    config: &CylinderConfig,
    rate: u32,
    substeps: usize,
    seed: u64,
) -> (Slap, Knock) {
    let rate = f64::from(rate);
    let step_rate = rate * substeps as f64;
    (
        Slap::new(
            rate,
            step_rate,
            config.bore_m,
            config.stroke_m,
            config.rod_m,
            scratch.build.block,
            seed,
        ),
        Knock::new(
            rate,
            step_rate,
            config.bore_m,
            f64::from(scratch.experimental.knock),
        ),
    )
}
fn crossed(from: f64, to: f64, event: f64) -> bool {
    ((from - event) / (2. * TAU)).floor() < ((to - event) / (2. * TAU)).floor()
}

#[cfg(test)]
mod retune_tests {
    use super::*;

    fn gas_signature(engine: &Engine) -> Vec<[u64; 4]> {
        engine
            .cylinders
            .iter()
            .map(|cylinder| {
                let gas = cylinder.gas_state().unwrap();
                [
                    gas.mass_kg().to_bits(),
                    gas.volume_m3().to_bits(),
                    gas.internal_energy_j().to_bits(),
                    gas.temperature_k().to_bits(),
                ]
            })
            .collect()
    }

    #[test]
    fn retune_preserves_live_gas_phase_and_free_crank_while_only_dsp_fades() {
        let scratch = Scratch::default();
        let mut running = Engine::new(&scratch, 48_000).unwrap();
        let mut unchanged = Engine::new(&scratch, 48_000).unwrap();
        let command = Commands {
            imposed_rpm: None,
            throttle: 0.35,
            ..Default::default()
        };
        for _ in 0..4800 {
            running.next(command);
            unchanged.next(command);
        }
        assert!(!running.failed());
        let gas = gas_signature(&running);
        let angle = running.angle.to_bits();
        let shaft = running.crank.state();
        let mass = running.manifolds.total_mass_kg().to_bits();
        let energy = running.manifolds.total_internal_energy_j().to_bits();
        let mut tuning = scratch.clone();
        tuning.sound.bass_db = 6.;
        tuning.sound.presence_db = -4.;
        tuning.sound.mechanical_pitch_hz = 1800.;
        let mut prepared = Engine::new(&tuning, 48_000).unwrap();
        assert!(running.apply_sound_tuning(&mut prepared));
        assert_eq!(gas_signature(&running), gas);
        assert_eq!(running.angle.to_bits(), angle);
        assert_eq!(running.crank.state().rpm.to_bits(), shaft.rpm.to_bits());
        assert_eq!(running.manifolds.total_mass_kg().to_bits(), mass);
        assert_eq!(
            running.manifolds.total_internal_energy_j().to_bits(),
            energy
        );
        assert!(running.previous_tone.is_some());
        let mut difference = 0.;
        for frame in 0..2000 {
            let a = running.next(command);
            let b = unchanged.next(command);
            assert!(!running.failed());
            assert_eq!(a.rpm.to_bits(), b.rpm.to_bits());
            assert_eq!(a.map_pa.to_bits(), b.map_pa.to_bits());
            assert_eq!(a.heat_j.to_bits(), b.heat_j.to_bits());
            assert_eq!(a.fuel_injected_kg.to_bits(), b.fuel_injected_kg.to_bits());
            if frame == 0 {
                assert_eq!(a.exhaust.to_bits(), b.exhaust.to_bits());
                assert_eq!(a.mechanical.to_bits(), b.mechanical.to_bits());
            }
            difference += (a.exhaust - b.exhaust).abs();
        }
        assert!(difference > 0.001);
        assert!(running.previous_tone.is_none() && running.previous_mechanics.is_none());
        assert_eq!(gas_signature(&running), gas_signature(&unchanged));
    }

    #[test]
    fn retune_rejects_another_engine_or_rate_and_identical_sound_is_a_noop() {
        let scratch = Scratch::default();
        let mut running = Engine::new(&scratch, 48_000).unwrap();
        running.next(Commands::default());
        let gas = gas_signature(&running);
        let angle = running.angle.to_bits();
        let mut other_rate = Engine::new(&scratch, 96_000).unwrap();
        assert!(!running.apply_sound_tuning(&mut other_rate));
        let mut other_build = scratch.clone();
        other_build.build.compression += 0.5;
        let mut other_engine = Engine::new(&other_build, 48_000).unwrap();
        assert!(!running.apply_sound_tuning(&mut other_engine));
        let mut identical = Engine::new(&scratch, 48_000).unwrap();
        assert!(running.apply_sound_tuning(&mut identical));
        assert!(running.previous_tone.is_none());
        assert_eq!(gas_signature(&running), gas);
        assert_eq!(running.angle.to_bits(), angle);
    }
    #[test]
    fn live_sound_retune_keeps_gas_crank_and_thermal_state() {
        let scratch = Scratch::default();
        let mut engine = Engine::new(&scratch, 96_000).unwrap();
        for _ in 0..9600 {
            engine.next(Commands::default());
        }
        let gas = gas_signature(&engine);
        let angle = engine.angle.to_bits();
        let state = engine.state();
        let mass = engine.manifolds.total_mass_kg().to_bits();
        let energy = engine.manifolds.total_internal_energy_j().to_bits();
        let mut sound = scratch.sound;
        sound.bass_db = 8.;
        sound.primary_length_scale = 1.2;
        assert!(engine.set_sound_tuning(&sound));
        assert_eq!(gas_signature(&engine), gas);
        assert_eq!(engine.angle.to_bits(), angle);
        assert_eq!(engine.state().rpm.to_bits(), state.rpm.to_bits());
        assert_eq!(engine.manifolds.total_mass_kg().to_bits(), mass);
        assert_eq!(engine.manifolds.total_internal_energy_j().to_bits(), energy);
        for _ in 0..3200 {
            engine.next(Commands::default());
        }
        assert!(!engine.failed());
        assert!(engine.previous_tone.is_none());
    }
}

#[cfg(test)]
mod mechanical_tests {
    use super::*;

    fn run(scratch: &Scratch, rpm: f64, throttle: f64, mut each: impl FnMut(&Engine, Sample)) {
        let mut engine = Engine::new(scratch, 96_000).unwrap();
        let command = Commands {
            imposed_rpm: Some(rpm),
            throttle,
            ..Default::default()
        };
        for i in 0..96_000 * 2 {
            let s = engine.next(command);
            assert!(!engine.failed() && s.mechanical.is_finite());
            if i >= 96_000 {
                each(&engine, s);
            }
        }
    }
    /// Modal-bank output energy of valve contacts and of piston slap alone.
    fn layer_energy(scratch: &Scratch, rpm: f64, throttle: f64) -> (f64, f64) {
        let engine = Engine::new(scratch, 96_000).unwrap();
        let (mut contact, mut slap) = (engine.mechanics.clone(), engine.mechanics.clone());
        let (mut a, mut b) = (0., 0.);
        run(scratch, rpm, throttle, |e, _| {
            let [c, _, s] = e.mechanical_excitation();
            a += f64::from(contact.next(c, 0.)).powi(2);
            b += f64::from(slap.next(0., s)).powi(2);
        });
        (a, b)
    }
    fn db(ratio: f64) -> f64 {
        10. * ratio.log10()
    }

    #[test]
    fn slap_sits_below_valve_contacts_and_drops_at_idle() {
        let scratch = Scratch::default();
        let (contact, slap) = layer_energy(&scratch, 3000., 0.7);
        let (idle_contact, idle_slap) = layer_energy(&scratch, 850., 0.1);
        let (loaded, idle) = (db(slap / contact), db(idle_slap / idle_contact));
        println!("slap vs contacts: {loaded:.1} dB at 3000/0.7, {idle:.1} dB at idle");
        assert!((-10.0..=-4.).contains(&loaded), "{loaded} dB");
        assert!(idle < loaded - 3., "{idle} dB");
    }
    #[test]
    fn slap_energy_rises_with_load_at_fixed_rpm() {
        let scratch = Scratch::default();
        let slap = |throttle| layer_energy(&scratch, 3000., throttle).1;
        let (closed, part, loaded) = (slap(0.), slap(0.3), slap(1.));
        println!("slap energy {closed:e} {part:e} {loaded:e}");
        assert!(part > closed * 1.2 && loaded > part * 1.2);
    }
    /// Fraction of mechanical energy in 5–9 kHz (two-pole-pair SVF band).
    fn knock_band(knock: f32, throttle: f64, advance: f32) -> (f64, Vec<Sample>) {
        let mut scratch = Scratch::default();
        scratch.experimental.knock = knock;
        scratch.sound.ignition_retard_deg = -advance;
        let mut filters = [
            StateVariableFilter::new(96000., 5000., 0.707, SvfMode::Highpass),
            StateVariableFilter::new(96000., 5000., 0.707, SvfMode::Highpass),
            StateVariableFilter::new(96000., 9000., 0.707, SvfMode::Lowpass),
            StateVariableFilter::new(96000., 9000., 0.707, SvfMode::Lowpass),
        ];
        let (mut band, mut total, mut samples) = (0., 0., Vec::new());
        run(&scratch, 4000., throttle, |_, s| {
            let x = filters
                .iter_mut()
                .fold(s.mechanical, |x, f| f.next_sample(x));
            band += f64::from(x).powi(2);
            total += f64::from(s.mechanical).powi(2);
            samples.push(s);
        });
        (band / total, samples)
    }
    #[test]
    fn knock_rings_5_to_9_khz_only_in_the_observation_and_off_is_inert() {
        let (off, quiet) = knock_band(0., 1., 15.);
        let (on, knocking) = knock_band(1., 1., 15.);
        println!("5–9 kHz share: off {off:.3}, knock {on:.3}");
        // Measured: 0.012 → 0.222.
        assert!(on > off * 5. && on > 0.1, "{off} → {on}");
        // Gas, crank and exhaust never see the ring; only mechanics differ.
        for (a, b) in quiet.iter().zip(&knocking) {
            assert_eq!(a.exhaust.to_bits(), b.exhaust.to_bits());
            assert_eq!(a.torque_nm.to_bits(), b.torque_nm.to_bits());
            assert_eq!(a.heat_j.to_bits(), b.heat_j.to_bits());
        }
        // Where the end gas does not autoignite, knock on is bit-identical.
        let (_, idle_off) = knock_band(0., 0.05, 0.);
        let (_, idle_on) = knock_band(1., 0.05, 0.);
        for (a, b) in idle_off.iter().zip(&idle_on) {
            assert_eq!(a.mechanical.to_bits(), b.mechanical.to_bits());
        }
    }
}
