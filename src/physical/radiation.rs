//! Observation of intake flow and mechanical contacts. Never feeds the solver.
//! Airbox damping removes sharp flow corners; turbulence requires real flow.
//! Finite contact bursts replace identical one-sample mechanical impulses.
//! Block motion and small head contacts use separate, damped structural paths.
use crate::engine_build::BlockMaterial;
use bdsp::svf::{StateVariableFilter, SvfMode};

// Four large block modes, then four smaller head/contact modes. The previous
// common 1–8 kHz bank made both sources sound like a ringing sheet of metal.
// These are perceptual transfer estimates, not measured modes of a given engine.
const RATIOS: [f32; 8] = [0.13, 0.23, 0.38, 0.62, 0.45, 0.75, 1.16, 1.8];
const LEVEL: f32 = 0.62;

/// Inline modal bank. Clone/retune never allocate.
#[derive(Clone)]
pub(crate) struct Modes {
    modes: [StateVariableFilter; 8],
    ratio: [f32; 8],
    q_scale: [f32; 8],
    head: [f32; 8],
    block: [f32; 8],
    gain: [f32; 8],
    limit_hz: f32,
    head_radiation: StateVariableFilter,
    block_radiation: StateVariableFilter,
}
impl Modes {
    pub fn new(
        rate: f32,
        pitch_hz: f32,
        resonance: f32,
        block: BlockMaterial,
        bore_mm: f32,
        seed: u64,
    ) -> Self {
        // Cast iron: slower waves, better damped, weaker low modes.
        let (stiffness, damping, low) = match block {
            BlockMaterial::CastIron => (0.9, 0.7, 0.8),
            BlockMaterial::Aluminium => (1., 1., 1.),
        };
        let size = (86. / bore_mm.max(1.)).powf(0.25);
        let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
        let mut jitter = || {
            // splitmix64 → ±6 % per mode, fixed per engine seed.
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            1. + 0.06 * (((z ^ (z >> 31)) >> 40) as f32 / 8388608. - 1.)
        };
        let mut modes = Self {
            modes: std::array::from_fn(|_| {
                StateVariableFilter::new(rate, 1000., 1., SvfMode::Bandpass)
            }),
            ratio: RATIOS.map(|r| r * jitter() * size * stiffness),
            // Broad, lossy responses at the neutral Q control. Pitch/Q remain
            // editable, but normal contacts no longer sustain narrow whistles.
            q_scale: [0.60, 0.58, 0.55, 0.50, 0.65, 0.60, 0.55, 0.50].map(|q| q * damping),
            head: [0., 0., 0., 0., 1., 0.65, 0.32, 0.12],
            block: [1., 0.85, 0.45, 0.16, 0., 0., 0., 0.].map(|w| w * low),
            gain: [LEVEL; 8],
            limit_hz: rate * 0.4,
            // Radiation/structural loss is frequency dependent. In particular
            // the differentiated pressure must not excite bright contact modes.
            head_radiation: StateVariableFilter::new(
                rate,
                (3600. * size * stiffness).min(rate * 0.35),
                0.707,
                SvfMode::Lowpass,
            ),
            block_radiation: StateVariableFilter::new(
                rate,
                (1800. * size * stiffness).min(rate * 0.35),
                0.707,
                SvfMode::Lowpass,
            ),
        };
        modes.retune(pitch_hz, resonance);
        modes
    }
    pub fn retune(&mut self, pitch_hz: f32, resonance: f32) {
        for i in 0..8 {
            let hz = pitch_hz * self.ratio[i];
            // Modes past 0.4 fs are muted, not piled up on the limit.
            self.gain[i] = if hz <= self.limit_hz { LEVEL } else { 0. };
            self.modes[i].set_cutoff(hz.min(self.limit_hz));
            self.modes[i].set_q(resonance * self.q_scale[i]);
        }
    }
    pub fn next(&mut self, head: f32, block: f32) -> f32 {
        let head = self.head_radiation.next_sample(head);
        let block = self.block_radiation.next_sample(block);
        let mut out = 0.;
        for i in 0..8 {
            out += self.modes[i].next_sample(head * self.head[i] + block * self.block[i])
                * self.gain[i];
        }
        out
    }
}

/// Throttle/compressor gas state for the flow-noise observers, one per frame.
#[derive(Clone, Copy, Default)]
pub(crate) struct Air {
    /// Signed throttle flow, supply → manifold positive (kg/s).
    pub throttle_kg_s: f32,
    /// Butterfly + bypass + leak aperture, and the full bore it sits in (m²).
    pub gap_m2: f32,
    pub bore_m2: f32,
    /// Upstream of the throttle: atmosphere, or the turbo charge volume.
    pub supply_pa: f32,
    pub supply_k: f32,
    pub manifold_pa: f32,
    pub manifold_k: f32,
    /// Zero without a turbo.
    pub shaft_rpm: f32,
    pub compressor_kg_s: f32,
}

impl Air {
    /// Isentropic throttle jet speed from the pressure ratio, sonic when
    /// choked; zero at ratio 1. Either flow direction.
    pub fn jet_m_s(&self) -> f32 {
        let (high, low, temperature) = if self.supply_pa >= self.manifold_pa {
            (self.supply_pa, self.manifold_pa, self.supply_k)
        } else {
            (self.manifold_pa, self.supply_pa, self.manifold_k)
        };
        let ratio = (low / high).clamp(0., 1.);
        if ratio > CHOKE {
            2. * CP_AIR * temperature * (1. - ratio.powf(0.2857))
        } else {
            2.8 / 2.4 * R_AIR * temperature
        }
        .sqrt()
    }
    fn pressure_ratio(&self) -> f32 {
        let (a, b) = (self.supply_pa, self.manifold_pa);
        (a.min(b) / a.max(b)).clamp(0., 1.)
    }
}

const R_AIR: f32 = 287.;
const CP_AIR: f32 = 1005.;
// Critical pressure ratio for γ = 1.4; below it the throttle jet is sonic.
const CHOKE: f32 = 0.528;
// Shedding (Strouhal 0.21) off the throttle shaft/plate edge, ~15 % of the
// bore. Taking the bore itself would put the peak at 5–200 Hz, under the
// firing orders, for every operating point.
const SHEDDING: f32 = 0.21 / 0.15;
// Jet-thickness Strouhal. A sub-millimetre gap peaks ultrasonically; only its
// rising low tail passes the airbox, so the audible maximum sits where that
// tail meets the airbox roll-off: the band centre is clamped at AIRBOX_HZ and
// the power scaled by the audible fraction. (The former 8 kHz clamp kept the
// ultrasonic power and put 26 % of the idle intake in 4-8 kHz; X-009.)
const JET_ST: f32 = 0.2;
// Airbox + filter element as one 2nd-order low-pass. A ~8 L box (~0.2 m) is a
// lumped compliance up to ~c/2L = 0.9 kHz; box modes and the paper element
// leak and absorb above it, taken as a roll-off one octave higher. Estimate.
const AIRBOX_HZ: f32 = 2000.;
// Edge tone of the underexpanded jet on the plate, about a quarter bore away.
const EDGE_ST: f32 = 0.1 / 0.25;
// The inducer sees the 6 full blades of a 6+6 splitter wheel; the second
// order is the splitter pass. 25 mm exducer radius. Estimates, not a map.
const BLADES: f32 = 6.;
const TIP_RADIUS_M: f32 = 0.025;
// Fixed calibrations, referenced to 10 g/s, 20 m/s, 340 m/s, 300 m/s tip and
// a 1 kHz band, set jointly with the tonal PA_TO_SAMPLE of `IntakeAcoustic`
// against the pre-2026-09-29 fixed layer (default I4, `final_proof`; X-023).
// DUCT: re-set −4 dB after the X-024 intake runners raised WOT flow; NA WOT /
// high rpm RMS +2.4 / +3.1 dB over that layer, peaks +5.6 / +5.7 (cruise −7).
// HISS: a fully audible jet (peak under AIRBOX_HZ) of 10 g/s at 340 m/s; the
// 0.2 mm idle gap keeps ~5e-4 of that amplitude, a 1.5 mm cruise gap ~4 %:
// ~5 dB under duct and tone at 2000 / 0.3, about half of a quiet idle.
// Edge tone and whine unchanged (whine -13 dB re turbo intake at 4500 / 1.0).
const FLOW_REF: f32 = 0.01;
const DUCT: f32 = 0.0126;
const HISS: f32 = 2.0;
const WHISTLE: f32 = 0.015;
const WHINE: f32 = 9.2e-5;

pub(crate) struct Radiation {
    // Duct shedding, throttle jet, edge tone, compressor haystack.
    bands: [StateVariableFilter; 4],
    centre: [f32; 4],
    airbox: StateVariableFilter,
    gain: [f32; 4],
    target: [f32; 4],
    air_noise_gain: f32,
    air_noise_target: f32,
    air_noise_step: f32,
    whine_phase: f32,
    whine_hz: f32,
    splitter: f32,
    rate: f32,
    control: u32,
    countdown: u32,
    slew: f32,
    contact_envelope: f32,
    flow_step: f32,
    contact_decay: f32,
    noise_scale: f32,
    seed: u64,
}
impl Radiation {
    pub fn new(rate: u32, seed: u64) -> Self {
        let rate = rate as f32;
        // Control rate of at least 6 kHz: 16 samples at 96 kHz, 1 at 8 kHz.
        let control = (rate / 6000.).max(1.) as u32;
        Self {
            bands: [0.7, 0.7, 12., 4.]
                .map(|q| StateVariableFilter::new(rate, 1000., q, SvfMode::Bandpass)),
            centre: [1000.; 4],
            airbox: StateVariableFilter::new(
                rate,
                AIRBOX_HZ.min(rate * 0.35),
                0.707,
                SvfMode::Lowpass,
            ),
            gain: [0.; 4],
            target: [0.; 4],
            // One is the historical source balance. Engine construction applies
            // its chosen sound setting immediately, before any sample is read.
            air_noise_gain: 1.,
            air_noise_target: 1.,
            air_noise_step: 1. - (-1. / (rate * 0.020)).exp(),
            whine_phase: 0.,
            whine_hz: 0.,
            splitter: 0.,
            rate,
            control,
            countdown: 0,
            // 10 ms cutoff glide: no zipper from control-rate steps.
            slew: 1. - (-(control as f32) / (rate * 0.01)).exp(),
            contact_envelope: 0.,
            flow_step: 1. - (-1. / (rate * 0.002)).exp(),
            contact_decay: (-1. / (rate * 0.0015)).exp(),
            noise_scale: (rate / 48000.).sqrt(),
            seed: seed ^ 0x5e2d_908f_7531_b4a9,
        }
    }
    /// Observation gain for duct, jet and edge turbulence after the airbox.
    /// Pulsations and compressor blade-pass sound are unaffected. Retuning keeps
    /// every filter/RNG state and follows a 20 ms time constant; preparation or
    /// reset can apply a gain immediately without a start-up ramp.
    pub fn set_air_noise(&mut self, gain: f32, immediate: bool) {
        if !gain.is_finite() {
            return;
        }
        self.air_noise_target = gain.clamp(0., 1.);
        if immediate {
            self.air_noise_gain = self.air_noise_target;
        }
    }
    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / 8388608. - 1.
    }
    /// Control-rate targets from gas state: amplitudes and band centres.
    fn control(&mut self, air: Air) {
        let limit = self.rate * 0.4;
        let flow = air.throttle_kg_s.abs();
        let bore = (air.bore_m2 * 4. / std::f32::consts::PI).sqrt();
        let mut target = [0.; 4];
        let mut centre = self.centre;
        if flow > 0. && bore > 0. {
            // Duct dipole in the plane-wave band (below the 1.84 c / (pi d)
            // ≈ 3.7 kHz cut-on of a 55 mm duct): power ∝ U⁴ (Nelson & Morfey
            // 1981), amplitude ∝ U², not the free-field U³. Spread over a
            // constant-Q band whose white-noise power grows with its centre.
            let density = air.supply_pa / (R_AIR * air.supply_k);
            let duct = (flow / (density * air.bore_m2)).min(400.);
            centre[0] = (SHEDDING * duct / bore).clamp(20., limit);
            target[0] = DUCT * (duct / 20.).powi(2) * (1000. / centre[0]).sqrt();
            // Throttle jet: Lighthill power ∝ ṁU⁷ at fixed flow; zero at ratio 1.
            let jet = air.jet_m_s();
            let ratio = air.pressure_ratio();
            let gap = air.gap_m2.clamp(1e-9, air.bore_m2) / (std::f32::consts::PI * bore);
            // Below its peak the jet spectrum rises as f², so the airbox passes
            // (f_airbox / f_peak)³ of its power. The ultrasonic rest of a thin
            // idle jet is not folded into the audible band.
            let peak = JET_ST * jet / gap;
            let corner = AIRBOX_HZ.min(limit);
            centre[1] = peak.clamp(20., corner);
            let audible = (corner / peak).min(1.).powf(1.5);
            let drive = (flow / FLOW_REF).sqrt() * (jet / 340.).powf(3.5);
            target[1] = HISS * drive * audible * (1000. / centre[1]).sqrt();
            centre[2] = (EDGE_ST * jet / bore).clamp(20., limit);
            target[2] = WHISTLE * drive * ((CHOKE - ratio) / CHOKE).clamp(0., 1.);
        }
        // Compressor: blade pass ∝ flow × tip speed², faded out by 0.45 fs.
        self.whine_hz = (air.shaft_rpm / 60. * BLADES).max(0.);
        let tip = air.shaft_rpm * std::f32::consts::TAU / 60. * TIP_RADIUS_M;
        let fade = |hz: f32| ((self.rate * 0.45 - hz) / (self.rate * 0.05)).clamp(0., 1.);
        // Splitter order at 0.4 of the full-blade order, faded on its own.
        self.splitter = 0.4 * fade(self.whine_hz * 2.);
        target[3] = WHINE * fade(self.whine_hz) * air.compressor_kg_s.abs() / FLOW_REF
            * (tip / 300.).powi(2);
        centre[3] = self.whine_hz.clamp(20., self.rate * 0.45);
        for i in 0..4 {
            self.target[i] = if target[i].is_finite() { target[i] } else { 0. };
            if centre[i].is_finite() {
                self.centre[i] += (centre[i] - self.centre[i]) * self.slew;
                self.bands[i].set_cutoff(self.centre[i]);
            }
        }
    }
    /// Turbulent flow noise, throttle hiss/whistle and compressor whine.
    /// Exactly silent without throttle and compressor flow.
    fn breath(&mut self, air: Air) -> f32 {
        if self.countdown == 0 {
            self.countdown = self.control;
            self.control(air);
        }
        self.countdown -= 1;
        for i in 0..4 {
            self.gain[i] += (self.target[i] - self.gain[i]) * self.flow_step;
        }
        let duct = self.noise() * self.noise_scale;
        let jet = self.noise() * self.noise_scale;
        let blades = self.noise() * self.noise_scale;
        let turbulence = self.bands[0].next_sample(duct * self.gain[0])
            + self.bands[1].next_sample(jet * self.gain[1])
            // Q 12 band: normalise its peak gain to one.
            + self.bands[2].next_sample(jet * self.gain[2]) / 12.;
        self.whine_phase = (self.whine_phase + self.whine_hz / self.rate).fract();
        let angle = self.whine_phase * std::f32::consts::TAU;
        let whine = (angle.sin() + (angle * 2.).sin() * self.splitter) * self.gain[3]
            + self.bands[3].next_sample(blades * self.gain[3]) * 0.05;
        let turbulence = self.airbox.next_sample(turbulence);
        if self.air_noise_gain != self.air_noise_target {
            let previous = self.air_noise_gain;
            self.air_noise_gain +=
                (self.air_noise_target - self.air_noise_gain) * self.air_noise_step;
            // Also finish if f32 rounding prevents further progress near a
            // nonzero target; otherwise the gain could stall short of one.
            if (self.air_noise_target - self.air_noise_gain).abs() < 1e-6
                || self.air_noise_gain == previous
            {
                self.air_noise_gain = self.air_noise_target;
            }
        }
        // Preserve the historical floating-point path at one. Filtering and
        // random draws continue even at zero, so reopening cannot reset the
        // source or alter any mechanical/compressor random sequence.
        let turbulence = if self.air_noise_gain == 1. {
            turbulence
        } else {
            turbulence * self.air_noise_gain
        };
        turbulence + whine
    }
    /// `pulse`: radiated intake pressure from `IntakeAcoustic`, as a sample.
    pub fn next(&mut self, pulse: f32, impact: f32, air: Air) -> (f32, f32) {
        self.contact_envelope = self.contact_envelope * self.contact_decay + impact;
        let contact_noise = self.noise() * self.noise_scale;
        let intake = pulse + self.breath(air);
        // The existing material/pitch resonator receives a short contact burst,
        // with a small coherent onset. No free-running tone or independent hiss.
        let mechanical = impact * 0.12 + contact_noise * self.contact_envelope * 0.14;
        (intake, mechanical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 55 mm bore at 300 K ambient; `open` is the butterfly area fraction.
    fn air(flow: f32, manifold_pa: f32, open: f32) -> Air {
        let bore_m2 = 2.376e-3;
        Air {
            throttle_kg_s: flow,
            gap_m2: bore_m2 * open,
            bore_m2,
            supply_pa: 101325.,
            supply_k: 300.,
            manifold_pa,
            manifold_k: 310.,
            ..Default::default()
        }
    }
    fn turbo(shaft_rpm: f32, compressor_kg_s: f32) -> Air {
        Air {
            shaft_rpm,
            compressor_kg_s,
            ..air(0., 101325., 1.)
        }
    }
    fn intake(air: Air, rate: u32) -> Vec<f32> {
        let mut r = Radiation::new(rate, 21);
        (0..rate).map(|_| r.next(0., 0., air).0).collect()
    }
    #[test]
    fn air_noise_fixed_gain_is_linear_and_one_preserves_the_reference() {
        for rate in [48000, 96000] {
            let mut reference = Radiation::new(rate, 21);
            let mut one = Radiation::new(rate, 21);
            let mut reduced = Radiation::new(rate, 21);
            let mut zero = Radiation::new(rate, 21);
            one.set_air_noise(1., true);
            reduced.set_air_noise(0.2, true);
            zero.set_air_noise(0., true);
            let mut energy = 0.;
            for frame in 0..rate / 4 {
                let phase = frame as f32 / rate as f32 * std::f32::consts::TAU;
                let moving = air(0.04 + 0.02 * (100. * phase).sin(), 55000., 0.3);
                let impact = if frame % 400 == 0 { 0.015 } else { 0. };
                let a = reference.next(0., impact, moving);
                // Idempotent live updates must not start a new fade or reset.
                one.set_air_noise(1., false);
                assert_eq!(one.next(0., impact, moving), a);
                let b = reduced.next(0., impact, moving);
                let c = zero.next(0., impact, moving);
                assert_eq!(b.0.to_bits(), (a.0 * 0.2).to_bits());
                assert_eq!(c.0, 0.);
                assert_eq!(b.1.to_bits(), a.1.to_bits());
                assert_eq!(c.1.to_bits(), a.1.to_bits());
                energy += a.0 * a.0;
            }
            assert!(energy > 0.);
        }
    }

    #[test]
    fn air_noise_zero_preserves_pulses_and_compressor_sample_for_sample() {
        for rate in [48000, 96000] {
            let mut muted = Radiation::new(rate, 18);
            let mut no_turbulence = Radiation::new(rate, 18);
            muted.set_air_noise(0., true);
            let mut compressor_energy = 0.;
            for frame in 0..rate / 4 {
                let pulse =
                    0.02 * (frame as f32 * 100. * std::f32::consts::TAU / rate as f32).sin();
                let moving = Air {
                    shaft_rpm: 90000.,
                    compressor_kg_s: 0.05,
                    ..air(0.04, 55000., 0.3)
                };
                let expected = no_turbulence.next(pulse, 0., turbo(90000., 0.05));
                let actual = muted.next(pulse, 0., moving);
                assert_eq!(actual, expected);
                compressor_energy += (actual.0 - pulse).powi(2);
            }
            assert!(compressor_energy > 0.);
            let mut pulse_only = Radiation::new(rate, 18);
            pulse_only.set_air_noise(0., true);
            for frame in 0..1000 {
                let pulse = if frame % 100 == 0 { 0.01 } else { 0. };
                assert_eq!(pulse_only.next(pulse, 0., air(0.04, 55000., 0.3)).0, pulse);
            }
        }
    }

    #[test]
    fn air_noise_live_retune_slews_without_resetting_source_state() {
        for rate in [48000, 96000] {
            let mut reference = Radiation::new(rate, 31);
            let mut edited = Radiation::new(rate, 31);
            let moving = air(0.04, 55000., 0.3);
            for _ in 0..rate / 20 {
                assert_eq!(edited.next(0., 0., moving), reference.next(0., 0., moving));
            }
            edited.set_air_noise(0., false);
            assert_eq!(edited.air_noise_gain, 1.);
            let mut previous_gain = 1.;
            for frame in 0..rate / 2 {
                let a = reference.next(0., 0.001, moving);
                let b = edited.next(0., 0.001, moving);
                assert!(edited.air_noise_gain <= previous_gain);
                assert!(previous_gain - edited.air_noise_gain < 0.0011);
                // Exact sample equality; +0 and -0 are the same muted signal.
                assert_eq!(b.0, a.0 * edited.air_noise_gain);
                assert_eq!(b.1.to_bits(), a.1.to_bits());
                if frame + 1 == rate / 50 {
                    assert!((edited.air_noise_gain - (-1_f32).exp()).abs() < 2e-4);
                }
                previous_gain = edited.air_noise_gain;
            }
            assert_eq!(edited.air_noise_gain, 0.);
            // Muting did not stop filtering or random draws. Reopening during
            // preparation therefore rejoins the uninterrupted reference exactly.
            edited.set_air_noise(1., true);
            assert_eq!(edited.next(0., 0., moving), reference.next(0., 0., moving));
        }
    }

    #[test]
    fn air_noise_retuning_cannot_create_a_sound_without_flow() {
        for rate in [48000, 96000] {
            let mut r = Radiation::new(rate, 13);
            r.set_air_noise(0., true);
            r.set_air_noise(1., false);
            let still = air(0., 20000., 0.01);
            for _ in 0..rate / 2 {
                assert_eq!(r.next(0., 0., still), (0., 0.));
            }
            assert_eq!(r.air_noise_gain, 1.);
        }
    }
    #[test]
    fn no_flow_or_contacts_cannot_generate_sound_and_tails_decay() {
        for rate in [8000, 48000, 96000, 384000] {
            let mut r = Radiation::new(rate, 13);
            // Vacuum and a spinning shaft are not sound without mass flow.
            let still = Air {
                shaft_rpm: 90000.,
                ..air(0., 20000., 0.01)
            };
            for _ in 0..1000 {
                assert_eq!(r.next(0., 0., still), (0., 0.));
            }
            let moving = Air {
                compressor_kg_s: 0.05,
                ..air(0.05, 20000., 0.01)
            };
            for _ in 0..1000 {
                let (a, b) = r.next(0.1, 0.01, moving);
                assert!(a.is_finite() && b.is_finite());
            }
            for _ in 0..rate {
                r.next(0., 0., still);
            }
            let (a, b) = r.next(0., 0., still);
            assert!(a.abs() < 1e-10 && b.abs() < 1e-10);
        }
    }
    #[test]
    fn seed_is_reproducible_and_periodic_events_do_not_repeat_an_identical_wave() {
        let mut a = Radiation::new(48000, 15);
        let mut b = Radiation::new(48000, 15);
        let mut previous = [0.; 480];
        let mut difference = 0.;
        let mut power = 0.;
        for i in 0..48000 {
            let phase = i % 480;
            let flow = (phase as f32 / 480. * std::f32::consts::TAU).sin() * 0.03;
            let impulse = if phase == 0 { 0.015 } else { 0. };
            let breath = air(0.04 + flow, 60000., 0.05);
            let x = a.next(flow, impulse, breath);
            assert_eq!(x, b.next(flow, impulse, breath));
            if i > 24000 {
                difference += (x.1 - previous[phase]).powi(2);
                power += x.1 * x.1;
            }
            previous[phase] = x.1;
        }
        assert!(power > 1e-6 && difference > power * 0.5);
    }
    #[test]
    fn duct_noise_gets_louder_and_brighter_with_air_speed() {
        // Wide open: no throttle pressure drop, only duct turbulence.
        let slow = intake(air(0.02, 101000., 1.), 48000);
        let fast = intake(air(0.08, 101000., 1.), 48000);
        let (slow, fast) = (&slow[4800..], &fast[4800..]);
        // 4x air speed: plane-wave U² is +24 dB; the constant-Q band keeps
        // total power independent of its centre.
        let db = 20. * (rms(fast) / rms(slow)).log10();
        let bright = brightness(fast) / brightness(slow);
        assert!(db > 18. && bright > 2.5, "{db} dB, x{bright}");
    }
    #[test]
    fn throttle_hiss_needs_a_pressure_drop_at_equal_flow() {
        // 30 % open at 55 kPa: 4 mm jet, peak ~15 kHz, 5 % of its amplitude
        // audible.
        let part = intake(air(0.02, 55000., 0.3), 48000);
        let open = intake(air(0.02, 101000., 1.), 48000);
        let db = 20. * (rms(&part[4800..]) / rms(&open[4800..])).log10();
        assert!(db > 12., "{db} dB");
    }
    #[test]
    fn a_thin_jet_hides_its_hiss_ultrasonically() {
        // Same flow and pressure drop through a 0.7 mm gap: peak ~90 kHz,
        // (2 / 90)^1.5 of the amplitude reaches the band; no 4-8 kHz pile-up.
        let wide = intake(air(0.02, 55000., 0.3), 48000);
        let thin = intake(air(0.02, 55000., 0.05), 48000);
        let db = 20. * (rms(&thin[4800..]) / rms(&wide[4800..])).log10();
        let x = &thin[4800..];
        let high: f32 = (40..80)
            .map(|k| dft_power(x, k as f32 * 100., 48000.))
            .sum();
        let all: f32 = (1..240)
            .map(|k| dft_power(x, k as f32 * 100., 48000.))
            .sum();
        println!(
            "thin vs wide {db:.1} dB, thin 4-8 kHz share {:.3}",
            high / all
        );
        assert!(db < -6. && high < all * 0.1, "{db} dB, {}", high / all);
    }
    fn dft_power(x: &[f32], hz: f32, rate: f32) -> f32 {
        let w = std::f32::consts::TAU * hz / rate;
        let (re, im) = x.iter().enumerate().fold((0., 0.), |(re, im), (i, v)| {
            (re + v * (w * i as f32).cos(), im + v * (w * i as f32).sin())
        });
        (re * re + im * im) / (x.len() * x.len()) as f32
    }
    #[test]
    fn compressor_whine_sits_at_full_blade_pass_and_fades_before_nyquist() {
        // 90 000 rpm x 6 blades / 60 = 9 kHz; splitter order at 18 kHz.
        let x = intake(turbo(90000., 0.05), 48000);
        let x = &x[4800..];
        let line = dft_power(x, 9000., 48000.);
        for off in [8500., 9500., 4500.] {
            assert!(line > dft_power(x, off, 48000.) * 1e3, "{off} Hz");
        }
        let splitter = dft_power(x, 18000., 48000.) / line;
        assert!((splitter.sqrt() - 0.4).abs() < 0.05, "{splitter}");
        // 180 000 rpm: 18 kHz passes at 48 kHz, its 36 kHz order may not.
        let high = intake(turbo(180000., 0.05), 48000);
        assert!(dft_power(&high[4800..], 18000., 48000.) > 0.);
        assert!(high.iter().all(|v| v.is_finite()));
        // 400 000 rpm = 40 kHz > 0.45 fs: silent, not aliased.
        assert!(
            intake(turbo(400000., 0.05), 48000)[4800..]
                .iter()
                .all(|v| *v == 0.)
        );
        // No compressor flow, no whine.
        assert!(intake(turbo(90000., 0.), 48000).iter().all(|v| *v == 0.));
    }

    fn modes(seed: u64) -> Modes {
        Modes::new(48000., 2400., 2., BlockMaterial::Aluminium, 86., seed)
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }
    // Mean-square frequency proxy: E[dx²] / E[x²] rises with spectral centroid.
    fn brightness(x: &[f32]) -> f32 {
        x.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>()
            / x.iter().map(|v| v * v).sum::<f32>()
    }
    fn impulse(m: &mut Modes, head: f32, block: f32) -> Vec<f32> {
        (0..4800)
            .map(|i| {
                if i == 0 {
                    m.next(head, block)
                } else {
                    m.next(0., 0.)
                }
            })
            .collect()
    }
    #[test]
    fn modes_silence_decay_seed_and_excitation_differ() {
        let mut m = modes(1);
        for _ in 0..1000 {
            assert_eq!(m.next(0., 0.), 0.);
        }
        let h = impulse(&mut m, 1., 0.);
        assert!(rms(&h[4000..]) < rms(&h[..400]) * 1e-4);
        assert_ne!(h, impulse(&mut modes(2), 1., 0.));
        let b = impulse(&mut modes(1), 0., 1.);
        assert!(brightness(&h) > brightness(&b) * 1.3);
        // Low sample rates mute modes above 0.4 fs instead of stacking them.
        let mut low = Modes::new(8000., 2400., 2., BlockMaterial::CastIron, 86., 1);
        assert!(impulse(&mut low, 1., 1.).iter().all(|v| v.is_finite()));
    }
    #[test]
    fn neutral_contacts_retain_body_without_upper_band_hiss() {
        let mut r = Radiation::new(48000, 15);
        let mut old = StateVariableFilter::new(48000., 2400., 2., SvfMode::Bandpass);
        let mut new = modes(15);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for i in 0..96000 {
            let impact = if i % 480 == 0 { 0.015 } else { 0. };
            let contact = r.next(0., impact, Air::default()).1;
            a.push(old.next_sample(contact));
            b.push(new.next(contact, 0.));
        }
        let db = 20. * (rms(&b) / rms(&a)).log10();
        // A level decrease alone cannot satisfy this normalized spectral test.
        // Keep useful contact energy while suppressing the harsh upper skirt.
        let bright = brightness(&b) / brightness(&a);
        assert!(
            (-9. ..=3.).contains(&db) && bright < 0.55,
            "{db} dB, x{bright}"
        );
    }

    #[test]
    fn block_and_contacts_have_distinct_damped_transfer() {
        let mut m = modes(15);
        let head = impulse(&mut m, 1., 0.);
        let block = impulse(&mut modes(15), 0., 1.);
        let power = |x: &[f32], hz| dft_power(x, hz, 48000.);
        assert!(power(&block, 4000.) < power(&block, 600.) * 0.01);
        let head_tilt = power(&head, 2000.) / power(&head, 600.);
        let block_tilt = power(&block, 2000.) / power(&block, 600.);
        assert!(
            head_tilt > block_tilt * 4.,
            "head {head_tilt}, block {block_tilt}"
        );
        for signal in [&head, &block] {
            assert!(rms(&signal[720..]) < rms(&signal[..720]) * 0.001);
        }
        // Controls keep working, including at low transport rates and their
        // extreme settings. No source creates energy once the input stops.
        for rate in [8000., 48000., 96000., 384000.] {
            for pitch in [600., 6000.] {
                let mut m = Modes::new(rate, pitch, 8., BlockMaterial::Aluminium, 86., 15);
                let mut tail = 0.;
                for i in 0..rate as usize {
                    let y = m.next(if i == 0 { 1. } else { 0. }, 0.);
                    assert!(y.is_finite());
                    tail = y;
                }
                assert!(tail.abs() < 1e-6);
            }
        }
    }
}
