//! Minimal real-time control surface for the recording-free instrument.
use bess::standalone::{CombustionState, Commands, Config, Levels, Synth};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::bounded;
use std::{
    io::{self, BufRead},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Message {
    commands: Commands,
    levels: Levels,
}

fn value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}
fn update(line: &str, message: &mut Message) -> Result<bool, String> {
    let mut parts = line.split_whitespace();
    let key = parts.next().unwrap_or("");
    if key == "quit" || key == "exit" {
        return Ok(false);
    }
    let arg = parts
        .next()
        .ok_or("enter a control and value, e.g. rpm 3000")?;
    if parts.next().is_some() {
        return Err("one value per command".into());
    }
    if key == "combustion" {
        message.commands.combustion = match arg {
            "firing" => CombustionState::Firing,
            "motoring" => CombustionState::Motoring,
            "fuel_cut" => CombustionState::FuelCut,
            _ => return Err("combustion: firing, motoring or fuel_cut".into()),
        };
    } else {
        let v: f32 = arg.parse().map_err(|_| "invalid number")?;
        match key {
            "rpm" => message.commands.rpm = v,
            "load" => message.commands.load = v,
            "volume" => message.commands.volume = v,
            "exhaust" => message.levels.exhaust = v,
            "intake" => message.levels.intake = v,
            "block" => message.levels.block = v,
            _ => {
                return Err(
                    "controls: rpm, load, volume, exhaust, intake, block, combustion, quit".into(),
                );
            }
        }
    }
    message.commands.validate()?;
    message.levels.validate()?;
    Ok(true)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help") {
        println!(
            "standalone_live [--preset single|four-even|four-split | --config FILE] [--volume 0.6] [--seconds 5]\nEnter: rpm 3000, load 0.5, volume 0.6, exhaust 0.7, intake 0.3, block 0.2, combustion fuel_cut, quit"
        );
        return Ok(());
    }
    let config = if let Some(path) = value(&args, "--config") {
        serde_json::from_str::<Config>(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        Config::preset(value(&args, "--preset").unwrap_or("four-even")).ok_or("unknown preset")?
    };
    config.validate()?;
    let mut commands = Commands::default();
    if let Some(v) = value(&args, "--volume") {
        commands.volume = v.parse().map_err(|_| "invalid volume")?;
    }
    commands.validate()?;
    let levels = Levels {
        exhaust: config.calibration.exhaust_level,
        intake: config.calibration.intake_level,
        block: config.calibration.block_level,
    };
    let mut message = Message { commands, levels };
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .collect();
    let supported = [default.channels(), 2, 1]
        .into_iter()
        .find_map(|channels| {
            [
                cpal::SampleFormat::F32,
                cpal::SampleFormat::I16,
                cpal::SampleFormat::U16,
            ]
            .into_iter()
            .find_map(|format| {
                ranges
                    .iter()
                    .filter(|r| r.channels() == channels && r.sample_format() == format)
                    .find_map(|r| r.try_with_sample_rate(48_000))
            })
        })
        .unwrap_or(default);
    let rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let stream_config = supported.config();
    let mut synth = Synth::new(rate, config, commands)?;
    let (tx, rx) = bounded::<Message>(8);
    let max_ns = Arc::new(AtomicU64::new(0));
    let callbacks = Arc::new(AtomicU64::new(0));
    let applied = Arc::new(AtomicU64::new(0));
    let max_in_callback = max_ns.clone();
    let calls_in_callback = callbacks.clone();
    let applied_in_callback = applied.clone();
    macro_rules! build_stream {
        ($type:ty, $convert:expr) => {{
            device.build_output_stream(
                stream_config,
                move |data: &mut [$type], _: &cpal::OutputCallbackInfo| {
                    let start = Instant::now();
                    while let Ok(next) = rx.try_recv() {
                        let _ = synth.set_commands(next.commands);
                        let _ = synth.set_levels(next.levels);
                        applied_in_callback.fetch_add(1, Ordering::Relaxed);
                    }
                    for frame in data.chunks_mut(channels) {
                        let sample = synth.next_sample();
                        for channel in frame {
                            *channel = ($convert)(sample);
                        }
                    }
                    max_in_callback.fetch_max(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    calls_in_callback.fetch_add(1, Ordering::Relaxed);
                },
                |error| eprintln!("audio stream error: {error}"),
                None,
            )
        }};
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build_stream!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => build_stream!(i16, |s: f32| (s * i16::MAX as f32) as i16),
        cpal::SampleFormat::U16 => {
            build_stream!(u16, |s: f32| ((s * 0.5 + 0.5) * u16::MAX as f32) as u16)
        }
        _ => return Err(format!("unsupported audio format: {format:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    println!(
        "Playing {rate} Hz / {channels} channels / {format:?}; mono signal duplicated across channels."
    );
    println!(
        "Controls: rpm, load, volume, exhaust, intake, block, combustion; enter quit to stop."
    );
    if let Some(v) = value(&args, "--seconds") {
        let seconds: f32 = v.parse().map_err(|_| "invalid seconds")?;
        if !seconds.is_finite() || !(0.1..=60.).contains(&seconds) {
            return Err("seconds must be 0.1–60".into());
        }
        std::thread::sleep(Duration::from_secs_f32(seconds));
    } else {
        for line in io::stdin().lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            let mut candidate = message;
            match update(&line, &mut candidate) {
                Ok(false) => break,
                Ok(true) => {
                    message = candidate;
                    tx.send(message).map_err(|e| e.to_string())?;
                }
                Err(error) => eprintln!("{error}"),
            }
        }
    }
    drop(stream);
    println!(
        "callbacks={} commands_applied={} maximum_callback_ms={:.3}",
        callbacks.load(Ordering::Relaxed),
        applied.load(Ordering::Relaxed),
        max_ns.load(Ordering::Relaxed) as f64 / 1e6
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_validate_before_submission() {
        let mut message = Message {
            commands: Commands::default(),
            levels: Levels {
                exhaust: 0.7,
                intake: 0.3,
                block: 0.2,
            },
        };
        assert!(update("rpm 0", &mut message).unwrap());
        assert!(update("combustion fuel_cut", &mut message).unwrap());
        assert_eq!(message.commands.combustion, CombustionState::FuelCut);
        assert!(update("intake NaN", &mut message).is_err());
        assert!(!update("quit", &mut message).unwrap());
    }
}
