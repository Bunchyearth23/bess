//! Conservative lumped intake and exhaust volumes, with explicit external ports.
//! Dimensions, Cd and loss coefficients are engineering estimates, not flow maps.
//! Individual throttles currently share one equivalent intake volume. Each
//! cylinder draws through its own runner: a lumped inertance (the runner gas
//! column) feeding a port volume of half the runner, the one-segment π model of
//! a pipe (first mode ≈ 0.225 c/L vs the exact quarter-wave 0.25 c/L). This is
//! what gives a 0D model inertial ram and Helmholtz tuning (Engelman 1953;
//! Heywood, *Internal Combustion Engine Fundamentals*, §7.6); the column's
//! kinetic energy is not a separate ledger term, as for the compressor duct.
//! Exhaust has up to two separate collectors. Every volume's gas properties
//! follow its tracked air/fuel/product composition (`thermo::Mixture`); each
//! port carries the donor's enthalpy, and compositions are refreshed after
//! the step at constant energy (fixed volumes: no p dV work to reconcile).
use super::{
    cylinder::Reservoir,
    gas::Orifice,
    thermo::{EnergyInput, EnergyLedger, GasState, MIN_TEMPERATURE_K, Mixture, ThermoError},
};
use crate::engine_build::{Catalyst, EngineBuild, Muffler, ResolvedTuning};
use std::f64::consts::PI;
const TAIL_SPECIES_CELLS: usize = 16;

pub const ATMOSPHERE: Reservoir = Reservoir {
    pressure_pa: 101325.0,
    temperature_k: 300.0,
};

/// Tracer-and-wall model: fuel/oxygen are constituents of the gas mass already
/// transported by the caller, never extra mass added to the finite volume.
/// Rates, light-off and thermal capacities are explicit engineering estimates.
#[derive(Clone, Debug)]
pub struct AfterTreatment {
    fuel_kg: f64,
    oxygen_kg: f64,
    wall_temperature_k: f64,
    wall_capacity_j_k: f64,
    catalyst: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct AfterTreatmentStep {
    /// Pass to Manifolds::set_exhaust_heat_j; reaction minus solid-wall cooling.
    pub gas_heat_j: f64,
    pub chemical_heat_j: f64,
    pub fuel_burned_kg: f64,
    pub unburnt_fuel_out_kg: f64,
    pub oxygen_out_kg: f64,
    pub wall_energy_change_j: f64,
    pub ambient_heat_j: f64,
}
impl AfterTreatment {
    pub fn new(catalyst: Catalyst) -> Self {
        Self {
            fuel_kg: 0.0,
            oxygen_kg: 0.0,
            wall_temperature_k: 450.0,
            wall_capacity_j_k: if catalyst == Catalyst::None {
                150.0
            } else {
                650.0
            },
            catalyst: catalyst != Catalyst::None,
        }
    }
    pub fn wall_temperature_k(&self) -> f64 {
        self.wall_temperature_k
    }
    pub fn stored_fuel_kg(&self) -> f64 {
        self.fuel_kg
    }
    /// Resident species are authoritative when coupled to Manifolds. Only the
    /// solid wall persists here; transport/washout is already handled upstream.
    pub fn step_resident(
        &mut self,
        dt: f64,
        temperature_k: f64,
        gas_mass_kg: f64,
        fuel_kg: f64,
        oxygen_kg: f64,
    ) -> Result<AfterTreatmentStep, ThermoError> {
        if !(0.0..=gas_mass_kg).contains(&fuel_kg)
            || !(0.0..=gas_mass_kg * 0.233).contains(&oxygen_kg)
        {
            return Err(ThermoError::InvalidInput);
        }
        let mut next = self.clone();
        next.fuel_kg = fuel_kg;
        next.oxygen_kg = oxygen_kg;
        let result = next.step(dt, temperature_k, gas_mass_kg, 0.0, 0.0, 0.0)?;
        *self = next;
        Ok(result)
    }
    /// No ignition without both fuel and oxygen. Tailpipe outflow washes unused
    /// tracers out of the collector. Gas mass is the current collector mass.
    pub fn step(
        &mut self,
        dt: f64,
        gas_temperature_k: f64,
        gas_mass_kg: f64,
        fuel_in_kg: f64,
        oxygen_in_kg: f64,
        tailpipe_out_kg: f64,
    ) -> Result<AfterTreatmentStep, ThermoError> {
        if !(1e-8..=1.0 / 96000.0).contains(&dt)
            || !(200.0..=3500.0).contains(&gas_temperature_k)
            || !(1e-12..=10.0).contains(&gas_mass_kg)
            || [fuel_in_kg, oxygen_in_kg, tailpipe_out_kg]
                .iter()
                .any(|x| !(0.0..=1.0).contains(x))
            || self.fuel_kg + fuel_in_kg > 1.0
            || self.oxygen_kg + oxygen_in_kg > 1.0
        {
            return Err(ThermoError::InvalidInput);
        }
        let fuel = self.fuel_kg + fuel_in_kg;
        let oxygen = self.oxygen_kg + oxygen_in_kg;
        let gas_ignition = ((gas_temperature_k - 900.0) / 400.0).clamp(0.0, 1.0);
        let catalyst_ignition = if self.catalyst {
            ((self.wall_temperature_k - 550.0) / 250.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let rate = gas_ignition / 0.006 + catalyst_ignition / 0.03;
        let burn = fuel.min(oxygen / 3.5) * (1.0 - (-dt * rate).exp());
        // Guard budgets only (collector gas is mostly products); the gas
        // step itself uses the collector's tracked composition.
        let u = |t| Mixture::PRODUCTS.internal_energy(t);
        let burn = burn.min(0.02 * gas_mass_kg * u(gas_temperature_k) / 43e6);
        let heating_room = gas_mass_kg * (u(3500.0) - u(gas_temperature_k)).max(0.0);
        let cooling_room = gas_mass_kg * (u(gas_temperature_k) - u(200.0)).max(0.0);
        let burn = burn.min(heating_room * 0.5 / (0.8 * 43e6));
        let chemical_heat = burn * 43e6;
        let washout = (tailpipe_out_kg / gas_mass_kg).clamp(0.0, 1.0);
        let unburnt_out = (fuel - burn) * washout;
        let oxygen_out = (oxygen - burn * 3.5) * washout;
        self.fuel_kg = fuel - burn - unburnt_out;
        self.oxygen_kg = oxygen - burn * 3.5 - oxygen_out;
        let gas_to_wall = (8.0 * (gas_temperature_k - self.wall_temperature_k) * dt)
            .clamp(-heating_room * 0.5, cooling_room * 0.5);
        let mut ambient = 1.5 * (self.wall_temperature_k - 300.0) * dt;
        let proposed = 0.2 * chemical_heat + gas_to_wall - ambient;
        // Any high-temperature guard heat is explicitly exported to ambient.
        let excess = (self.wall_temperature_k + proposed / self.wall_capacity_j_k - 1800.0)
            .max(0.0)
            * self.wall_capacity_j_k;
        ambient += excess;
        let wall_change = proposed - excess;
        self.wall_temperature_k += wall_change / self.wall_capacity_j_k;
        Ok(AfterTreatmentStep {
            gas_heat_j: 0.8 * chemical_heat - gas_to_wall,
            chemical_heat_j: chemical_heat,
            fuel_burned_kg: burn,
            unburnt_fuel_out_kg: unburnt_out,
            oxygen_out_kg: oxygen_out,
            wall_energy_change_j: wall_change,
            ambient_heat_j: ambient,
        })
    }
}

/// Integrated transfer over one time step. Incoming and outgoing mass remain
/// separate so simultaneous reverse flows retain their different enthalpies.
#[derive(Debug, Clone, Copy, Default)]
pub struct Exchange {
    pub mass_in_kg: f64,
    pub enthalpy_in_j: f64,
    pub mass_out_kg: f64,
}
impl Exchange {
    /// Positive mass enters this manifold. `donor_h` is J/kg and only used for
    /// incoming mass; outflow uses this manifold's old-state enthalpy.
    pub fn add_signed(&mut self, mass_into_manifold: f64, donor_h: f64) {
        if mass_into_manifold >= 0.0 {
            self.mass_in_kg += mass_into_manifold;
            self.enthalpy_in_j += mass_into_manifold * donor_h;
        } else {
            self.mass_out_kg -= mass_into_manifold;
        }
    }
    fn valid(self) -> bool {
        (0.0..=1.0).contains(&self.mass_in_kg)
            && (0.0..=1.0).contains(&self.mass_out_kg)
            && (0.0..=1e7).contains(&self.enthalpy_in_j)
            && (self.mass_in_kg > 0.0 || self.enthalpy_in_j == 0.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Flows {
    /// Direct exchange with the plenum (audits); cylinders use `runner`.
    pub intake: Exchange,
    /// Cylinder `i` ↔ its runner port volume, and its incoming constituents.
    pub runner: [Exchange; 12],
    pub runner_species: [Species; 12],
    pub exhaust: [Exchange; 2],
    /// Incoming constituents only; outgoing species use old manifold fractions.
    pub intake_species: Species,
    pub exhaust_species: [Species; 2],
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Species {
    pub fresh_air_kg: f64,
    pub fuel_kg: f64,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Composition {
    pub fresh_air_fraction: f64,
    pub fuel_fraction: f64,
}
impl Composition {
    pub fn mixture(self) -> Mixture {
        Mixture::from_fractions(self.fresh_air_fraction, self.fuel_fraction)
    }
}
impl Species {
    pub fn composition(self, mass: f64) -> Composition {
        self.composition_per_kg(1.0 / mass)
    }
    /// `composition(mass)` with `inverse = 1.0 / mass` already evaluated.
    fn composition_per_kg(self, inverse: f64) -> Composition {
        Composition {
            fresh_air_fraction: (self.fresh_air_kg * inverse).clamp(0.0, 1.0),
            fuel_fraction: (self.fuel_kg * inverse).clamp(0.0, 1.0),
        }
    }
    pub(super) fn transport(
        self,
        old_mass: f64,
        out: f64,
        incoming: Self,
        mass_in: f64,
    ) -> Result<Self, ThermoError> {
        if !incoming.fresh_air_kg.is_finite()
            || !incoming.fuel_kg.is_finite()
            || incoming.fresh_air_kg < 0.0
            || incoming.fuel_kg < 0.0
            || incoming.fresh_air_kg + incoming.fuel_kg > mass_in * (1.0 + 1e-10) + 1e-15
        {
            return Err(ThermoError::InvalidInput);
        }
        let retained = (1.0 - out / old_mass).clamp(0.0, 1.0);
        Ok(Self {
            fresh_air_kg: self.fresh_air_kg * retained + incoming.fresh_air_kg,
            fuel_kg: self.fuel_kg * retained + incoming.fuel_kg,
        })
    }
}
#[derive(Debug, Clone, Copy)]
pub struct FlowBudgets {
    pub intake_kg: f64,
    pub exhaust_kg: [f64; 2],
}
#[derive(Debug, Clone, Copy, Default)]
pub struct ManifoldStep {
    /// Species-advection CFL limiting, mass difference over all tailpipe ports.
    pub transport_limited_mass_kg: f64,
    /// Incoming constituents to the charge volume on reverse throttle flow.
    pub supply_species: Species,
    /// Opposite side of the throttle port, ready for Induction::step. Positive
    /// incoming mass is reverse flow from manifold into the compressor charge.
    pub supply_exchange: Exchange,
    /// Positive atmosphere (or specified compressor supply) -> intake.
    pub intake_mass_flow_kg_s: f64,
    /// Positive collector -> finite tailpipe/muffler volume.
    pub exhaust_mass_flow_kg_s: [f64; 2],
    /// Positive tailpipe -> atmosphere, the true outside-system gas boundary.
    pub external_tailpipe_mass_flow_kg_s: [f64; 2],
    pub intake_ledger: EnergyLedger,
    /// Summed numerical corrections of the runner port volumes.
    pub runner_correction_j: f64,
    pub exhaust_ledger: [EnergyLedger; 2],
    pub tailpipe_ledger: [EnergyLedger; 2],
    /// Only the external ports; cylinder exchange is excluded.
    pub external_mass_in_kg: f64,
    pub external_mass_out_kg: f64,
    pub external_enthalpy_in_j: f64,
    pub external_enthalpy_out_j: f64,
}

#[derive(Clone)]
pub struct Manifolds {
    intake: GasState,
    runners: [GasState; 12],
    runner_species: [Species; 12],
    /// Plenum → port mass flow, the momentum state of each runner column.
    runner_flow_kg_s: [f64; 12],
    runner_count: usize,
    runner_area_m2: f64,
    runner_length_m: f64,
    exhaust: [GasState; 2],
    tailpipe: [GasState; 2],
    throttle_area_m2: f64,
    exhaust_area_m2: f64,
    exhaust_cd: f64,
    /// Turbocharged: collector → turbine nozzle ∥ wastegate → catalyst/muffler
    /// volume → pipe. Zero turbine area keeps the single lumped restriction.
    turbine_area_m2: f64,
    wastegate_area_m2: f64,
    wastegate: f64,
    active_bank_mask: [bool; 2],
    supply: Reservoir,
    supply_mass_limit_kg: f64,
    exhaust_heat_j: [f64; 2],
    tailpipe_heat_j: [f64; 2],
    intake_species: Species,
    exhaust_species: [Species; 2],
    tailpipe_species: [Species; 2],
    tailpipe_cells: [[Species; TAIL_SPECIES_CELLS]; 2],
    supply_composition: Composition,
}

fn state(volume: f64, p: f64, t: f64, mixture: Mixture) -> Result<GasState, ThermoError> {
    GasState::at_pressure(volume, p, t, mixture)
}
fn reservoir(gas: GasState) -> Reservoir {
    let (temperature_k, pressure_pa) = gas.temperature_pressure();
    Reservoir {
        pressure_pa,
        temperature_k,
    }
}
/// Mass removable at old-state enthalpy before the 200 K floor, with a factor
/// two margin. This budget is shared across every connection to the donor.
fn removable(gas: GasState) -> f64 {
    removable_at(gas, gas.temperature_k())
}
/// `removable` with the donor's already evaluated `temperature_k()`.
fn removable_at(gas: GasState, temperature_k: f64) -> f64 {
    let floor_u = gas.mixture().internal_energy(MIN_TEMPERATURE_K);
    let h = gas.mixture().enthalpy(temperature_k);
    ((gas.internal_energy_j() - gas.mass_kg() * floor_u) / (h - floor_u))
        .max(0.0)
        .min(gas.mass_kg() * 0.9)
        * 0.5
}
/// Orifice flow `from` → `to`; the upstream side supplies the gas properties.
fn rate(
    (from, from_mixture): (Reservoir, Mixture),
    (to, to_mixture): (Reservoir, Mixture),
    area: f64,
    cd: f64,
) -> f64 {
    let upstream = if from.pressure_pa >= to.pressure_pa {
        from_mixture.properties(from.temperature_k)
    } else {
        to_mixture.properties(to.temperature_k)
    };
    Orifice {
        area_m2: area,
        discharge_coefficient: cd,
    }
    .mass_flow_from_states(
        from.pressure_pa,
        from.temperature_k,
        to.pressure_pa,
        to.temperature_k,
        upstream,
    )
}

/// Upwind constituent transport through a compressible mean-gas volume. Each
/// cell holds 1/N of the current gas mass. Linearly interpolating face mass flow
/// makes every cell's mass change equal to (left-right)/N, conserving both the
/// complete mass and constituent inventories as the mean volume fills/empties.
/// Temperature stays the shared 0D temperature; no acoustic wave solver here.
fn advect_tail(
    cells: &mut [Species; TAIL_SPECIES_CELLS],
    old_mass: f64,
    left: f64,
    right: f64,
    collector: Composition,
) -> Result<Species, ThermoError> {
    let cell_mass = old_mass / TAIL_SPECIES_CELLS as f64;
    let per_kg = 1.0 / cell_mass;
    let mut flux = [Species::default(); TAIL_SPECIES_CELLS + 1];
    for (face, transfer) in flux.iter_mut().enumerate() {
        let dm = left + (right - left) * face as f64 / TAIL_SPECIES_CELLS as f64;
        let donor = if dm >= 0.0 {
            if face == 0 {
                collector
            } else {
                cells[face - 1].composition_per_kg(per_kg)
            }
        } else if face == TAIL_SPECIES_CELLS {
            Composition {
                fresh_air_fraction: 1.0,
                fuel_fraction: 0.0,
            }
        } else {
            cells[face].composition_per_kg(per_kg)
        };
        *transfer = Species {
            fresh_air_kg: dm * donor.fresh_air_fraction,
            fuel_kg: dm * donor.fuel_fraction,
        };
    }
    let mut total = Species::default();
    for (i, cell) in cells.iter_mut().enumerate() {
        cell.fresh_air_kg += flux[i].fresh_air_kg - flux[i + 1].fresh_air_kg;
        cell.fuel_kg += flux[i].fuel_kg - flux[i + 1].fuel_kg;
        if cell.fresh_air_kg < -1e-15 || cell.fuel_kg < -1e-15 {
            return Err(ThermoError::InvalidState);
        }
        cell.fresh_air_kg = cell.fresh_air_kg.max(0.0);
        cell.fuel_kg = cell.fuel_kg.max(0.0);
        total.fresh_air_kg += cell.fresh_air_kg;
        total.fuel_kg += cell.fuel_kg;
    }
    Ok(total)
}

impl Manifolds {
    pub fn new(
        build: &EngineBuild,
        tuning: &ResolvedTuning,
        cylinders: u32,
        displacement_m3: f64,
    ) -> Result<Self, String> {
        build.validate()?;
        if !(1..=12).contains(&cylinders) || !(1e-6..=0.5).contains(&displacement_m3) {
            return Err("Invalid manifold cylinder count/displacement".into());
        }
        let intake_volume = tuning.plenum_volume_m3;
        let collector_volume = (displacement_m3 * 0.4).max(0.0001);
        let runner = state(
            tuning.runner_area_m2 * tuning.runner_length_m * 0.5,
            ATMOSPHERE.pressure_pa,
            310.0,
            Mixture::AIR,
        )
        .map_err(|e| format!("{e:?}"))?;
        let intake = state(intake_volume, ATMOSPHERE.pressure_pa, 310.0, Mixture::AIR)
            .map_err(|e| format!("{e:?}"))?;
        // Collector and tailpipe start full of products (no tracer species).
        let exhaust = state(
            collector_volume,
            ATMOSPHERE.pressure_pa,
            650.0,
            Mixture::PRODUCTS,
        )
        .map_err(|e| format!("{e:?}"))?;
        let exhaust_area_m2 = PI * (f64::from(build.exhaust_mm) * 0.001).powi(2) / 4.0;
        // Two metres of pipe plus the estimated muffler cavity. Resident exhaust
        // products isolate the collector from instantaneous fresh-air reentry.
        let muffler_volume = match build.muffler {
            Muffler::None => 0.0,
            Muffler::StraightThrough => 0.002,
            Muffler::Baffled => 0.004,
            Muffler::ReverseFlow => 0.006,
        };
        let tailpipe = state(
            exhaust_area_m2 * 2.0 + muffler_volume,
            ATMOSPHERE.pressure_pa,
            650.0,
            Mixture::PRODUCTS,
        )
        .map_err(|e| format!("{e:?}"))?;
        let cat_loss = match build.catalyst {
            Catalyst::None => 0.0,
            Catalyst::HighFlow => 0.4,
            Catalyst::Standard => 1.2,
        };
        let muffler_loss = match build.muffler {
            Muffler::None => 0.1,
            Muffler::StraightThrough => 0.5,
            Muffler::Baffled => 1.6,
            Muffler::ReverseFlow => 2.5,
        };
        Ok(Self {
            intake,
            runners: [runner; 12],
            runner_species: [Species {
                fresh_air_kg: runner.mass_kg(),
                fuel_kg: 0.0,
            }; 12],
            runner_flow_kg_s: [0.0; 12],
            runner_count: cylinders as usize,
            runner_area_m2: tuning.runner_area_m2,
            runner_length_m: tuning.runner_length_m,
            exhaust: [exhaust; 2],
            tailpipe: [tailpipe; 2],
            throttle_area_m2: tuning.throttle_area_m2,
            exhaust_area_m2,
            exhaust_cd: 0.85 / (1.0_f64 + cat_loss + muffler_loss).sqrt(),
            turbine_area_m2: tuning.turbine_area_m2,
            wastegate_area_m2: tuning.wastegate_area_m2,
            wastegate: 0.0,
            active_bank_mask: [true, false],
            supply: ATMOSPHERE,
            supply_mass_limit_kg: f64::INFINITY,
            exhaust_heat_j: [0.0; 2],
            tailpipe_heat_j: [0.0; 2],
            intake_species: Species {
                fresh_air_kg: intake.mass_kg(),
                fuel_kg: 0.0,
            },
            exhaust_species: [Species::default(); 2],
            tailpipe_species: [Species::default(); 2],
            tailpipe_cells: [[Species::default(); TAIL_SPECIES_CELLS]; 2],
            supply_composition: Composition {
                fresh_air_fraction: 1.0,
                fuel_fraction: 0.0,
            },
        })
    }
    /// Configure before the simulation starts. Each collector retains its own
    /// physical volume and its own `exhaust_mm` pipe (dual exhaust, as the
    /// acoustic tails); one turbine's nozzle/wastegate area is shared.
    pub fn set_active_banks(&mut self, banks: usize) -> Result<(), ThermoError> {
        if !(1..=2).contains(&banks) {
            return Err(ThermoError::InvalidInput);
        }
        self.set_active_bank_mask([true, banks == 2])
    }
    /// Configure before stepping. Bank indices are stable even when only the
    /// second bank is connected; unused collectors have no ports or budget.
    pub fn set_active_bank_mask(&mut self, mask: [bool; 2]) -> Result<(), ThermoError> {
        if !mask.iter().any(|active| *active) {
            return Err(ThermoError::InvalidInput);
        }
        self.active_bank_mask = mask;
        Ok(())
    }
    pub fn intake(&self) -> Reservoir {
        reservoir(self.intake)
    }
    /// Gas properties follow the transported species (same mass and energy).
    fn refresh_mixtures(&mut self) {
        let mix = |gas: &mut GasState, species: Species| {
            gas.set_mixture(species.composition(gas.mass_kg()).mixture());
        };
        mix(&mut self.intake, self.intake_species);
        for i in 0..self.runner_count {
            mix(&mut self.runners[i], self.runner_species[i]);
        }
        for i in 0..2 {
            mix(&mut self.exhaust[i], self.exhaust_species[i]);
            mix(&mut self.tailpipe[i], self.tailpipe_species[i]);
        }
    }
    pub fn intake_composition(&self) -> Composition {
        self.intake_species.composition(self.intake.mass_kg())
    }
    /// Port volume at the intake valve of cylinder `i`.
    pub fn runner(&self, i: usize) -> Reservoir {
        reservoir(self.runners[i])
    }
    pub fn runner_composition(&self, i: usize) -> Composition {
        self.runner_species[i].composition(self.runners[i].mass_kg())
    }
    /// Cylinder `i` may draw half the safe port budget; the rest is reserved
    /// for reverse flow into the plenum.
    pub fn runner_budget_kg(&self, i: usize) -> f64 {
        removable(self.runners[i]) * 0.5
    }
    /// `(runner(i), runner_composition(i), runner_budget_kg(i))` from one
    /// temperature evaluation, for the per-cylinder hot path.
    pub fn runner_port(&self, i: usize) -> (Reservoir, Composition, f64) {
        let gas = self.runners[i];
        let (temperature_k, pressure_pa) = gas.temperature_pressure();
        (
            Reservoir {
                pressure_pa,
                temperature_k,
            },
            self.runner_composition(i),
            removable_at(gas, temperature_k) * 0.5,
        )
    }
    /// `(exhaust_bank(b), exhaust_composition(b), outgoing_budgets().exhaust_kg[b])`
    /// from one temperature evaluation.
    pub fn exhaust_port(&self, bank: usize) -> (Reservoir, Composition, f64) {
        let bank = bank.min(1);
        let gas = self.exhaust[bank];
        let (temperature_k, pressure_pa) = gas.temperature_pressure();
        let budget = if self.active_bank_mask[bank] {
            removable_at(gas, temperature_k) * 0.5
        } else {
            0.0
        };
        (
            Reservoir {
                pressure_pa,
                temperature_k,
            },
            self.exhaust_composition(bank),
            budget,
        )
    }
    pub fn exhaust_composition(&self, bank: usize) -> Composition {
        self.exhaust_species[bank.min(1)].composition(self.exhaust[bank.min(1)].mass_kg())
    }
    pub fn exhaust_species(&self, bank: usize) -> Species {
        self.exhaust_species[bank.min(1)]
    }
    pub fn total_species(&self) -> Species {
        Species {
            fresh_air_kg: self.intake_species.fresh_air_kg
                + self.runner_species[..self.runner_count]
                    .iter()
                    .map(|s| s.fresh_air_kg)
                    .sum::<f64>()
                + self
                    .exhaust_species
                    .iter()
                    .map(|s| s.fresh_air_kg)
                    .sum::<f64>()
                + self
                    .tailpipe_species
                    .iter()
                    .map(|s| s.fresh_air_kg)
                    .sum::<f64>(),
            fuel_kg: self.intake_species.fuel_kg
                + self.runner_species[..self.runner_count]
                    .iter()
                    .map(|s| s.fuel_kg)
                    .sum::<f64>()
                + self.exhaust_species.iter().map(|s| s.fuel_kg).sum::<f64>()
                + self.tailpipe_species.iter().map(|s| s.fuel_kg).sum::<f64>(),
        }
    }
    /// Reactants become products without changing total gas mass. Matching the
    /// chemical heat step is the caller's responsibility; no extra heat here.
    pub fn consume_exhaust_reactants(
        &mut self,
        bank: usize,
        fuel_kg: f64,
        oxygen_kg: f64,
    ) -> Result<(), ThermoError> {
        if bank > 1
            || !fuel_kg.is_finite()
            || !oxygen_kg.is_finite()
            || fuel_kg < 0.0
            || oxygen_kg < 0.0
        {
            return Err(ThermoError::InvalidInput);
        }
        let s = &mut self.exhaust_species[bank];
        let fresh = oxygen_kg / 0.233;
        if fuel_kg > s.fuel_kg + 1e-15 || fresh > s.fresh_air_kg + 1e-15 {
            return Err(ThermoError::InvalidInput);
        }
        s.fuel_kg = (s.fuel_kg - fuel_kg).max(0.0);
        s.fresh_air_kg = (s.fresh_air_kg - fresh).max(0.0);
        let mixture = self.exhaust_composition(bank).mixture();
        self.exhaust[bank].set_mixture(mixture);
        Ok(())
    }
    pub fn set_supply_composition(&mut self, composition: Composition) -> Result<(), ThermoError> {
        if !(0.0..=1.0).contains(&composition.fresh_air_fraction)
            || !(0.0..=1.0).contains(&composition.fuel_fraction)
            || composition.fresh_air_fraction + composition.fuel_fraction > 1.0 + 1e-10
        {
            return Err(ThermoError::InvalidInput);
        }
        self.supply_composition = composition;
        Ok(())
    }
    pub fn exhaust(&self) -> Reservoir {
        self.exhaust_bank(0)
    }
    pub fn exhaust_bank(&self, bank: usize) -> Reservoir {
        reservoir(self.exhaust[bank.min(1)])
    }
    pub fn tailpipe_pressure_pa(&self, bank: usize) -> f64 {
        self.tailpipe[bank.min(1)].pressure_pa()
    }
    pub fn exhaust_mass_kg(&self, bank: usize) -> f64 {
        self.exhaust[bank.min(1)].mass_kg()
    }
    pub fn total_mass_kg(&self) -> f64 {
        self.intake.mass_kg()
            + self.runners[..self.runner_count]
                .iter()
                .map(|g| g.mass_kg())
                .sum::<f64>()
            + self.exhaust.iter().map(|g| g.mass_kg()).sum::<f64>()
            + self.tailpipe.iter().map(|g| g.mass_kg()).sum::<f64>()
    }
    pub fn total_internal_energy_j(&self) -> f64 {
        self.intake.internal_energy_j()
            + self.runners[..self.runner_count]
                .iter()
                .map(|g| g.internal_energy_j())
                .sum::<f64>()
            + self
                .exhaust
                .iter()
                .map(|g| g.internal_energy_j())
                .sum::<f64>()
            + self
                .tailpipe
                .iter()
                .map(|g| g.internal_energy_j())
                .sum::<f64>()
    }
    /// Cylinder exchanges may consume half the safe donor budget. The other half
    /// is reserved for reverse throttle flow and forward tailpipe flow.
    pub fn outgoing_budgets(&self) -> FlowBudgets {
        FlowBudgets {
            intake_kg: removable(self.intake) * 0.5,
            exhaust_kg: std::array::from_fn(|i| {
                if self.active_bank_mask[i] {
                    removable(self.exhaust[i]) * 0.5
                } else {
                    0.0
                }
            }),
        }
    }
    /// Wastegate opening 0–1 for the next step (turbocharged builds only).
    pub fn set_wastegate(&mut self, opening: f64) -> Result<(), ThermoError> {
        if !(0.0..=1.0).contains(&opening) {
            return Err(ThermoError::InvalidInput);
        }
        self.wastegate = opening;
        Ok(())
    }
    pub fn set_supply(&mut self, supply: Reservoir) -> Result<(), ThermoError> {
        if !(10000.0..=1e6).contains(&supply.pressure_pa)
            || !(200.0..=2000.0).contains(&supply.temperature_k)
        {
            return Err(ThermoError::InvalidInput);
        }
        self.supply = supply;
        Ok(())
    }
    /// Per-step total available charge mass, set before evaluating the throttle.
    pub fn set_supply_limit(&mut self, mass_kg: f64) -> Result<(), ThermoError> {
        if !mass_kg.is_finite() || mass_kg < 0.0 {
            return Err(ThermoError::InvalidInput);
        }
        self.supply_mass_limit_kg = mass_kg;
        Ok(())
    }
    /// Signed physical heat for the next accepted step, e.g. catalyst reaction
    /// minus heat transferred to its solid wall. It is exposed in the gas ledger.
    pub fn set_exhaust_heat_j(&mut self, heat_j: [f64; 2]) -> Result<(), ThermoError> {
        if heat_j.iter().any(|h| !(-1e6..=1e6).contains(h)) {
            return Err(ThermoError::InvalidInput);
        }
        self.exhaust_heat_j = heat_j;
        Ok(())
    }
    /// Next accepted tailpipe step's physical heat, including negative turbine
    /// shaft extraction. Exposed separately for complete gas/shaft audits.
    pub fn set_tailpipe_heat_j(&mut self, heat_j: [f64; 2]) -> Result<(), ThermoError> {
        if heat_j.iter().any(|h| !(-1e6..=1e6).contains(h)) {
            return Err(ThermoError::InvalidInput);
        }
        self.tailpipe_heat_j = heat_j;
        Ok(())
    }
    /// Butterfly aperture = full area*(1-cos(angle)); calibrated leak/bypass are
    /// represented as actual areas. A closed throttle therefore still passes air.
    pub fn throttle_area_m2(&self, throttle: f64, idle_bypass: f64) -> f64 {
        self.throttle_area_m2
            * ((1.0 - (throttle.clamp(0.0, 1.0) * PI * 0.5).cos())
                // At 2 L this leak is 0.713 mm² (0.95 mm equivalent bore).
                // The former 4.75 mm² leak exceeded no-load idle air demand,
                // preventing the PI controller from reducing air at its stop.
                + 0.0003
                + 0.035 * idle_bypass.clamp(0.0, 1.0))
    }
    /// Rejected flows (budgets, validity) leave the state untouched. The state
    /// is advanced in place: a gas-step guard failing after that validation
    /// leaves it part-advanced (the engine then stays failed until `reset`).
    pub fn step(
        &mut self,
        dt: f64,
        throttle: f64,
        idle_bypass: f64,
        flows: Flows,
    ) -> Result<ManifoldStep, ThermoError> {
        self.step_ports(dt, throttle, idle_bypass, flows, true)
    }
    /// Closed boundaries are useful for auditing only: cylinders can exchange
    /// with volumes but the throttle/tailpipe no longer cross the system boundary.
    pub fn step_closed(&mut self, dt: f64, flows: Flows) -> Result<ManifoldStep, ThermoError> {
        self.step_ports(dt, 0.0, 0.0, flows, false)
    }
    fn step_ports(
        &mut self,
        dt: f64,
        throttle: f64,
        idle_bypass: f64,
        flows: Flows,
        boundaries: bool,
    ) -> Result<ManifoldStep, ThermoError> {
        if !(1e-8..=1.0 / 96000.0).contains(&dt)
            || !throttle.is_finite()
            || !idle_bypass.is_finite()
            || !flows.intake.valid()
            || flows.runner.iter().any(|f| !f.valid())
            || flows.runner[self.runner_count..]
                .iter()
                .any(|f| f.mass_in_kg != 0.0 || f.mass_out_kg != 0.0)
            || flows.exhaust.iter().any(|f| !f.valid())
            || (0..2).any(|i| {
                !self.active_bank_mask[i]
                    && (flows.exhaust[i].mass_in_kg != 0.0
                        || flows.exhaust[i].enthalpy_in_j != 0.0
                        || flows.exhaust_species[i].fresh_air_kg != 0.0
                        || flows.exhaust_species[i].fuel_kg != 0.0)
            })
        {
            return Err(ThermoError::InvalidInput);
        }
        // Old-state reservoirs and removable masses, each evaluated once.
        let old = |gas: GasState| {
            let (temperature_k, pressure_pa) = gas.temperature_pressure();
            let reservoir = Reservoir {
                pressure_pa,
                temperature_k,
            };
            (reservoir, removable_at(gas, temperature_k))
        };
        let (plenum, intake_removable) = old(self.intake);
        let mut runners = [(ATMOSPHERE, 0.0); 12];
        for (runner, gas) in runners.iter_mut().zip(&self.runners[..self.runner_count]) {
            *runner = old(*gas);
        }
        let active = |i: usize, gas: GasState| {
            if self.active_bank_mask[i] {
                old(gas)
            } else {
                (ATMOSPHERE, 0.0)
            }
        };
        let collectors = [0, 1].map(|i| active(i, self.exhaust[i]));
        let tails = [0, 1].map(|i| active(i, self.tailpipe[i]));
        if flows.intake.mass_out_kg > intake_removable * 0.5 * (1.0 + 1e-12)
            || (0..self.runner_count)
                .any(|i| flows.runner[i].mass_out_kg > runners[i].1 * 0.5 * (1.0 + 1e-12))
            || (0..2).any(|i| flows.exhaust[i].mass_out_kg > collectors[i].1 * 0.5 * (1.0 + 1e-12))
        {
            return Err(ThermoError::InvalidInput);
        }
        let mut result = ManifoldStep::default();
        // Runner columns: dṁ/dt = (A/L)(p_plenum − p_port) − (A/L)·K ṁ|ṁ|/(2ρA²),
        // entry/bend loss K = 0.5 (sharp-edged plenum entry, Idelchik) taken
        // semi-implicitly. Inertia lets flow overshoot pressure equilibrium:
        // that is the ram effect, so only mass/thermal budgets limit it.
        let plenum_mixture = self.intake.mixture();
        let plenum_h = plenum_mixture.enthalpy(plenum.temperature_k);
        let plenum_composition = self.intake_composition();
        let share = (intake_removable * 0.5 - flows.intake.mass_out_kg).max(0.0)
            / self.runner_count.max(1) as f64;
        let mut plenum_exchange = Exchange::default();
        let mut plenum_species = Species::default();
        for i in 0..self.runner_count {
            let (port, port_removable) = runners[i];
            let old_runner = self.runners[i];
            let port_mixture = old_runner.mixture();
            let (upstream, upstream_mixture) = if plenum.pressure_pa >= port.pressure_pa {
                (plenum, plenum_mixture)
            } else {
                (port, port_mixture)
            };
            let density =
                upstream.pressure_pa / (upstream_mixture.gas_constant() * upstream.temperature_k);
            let gain = dt * self.runner_area_m2 / self.runner_length_m;
            let old = self.runner_flow_kg_s[i];
            let loss = 0.5 * old.abs() / (2.0 * density * self.runner_area_m2.powi(2));
            let flow = (old + gain * (plenum.pressure_pa - port.pressure_pa)) / (1.0 + gain * loss);
            let dm = (flow * dt)
                .min(share)
                .max(-(port_removable - flows.runner[i].mass_out_kg).max(0.0));
            self.runner_flow_kg_s[i] = dm / dt;
            let port_h = port_mixture.enthalpy(port.temperature_k);
            let port_composition = self.runner_composition(i);
            plenum_exchange.add_signed(-dm, port_h);
            plenum_species.fresh_air_kg += (-dm).max(0.0) * port_composition.fresh_air_fraction;
            plenum_species.fuel_kg += (-dm).max(0.0) * port_composition.fuel_fraction;
            let mut input = flows.runner[i];
            input.add_signed(dm, plenum_h);
            let ledger = self.runners[i].step(
                old_runner.volume_m3(),
                EnergyInput {
                    mass_in_kg: input.mass_in_kg,
                    enthalpy_in_j: input.enthalpy_in_j,
                    mass_out_kg: input.mass_out_kg,
                    ..Default::default()
                },
            )?;
            result.runner_correction_j += ledger.numerical_correction_j;
            self.runner_species[i] = self.runner_species[i].transport(
                old_runner.mass_kg(),
                ledger.mass_out_kg,
                Species {
                    fresh_air_kg: flows.runner_species[i].fresh_air_kg
                        + dm.max(0.0) * plenum_composition.fresh_air_fraction,
                    fuel_kg: flows.runner_species[i].fuel_kg
                        + dm.max(0.0) * plenum_composition.fuel_fraction,
                },
                ledger.mass_in_kg,
            )?;
        }
        let supply_mixture = self.supply_composition.mixture();
        let intake_flux = if boundaries {
            rate(
                (self.supply, supply_mixture),
                (plenum, plenum_mixture),
                self.throttle_area_m2(throttle, idle_bypass),
                0.75,
            ) * dt
        } else {
            0.0
        };
        let intake_flux = intake_flux
            .max(-(intake_removable - flows.intake.mass_out_kg - plenum_exchange.mass_out_kg))
            .min(self.supply_mass_limit_kg);
        let mut intake_input = flows.intake;
        intake_input.mass_in_kg += plenum_exchange.mass_in_kg;
        intake_input.enthalpy_in_j += plenum_exchange.enthalpy_in_j;
        intake_input.mass_out_kg += plenum_exchange.mass_out_kg;
        let supply_h = supply_mixture.enthalpy(self.supply.temperature_k);
        let intake_h = plenum_mixture.enthalpy(plenum.temperature_k);
        intake_input.add_signed(intake_flux, supply_h);
        let old_intake = self.intake;
        result.intake_ledger = self.intake.step(
            old_intake.volume_m3(),
            EnergyInput {
                mass_in_kg: intake_input.mass_in_kg,
                enthalpy_in_j: intake_input.enthalpy_in_j,
                mass_out_kg: intake_input.mass_out_kg,
                ..Default::default()
            },
        )?;
        result.intake_mass_flow_kg_s = intake_flux / dt;
        result.supply_exchange.add_signed(-intake_flux, intake_h);
        result.supply_species = Species {
            fresh_air_kg: (-intake_flux).max(0.0) * plenum_composition.fresh_air_fraction,
            fuel_kg: (-intake_flux).max(0.0) * plenum_composition.fuel_fraction,
        };
        let incoming_species = Species {
            fresh_air_kg: flows.intake_species.fresh_air_kg
                + plenum_species.fresh_air_kg
                + intake_flux.max(0.0) * self.supply_composition.fresh_air_fraction,
            fuel_kg: flows.intake_species.fuel_kg
                + plenum_species.fuel_kg
                + intake_flux.max(0.0) * self.supply_composition.fuel_fraction,
        };
        self.intake_species = self.intake_species.transport(
            old_intake.mass_kg(),
            result.intake_ledger.mass_out_kg,
            incoming_species,
            result.intake_ledger.mass_in_kg,
        )?;
        if intake_flux >= 0.0 {
            result.external_mass_in_kg += intake_flux;
            result.external_enthalpy_in_j += intake_flux * supply_h;
        } else {
            result.external_mass_out_kg -= intake_flux;
            result.external_enthalpy_out_j -= intake_flux * intake_h;
        }
        let active_banks = self
            .active_bank_mask
            .iter()
            .filter(|active| **active)
            .count();
        for (i, exchange) in flows.exhaust.iter().enumerate() {
            if !self.active_bank_mask[i] {
                continue;
            }
            let mut input = *exchange;
            // Turbine nozzle Cd 0.9; the aftertreatment loss moves downstream.
            let (area, cd, tail_cd) = if self.turbine_area_m2 > 0.0 {
                (
                    (self.turbine_area_m2 + self.wastegate * self.wastegate_area_m2)
                        / active_banks as f64,
                    0.9,
                    self.exhaust_cd,
                )
            } else {
                (self.exhaust_area_m2, self.exhaust_cd, 0.9)
            };
            let (collector_reservoir, collector_removable) = collectors[i];
            let (tail_reservoir, tail_removable) = tails[i];
            let (old_exhaust, old_tail) = (self.exhaust[i], self.tailpipe[i]);
            let collector_composition = self.exhaust_composition(i);
            let collector = (collector_reservoir, old_exhaust.mixture());
            let tail = (tail_reservoir, old_tail.mixture());
            let collector_h = collector.1.enthalpy(collector.0.temperature_k);
            let tail_h = tail.1.enthalpy(tail.0.temperature_k);
            let ambient_h = Mixture::AIR.enthalpy(ATMOSPHERE.temperature_k);
            let out = rate(collector, tail, area, cd) * dt;
            let cfl_mass = old_tail.mass_kg() / TAIL_SPECIES_CELLS as f64 * 0.25;
            let bounded_out = out
                .min(collector_removable - exchange.mass_out_kg)
                .max(-tail_removable * 0.5)
                .clamp(-cfl_mass, cfl_mass);
            result.transport_limited_mass_kg += (out - bounded_out).abs();
            let out = bounded_out;
            input.add_signed(-out, tail_h);
            result.exhaust_ledger[i] = self.exhaust[i].step(
                old_exhaust.volume_m3(),
                EnergyInput {
                    heat_j: self.exhaust_heat_j[i],
                    mass_in_kg: input.mass_in_kg,
                    enthalpy_in_j: input.enthalpy_in_j,
                    mass_out_kg: input.mass_out_kg,
                    ..Default::default()
                },
            )?;
            result.exhaust_mass_flow_kg_s[i] = out / dt;
            let tail_composition = self.tailpipe_cells[i][0]
                .composition(old_tail.mass_kg() / TAIL_SPECIES_CELLS as f64);
            let incoming_species = Species {
                fresh_air_kg: flows.exhaust_species[i].fresh_air_kg
                    + (-out).max(0.0) * tail_composition.fresh_air_fraction,
                fuel_kg: flows.exhaust_species[i].fuel_kg
                    + (-out).max(0.0) * tail_composition.fuel_fraction,
            };
            self.exhaust_species[i] = self.exhaust_species[i].transport(
                old_exhaust.mass_kg(),
                result.exhaust_ledger[i].mass_out_kg,
                incoming_species,
                result.exhaust_ledger[i].mass_in_kg,
            )?;
            let tail_out = if boundaries {
                rate(
                    tail,
                    (ATMOSPHERE, Mixture::AIR),
                    self.exhaust_area_m2,
                    tail_cd,
                ) * dt
            } else {
                0.0
            };
            let bounded_tail_out = tail_out
                .min(tail_removable - (-out).max(0.0))
                .clamp(-cfl_mass, cfl_mass);
            result.transport_limited_mass_kg += (tail_out - bounded_tail_out).abs();
            let tail_out = bounded_tail_out;
            let mut tail_input = Exchange::default();
            tail_input.add_signed(out, collector_h);
            tail_input.add_signed(-tail_out, ambient_h);
            result.tailpipe_ledger[i] = self.tailpipe[i].step(
                old_tail.volume_m3(),
                EnergyInput {
                    heat_j: self.tailpipe_heat_j[i],
                    mass_in_kg: tail_input.mass_in_kg,
                    mass_out_kg: tail_input.mass_out_kg,
                    enthalpy_in_j: tail_input.enthalpy_in_j,
                    ..Default::default()
                },
            )?;
            self.tailpipe_species[i] = advect_tail(
                &mut self.tailpipe_cells[i],
                old_tail.mass_kg(),
                out,
                tail_out,
                collector_composition,
            )?;
            result.external_tailpipe_mass_flow_kg_s[i] = tail_out / dt;
            if tail_out >= 0.0 {
                result.external_mass_out_kg += tail_out;
                result.external_enthalpy_out_j += tail_out * tail_h;
            } else {
                result.external_mass_in_kg -= tail_out;
                result.external_enthalpy_in_j -= tail_out * ambient_h;
            }
        }
        self.exhaust_heat_j = [0.0; 2];
        self.tailpipe_heat_j = [0.0; 2];
        self.refresh_mixtures();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_build::EngineTuning;
    const DT: f64 = 1.0 / 96000.0;
    fn setup() -> Manifolds {
        let build = EngineBuild::default();
        Manifolds::new(
            &build,
            &EngineTuning::default().resolve(&build, 4),
            4,
            0.002,
        )
        .unwrap()
    }
    #[test]
    fn lone_second_bank_matches_first_bank_without_phantom_ports() {
        let mut first = setup();
        let mut second = setup();
        second.set_active_bank_mask([false, true]).unwrap();
        assert!(second.set_active_bank_mask([false, false]).is_err());
        assert_eq!(second.outgoing_budgets().exhaust_kg[0], 0.0);
        let dormant_mass = second.exhaust[0].mass_kg();
        let dormant_energy = second.tailpipe[0].internal_energy_j();
        for _ in 0..2000 {
            let mut a = Flows::default();
            a.exhaust[0].add_signed(1e-7, Mixture::from_fractions(0.2, 0.01).enthalpy(950.0));
            a.exhaust_species[0] = Species {
                fresh_air_kg: 2e-8,
                fuel_kg: 1e-9,
            };
            let mut b = a;
            b.exhaust.swap(0, 1);
            b.exhaust_species.swap(0, 1);
            first.set_tailpipe_heat_j([-0.0001, 0.0]).unwrap();
            second.set_tailpipe_heat_j([0.0, -0.0001]).unwrap();
            let sa = first.step(DT, 0.1, 0.0, a).unwrap();
            let sb = second.step(DT, 0.1, 0.0, b).unwrap();
            assert_eq!(sa.exhaust_mass_flow_kg_s[0], sb.exhaust_mass_flow_kg_s[1]);
            assert_eq!(sa.external_mass_in_kg, sb.external_mass_in_kg);
            assert_eq!(sa.external_mass_out_kg, sb.external_mass_out_kg);
            assert_eq!(sb.exhaust_mass_flow_kg_s[0], 0.0);
            assert_eq!(sb.external_tailpipe_mass_flow_kg_s[0], 0.0);
        }
        assert_eq!(
            first.exhaust_bank(0).pressure_pa,
            second.exhaust_bank(1).pressure_pa
        );
        assert_eq!(first.total_mass_kg(), second.total_mass_kg());
        assert!((first.total_internal_energy_j() - second.total_internal_energy_j()).abs() < 1e-10);
        assert_eq!(
            first.total_species().fuel_kg,
            second.total_species().fuel_kg
        );
        assert_eq!(second.exhaust[0].mass_kg(), dormant_mass);
        assert_eq!(second.tailpipe[0].internal_energy_j(), dormant_energy);
        let mut invalid = Flows::default();
        invalid.exhaust[0].mass_in_kg = 1e-8;
        assert!(second.step(DT, 0.0, 0.0, invalid).is_err());
    }
    #[test]
    fn shallow_tailpipe_breathing_does_not_instantly_mix_ambient_air_to_collector() {
        let mut cells = [Species::default(); TAIL_SPECIES_CELLS];
        let mass = 0.005;
        let displacement = mass * 0.0001;
        let mut net_air = 0.0;
        let mut inventory = Species::default();
        for i in 0..4000 {
            let flow = if i % 2 == 0 {
                -displacement
            } else {
                displacement
            };
            let head = cells[0].composition(mass / TAIL_SPECIES_CELLS as f64);
            let tail = cells[TAIL_SPECIES_CELLS - 1].composition(mass / TAIL_SPECIES_CELLS as f64);
            if flow < 0.0 {
                net_air += displacement * (1.0 - head.fresh_air_fraction)
            } else {
                net_air -= displacement * tail.fresh_air_fraction;
            }
            inventory = advect_tail(&mut cells, mass, flow, flow, Composition::default()).unwrap();
        }
        assert!((inventory.fresh_air_kg - net_air).abs() < 1e-12);
        assert!(
            cells[0]
                .composition(mass / TAIL_SPECIES_CELLS as f64)
                .fresh_air_fraction
                < 1e-4
        );
        assert!(
            cells[TAIL_SPECIES_CELLS - 1]
                .composition(mass / TAIL_SPECIES_CELLS as f64)
                .fresh_air_fraction
                > 0.1
        );
    }
    #[test]
    fn collector_depression_reaspirates_resident_products_before_ambient_air() {
        let mut m = setup();
        m.exhaust[0] = state(m.exhaust[0].volume_m3(), 50000.0, 650.0, Mixture::PRODUCTS).unwrap();
        let s = m.step(DT, 0.0, 0.0, Flows::default()).unwrap();
        assert!(s.exhaust_mass_flow_kg_s[0] < 0.0);
        assert_eq!(s.external_tailpipe_mass_flow_kg_s[0], 0.0);
        assert_eq!(m.exhaust_composition(0).fresh_air_fraction, 0.0);
    }
    #[test]
    fn complete_tailpipe_network_closes_energy_and_mass_with_turbine_heat() {
        let mut m = setup();
        let initial_mass = m.total_mass_kg();
        let initial_energy = m.total_internal_energy_j();
        let mut supplied_mass = 0.0;
        let mut supplied_energy = 0.0;
        let mut corrections = 0.0;
        for _ in 0..10000 {
            let mut f = Flows::default();
            f.exhaust[0].add_signed(1e-7, Mixture::PRODUCTS.enthalpy(900.0));
            m.set_tailpipe_heat_j([-0.001, 0.0]).unwrap();
            let s = m.step(DT, 0.0, 0.0, f).unwrap();
            assert_eq!(s.transport_limited_mass_kg, 0.0);
            supplied_mass += 1e-7 + s.external_mass_in_kg - s.external_mass_out_kg;
            supplied_energy += f.exhaust[0].enthalpy_in_j - 0.001 + s.external_enthalpy_in_j
                - s.external_enthalpy_out_j;
            corrections += s.intake_ledger.numerical_correction_j
                + s.exhaust_ledger
                    .iter()
                    .chain(s.tailpipe_ledger.iter())
                    .map(|l| l.numerical_correction_j)
                    .sum::<f64>();
        }
        assert!((m.total_mass_kg() - initial_mass - supplied_mass).abs() < 1e-12);
        assert!(
            (m.total_internal_energy_j() - initial_energy - supplied_energy - corrections).abs()
                < 1e-7
        );
        assert!(corrections.abs() < 1e-7);
    }
    #[test]
    fn recycled_products_do_not_turn_into_fresh_air_or_new_fuel() {
        let mut m = setup();
        let fresh_before = m.total_species().fresh_air_kg;
        for _ in 0..4000 {
            let dm = m
                .outgoing_budgets()
                .intake_kg
                .min(m.outgoing_budgets().exhaust_kg[0])
                * 0.01;
            let intake = m.intake_composition();
            let exhaust = m.exhaust_composition(0);
            let mut f = Flows::default();
            f.intake.mass_out_kg = dm;
            f.intake
                .add_signed(dm, exhaust.mixture().enthalpy(m.exhaust().temperature_k));
            f.intake_species = Species {
                fresh_air_kg: dm * exhaust.fresh_air_fraction,
                fuel_kg: dm * exhaust.fuel_fraction,
            };
            f.exhaust[0].mass_out_kg = dm;
            f.exhaust[0].add_signed(dm, intake.mixture().enthalpy(m.intake().temperature_k));
            f.exhaust_species[0] = Species {
                fresh_air_kg: dm * intake.fresh_air_fraction,
                fuel_kg: dm * intake.fuel_fraction,
            };
            m.step_closed(DT, f).unwrap();
        }
        assert!((m.total_species().fresh_air_kg - fresh_before).abs() < 1e-12);
        assert_eq!(m.total_species().fuel_kg, 0.0);
        assert!(m.intake_composition().fresh_air_fraction < 0.9);
        assert!(m.exhaust_composition(0).fresh_air_fraction > 0.1);
    }
    #[test]
    fn resident_reaction_consumes_manifold_constituents_not_total_mass() {
        let mut m = setup();
        let mut f = Flows::default();
        f.exhaust[0].add_signed(0.0001, Mixture::from_fractions(0.9, 0.05).enthalpy(3000.0));
        f.exhaust_species[0] = Species {
            fresh_air_kg: 0.00009,
            fuel_kg: 0.000005,
        };
        m.step_closed(DT, f).unwrap();
        let total_mass = m.total_mass_kg();
        let before = m.exhaust_species(0);
        let mut treatment = AfterTreatment::new(Catalyst::Standard);
        let s = treatment
            .step_resident(
                DT,
                m.exhaust().temperature_k,
                m.exhaust_mass_kg(0),
                before.fuel_kg,
                before.fresh_air_kg * 0.233,
            )
            .unwrap();
        m.consume_exhaust_reactants(0, s.fuel_burned_kg, s.fuel_burned_kg * 3.5)
            .unwrap();
        assert!(s.fuel_burned_kg > 0.0);
        assert_eq!(m.total_mass_kg(), total_mass);
        assert!((m.exhaust_species(0).fuel_kg - before.fuel_kg + s.fuel_burned_kg).abs() < 1e-15);
        assert!(
            (m.exhaust_species(0).fresh_air_kg - before.fresh_air_kg
                + s.fuel_burned_kg * 3.5 / 0.233)
                .abs()
                < 1e-15
        );
    }
    #[test]
    fn aftertreatment_needs_oxygen_and_accounts_reaction_wall_and_washout() {
        let mut dry = AfterTreatment::new(Catalyst::Standard);
        let no_oxygen = dry.step(DT, 1200.0, 0.001, 1e-5, 0.0, 0.0).unwrap();
        assert_eq!(no_oxygen.chemical_heat_j, 0.0);
        let mut reacting = AfterTreatment::new(Catalyst::Standard);
        let mut burned = 0.0;
        let mut out = 0.0;
        for i in 0..1000 {
            let s = reacting
                .step(
                    DT,
                    1200.0,
                    0.001,
                    if i == 0 { 1e-5 } else { 0.0 },
                    if i == 0 { 4e-5 } else { 0.0 },
                    1e-7,
                )
                .unwrap();
            burned += s.fuel_burned_kg;
            out += s.unburnt_fuel_out_kg;
            assert!(
                (s.chemical_heat_j - s.gas_heat_j - s.wall_energy_change_j - s.ambient_heat_j)
                    .abs()
                    < 1e-12
            );
        }
        assert!(burned > 0.0 && out > 0.0);
        assert!((reacting.stored_fuel_kg() + burned + out - 1e-5).abs() < 1e-15);
    }
    #[test]
    fn closed_exchange_conserves_mass_and_enthalpy_between_volumes() {
        let mut m = setup();
        let mass = m.total_mass_kg();
        let energy = m.total_internal_energy_j();
        for _ in 0..1000 {
            let dm = m.outgoing_budgets().exhaust_kg[0] * 0.001;
            let mut f = Flows::default();
            f.exhaust[0].mass_out_kg = dm;
            f.intake.add_signed(
                dm,
                m.exhaust[0].mixture().enthalpy(m.exhaust().temperature_k),
            );
            let s = m.step_closed(DT, f).unwrap();
            assert!(s.intake_ledger.numerical_correction_j.abs() < 1e-9);
        }
        assert!((m.total_mass_kg() - mass).abs() < 1e-14);
        assert!((m.total_internal_energy_j() - energy).abs() < 1e-8);
        assert!(m.intake().temperature_k > 310.0);
    }
    #[test]
    fn runner_column_conserves_and_rams_past_equilibrium() {
        let mut m = setup();
        let (mass, energy) = (m.total_mass_kg(), m.total_internal_energy_j());
        let (mut drawn, mut enthalpy, mut peak) = (0.0, 0.0, 0.0_f64);
        for i in 0..4000 {
            let mut f = Flows::default();
            // A 2 ms intake event on cylinder one, then the valve shuts.
            if i < 192 {
                let dm = m.runner_budget_kg(0) * 0.02;
                f.runner[0].mass_out_kg = dm;
                drawn += dm;
                enthalpy += dm * m.runners[0].mixture().enthalpy(m.runner(0).temperature_k);
            }
            let s = m.step_closed(DT, f).unwrap();
            assert!(s.runner_correction_j.abs() < 1e-9);
            if i >= 192 {
                peak = peak.max(m.runner(0).pressure_pa - m.intake().pressure_pa);
            }
        }
        assert!((m.total_mass_kg() - mass + drawn).abs() < 1e-14);
        assert!((m.total_internal_energy_j() - energy + enthalpy).abs() < 1e-8);
        // The moving column piles up against the shut valve: ram overshoot.
        assert!(peak > 500.0, "{peak} Pa");
    }
    #[test]
    fn cylinder_pumping_makes_vacuum_and_open_throttle_restores_pressure() {
        let simulate = |throttle| {
            let mut m = setup();
            for _ in 0..48000 {
                let p = m.intake();
                let dm = (p.pressure_pa / (287.0 * p.temperature_k) * 0.002 * 1500.0 / 120.0 * DT)
                    .min(m.outgoing_budgets().intake_kg);
                m.step(
                    DT,
                    throttle,
                    0.0,
                    Flows {
                        intake: Exchange {
                            mass_out_kg: dm,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            m.intake().pressure_pa
        };
        let closed = simulate(0.0);
        let open = simulate(1.0);
        assert!(closed < 40000.0, "{closed}");
        assert!(open > 95000.0, "{open}");
    }
    #[test]
    fn narrow_restrictive_exhaust_raises_backpressure() {
        let simulate = |diameter| {
            let build = EngineBuild {
                exhaust_mm: diameter,
                ..Default::default()
            };
            let tuning = EngineTuning::default().resolve(&build, 4);
            let mut m = Manifolds::new(&build, &tuning, 4, 0.002).unwrap();
            for _ in 0..20000 {
                let mut f = Flows::default();
                f.exhaust[0].add_signed(0.05 * DT, Mixture::PRODUCTS.enthalpy(800.0));
                m.step(DT, 0.2, 0.0, f).unwrap();
            }
            m.exhaust().pressure_pa
        };
        assert!(simulate(35.0) > simulate(100.0) + 1000.0);
    }
    #[test]
    fn oversized_donor_demand_rejects_atomically() {
        let mut m = setup();
        let before = m.total_mass_kg();
        let f = Flows {
            intake: Exchange {
                mass_out_kg: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(m.step(DT, 0.0, 0.0, f).is_err());
        assert_eq!(m.total_mass_kg(), before);
    }
    #[test]
    fn open_boundary_ledger_closes() {
        let mut m = setup();
        let before_m = m.total_mass_kg();
        let before_e = m.total_internal_energy_j();
        let mut f = Flows::default();
        f.exhaust[0].add_signed(1e-6, Mixture::PRODUCTS.enthalpy(1800.0));
        let s = m.step(DT, 0.5, 0.0, f).unwrap();
        let correction = s.intake_ledger.numerical_correction_j
            + s.exhaust_ledger
                .iter()
                .map(|l| l.numerical_correction_j)
                .sum::<f64>();
        assert!(
            (m.total_mass_kg() - before_m - 1e-6 - s.external_mass_in_kg + s.external_mass_out_kg)
                .abs()
                < 1e-15
        );
        assert!(
            (m.total_internal_energy_j()
                - before_e
                - f.exhaust[0].enthalpy_in_j
                - s.external_enthalpy_in_j
                + s.external_enthalpy_out_j
                - correction)
                .abs()
                < 1e-10
        );
    }
}
