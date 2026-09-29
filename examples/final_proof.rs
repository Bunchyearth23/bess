//! Final-proof renders: every layer and the mix, NA and turbo, fixed gain.
use bess::{
    automation_voice::AutomationVoice, engine_build::Aspiration, output_limiter::OutputLimiter,
    scratch::Scratch,
};
const RATE: u32 = 48000;
fn main() -> Result<(), String> {
    let _guard = bess::realtime::DenormalGuard::enter();
    let dir = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Output directory required")?,
    );
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut turbo = Scratch::default();
    turbo.build.aspiration = Aspiration::Turbo;
    turbo.build.boost_bar = 0.8;
    for (engine, scratch) in [("na", Scratch::default()), ("turbo", turbo)] {
        let points: [(&str, f32, f32); 5] = [
            ("idle", 850., 0.1),
            ("cruise", 2000., 0.3),
            ("loaded", 3000., 0.7),
            ("wot", 4500., 1.0),
            ("high", 6000., 0.7),
        ];
        for (label, rpm, load) in points {
            render(
                &dir,
                &format!("{engine}-{label}"),
                &scratch,
                |_| (rpm, load),
                3.,
            )?;
        }
        // 1000 → 6000 rpm pull at 0.8, then lift-off.
        render(
            &dir,
            &format!("{engine}-sweep"),
            &scratch,
            |t| {
                if t < 6. {
                    (1000. + 5000. * t / 6., 0.8)
                } else {
                    (6000. - 2500. * (t - 6.), 0.)
                }
            },
            8.,
        )?;
    }
    Ok(())
}
fn render(
    dir: &std::path::Path,
    name: &str,
    scratch: &Scratch,
    at: impl Fn(f32) -> (f32, f32),
    seconds: f32,
) -> Result<(), String> {
    let mut voice = AutomationVoice::new(RATE, scratch)?;
    let (rpm0, load0) = at(0.);
    for _ in 0..RATE {
        voice.next(rpm0, load0);
    }
    let mut streams = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut limiters = [(); 4].map(|_| OutputLimiter::new(RATE));
    for i in 0..(seconds * RATE as f32) as usize {
        let (rpm, load) = at(i as f32 / RATE as f32);
        let x = voice.next(rpm, load);
        let mix = x.exhaust + x.intake + x.mechanical;
        for (k, v) in [x.exhaust, x.intake, x.mechanical, mix]
            .into_iter()
            .enumerate()
        {
            streams[k].push(limiters[k].next(v * 16. * 0.8));
        }
    }
    if voice.failed() {
        return Err(format!("{name}: physical solver failed"));
    }
    for (layer, pcm) in ["exhaust", "intake", "mechanical", "mix"]
        .into_iter()
        .zip(streams)
    {
        bess::render::write_pcm(&dir.join(format!("{name}-{layer}.wav")), &pcm)?;
    }
    Ok(())
}
