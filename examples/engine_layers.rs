//! Isolated physical intake/mechanical auditions, same fixed gain before/after.
use bess::{automation_voice::AutomationVoice, output_limiter::OutputLimiter, scratch::Scratch};
fn main() -> Result<(), String> {
    let _guard = bess::realtime::DenormalGuard::enter();
    let dir = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Output directory required")?,
    );
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Optional second argument: `turbo` (0.8 bar build) or a knock intensity
    // 0..1, which adds a full-load clip.
    let mut scratch = Scratch::default();
    let mut cases = vec![("idle", 850., 0.1), ("loaded", 3000., 0.7)];
    match std::env::args().nth(2).as_deref() {
        Some("turbo") => {
            scratch.build.aspiration = bess::engine_build::Aspiration::Turbo;
            scratch.build.boost_bar = 0.8;
        }
        Some(knock) => {
            scratch.experimental.knock = knock.parse().map_err(|_| "Knock: 0..1")?;
            cases.push(("full", 4000., 1.));
        }
        None => {}
    }
    for (label, rpm, load) in cases {
        let mut voice = AutomationVoice::new(48000, &scratch)?;
        for _ in 0..48000 {
            voice.next(rpm, load);
        }
        let mut streams = [Vec::new(), Vec::new()];
        let mut limiters = [OutputLimiter::new(48000), OutputLimiter::new(48000)];
        for _ in 0..144000 {
            let x = voice.next(rpm, load);
            for (i, v) in [x.intake, x.mechanical].into_iter().enumerate() {
                streams[i].push(limiters[i].next(v * 16. * 0.8));
            }
        }
        if voice.failed() {
            return Err("Physical solver failed".into());
        }
        for (name, pcm) in ["intake", "mechanical"].into_iter().zip(streams) {
            bess::render::write_pcm(&dir.join(format!("{label}-{name}.wav")), &pcm)?;
        }
    }
    Ok(())
}
