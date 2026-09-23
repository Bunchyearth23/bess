"""Inspect exported BeamNG engine/exhaust loops without playing the game.

Usage:
  python tools/audit_variant_audio.py variant-r4.zip variant-r5.zip --output audit.json

Requires NumPy. Measurements are diagnostics, not a score for perceived realism.
The two load rows in a BeamNG sfxBlend2D are labeled off/on here.
"""

from __future__ import annotations

import argparse
import bisect
import io
import json
import math
import re
import wave
import zipfile
from pathlib import Path

import numpy as np


SAMPLE_RATE = 48_000
PHASE_BINS = 2048


def read_pcm24(wav_data: bytes) -> np.ndarray:
    with wave.open(io.BytesIO(wav_data), "rb") as wav:
        if (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) != (48_000, 1, 3):
            raise ValueError("Expected mono 48 kHz / 24-bit PCM")
        raw = np.frombuffer(wav.readframes(wav.getnframes()), dtype=np.uint8).reshape(-1, 3)
    values = (
        raw[:, 0].astype(np.int32)
        | (raw[:, 1].astype(np.int32) << 8)
        | (raw[:, 2].astype(np.int32) << 16)
    )
    values[values >= 0x800000] -= 0x1000000
    return values.astype(np.float64) / 0x800000


def rms(x: np.ndarray) -> float:
    return float(np.sqrt(np.mean(np.square(x))))


def phase_template(x: np.ndarray, rpm: float) -> np.ndarray:
    """Average at common 720-degree crank angle, suppressing non-periodic noise."""
    phase = (np.arange(x.size) * (rpm / (120 * SAMPLE_RATE))) % 1.0
    position = phase * PHASE_BINS
    left = position.astype(np.int32)
    right = (left + 1) % PHASE_BINS
    fraction = position - left
    total = np.bincount(left, weights=x * (1 - fraction), minlength=PHASE_BINS)
    total += np.bincount(right, weights=x * fraction, minlength=PHASE_BINS)
    weight = np.bincount(left, weights=1 - fraction, minlength=PHASE_BINS)
    weight += np.bincount(right, weights=fraction, minlength=PHASE_BINS)
    return total / np.maximum(weight, 1e-12)


def normalized_correlation(a: np.ndarray, b: np.ndarray) -> float:
    a = a - np.mean(a)
    b = b - np.mean(b)
    denominator = np.linalg.norm(a) * np.linalg.norm(b)
    return float(np.dot(a, b) / denominator) if denominator > 0 else 0.0


def transient_metrics(x: np.ndarray) -> dict[str, float | int]:
    """Contrast and timing of energy bursts in the 1.2-7 kHz mechanical band."""
    spectrum = np.fft.rfft(x)
    hz = np.fft.rfftfreq(x.size, 1 / SAMPLE_RATE)
    spectrum[(hz < 1200) | (hz > 7000)] = 0
    band = np.fft.irfft(spectrum, n=x.size)
    frame_size = 48  # one millisecond
    frames = band[: band.size // frame_size * frame_size].reshape(-1, frame_size)
    envelope = np.sqrt(np.mean(frames * frames, axis=1))
    median = float(np.median(envelope))
    mad = float(np.median(np.abs(envelope - median)))
    threshold = median + 2.5 * mad
    peaks = np.flatnonzero(
        (envelope[1:-1] > envelope[:-2])
        & (envelope[1:-1] >= envelope[2:])
        & (envelope[1:-1] > threshold)
    ) + 1
    if peaks.size:
        selected = [int(peaks[0])]
        for index in peaks[1:]:
            if index - selected[-1] >= 3:
                selected.append(int(index))
            elif envelope[index] > envelope[selected[-1]]:
                selected[-1] = int(index)
        peaks = np.array(selected, dtype=np.int32)
    intervals = np.diff(peaks)
    return {
        "burst_rate_hz": round(float(peaks.size / (envelope.size / 1000)), 3),
        "inter_burst_cv": round(float(np.std(intervals) / np.mean(intervals)), 3)
        if intervals.size > 2 else 0.0,
        "envelope_p95_to_median": round(
            float(np.percentile(envelope, 95) / max(median, 1e-12)), 3
        ),
    }


def cycle_variation(x: np.ndarray, rpm: float) -> dict[str, float | int]:
    boundaries = np.arange(0, x.size, SAMPLE_RATE * 120 / rpm).astype(np.int32)
    cycle_rms = np.array([rms(x[a:b]) for a, b in zip(boundaries, boundaries[1:]) if b > a])
    return {
        "complete_cycles": int(cycle_rms.size),
        "cycle_rms_cv": round(float(np.std(cycle_rms) / max(np.mean(cycle_rms), 1e-12)), 4),
    }


def relative_phase_sweep(
    exhaust: np.ndarray,
    engine: np.ndarray,
    firing_hz: float,
    exhaust_gain_db: float = 0,
    engine_gain_db: float = 0,
    cross: np.ndarray | None = None,
) -> dict[str, float]:
    """Circularly offset stems over one firing period to bound static phase effects.

    This models a sample offset, not BeamNG spatial delay or load interpolation.
    """
    if cross is None:
        cross = np.fft.irfft(
            np.fft.rfft(exhaust) * np.conj(np.fft.rfft(engine)), n=exhaust.size
        ) / exhaust.size
    exhaust_gain = 10 ** (exhaust_gain_db / 20)
    engine_gain = 10 ** (engine_gain_db / 20)
    offsets = max(1, int(round(SAMPLE_RATE / firing_hz)))
    mixed_rms = np.sqrt(
        np.maximum(
            (rms(exhaust) * exhaust_gain) ** 2
            + (rms(engine) * engine_gain) ** 2
            + 2 * exhaust_gain * engine_gain * cross[:offsets],
            0,
        )
    )
    base = max(rms(exhaust) * exhaust_gain, 1e-12)
    zero_offset = max(float(mixed_rms[0]), 1e-12)
    return {
        "minimum_vs_exhaust_db": round(20 * math.log10(max(float(np.min(mixed_rms)), 1e-12) / base), 2),
        "maximum_vs_exhaust_db": round(20 * math.log10(max(float(np.max(mixed_rms)), 1e-12) / base), 2),
        "minimum_vs_zero_offset_db": round(20 * math.log10(max(float(np.min(mixed_rms)), 1e-12) / zero_offset), 2),
        "maximum_vs_zero_offset_db": round(20 * math.log10(max(float(np.max(mixed_rms)), 1e-12) / zero_offset), 2),
        "swept_period_ms": round(1000 / firing_hz, 3),
    }


def analyze_loop(x: np.ndarray, rpm: float, cylinders: int) -> tuple[dict, np.ndarray]:
    spectrum = np.fft.rfft((x - np.mean(x)) * np.hanning(x.size))
    power = np.abs(spectrum) ** 2
    hz = np.fft.rfftfreq(x.size, 1 / SAMPLE_RATE)

    def energy(low: float, high: float) -> float:
        return float(np.sum(power[(hz >= low) & (hz < high)]))

    low_mid = energy(80, 4000)
    high = energy(4000, 10000)
    useful = energy(80, 10000)
    firing_hz = rpm * cylinders / 120
    order_mask = np.zeros(hz.size, dtype=bool)
    for harmonic in range(1, int(4000 / firing_hz) + 1):
        order_mask |= np.abs(hz - harmonic * firing_hz) <= 3
    band_mask = (hz >= 50) & (hz < 4000)
    harmonic_fraction = float(np.sum(power[order_mask & band_mask]) / max(np.sum(power[band_mask]), 1e-30))
    high_power = power[(hz >= 4000) & (hz < 10000)]
    flatness = float(np.exp(np.mean(np.log(high_power + 1e-30))) / max(np.mean(high_power), 1e-30))
    mid_power = power[(hz >= 1200) & (hz < 4000)]
    mid_flatness = float(np.exp(np.mean(np.log(mid_power + 1e-30))) / max(np.mean(mid_power), 1e-30))
    differences = np.abs(np.diff(x))
    seam_jump = abs(float(x[0] - x[-1]))
    edge_length = int(.01 * SAMPLE_RATE)
    seam_rms_db = 20 * math.log10(max(rms(x[:edge_length]), 1e-12) / max(rms(x[-edge_length:]), 1e-12))
    measures = {
        "duration_s": round(x.size / SAMPLE_RATE, 4),
        "rms": round(rms(x), 6),
        "peak": round(float(np.max(np.abs(x))), 6),
        # A broad-band proxy only: mechanical clicks also contribute here.
        "hiss_4_10k_share": round(high / max(useful, 1e-30), 4),
        "mid_1k2_4k_share": round(energy(1200, 4000) / max(useful, 1e-30), 4),
        "high_to_low_mid_db": round(10 * math.log10(max(high, 1e-30) / max(low_mid, 1e-30)), 2),
        "high_band_flatness": round(flatness, 4),
        "mid_band_flatness": round(mid_flatness, 4),
        "firing_harmonic_fraction_50_4k": round(harmonic_fraction, 4),
        "seam_jump_percentile": round(float(np.mean(differences <= seam_jump)), 4),
        "seam_jump_over_p95": round(seam_jump / max(float(np.percentile(differences, 95)), 1e-12), 4),
        "first_last_10ms_rms_db": round(seam_rms_db, 2),
    }
    measures.update(transient_metrics(x))
    measures.update(cycle_variation(x, rpm))
    return measures, phase_template(x, rpm)


def blend_rows(z: zipfile.ZipFile, path: str) -> dict[tuple[int, float], str]:
    blend = json.loads(z.read(path))
    if len(blend["samples"]) != 2:
        raise ValueError(f"Expected two load rows: {path}")
    return {
        (load, float(rpm)): wav_path
        for load, row in enumerate(blend["samples"])
        for wav_path, rpm in row
    }


def cylinder_count(z: zipfile.ZipFile, manifest: dict | None, override: int | None) -> int:
    if override is not None:
        return override
    engine_paths = [manifest["engine_path"]] if manifest and manifest.get("engine_path") else [
        name for name in z.namelist() if name.endswith(".jbeam")
    ]
    for path in engine_paths:
        source = z.read(path).decode("utf-8")
        # An imported JBeam may contain commented-out earlier values.
        matches = re.findall(r'"fundamentalFrequencyCylinderCount"\s*:\s*(\d+)', source)
        if matches:
            return int(matches[-1])
    raise ValueError("Cannot infer cylinders; pass --cylinders")


def nominal_main_gains(z: zipfile.ZipFile, manifest: dict | None) -> tuple[float, float]:
    """Return only JBeam mainGain in dB (other filters and spatial effects excluded)."""
    engine_paths = [manifest["engine_path"]] if manifest and manifest.get("engine_path") else [
        name for name in z.namelist() if name.endswith(".jbeam")
    ]
    for path in engine_paths:
        source = z.read(path).decode("utf-8")
        def gain(section: str) -> float | None:
            match = re.search(r'"' + section + r'"\s*:\s*\{([^}]*)\}', source)
            if not match:
                return None
            value = re.search(r'"mainGain"\s*:\s*(-?\d+(?:\.\d+)?)', match.group(1))
            return float(value.group(1)) if value else None
        engine_gain, exhaust_gain = gain("soundConfig"), gain("soundConfigExhaust")
        if engine_gain is not None and exhaust_gain is not None:
            return engine_gain, exhaust_gain
    return 0.0, 0.0


def pitch_match_loop(x: np.ndarray, sample_rpm: float, target_rpm: float, seconds: float = 4) -> np.ndarray:
    """Approximate BeamNG playback pitch while repeating at the WAV boundary."""
    position = (np.arange(int(seconds * SAMPLE_RATE)) * target_rpm / sample_rpm) % x.size
    left = position.astype(np.int32)
    fraction = position - left
    return x[left] * (1 - fraction) + x[(left + 1) % x.size] * fraction


def adjacent_blend(
    z: zipfile.ZipFile,
    sample_paths: dict[tuple[int, float], str],
    load: int,
    rpm_a: float,
    rpm_b: float,
    target_rpm: float,
    cylinders: int,
) -> dict[str, float]:
    sample_a = read_pcm24(z.read(sample_paths[(load, rpm_a)]))
    sample_b = read_pcm24(z.read(sample_paths[(load, rpm_b)]))
    a = pitch_match_loop(sample_a, rpm_a, target_rpm)
    b = pitch_match_loop(sample_b, rpm_b, target_rpm)
    fraction = (target_rpm - rpm_a) / (rpm_b - rpm_a)
    weights = (1 - fraction, fraction)
    endpoint_mean = weights[0] * rms(a) + weights[1] * rms(b)
    mixed = weights[0] * a + weights[1] * b
    cross = np.fft.irfft(np.fft.rfft(a) * np.conj(np.fft.rfft(b)), n=a.size) / a.size
    offsets = max(1, int(round(SAMPLE_RATE * 120 / (target_rpm * cylinders))))
    shifted_rms = np.sqrt(np.maximum(
        (weights[0] * rms(a)) ** 2 + (weights[1] * rms(b)) ** 2
        + 2 * weights[0] * weights[1] * cross[:offsets], 0
    ))

    def db(value: float) -> float:
        return round(20 * math.log10(max(value, 1e-12) / max(endpoint_mean, 1e-12)), 2)

    return {
        "target_rpm": target_rpm,
        "weight_a": round(weights[0], 4),
        "weight_b": round(weights[1], 4),
        "nominal_blend_vs_endpoint_rms_db": db(rms(mixed)),
        "best_offset_vs_endpoint_rms_db": db(float(np.max(shifted_rms))),
        "worst_offset_vs_endpoint_rms_db": db(float(np.min(shifted_rms))),
        "zero_offset_waveform_correlation": round(normalized_correlation(a, b), 4),
    }


def analyze_zip(path: Path, target_rpm: float, blend_rpm: float, override_cylinders: int | None) -> dict:
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        manifest_path = next((name for name in names if name.endswith("manifest.json")), None)
        manifest = json.loads(z.read(manifest_path)) if manifest_path else None
        if manifest:
            exhaust_blend, engine_blend = manifest["blend_path"], manifest["engine_blend_path"]
        else:
            blends = [name for name in names if name.endswith(".sfxBlend2D.json")]
            engine_blend = next(name for name in blends if "_ENGINE_" in name)
            exhaust_blend = next(name for name in blends if name != engine_blend)
        cylinders = cylinder_count(z, manifest, override_cylinders)
        engine_gain_db, exhaust_gain_db = nominal_main_gains(z, manifest)
        exhaust_rows = blend_rows(z, exhaust_blend)
        engine_rows = blend_rows(z, engine_blend)
        if exhaust_rows.keys() != engine_rows.keys():
            raise ValueError("Engine and exhaust RPM/load rows do not match")
        sample_results = []
        templates: dict[tuple[int, float, str], np.ndarray] = {}
        for (load, rpm), exhaust_path in sorted(exhaust_rows.items()):
            exhaust = read_pcm24(z.read(exhaust_path))
            engine = read_pcm24(z.read(engine_rows[(load, rpm)]))
            exhaust_metrics, exhaust_template = analyze_loop(exhaust, rpm, cylinders)
            engine_metrics, engine_template = analyze_loop(engine, rpm, cylinders)
            templates[(load, rpm, "exhaust")] = exhaust_template
            templates[(load, rpm, "engine")] = engine_template
            # BeamNG loops the stems independently. Compare over the longer
            # loop while repeating the shorter one at its own boundary.
            pair_frames = max(exhaust.size, engine.size)
            exhaust_pair = np.resize(exhaust, pair_frames)
            engine_pair = np.resize(engine, pair_frames)
            pair_rms = rms(engine_pair + exhaust_pair)
            incoherent_rms = math.hypot(rms(engine_pair), rms(exhaust_pair))
            cross = np.fft.irfft(
                np.fft.rfft(exhaust_pair) * np.conj(np.fft.rfft(engine_pair)), n=pair_frames
            ) / pair_frames
            phase_order = cylinders // 2
            ex_fft = np.fft.rfft(exhaust_template)[phase_order]
            en_fft = np.fft.rfft(engine_template)[phase_order]
            phase_difference = math.degrees(np.angle(en_fft * np.conj(ex_fft)))
            sample_results.append({
                "load": "off" if load == 0 else "on",
                "rpm": rpm,
                "exhaust": exhaust_metrics,
                "engine": engine_metrics,
                "pair": {
                    "waveform_correlation": round(normalized_correlation(exhaust_pair, engine_pair), 4),
                    "phase_template_correlation": round(normalized_correlation(exhaust_template, engine_template), 4),
                    "firing_phase_difference_degrees": round(phase_difference, 1),
                    "coherent_sum_db": round(20 * math.log10(max(pair_rms, 1e-12) / max(incoherent_rms, 1e-12)), 2),
                    "engine_to_exhaust_rms": round(rms(engine) / max(rms(exhaust), 1e-12), 4),
                    "nominal_main_gain_engine_to_exhaust_rms": round(
                        rms(engine) / max(rms(exhaust), 1e-12) * 10 ** ((engine_gain_db - exhaust_gain_db) / 20), 4
                    ),
                    "relative_phase_sweep": relative_phase_sweep(
                        exhaust_pair, engine_pair, rpm * cylinders / 120, cross=cross
                    ),
                    "nominal_main_gain_phase_sweep": relative_phase_sweep(
                        exhaust_pair, engine_pair, rpm * cylinders / 120,
                        exhaust_gain_db=exhaust_gain_db, engine_gain_db=engine_gain_db,
                        cross=cross,
                    ),
                },
            })
        phase_results = []
        for load in (0, 1):
            rpms = sorted(rpm for row, rpm in exhaust_rows if row == load)
            for stem in ("exhaust", "engine"):
                for left, right in zip(rpms, rpms[1:]):
                    phase_results.append({
                        "load": "off" if load == 0 else "on",
                        "stem": stem,
                        "rpm_a": left,
                        "rpm_b": right,
                        "cycle_template_correlation": round(
                            normalized_correlation(templates[(load, left, stem)], templates[(load, right, stem)]), 4
                        ),
                    })
        adjacent_levels = []
        sample_map = {(row["load"], row["rpm"]): row for row in sample_results}
        for load in ("off", "on"):
            rpms = sorted(rpm for row_load, rpm in sample_map if row_load == load)
            for left, right in zip(rpms, rpms[1:]):
                a, b = sample_map[(load, left)], sample_map[(load, right)]
                adjacent_levels.append({
                    "load": load,
                    "rpm_a": left,
                    "rpm_b": right,
                    "exhaust_rms_step_db": round(20 * math.log10(b["exhaust"]["rms"] / max(a["exhaust"]["rms"], 1e-12)), 2),
                    "engine_rms_step_db": round(20 * math.log10(b["engine"]["rms"] / max(a["engine"]["rms"], 1e-12)), 2),
                    "engine_exhaust_ratio_step_db": round(20 * math.log10(b["pair"]["engine_to_exhaust_rms"] / max(a["pair"]["engine_to_exhaust_rms"], 1e-12)), 2),
                })
        adjacent_blends = []
        rpms = sorted(rpm for load, rpm in exhaust_rows if load == 0)
        hi = bisect.bisect_right(rpms, blend_rpm)
        focus_pairs = [(rpms[i], rpms[i + 1]) for i in (hi - 1, hi) if 0 <= i < len(rpms) - 1]
        for rpm_a, rpm_b in focus_pairs:
            targets = [(rpm_a + rpm_b) / 2]
            if rpm_a <= blend_rpm <= rpm_b:
                targets.append(blend_rpm)
            for load in (0, 1):
                for stem, paths in (("exhaust", exhaust_rows), ("engine", engine_rows)):
                    for target in targets:
                        adjacent_blends.append({
                            "load": "off" if load == 0 else "on",
                            "stem": stem,
                            "rpm_a": rpm_a,
                            "rpm_b": rpm_b,
                            **adjacent_blend(z, paths, load, rpm_a, rpm_b, target, cylinders),
                        })
    selected = [
        min((row for row in sample_results if row["load"] == load), key=lambda row: abs(row["rpm"] - target_rpm))
        for load in ("off", "on")
    ]
    phase_summary = {}
    for load in ("off", "on"):
        for stem in ("exhaust", "engine"):
            values = [row["cycle_template_correlation"] for row in phase_results if row["load"] == load and row["stem"] == stem]
            phase_summary[f"{load}_{stem}"] = {
                "mean": round(float(np.mean(values)), 4),
                "negative_pairs": sum(value < 0 for value in values),
                "pairs": len(values),
            }
    return {
        "archive": str(path),
        "cylinders": cylinders,
        "nominal_jbeam_main_gain_db": {"engine": engine_gain_db, "exhaust": exhaust_gain_db},
        "sample_count": len(sample_results) * 2,
        "near_target_rpm": selected,
        "phase_summary": phase_summary,
        "samples": sample_results,
        "adjacent_rpm_phase": phase_results,
        "adjacent_rpm_levels": adjacent_levels,
        "adjacent_rpm_blends": adjacent_blends,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="+", type=Path)
    parser.add_argument("--rpm", type=float, default=5200, help="RPM for compact terminal summary")
    parser.add_argument("--blend-rpm", type=float, default=5200, help="Inspect adjacent RPM blends around this RPM")
    parser.add_argument("--cylinders", type=int, help="Override JBeam cylinder count")
    parser.add_argument("--output", type=Path, help="Write complete JSON diagnostics")
    args = parser.parse_args()
    result = {
        "method": "PCM24 diagnostics. Pair phase sweeps repeat shorter stems at their own loop boundaries, then apply optional JBeam mainGain only. Adjacent-RPM blends are four-second pitch-matched sample approximations with both loops starting at their first frame; BeamNG mixing, occlusion, and room acoustics are not modeled. Metrics do not imply perceived quality.",
        "archives": [analyze_zip(path, args.rpm, args.blend_rpm, args.cylinders) for path in args.archives],
    }
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(f"Wrote {args.output}")
    for archive in result["archives"]:
        print(f"{archive['archive']}: {archive['sample_count']} samples, {archive['cylinders']} cylinders")
        for row in archive["near_target_rpm"]:
            e = row["engine"]
            pair = row["pair"]
            print(
                f"  {row['load']:>3} {row['rpm']:g} rpm: engine hiss={e['hiss_4_10k_share']:.3f}, "
                f"harmonic={e['firing_harmonic_fraction_50_4k']:.3f}, "
                f"cycle-CV={e['cycle_rms_cv']:.3f}, bursts={e['burst_rate_hz']:.1f}/s, "
                f"engine/exhaust={pair['engine_to_exhaust_rms']:.3f}, "
                f"pair phase={pair['firing_phase_difference_degrees']:+.0f} degrees"
            )


if __name__ == "__main__":
    main()
