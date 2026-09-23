"""Measure settled level and broad high-frequency energy in mono PCM24 WAVs."""
import json
import math
import sys
import wave


def measure(path):
    with wave.open(path, "rb") as source:
        if source.getnchannels() != 1 or source.getsampwidth() != 3:
            raise ValueError("expected mono PCM24")
        rate = source.getframerate()
        raw = source.readframes(source.getnframes())
    count = len(raw) // 3
    skip = min(rate, count // 4)
    cutoffs = (1000., 2000., 4000.)
    smooth = [math.exp(-2 * math.pi * hz / rate) for hz in cutoffs]
    lows = [0.] * len(cutoffs)
    total = 0.
    highs = [0.] * len(cutoffs)
    peak = 0.
    for i in range(count):
        x = int.from_bytes(raw[3 * i:3 * i + 3], "little", signed=True) / 8388608.
        for j, alpha in enumerate(smooth):
            lows[j] = alpha * lows[j] + (1. - alpha) * x
        if i >= skip:
            total += x * x
            peak = max(peak, abs(x))
            for j, low in enumerate(lows):
                highs[j] += (x - low) ** 2
    n = count - skip
    rms = math.sqrt(total / n)
    return {
        "file": path, "rms": round(rms, 7), "peak": round(peak, 7),
        "highpass_rms": {str(int(hz)): round(math.sqrt(power / n), 7)
                         for hz, power in zip(cutoffs, highs)},
    }


print(json.dumps([measure(path) for path in sys.argv[1:]], indent=2))
