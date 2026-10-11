//! Offline conservative branched exhaust: distributed Euler primaries/tail,
//! a finite compliant collector and transfers supplied by physical cylinders.
//!
//! This is a qualification pipeline, not the production Engine quality switch.
//! Exhaust1d preserves the fixed-gamma, whole-pipe mixed reference.
//! SpeciesExhaust1d transports frozen species per cell and optionally uses
//! the cylinder's variable caloric EOS. Wall losses/catalyst/turbo remain open.
use super::{
    cylinder::Reservoir,
    finite_volume::{
        Boundary, EulerPipe, GasEquation, Primitive, SpeciesBoundary, SpeciesConserved,
        SpeciesPipe, SpeciesState,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct ReferenceGas {
    pub gamma: f64,
    pub gas_constant: f64,
    pub pressure_pa: f64,
    pub temperature_k: f64,
    pub fresh_air_fraction: f64,
    pub fuel_fraction: f64,
}

impl Default for ReferenceGas {
    fn default() -> Self {
        Self {
            gamma: 1.33,
            gas_constant: 287.,
            pressure_pa: 101325.,
            temperature_k: 673.,
            fresh_air_fraction: 0.,
            fuel_fraction: 0.,
        }
    }
}

impl ReferenceGas {
    fn primitive(self) -> Result<Primitive, &'static str> {
        if !(1.01..=1.67).contains(&self.gamma)
            || !(100.0..=600.).contains(&self.gas_constant)
            || !(200.0..=3500.).contains(&self.temperature_k)
            || !(100.0..=1e7).contains(&self.pressure_pa)
            || !(0.0..=1.).contains(&self.fresh_air_fraction)
            || !(0.0..=1.).contains(&self.fuel_fraction)
            || self.fresh_air_fraction + self.fuel_fraction > 1.
        {
            return Err("Invalid common 1D gas reference");
        }
        Ok(Primitive {
            density: self.pressure_pa / (self.gas_constant * self.temperature_k),
            velocity: 0.,
            pressure: self.pressure_pa,
        })
    }
}

/// Signed transfer INTO the primary. Use the same values removed from the
/// cylinder; energy is its signed enthalpy transfer, not a second flow estimate.
#[derive(Clone, Copy, Debug, Default)]
pub struct Transfer {
    pub mass_kg: f64,
    pub energy_j: f64,
    pub fresh_air_kg: f64,
    pub fuel_kg: f64,
}

impl Transfer {
    fn add(&mut self, other: Self) {
        self.mass_kg += other.mass_kg;
        self.energy_j += other.energy_j;
        self.fresh_air_kg += other.fresh_air_kg;
        self.fuel_kg += other.fuel_kg;
    }
    fn scale(self, scale: f64) -> Self {
        Self {
            mass_kg: self.mass_kg * scale,
            energy_j: self.energy_j * scale,
            fresh_air_kg: self.fresh_air_kg * scale,
            fuel_kg: self.fuel_kg * scale,
        }
    }
    pub fn residual(self, initial: Self, source: Self, outlet: Self) -> Self {
        let mut result = self;
        result.add(initial.scale(-1.));
        result.add(source.scale(-1.));
        result.add(outlet);
        result
    }
    fn finite(self) -> bool {
        [self.mass_kg, self.energy_j, self.fresh_air_kg, self.fuel_kg]
            .iter()
            .all(|v| v.is_finite())
    }
}

/// Mean gas state and donor composition at the cylinder port. The reference
/// uses the whole-pipe composition; SpeciesExhaust1d uses the first cell.
#[derive(Clone, Copy, Debug)]
pub struct PipePort {
    pub reservoir: Reservoir,
    pub fresh_air_fraction: f64,
    pub fuel_fraction: f64,
    pub gamma: f64,
    pub sound_speed_m_s: f64,
    /// Conservative bound from the first cell and pipe species inventory.
    pub incoming_budget_kg: f64,
}

struct Branch {
    pipe: EulerPipe,
    area: f64,
    fresh: f64,
    fuel: f64,
}

impl Branch {
    fn new(length: f64, area: f64, spacing: f64, gas: ReferenceGas) -> Result<Self, &'static str> {
        let cells = (length / spacing).ceil().max(8.) as usize;
        let pipe = EulerPipe::new(length, area, cells, gas.gamma, gas.primitive()?)?;
        let mass = pipe.totals()[0];
        Ok(Self {
            pipe,
            area,
            fresh: mass * gas.fresh_air_fraction,
            fuel: mass * gas.fuel_fraction,
        })
    }
    fn end(&self, right: bool) -> Primitive {
        self.pipe
            .primitive(if right { self.pipe.cells() - 1 } else { 0 })
            .expect("nonempty branch")
    }
    fn fractions(&self) -> [f64; 2] {
        let mass = self.pipe.totals()[0];
        [self.fresh / mass, self.fuel / mass]
    }
    fn inventory(&self) -> Transfer {
        let totals = self.pipe.totals();
        Transfer {
            mass_kg: totals[0],
            energy_j: totals[2],
            fresh_air_kg: self.fresh,
            fuel_kg: self.fuel,
        }
    }
    fn add_species(&mut self, transfer: Transfer) -> Result<(), &'static str> {
        self.fresh += transfer.fresh_air_kg;
        self.fuel += transfer.fuel_kg;
        let mass = self.pipe.totals()[0];
        if self.fresh < -1e-14 || self.fuel < -1e-14 || self.fresh + self.fuel > mass + 1e-12 {
            return Err("1D species inventory left its conservative domain");
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Geometry {
    pub primary_lengths_m: Vec<f64>,
    pub primary_area_m2: f64,
    pub collector_volume_m3: f64,
    pub tail_length_m: f64,
    pub tail_area_m2: f64,
    /// 4 mm is the qualification reference; coarser meshes are diagnostic.
    pub cell_size_m: f64,
}

/// One junction owns its inventory; every interface flux is computed once and
/// installed at the adjoining pipe face with exactly the opposite transfer.
pub struct Exhaust1d {
    primaries: Vec<Branch>,
    tail: Branch,
    collector: Transfer,
    collector_volume: f64,
    gas: ReferenceGas,
    inlet_total: Transfer,
    outlet_total: Transfer,
    junction_flux: Vec<[f64; 3]>,
    failed: bool,
}

impl Exhaust1d {
    pub fn new(geometry: &Geometry, gas: ReferenceGas) -> Result<Self, &'static str> {
        let reference = gas.primitive()?;
        if geometry.primary_lengths_m.is_empty()
            || geometry.primary_lengths_m.len() > 12
            || !(0.0001..=0.05).contains(&geometry.collector_volume_m3)
            || !(0.002..=0.04).contains(&geometry.cell_size_m)
        {
            return Err("Invalid offline 1D network geometry");
        }
        let primaries = geometry
            .primary_lengths_m
            .iter()
            .map(|&length| Branch::new(length, geometry.primary_area_m2, geometry.cell_size_m, gas))
            .collect::<Result<Vec<_>, _>>()?;
        let mass = reference.density * geometry.collector_volume_m3;
        Ok(Self {
            junction_flux: vec![[0.; 3]; primaries.len()],
            primaries,
            tail: Branch::new(
                geometry.tail_length_m,
                geometry.tail_area_m2,
                geometry.cell_size_m,
                gas,
            )?,
            collector: Transfer {
                mass_kg: mass,
                energy_j: gas.pressure_pa * geometry.collector_volume_m3 / (gas.gamma - 1.),
                fresh_air_kg: mass * gas.fresh_air_fraction,
                fuel_kg: mass * gas.fuel_fraction,
            },
            collector_volume: geometry.collector_volume_m3,
            gas,
            inlet_total: Transfer::default(),
            outlet_total: Transfer::default(),
            failed: false,
        })
    }
    pub fn stable_time_step(&self) -> f64 {
        self.primaries
            .iter()
            .map(|p| p.pipe.stable_time_step())
            .fold(self.tail.pipe.stable_time_step(), f64::min)
    }
    pub fn port(&self, cylinder: usize) -> PipePort {
        let branch = &self.primaries[cylinder];
        let q = branch.end(false);
        let [fresh_air_fraction, fuel_fraction] = branch.fractions();
        PipePort {
            reservoir: Reservoir {
                pressure_pa: q.pressure,
                temperature_k: q.pressure / (q.density * self.gas.gas_constant),
            },
            fresh_air_fraction,
            fuel_fraction,
            gamma: self.gas.gamma,
            sound_speed_m_s: q.sound_speed(self.gas.gamma),
            incoming_budget_kg: 0.2 * q.density * branch.area * branch.pipe.dx(),
        }
    }
    pub fn inventory(&self) -> Transfer {
        let mut total = self.collector;
        for p in &self.primaries {
            total.add(p.inventory());
        }
        total.add(self.tail.inventory());
        total
    }
    pub fn inlet_total(&self) -> Transfer {
        self.inlet_total
    }
    pub fn outlet_total(&self) -> Transfer {
        self.outlet_total
    }
    pub fn collector_pressure_pa(&self) -> f64 {
        (self.gas.gamma - 1.) * self.collector.energy_j / self.collector_volume
    }
    pub fn tail_pressure_pa(&self) -> f64 {
        self.tail.end(true).pressure
    }
    pub fn cells(&self) -> usize {
        self.primaries.iter().map(|p| p.pipe.cells()).sum::<usize>() + self.tail.pipe.cells()
    }
    /// Advance at a globally CFL-bounded step. A numerical/domain failure
    /// stops the network permanently, since partial branches may have advanced.
    pub fn advance(
        &mut self,
        dt: f64,
        sources: &[Transfer],
        ambient: Reservoir,
    ) -> Result<(), &'static str> {
        if self.failed {
            return Err("Offline 1D network has stopped");
        }
        let result = self.advance_checked(dt, sources, ambient);
        self.failed = result.is_err();
        result
    }
    fn advance_checked(
        &mut self,
        dt: f64,
        sources: &[Transfer],
        ambient: Reservoir,
    ) -> Result<(), &'static str> {
        if !(1e-9..=1. / 16000.).contains(&dt)
            || dt > self.stable_time_step() * (1. + 1e-10)
            || sources.len() != self.primaries.len()
            || sources.iter().any(|s| !s.finite())
            || !(200.0..=3500.).contains(&ambient.temperature_k)
            || !(100.0..=1e7).contains(&ambient.pressure_pa)
        {
            return Err("Invalid offline 1D network step, source or ambient");
        }
        let collector = Primitive {
            density: self.collector.mass_kg / self.collector_volume,
            pressure: self.collector_pressure_pa(),
            velocity: 0.,
        };
        let collector_fractions = [
            self.collector.fresh_air_kg / self.collector.mass_kg,
            self.collector.fuel_kg / self.collector.mass_kg,
        ];
        for (flux, primary) in self.junction_flux.iter_mut().zip(&self.primaries) {
            *flux = EulerPipe::interface_flux(primary.end(true), collector, self.gas.gamma)?;
        }
        let tail_flux = EulerPipe::interface_flux(collector, self.tail.end(false), self.gas.gamma)?;
        let ambient_q = Primitive {
            pressure: ambient.pressure_pa,
            velocity: 0.,
            density: ambient.pressure_pa / (self.gas.gas_constant * ambient.temperature_k),
        };
        let outlet_flux =
            EulerPipe::interface_flux(self.tail.end(true), ambient_q, self.gas.gamma)?;
        let mut collector_delta = Transfer::default();
        for ((primary, source), flux) in self
            .primaries
            .iter_mut()
            .zip(sources)
            .zip(&self.junction_flux)
        {
            let fractions = if flux[0] >= 0. {
                primary.fractions()
            } else {
                collector_fractions
            };
            let face = primary.end(false);
            let inlet = [
                source.mass_kg / (dt * primary.area),
                face.pressure,
                source.energy_j / (dt * primary.area),
            ];
            let before = primary.pipe.diagnostics().boundary_by_side;
            primary
                .pipe
                .advance(dt, [Boundary::Flux(inlet), Boundary::Flux(*flux)])?;
            let after = primary.pipe.diagnostics().boundary_by_side;
            // Use the solver's accepted SSP-integrated face transfer, including
            // adaptive recovery, instead of assuming a successful nominal dt.
            let into_collector = Transfer {
                mass_kg: -(after[1][0] - before[1][0]),
                energy_j: -(after[1][2] - before[1][2]),
                fresh_air_kg: -(after[1][0] - before[1][0]) * fractions[0],
                fuel_kg: -(after[1][0] - before[1][0]) * fractions[1],
            };
            collector_delta.add(into_collector);
            let mut species = *source;
            species.add(into_collector.scale(-1.));
            primary.add_species(species)?;
            self.inlet_total.add(*source);
        }
        let tail_fractions = self.tail.fractions();
        let inflow_fractions = if tail_flux[0] >= 0. {
            collector_fractions
        } else {
            tail_fractions
        };
        // Incoming atmospheric gas carries fresh air; outgoing gas carries
        // the tail's current conservative well-mixed inventory.
        let outflow_fractions = if outlet_flux[0] >= 0. {
            tail_fractions
        } else {
            [1., 0.]
        };
        let before = self.tail.pipe.diagnostics().boundary_by_side;
        self.tail
            .pipe
            .advance(dt, [Boundary::Flux(tail_flux), Boundary::Flux(outlet_flux)])?;
        let after = self.tail.pipe.diagnostics().boundary_by_side;
        let into_tail = Transfer {
            mass_kg: after[0][0] - before[0][0],
            energy_j: after[0][2] - before[0][2],
            fresh_air_kg: (after[0][0] - before[0][0]) * inflow_fractions[0],
            fuel_kg: (after[0][0] - before[0][0]) * inflow_fractions[1],
        };
        let out = Transfer {
            mass_kg: -(after[1][0] - before[1][0]),
            energy_j: -(after[1][2] - before[1][2]),
            fresh_air_kg: -(after[1][0] - before[1][0]) * outflow_fractions[0],
            fuel_kg: -(after[1][0] - before[1][0]) * outflow_fractions[1],
        };
        let mut species = into_tail;
        species.add(out.scale(-1.));
        self.tail.add_species(species)?;
        self.outlet_total.add(out);
        collector_delta.add(into_tail.scale(-1.));
        self.collector.add(collector_delta);
        if self.collector.mass_kg <= 0.
            || self.collector.energy_j <= 0.
            || self.collector.fresh_air_kg < -1e-14
            || self.collector.fuel_kg < -1e-14
            || self.collector.fresh_air_kg + self.collector.fuel_kg > self.collector.mass_kg + 1e-12
        {
            return Err("Collector left its conservative domain");
        }
        Ok(())
    }
}

struct SpeciesBranch {
    pipe: SpeciesPipe,
    area: f64,
}

impl SpeciesBranch {
    fn new(
        length: f64,
        area: f64,
        spacing: f64,
        eos: GasEquation,
        q: SpeciesState,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            pipe: SpeciesPipe::new(
                length,
                area,
                (length / spacing).ceil().max(8.) as usize,
                eos,
                q,
            )?,
            area,
        })
    }
    fn end(&self, right: bool) -> SpeciesState {
        self.pipe
            .state(if right { self.pipe.cells() - 1 } else { 0 })
            .expect("accepted branch")
    }
    fn inventory(&self) -> Transfer {
        transfer(self.pipe.totals())
    }
}

fn transfer(u: SpeciesConserved) -> Transfer {
    Transfer {
        mass_kg: u[0],
        energy_j: u[2],
        fresh_air_kg: u[3],
        fuel_kg: u[4],
    }
}

/// Offline distributed species/EOS bench. Only the finite collector is well
/// mixed. Five shared face fluxes close mass/total energy/air/fuel inventories.
/// Spatial order one is explicit; this is not the production quality switch.
pub struct SpeciesExhaust1d {
    primaries: Vec<SpeciesBranch>,
    tail: SpeciesBranch,
    collector: Transfer,
    collector_volume: f64,
    eos: GasEquation,
    inlet_total: Transfer,
    outlet_total: Transfer,
    junction_flux: Vec<SpeciesConserved>,
    failed: bool,
}

impl SpeciesExhaust1d {
    pub fn new(
        geometry: &Geometry,
        gas: ReferenceGas,
        eos: GasEquation,
    ) -> Result<Self, &'static str> {
        gas.primitive()?;
        if geometry.primary_lengths_m.is_empty()
            || geometry.primary_lengths_m.len() > 12
            || !(0.0001..=0.05).contains(&geometry.collector_volume_m3)
            || !(0.002..=0.04).contains(&geometry.cell_size_m)
        {
            return Err("Invalid distributed network geometry");
        }
        let q = eos.at_pressure(
            gas.pressure_pa,
            gas.temperature_k,
            gas.fresh_air_fraction,
            gas.fuel_fraction,
        )?;
        let initial = eos.conserved(q)?;
        let primaries = geometry
            .primary_lengths_m
            .iter()
            .map(|&length| {
                SpeciesBranch::new(
                    length,
                    geometry.primary_area_m2,
                    geometry.cell_size_m,
                    eos,
                    q,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            junction_flux: vec![[0.; 5]; primaries.len()],
            primaries,
            tail: SpeciesBranch::new(
                geometry.tail_length_m,
                geometry.tail_area_m2,
                geometry.cell_size_m,
                eos,
                q,
            )?,
            collector: transfer(initial.map(|v| v * geometry.collector_volume_m3)),
            collector_volume: geometry.collector_volume_m3,
            eos,
            inlet_total: Transfer::default(),
            outlet_total: Transfer::default(),
            failed: false,
        })
    }
    fn collector_state(&self) -> Result<SpeciesState, &'static str> {
        self.eos.state([
            self.collector.mass_kg / self.collector_volume,
            0.,
            self.collector.energy_j / self.collector_volume,
            self.collector.fresh_air_kg / self.collector_volume,
            self.collector.fuel_kg / self.collector_volume,
        ])
    }
    pub fn stable_time_step(&self) -> f64 {
        self.primaries
            .iter()
            .map(|p| p.pipe.stable_time_step())
            .fold(self.tail.pipe.stable_time_step(), f64::min)
    }
    pub fn port(&self, cylinder: usize) -> PipePort {
        let branch = &self.primaries[cylinder];
        let q = branch.end(false);
        PipePort {
            reservoir: Reservoir {
                pressure_pa: q.gas.pressure,
                temperature_k: self.eos.temperature(q),
            },
            fresh_air_fraction: q.fresh_air_fraction,
            fuel_fraction: q.fuel_fraction,
            gamma: self.eos.gamma(q),
            sound_speed_m_s: self.eos.sound_speed(q),
            incoming_budget_kg: 0.2 * q.gas.density * branch.area * branch.pipe.dx(),
        }
    }
    pub fn inventory(&self) -> Transfer {
        let mut total = self.collector;
        for p in &self.primaries {
            total.add(p.inventory());
        }
        total.add(self.tail.inventory());
        total
    }
    pub fn inlet_total(&self) -> Transfer {
        self.inlet_total
    }
    pub fn outlet_total(&self) -> Transfer {
        self.outlet_total
    }
    pub fn collector_pressure_pa(&self) -> f64 {
        self.collector_state()
            .expect("accepted collector")
            .gas
            .pressure
    }
    pub fn tail_pressure_pa(&self) -> f64 {
        self.tail.end(true).gas.pressure
    }
    pub fn cells(&self) -> usize {
        self.primaries.iter().map(|p| p.pipe.cells()).sum::<usize>() + self.tail.pipe.cells()
    }
    pub fn rejected_trials(&self) -> u64 {
        self.primaries
            .iter()
            .map(|p| p.pipe.rejected_trials())
            .sum::<u64>()
            + self.tail.pipe.rejected_trials()
    }
    pub fn advance(
        &mut self,
        dt: f64,
        sources: &[Transfer],
        ambient: Reservoir,
    ) -> Result<(), &'static str> {
        if self.failed {
            return Err("Distributed 1D network has stopped");
        }
        let result = self.advance_checked(dt, sources, ambient);
        self.failed = result.is_err();
        result
    }
    fn advance_checked(
        &mut self,
        dt: f64,
        sources: &[Transfer],
        ambient: Reservoir,
    ) -> Result<(), &'static str> {
        if !(1e-9..=1. / 16000.).contains(&dt)
            || dt > self.stable_time_step() * (1. + 1e-10)
            || sources.len() != self.primaries.len()
            || sources.iter().any(|s| !s.finite())
        {
            return Err("Invalid distributed network step/source");
        }
        let collector = self.collector_state()?;
        // Reject invalid external species transfers before advancing any tube.
        // Signed reverse flow has the same donor-fraction bounds as inflow.
        for source in sources {
            if source.mass_kg == 0. {
                if source.fresh_air_kg != 0. || source.fuel_kg != 0. || source.energy_j != 0. {
                    return Err("Species/enthalpy supplied without mass transfer");
                }
            } else {
                let fresh = source.fresh_air_kg / source.mass_kg;
                let fuel = source.fuel_kg / source.mass_kg;
                if fresh < 0.
                    || fuel < 0.
                    || fresh + fuel > 1. + 8. * f64::EPSILON
                    || source.mass_kg * source.energy_j < 0.
                {
                    return Err("Invalid conservative donor species/enthalpy transfer");
                }
            }
        }
        let atmosphere =
            self.eos
                .at_pressure(ambient.pressure_pa, ambient.temperature_k, 1., 0.)?;
        for (f, p) in self.junction_flux.iter_mut().zip(&self.primaries) {
            *f = SpeciesPipe::interface_flux(self.eos, p.end(true), collector)?;
        }
        let tail_flux = SpeciesPipe::interface_flux(self.eos, collector, self.tail.end(false))?;
        let outlet_flux = SpeciesPipe::interface_flux(self.eos, self.tail.end(true), atmosphere)?;
        let mut delta = Transfer::default();
        for ((primary, source), flux) in self
            .primaries
            .iter_mut()
            .zip(sources)
            .zip(&self.junction_flux)
        {
            let scale = dt * primary.area;
            let inlet = [
                source.mass_kg / scale,
                primary.end(false).gas.pressure,
                source.energy_j / scale,
                source.fresh_air_kg / scale,
                source.fuel_kg / scale,
            ];
            let before = primary.pipe.boundary_by_side();
            primary.pipe.advance(
                dt,
                [SpeciesBoundary::Flux(inlet), SpeciesBoundary::Flux(*flux)],
            )?;
            let after = primary.pipe.boundary_by_side();
            let into_collector = transfer(std::array::from_fn(|k| -(after[1][k] - before[1][k])));
            delta.add(into_collector);
            self.inlet_total.add(*source);
        }
        let before = self.tail.pipe.boundary_by_side();
        self.tail.pipe.advance(
            dt,
            [
                SpeciesBoundary::Flux(tail_flux),
                SpeciesBoundary::Flux(outlet_flux),
            ],
        )?;
        let after = self.tail.pipe.boundary_by_side();
        let into_tail = transfer(std::array::from_fn(|k| after[0][k] - before[0][k]));
        let out = transfer(std::array::from_fn(|k| -(after[1][k] - before[1][k])));
        delta.add(into_tail.scale(-1.));
        self.outlet_total.add(out);
        self.collector.add(delta);
        self.collector_state()?;
        Ok(())
    }
}

#[cfg(test)]
mod species_network_tests {
    use super::*;
    use crate::physical::thermo::Mixture;

    fn network(spacing: f64) -> SpeciesExhaust1d {
        SpeciesExhaust1d::new(
            &Geometry {
                primary_lengths_m: vec![0.2, 0.24],
                primary_area_m2: 0.001,
                collector_volume_m3: 0.003,
                tail_length_m: 0.3,
                tail_area_m2: 0.002,
                cell_size_m: spacing,
            },
            ReferenceGas::default(),
            GasEquation::SharedMixture,
        )
        .unwrap()
    }

    #[test]
    fn distributed_uniform_network_keeps_pressure_and_independent_inventories() {
        let mut n = network(0.004);
        let initial = n.inventory();
        for _ in 0..300 {
            n.advance(
                n.stable_time_step(),
                &[Transfer::default(); 2],
                Reservoir {
                    pressure_pa: 101325.,
                    temperature_k: 673.,
                },
            )
            .unwrap();
        }
        let r = n
            .inventory()
            .residual(initial, n.inlet_total(), n.outlet_total());
        assert!(r.mass_kg.abs() < 1e-14 && r.energy_j.abs() < 1e-8);
        assert!(r.fresh_air_kg.abs() < 1e-14 && r.fuel_kg.abs() < 1e-14);
        assert!((n.collector_pressure_pa() - 101325.).abs() < 1e-6);
    }

    fn pulse(spacing: f64) -> f64 {
        let mut n = network(spacing);
        let initial = n.inventory();
        let mut time = 0.;
        let mut integral = 0.;
        let mut maximum_energy = 0_f64;
        while time < 0.012 {
            let dt = n.stable_time_step().min(0.012 - time);
            if dt < 1e-9 {
                break;
            }
            let flow = if time < 0.002 {
                0.003 * (std::f64::consts::PI * time / 0.002).sin().powi(2)
            } else {
                0.
            };
            let mass = flow * dt;
            let source = Transfer {
                mass_kg: mass,
                energy_j: mass * Mixture::from_fractions(0.15, 0.02).enthalpy(673.),
                fresh_air_kg: 0.15 * mass,
                fuel_kg: 0.02 * mass,
            };
            n.advance(
                dt,
                &[source, Transfer::default()],
                Reservoir {
                    pressure_pa: 101325.,
                    temperature_k: 673.,
                },
            )
            .unwrap();
            time += dt;
            integral += (n.tail_pressure_pa() - 101325.) * dt;
            let r = n
                .inventory()
                .residual(initial, n.inlet_total(), n.outlet_total());
            maximum_energy = maximum_energy.max(r.energy_j.abs());
            assert!(r.mass_kg.abs() < 1e-13);
            assert!(r.fresh_air_kg.abs() < 1e-13 && r.fuel_kg.abs() < 1e-13);
        }
        assert!(maximum_energy < 1e-7);
        eprintln!(
            "distributed pulse grid {spacing}: tail integral {integral:.8e}, maximum energy residual {maximum_energy:.8e}"
        );
        integral
    }

    #[test]
    fn distributed_pulse_closes_species_energy_and_refines() {
        let coarse = pulse(0.016);
        let medium = pulse(0.008);
        let fine = pulse(0.004);
        assert!((medium - fine).abs() < (coarse - fine).abs());
    }

    #[test]
    fn injected_species_front_does_not_instantly_reach_collector() {
        let mut n = network(0.004);
        let dt = n.stable_time_step();
        let mass = 0.003 * dt;
        n.advance(
            dt,
            &[
                Transfer {
                    mass_kg: mass,
                    energy_j: mass * Mixture::AIR.enthalpy(673.),
                    fresh_air_kg: mass,
                    fuel_kg: 0.,
                },
                Transfer::default(),
            ],
            Reservoir {
                pressure_pa: 101325.,
                temperature_k: 673.,
            },
        )
        .unwrap();
        assert!(n.port(0).fresh_air_fraction > 0.);
        assert_eq!(n.primaries[0].end(true).fresh_air_fraction, 0.);
        assert_eq!(n.collector.fresh_air_kg, 0.);
    }

    #[test]
    fn invalid_donor_fractions_are_rejected_before_modifying_network() {
        let mut n = network(0.004);
        let initial = n.inventory();
        let dt = n.stable_time_step();
        assert!(
            n.advance(
                dt,
                &[
                    Transfer {
                        mass_kg: 1e-8,
                        energy_j: 0.01,
                        fresh_air_kg: 8e-9,
                        fuel_kg: 3e-9
                    },
                    Transfer::default()
                ],
                Reservoir {
                    pressure_pa: 101325.,
                    temperature_k: 673.
                }
            )
            .is_err()
        );
        let residual = n
            .inventory()
            .residual(initial, Transfer::default(), Transfer::default());
        assert_eq!(residual.mass_kg, 0.);
        assert_eq!(residual.energy_j, 0.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn network(spacing: f64) -> Exhaust1d {
        Exhaust1d::new(
            &Geometry {
                primary_lengths_m: vec![0.2, 0.24],
                primary_area_m2: 0.001,
                collector_volume_m3: 0.003,
                tail_length_m: 0.3,
                tail_area_m2: 0.002,
                cell_size_m: spacing,
            },
            ReferenceGas::default(),
        )
        .unwrap()
    }
    #[test]
    fn uniform_closed_valves_create_no_pressure_or_species() {
        let mut n = network(0.004);
        let initial = n.inventory();
        for _ in 0..1000 {
            n.advance(
                n.stable_time_step(),
                &[Transfer::default(); 2],
                Reservoir {
                    pressure_pa: 101325.,
                    temperature_k: 673.,
                },
            )
            .unwrap();
        }
        let residual = n
            .inventory()
            .residual(initial, n.inlet_total(), n.outlet_total());
        assert!(residual.mass_kg.abs() < 1e-14 && residual.energy_j.abs() < 1e-8);
        assert!((n.collector_pressure_pa() - 101325.).abs() < 1e-7);
        assert!(residual.fresh_air_kg.abs() < 1e-14);
    }
    fn pulse(spacing: f64) -> (f64, f64) {
        let mut n = network(spacing);
        let initial = n.inventory();
        let mut time = 0.;
        let mut maximum = 0_f64;
        let mut integral = 0.;
        while time < 0.012 {
            let dt = n.stable_time_step().min(0.012 - time);
            if dt < 1e-9 {
                break;
            }
            let flow = if time < 0.002 {
                0.003 * (std::f64::consts::PI * time / 0.002).sin().powi(2)
            } else {
                0.
            };
            let mass = flow * dt;
            let source = Transfer {
                mass_kg: mass,
                energy_j: mass * 287. * 673. * 1.33 / 0.33,
                fresh_air_kg: mass * 0.15,
                fuel_kg: mass * 0.02,
            };
            n.advance(
                dt,
                &[source, Transfer::default()],
                Reservoir {
                    pressure_pa: 101325.,
                    temperature_k: 673.,
                },
            )
            .unwrap();
            time += dt;
            integral += (n.tail_pressure_pa() - 101325.) * dt;
            let residual = n
                .inventory()
                .residual(initial, n.inlet_total(), n.outlet_total());
            maximum = maximum.max(residual.energy_j.abs());
            assert!(residual.mass_kg.abs() < 1e-13);
            assert!(residual.fresh_air_kg.abs() < 1e-13 && residual.fuel_kg.abs() < 1e-13);
        }
        assert!(maximum < 1e-7, "energy residual {maximum}");
        (integral, maximum)
    }
    #[test]
    fn branched_pulse_closes_all_inventories_and_converges_with_refinement() {
        let coarse = pulse(0.016).0;
        let medium = pulse(0.008).0;
        let reference = pulse(0.004).0;
        assert!(
            (medium - reference).abs() < (coarse - reference).abs(),
            "integrals: {coarse}, {medium}, {reference}"
        );
    }
}
