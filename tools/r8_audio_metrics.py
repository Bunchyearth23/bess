"""Compare BESS audio renders without changing the project or vehicle mods.

Example:
  python tools/r8_audio_metrics.py --source source.wav --r7 r7.wav \
      --r8 r8.wav --output output/r8-audio-metrics.json

Requires NumPy. Band shares use the AC power from 80-10,000 Hz as their
denominator; 1-4 kHz intentionally overlaps the other bands. A dynamic RPM
render naturally changes 50 ms level, so those values are diagnostics rather
than a pumping or perceived-quality score. Correlation uses the common initial
duration and finds the maximum absolute coefficient within +/-50 ms by default.
It cannot establish that either recording sounds realistic.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
from pathlib import Path

import numpy as np


BANDS_HZ = [(80, 500), (500, 2000), (2000, 5000), (1000, 4000)]
ANALYSIS_BAND_HZ = (80, 10000)


def read_wav(path: Path) -> tuple[int, np.ndarray]:
    """Decode common uncompressed mono/stereo PCM or 32-bit float RIFF WAV."""
    data = path.read_bytes()
    if len(data) < 44 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError(f"Not a RIFF/WAVE file: {path}")
    offset = 12
    fmt = None
    format_extension = None
    payload = None
    while offset + 8 <= len(data):
        kind = data[offset : offset + 4]
        size = struct.unpack_from("<I", data, offset + 4)[0]
        end = offset + 8 + size
        if end > len(data):
            raise ValueError(f"Truncated WAV chunk: {path}")
        chunk = data[offset + 8 : end]
        if kind == b"fmt ":
            if len(chunk) < 16:
                raise ValueError(f"Invalid WAV format chunk: {path}")
            fmt = struct.unpack_from("<HHIIHH", chunk)
            format_extension = chunk
        elif kind == b"data":
            payload = chunk
        offset = end + size % 2
    if fmt is None or payload is None:
        raise ValueError(f"Missing WAV format or audio chunk: {path}")
    encoding, channels, rate, _, block_align, bits = fmt
    if encoding == 65534:  # WAVE_FORMAT_EXTENSIBLE
        if format_extension is None or len(format_extension) < 40:
            raise ValueError(f"Incomplete extensible WAV format: {path}")
        subtype = format_extension[24:40]
        canonical_tail = bytes.fromhex("00001000800000aa00389b71")
        if subtype[4:] != canonical_tail:
            raise ValueError(f"Unsupported extensible WAV subtype: {path}")
        encoding = struct.unpack_from("<H", subtype)[0]
    if channels < 1 or channels > 2 or rate < 8000 or block_align == 0:
        raise ValueError(f"Unsupported WAV channel count or sample rate: {path}")
    if len(payload) % block_align:
        raise ValueError(f"Incomplete WAV frame: {path}")
    if encoding == 3 and bits == 32:
        values = np.frombuffer(payload, dtype="<f4").astype(np.float64)
    elif encoding == 1 and bits == 16:
        values = np.frombuffer(payload, dtype="<i2").astype(np.float64) / 32768
    elif encoding == 1 and bits == 24:
        bytes3 = np.frombuffer(payload, dtype=np.uint8).reshape(-1, 3)
        values = (
            bytes3[:, 0].astype(np.int32)
            | (bytes3[:, 1].astype(np.int32) << 8)
            | (bytes3[:, 2].astype(np.int32) << 16)
        )
        values[values >= 0x800000] -= 0x1000000
        values = values.astype(np.float64) / 8388608
    elif encoding == 1 and bits == 32:
        values = np.frombuffer(payload, dtype="<i4").astype(np.float64) / 2147483648
    else:
        raise ValueError(f"Unsupported WAV encoding={encoding}, bits={bits}: {path}")
    if values.size % channels:
        raise ValueError(f"Invalid WAV frame alignment: {path}")
    samples = values.reshape(-1, channels).mean(axis=1)
    if samples.size < 2048 or not np.all(np.isfinite(samples)):
        raise ValueError(f"WAV is too short or has non-finite samples: {path}")
    return rate, samples


def db(value: float) -> float:
    return 20 * math.log10(max(value, 1e-15))


def spectrum_power(samples: np.ndarray, rate: int) -> tuple[np.ndarray, np.ndarray]:
    """Average 16,384-sample Hann-windowed periodograms, 50% overlap."""
    width = min(16384, 1 << (samples.size.bit_length() - 1))
    hop = width // 2
    window = np.hanning(width)
    total = np.zeros(width // 2 + 1, dtype=np.float64)
    count = 0
    for start in range(0, samples.size - width + 1, hop):
        frame = samples[start : start + width]
        spectrum = np.fft.rfft((frame - frame.mean()) * window)
        total += np.abs(spectrum) ** 2
        count += 1
    return np.fft.rfftfreq(width, 1 / rate), total / count


def metrics(path: Path) -> tuple[int, np.ndarray, dict]:
    rate, samples = read_wav(path)
    ac = samples - samples.mean()
    ac_rms = float(np.sqrt(np.mean(ac * ac)))
    frequencies, power = spectrum_power(samples, rate)
    denominator = float(
        power[(frequencies >= ANALYSIS_BAND_HZ[0]) & (frequencies < ANALYSIS_BAND_HZ[1])].sum()
    )
    shares = {}
    for low, high in BANDS_HZ:
        numerator = float(power[(frequencies >= low) & (frequencies < high)].sum())
        shares[f"{low}-{high}"] = round(numerator / max(denominator, 1e-30), 6)

    frame_size = min(round(rate * 0.05), samples.size)
    window_count = samples.size // frame_size
    frames = ac[: window_count * frame_size].reshape(window_count, frame_size)
    window_rms = np.sqrt(np.mean(frames * frames, axis=1))
    rms_p5 = float(np.percentile(window_rms, 5))
    rms_p95 = float(np.percentile(window_rms, 95))
    result = {
        "path": str(path.resolve()),
        "sample_rate_hz": rate,
        "duration_s": round(samples.size / rate, 4),
        "mean": round(float(samples.mean()), 8),
        "ac_rms": round(ac_rms, 8),
        "ac_rms_dbfs": round(db(ac_rms), 3),
        "absolute_peak": round(float(np.max(np.abs(samples))), 8),
        "band_power_share_of_80_10000_hz": shares,
        "rms_50ms": {
            "window_count": window_count,
            "cv": round(float(window_rms.std() / max(window_rms.mean(), 1e-15)), 5),
            "p5_dbfs": round(db(rms_p5), 3),
            "p95_dbfs": round(db(rms_p95), 3),
            "p95_minus_p5_db": round(db(rms_p95) - db(rms_p5), 3),
        },
    }
    return rate, samples, result


def aligned_correlation(
    reference: np.ndarray, candidate: np.ndarray, rate: int, max_lag_ms: float
) -> dict:
    count = min(reference.size, candidate.size)
    a = reference[:count] - reference[:count].mean()
    b = candidate[:count] - candidate[:count].mean()
    max_lag = min(round(rate * max_lag_ms / 1000), count - 1)
    size = 1 << (2 * count - 1).bit_length()
    circular = np.fft.irfft(
        np.conj(np.fft.rfft(a, n=size)) * np.fft.rfft(b, n=size), n=size
    )
    a2 = np.concatenate(([0.0], np.cumsum(a * a)))
    b2 = np.concatenate(([0.0], np.cumsum(b * b)))
    lag = np.arange(-max_lag, max_lag + 1)
    numerator = circular[lag % size]
    a_start = np.maximum(0, -lag)
    a_end = np.minimum(count, count - lag)
    b_start = np.maximum(0, lag)
    b_end = np.minimum(count, count + lag)
    denominator = np.sqrt(
        np.maximum(a2[a_end] - a2[a_start], 0)
        * np.maximum(b2[b_end] - b2[b_start], 0)
    )
    correlation = numerator / np.maximum(denominator, 1e-30)
    best = int(np.argmax(np.abs(correlation)))
    return {
        "common_duration_s": round(count / rate, 4),
        "zero_lag": round(float(correlation[max_lag]), 5),
        "best_signed": round(float(correlation[best]), 5),
        "best_absolute": round(float(abs(correlation[best])), 5),
        "best_lag_ms": round(float(lag[best] * 1000 / rate), 4),
        "search_limit_ms": max_lag_ms,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--r7", required=True, type=Path, help="R7 WAV render")
    parser.add_argument("--r8", required=True, type=Path, help="R8 WAV render")
    parser.add_argument("--source", type=Path, help="Optional Automation reference WAV")
    parser.add_argument("--output", type=Path, help="Optional JSON output path")
    parser.add_argument("--max-lag-ms", type=float, default=50)
    args = parser.parse_args()
    if not 0 <= args.max_lag_ms <= 1000:
        parser.error("--max-lag-ms must be between 0 and 1000")

    items = {"r7": args.r7, "r8": args.r8}
    if args.source is not None:
        items["source"] = args.source
    loaded = {label: metrics(path) for label, path in items.items()}
    result = {
        "method": "AC RMS. Hann-windowed band power; shares use 80-10000 Hz. Non-overlapping 50 ms RMS windows. Correlation searches +/- max-lag-ms on common initial duration; it does not measure realism.",
        "files": {label: item[2] for label, item in loaded.items()},
        "r8_minus_r7_ac_rms_db": round(
            db(loaded["r8"][2]["ac_rms"]) - db(loaded["r7"][2]["ac_rms"]), 3
        ),
        "correlation": {},
    }
    for first, second in (("source", "r7"), ("source", "r8"), ("r7", "r8")):
        if first not in loaded:
            continue
        a_rate, a_samples, _ = loaded[first]
        b_rate, b_samples, _ = loaded[second]
        if a_rate != b_rate:
            result["correlation"][f"{first}_vs_{second}"] = {
                "unavailable": "WAV sample rates differ; compare equal-rate renders."
            }
        else:
            result["correlation"][f"{first}_vs_{second}"] = aligned_correlation(
                a_samples, b_samples, a_rate, args.max_lag_ms
            )
    serialized = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(serialized, encoding="utf-8")
    print(serialized, end="")


if __name__ == "__main__":
    main()
