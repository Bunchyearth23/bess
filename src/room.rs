//! Optional listening room: uniformly partitioned overlap-save convolution
//! with a frequency-domain delay line and deterministic synthetic IRs.
//! Only the live scratch listener constructs it (`Bench::enable_room`); WAV,
//! BeamNG export and level analysis never do, so they cannot hear it.
use bdsp::noise::{Noise, NoiseColor};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Room {
    #[default]
    Off,
    Garage,
    Outdoor,
    Hall,
}

impl Room {
    pub const ALL: [Self; 4] = [Self::Off, Self::Garage, Self::Outdoor, Self::Hall];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Dry",
            Self::Garage => "Garage",
            Self::Outdoor => "Open road",
            Self::Hall => "Parking hall",
        }
    }

    fn index(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Garage => Some(0),
            Self::Outdoor => Some(1),
            Self::Hall => Some(2),
        }
    }
}

/// Early reflections `(s, gain)`, then a three-band noise tail whose
/// RT60 (low < 400 Hz, mid, high > 3 kHz) falls with frequency.
struct Design {
    reflections: &'static [(f64, f64)],
    tail_start: f64,
    /// Tail energy relative to the early reflections.
    tail_db: f64,
    rt60: [f64; 3],
    air_hz: f64,
    length: f64,
    seed: u32,
}

// First reflections stay beyond 5.5 ms: the convolver's block latency
// (≤ 5.5 ms) is absorbed by the IR's silent head, so the wet path is on time.
const DESIGNS: [Design; 3] = [
    // Concrete single garage, ~6 × 3.5 × 2.5 m.
    Design {
        reflections: &[
            (0.0055, 0.8),
            (0.0068, -0.7),
            (0.0071, 0.6),
            (0.0093, 0.55),
            (0.0118, -0.45),
            (0.0149, 0.4),
            (0.0187, 0.3),
        ],
        tail_start: 0.008,
        tail_db: 3.,
        rt60: [0.75, 0.6, 0.35],
        air_hz: 7000.,
        length: 0.8,
        seed: 0x6A12_0001,
    },
    // Roadside: kerb/ground, a hedge and a building 10–20 m away.
    Design {
        reflections: &[(0.0056, 0.6), (0.019, 0.35), (0.047, -0.25), (0.083, 0.15)],
        tail_start: 0.02,
        tail_db: -12.,
        rt60: [0.3, 0.22, 0.12],
        air_hz: 9000.,
        length: 0.3,
        seed: 0x6A12_0002,
    },
    // Underground parking level, ~40 × 30 × 2.6 m.
    Design {
        reflections: &[
            (0.0062, 0.7),
            (0.0115, -0.5),
            (0.0172, 0.45),
            (0.0248, 0.4),
            (0.0331, -0.35),
            (0.0457, 0.3),
            (0.0612, 0.25),
        ],
        tail_start: 0.015,
        tail_db: 8.,
        rt60: [1.7, 1.5, 0.8],
        air_hz: 5000.,
        length: 1.6,
        seed: 0x6A12_0003,
    },
];

fn one_pole(x: &mut [f64], hz: f64, rate: f64) {
    let a = (-std::f64::consts::TAU * hz / rate).exp();
    let mut y = 0.;
    for v in x {
        y = *v + (y - *v) * a;
        *v = y;
    }
}

/// 100 Hz–4 kHz energy: normalizing it to a unit impulse's keeps a 100 %
/// wet engine near the dry level instead of trusting broadband energy.
fn band_energy(h: &[f64], rate: f64) -> f64 {
    let mut x = h.to_vec();
    x.resize(h.len() + (rate * 0.05) as usize, 0.);
    let mut low = x.clone();
    one_pole(&mut low, 100., rate);
    for (v, l) in x.iter_mut().zip(&low) {
        *v -= l;
    }
    one_pole(&mut x, 4000., rate);
    x.iter().map(|v| v * v).sum()
}

fn impulse_response(design: &Design, rate: f64) -> Vec<f32> {
    let len = (design.length * rate) as usize;
    let mut early = vec![0.; len];
    for &(t, g) in design.reflections {
        early[(t * rate).round() as usize] += g;
    }
    let mut noise = Noise::with_seed(NoiseColor::White, design.seed);
    let white: Vec<f64> = (0..len).map(|_| f64::from(noise.next_sample())).collect();
    let (mut low, mut mid) = (white.clone(), white.clone());
    one_pole(&mut low, 400., rate);
    one_pole(&mut mid, 3000., rate);
    let mut tail = vec![0.; len];
    let start = (design.tail_start * rate) as usize;
    for i in start..len {
        let t = (i - start) as f64 / rate;
        let decay = |rt60: f64| 10f64.powf(-3. * t / rt60);
        let onset = 1. - (-t / 0.01).exp();
        tail[i] = onset
            * (low[i] * decay(design.rt60[0])
                + (mid[i] - low[i]) * decay(design.rt60[1])
                + (white[i] - mid[i]) * decay(design.rt60[2]));
    }
    let energy = |x: &[f64]| x.iter().map(|v| v * v).sum::<f64>();
    let tail_gain = (energy(&early) / energy(&tail) * 10f64.powf(design.tail_db / 10.)).sqrt();
    let mut h: Vec<f64> = early
        .iter()
        .zip(&tail)
        .map(|(e, t)| e + t * tail_gain)
        .collect();
    one_pole(&mut h, design.air_hz, rate);
    let fade = len / 10;
    for (i, v) in h[len - fade..].iter_mut().enumerate() {
        *v *= 0.5 + 0.5 * (std::f64::consts::PI * i as f64 / fade as f64).cos();
    }
    let gain = (band_energy(&[1.], rate) / band_energy(&h, rate)).sqrt();
    h.iter().map(|v| (v * gain) as f32).collect()
}

/// Mono room with equal-power wet/dry mix. Room and mix changes take effect at
/// block boundaries and ramp over 30 ms; nothing allocates after `new`.
pub struct RoomReverb {
    block: usize,
    bins: usize,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
    frame: Vec<Complex32>,
    /// Previous and current input blocks (overlap-save window).
    input: Vec<f32>,
    /// Per room partition spectra, split re/im, `bins` per partition.
    spectra: [[Vec<f32>; 2]; 3],
    /// Input spectra ring, `slots × bins`, shared by every room.
    history: [Vec<f32>; 2],
    slots: usize,
    newest: usize,
    sum: [Vec<f32>; 2],
    /// Wet output being played: active room, then the room fading out.
    wet: [Vec<f32>; 2],
    position: usize,
    active: usize,
    previous: Option<usize>,
    fade: f32,
    room: Room,
    mix: f32,
    goal: f32,
    amount: f32,
    step: f32,
}

impl RoomReverb {
    pub fn new(rate: u32) -> Self {
        let rate = f64::from(rate.max(8000));
        // ≤ 5.5 ms so the first reflection lies beyond the block latency.
        let block = (1usize << (rate * 0.0055).log2().floor() as u32).clamp(32, 1024);
        let irs = DESIGNS.each_ref().map(|d| impulse_response(d, rate));
        Self::from_irs(rate as f32, block, irs.each_ref().map(|h| &h[block..]))
    }

    /// `y[n] = Σ h[k]·x[n − block − k]`: IRs exclude the block latency.
    pub fn from_irs(rate: f32, block: usize, irs: [&[f32]; 3]) -> Self {
        let n = 2 * block;
        let bins = block + 1;
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(n);
        let inverse = planner.plan_fft_inverse(n);
        let mut scratch = vec![
            Complex32::default();
            forward
                .get_inplace_scratch_len()
                .max(inverse.get_inplace_scratch_len())
        ];
        let spectra = irs.map(|h| {
            let mut parts = [Vec::new(), Vec::new()];
            for chunk in h.chunks(block) {
                let mut frame = vec![Complex32::default(); n];
                for (f, &v) in frame.iter_mut().zip(chunk) {
                    f.re = v;
                }
                forward.process_with_scratch(&mut frame, &mut scratch);
                parts[0].extend(frame[..bins].iter().map(|c| c.re));
                parts[1].extend(frame[..bins].iter().map(|c| c.im));
            }
            parts
        });
        let slots = spectra
            .iter()
            .map(|s| s[0].len() / bins)
            .max()
            .unwrap_or(1)
            .max(1);
        Self {
            block,
            bins,
            forward,
            inverse,
            scratch,
            frame: vec![Complex32::default(); n],
            input: vec![0.; n],
            spectra,
            history: [vec![0.; slots * bins], vec![0.; slots * bins]],
            slots,
            newest: 0,
            sum: [vec![0.; bins], vec![0.; bins]],
            wet: [vec![0.; block], vec![0.; block]],
            position: 0,
            active: 0,
            previous: None,
            fade: 1.,
            room: Room::Off,
            mix: 0.,
            goal: 0.,
            amount: 0.,
            step: 1. / (rate * 0.030),
        }
    }

    pub fn set(&mut self, room: Room, mix: f32) {
        self.room = room;
        self.mix = if mix.is_finite() {
            mix.clamp(0., 1.)
        } else {
            0.
        };
    }

    pub fn next(&mut self, dry: f32) -> f32 {
        let i = self.position;
        // A non-finite sample would otherwise poison the history for a whole IR.
        self.input[self.block + i] = if dry.is_finite() { dry } else { 0. };
        self.amount += (self.goal - self.amount).clamp(-self.step, self.step);
        let mut wet = self.wet[0][i];
        if self.previous.is_some() {
            self.fade = (self.fade + self.step).min(1.);
            wet = self.wet[1][i] + (wet - self.wet[1][i]) * self.fade;
        }
        self.position += 1;
        if self.position == self.block {
            self.position = 0;
            self.process_block();
        }
        if self.amount == 0. {
            return dry;
        }
        dry * (1. - self.amount).sqrt() + wet * self.amount.sqrt()
    }

    fn process_block(&mut self) {
        let (block, bins) = (self.block, self.bins);
        for (f, &x) in self.frame.iter_mut().zip(&self.input) {
            *f = Complex32::new(x, 0.);
        }
        self.input.copy_within(block.., 0);
        self.forward
            .process_with_scratch(&mut self.frame, &mut self.scratch);
        self.newest = (self.newest + 1) % self.slots;
        let at = self.newest * bins;
        for (k, c) in self.frame[..bins].iter().enumerate() {
            self.history[0][at + k] = c.re;
            self.history[1][at + k] = c.im;
        }
        // Commands land here so both wet buffers are valid before a ramp starts.
        if self.fade >= 1. {
            self.previous = None;
        }
        self.goal = if self.room == Room::Off { 0. } else { self.mix };
        if let Some(room) = self.room.index()
            && room != self.active
        {
            if self.amount == 0. {
                self.active = room;
            } else if self.previous.is_none() {
                self.previous = Some(self.active);
                self.active = room;
                self.fade = 0.;
            }
        }
        if self.goal == 0. && self.amount == 0. {
            return;
        }
        self.render(self.active, 0);
        if let Some(previous) = self.previous {
            self.render(previous, 1);
        }
    }

    fn render(&mut self, room: usize, out: usize) {
        let (block, bins, slots, newest) = (self.block, self.bins, self.slots, self.newest);
        let [sum_re, sum_im] = &mut self.sum;
        let [h_re, h_im] = &self.spectra[room];
        let [x_re, x_im] = &self.history;
        sum_re.fill(0.);
        sum_im.fill(0.);
        for p in 0..h_re.len() / bins {
            let slot = (newest + slots - p) % slots * bins;
            let h = h_re[p * bins..][..bins]
                .iter()
                .zip(&h_im[p * bins..][..bins]);
            let x = x_re[slot..][..bins].iter().zip(&x_im[slot..][..bins]);
            for (((sr, si), (xr, xi)), (hr, hi)) in
                sum_re.iter_mut().zip(sum_im.iter_mut()).zip(x).zip(h)
            {
                *sr += xr * hr - xi * hi;
                *si += xr * hi + xi * hr;
            }
        }
        // Real signals: rebuild the negative frequencies by conjugate symmetry.
        let n = 2 * block;
        for k in 0..bins {
            self.frame[k] = Complex32::new(sum_re[k], sum_im[k]);
        }
        for k in 1..block {
            self.frame[n - k] = self.frame[k].conj();
        }
        self.inverse
            .process_with_scratch(&mut self.frame, &mut self.scratch);
        let scale = 1. / n as f32;
        for (w, c) in self.wet[out].iter_mut().zip(&self.frame[block..]) {
            *w = c.re * scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random(len: usize, seed: u32) -> Vec<f32> {
        let mut noise = Noise::with_seed(NoiseColor::White, seed);
        (0..len).map(|_| noise.next_sample()).collect()
    }

    fn settle(reverb: &mut RoomReverb, room: Room) {
        reverb.set(room, 1.);
        for _ in 0..reverb.block * 40 {
            assert_eq!(reverb.next(0.), 0.);
        }
        assert_eq!(reverb.amount, 1.);
    }

    #[test]
    fn partitioned_output_matches_direct_convolution() {
        let block = 64;
        let h: Vec<f32> = random(1000, 7).iter().map(|v| v * 0.06).collect();
        let short = [0.5, -0.25, 0.125];
        let other = random(300, 9);
        let mut reverb = RoomReverb::from_irs(48_000., block, [&h, &short, &other]);
        settle(&mut reverb, Room::Garage);
        let mut x = vec![0.; 3000];
        x[0] = 1.;
        x[1500..].copy_from_slice(&random(1500, 11));
        let y: Vec<f32> = x.iter().map(|&v| reverb.next(v)).collect();
        let mut worst = 0f64;
        for (n, &out) in y.iter().enumerate() {
            let expected: f64 = (0..h.len())
                .filter(|&k| n >= block + k)
                .map(|k| f64::from(h[k]) * f64::from(x[n - block - k]))
                .sum();
            worst = worst.max((f64::from(out) - expected).abs());
        }
        println!("max |partitioned − direct| = {worst:e}");
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn synthetic_rooms_decay_like_their_design_and_highs_die_first() {
        let rate = 96_000.;
        for design in &DESIGNS {
            let h = impulse_response(design, rate);
            assert!(h[..(0.0055 * rate) as usize].iter().all(|&v| v == 0.));
            // Schroeder backward integration, T20 extrapolated to −60 dB, per band.
            let t20 = |h: &[f64]| {
                let mut edc: Vec<f64> = h
                    .iter()
                    .rev()
                    .scan(0., |s, v| {
                        *s += v * v;
                        Some(*s)
                    })
                    .collect();
                edc.reverse();
                let db = |i: usize| 10. * (edc[i] / edc[0]).log10();
                let at = |level: f64| (0..edc.len()).find(|&i| db(i) <= level).unwrap();
                3. * (at(-25.) - at(-5.)) as f64 / rate
            };
            let h: Vec<f64> = h.iter().map(|&v| f64::from(v)).collect();
            let mut low = h.clone();
            one_pole(&mut low, 300., rate);
            let mut high = h.clone();
            let mut smooth = h.clone();
            one_pole(&mut smooth, 2500., rate);
            for (v, s) in high.iter_mut().zip(&smooth) {
                *v -= s;
            }
            let (low, high) = (t20(&low), t20(&high));
            println!(
                "{:.2} s design: low T20 {low:.2} s, high T20 {high:.2} s",
                design.rt60[1]
            );
            assert!(high < low);
            assert!((low / design.rt60[0] - 1.).abs() < 0.35, "{low}");
        }
    }

    #[test]
    fn room_switches_and_mix_ramps_do_not_step() {
        let rate = 96_000;
        let mut reverb = RoomReverb::new(rate);
        let tone = |i: usize| 0.5 * (std::f32::consts::TAU * 110. * i as f32 / rate as f32).sin();
        let mut previous = 0.;
        let mut steady = 0f32;
        let mut worst = 0f32;
        let seconds = |s: f32| (s * rate as f32) as usize;
        for i in 0..seconds(4.) {
            match i {
                i if i == seconds(0.5) => reverb.set(Room::Garage, 0.8),
                i if i == seconds(1.5) => reverb.set(Room::Hall, 0.8),
                i if i == seconds(1.505) => reverb.set(Room::Outdoor, 0.8),
                i if i == seconds(2.5) => reverb.set(Room::Garage, 0.3),
                i if i == seconds(3.2) => reverb.set(Room::Off, 0.3),
                _ => {}
            }
            let y = reverb.next(tone(i));
            let jump = (y - previous).abs();
            previous = y;
            if i < seconds(0.5) {
                steady = steady.max(jump);
            } else {
                worst = worst.max(jump);
            }
        }
        // Dry 110 Hz at 0.5 moves ≤ 3.6e-3/sample; a hard switch would jump ~0.1+.
        println!("max sample step: dry {steady:e}, with room switching {worst:e}");
        assert!(worst < 4. * steady, "{worst} vs {steady}");
        assert_eq!(reverb.amount, 0.);
    }

    #[test]
    fn full_wet_rooms_stay_near_the_dry_level() {
        let rate = 96_000;
        let x: Vec<f32> = (0..rate as usize * 3)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (1..12)
                    .map(|h| (std::f32::consts::TAU * 45. * h as f32 * t).sin() / h as f32)
                    .sum::<f32>()
                    * 0.2
            })
            .collect();
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        for room in [Room::Garage, Room::Outdoor, Room::Hall] {
            let mut reverb = RoomReverb::new(rate);
            reverb.set(room, 1.);
            let y: Vec<f32> = x.iter().map(|&v| reverb.next(v)).collect();
            let db = 20. * (rms(&y[rate as usize..]) / rms(&x[rate as usize..])).log10();
            println!("{room:?}: 100 % wet {db:+.1} dB vs dry");
            assert!(db.abs() < 4., "{room:?}: {db}");
        }
    }
}
