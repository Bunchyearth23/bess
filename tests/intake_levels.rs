//! Historical full-airflow calibration (X-023), still available at 0 dB.
use bess::{automation_voice::AutomationVoice, scratch::Scratch};
use rustfft::{FftPlanner, num_complex::Complex};

// Default NA I4 with reference airflow, 48 kHz voice: 1 s settling, then 1 s measured.
fn intake(rpm: f32, load: f32) -> Vec<f32> {
    let _guard = bess::realtime::DenormalGuard::enter();
    let mut scratch = Scratch::default();
    // These old spectral/level criteria require the deliberately noise-dominant
    // balance. Keep guarding that reference without imposing it on the new
    // pulse-led default; source isolation and matched renders qualify the latter.
    scratch.sound.intake_air_noise = 1.;
    let mut voice = AutomationVoice::new(48000, &scratch).unwrap();
    for _ in 0..48000 {
        voice.next(rpm, load);
    }
    let x: Vec<f32> = (0..48000).map(|_| voice.next(rpm, load).intake).collect();
    assert!(!voice.failed());
    x
}
fn db(x: &[f32]) -> f32 {
    10. * (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).log10()
}
// Share of power in [low, high) Hz, one 1 s FFT (1 Hz bins).
fn share(x: &[f32], low: usize, high: usize) -> f32 {
    let mut bins: Vec<Complex<f32>> = x.iter().map(|&v| Complex::new(v, 0.)).collect();
    FftPlanner::new()
        .plan_fft_forward(bins.len())
        .process(&mut bins);
    let power: Vec<f32> = bins[..24000].iter().map(|c| c.norm_sqr()).collect();
    power[low..high].iter().sum::<f32>() / power.iter().sum::<f32>()
}

#[test]
fn reference_airflow_keeps_its_load_and_bandwidth_calibration() {
    let idle = intake(850., 0.1);
    let cruise = intake(2000., 0.3);
    let loaded = intake(3000., 0.7);
    let wot = intake(4500., 1.);
    let level = [&idle, &cruise, &loaded, &wot].map(|x| db(x));
    let idle_hiss = share(&idle, 4000, 8000);
    let cruise_low = share(&cruise, 0, 250);
    // Measured: -87.8 / -62.4 / -49.0 / -46.7 dB, 1 % idle 4-8 kHz, 47 %
    // cruise below 250 Hz (merged main before X-023: 26 % and 94 %).
    println!("levels {level:.1?} dB, idle 4-8 kHz {idle_hiss:.3}, cruise <250 Hz {cruise_low:.3}");
    assert!(level.windows(2).all(|w| w[0] < w[1]), "{level:?}");
    assert!(idle_hiss < 0.1, "{idle_hiss}");
    assert!(cruise_low < 0.75, "{cruise_low}");
}
