"""Inspect the generated real-source driving examples and transport telemetry."""
import csv
import json
import pathlib
import wave
import numpy as np

root = pathlib.Path(__file__).resolve().parents[1]
folder = root / 'output/drive-0.4'
results = []
signals = []
for prefix in ['01-', '02-', '03-']:
    candidates = sorted(folder.glob(f'{prefix}*.wav'))
    assert len(candidates) == 1, (folder, prefix)
    name = candidates[0].stem
    with wave.open(str(candidates[0])) as wav:
        assert (wav.getsampwidth(), wav.getframerate(), wav.getnchannels(), wav.getnframes()) == (3, 48000, 1, 768000)
        raw = np.frombuffer(wav.readframes(wav.getnframes()), dtype=np.uint8).reshape(-1, 3).astype(np.int32)
    value = raw[:, 0] + (raw[:, 1] << 8) + (raw[:, 2] << 16)
    value = np.where(value >= 8388608, value - 16777216, value) / 8388608.
    assert np.isfinite(value).all() and np.max(np.abs(value)) < .96
    assert np.sqrt(np.mean(value ** 2)) > .001
    with (folder / f'{name}.csv').open() as file:
        rows = list(csv.DictReader(file))
    assert len(rows) == 320
    assert all(802.99 <= float(r['rpm']) <= 4989.01 and 0 <= float(r['load']) <= 1 for r in rows)
    gears = [int(r['gear']) for r in rows]
    result = dict(name=name, rms=float(np.sqrt(np.mean(value ** 2))), peak=float(np.max(np.abs(value))),
                  max_step=float(np.max(np.abs(np.diff(value)))), final_speed=float(rows[-1]['speed_kmh']),
                  final_gear=gears[-1], shifts=sum(a != b for a, b in zip(gears, gears[1:])),
                  rpm_range=[min(float(r['rpm']) for r in rows), max(float(r['rpm']) for r in rows)])
    results.append(result)
    signals.append(value)
    print(result)
assert results[0]['shifts'] >= 2
assert results[0]['final_speed'] > results[1]['final_speed'] + 10
assert results[2]['final_speed'] == 0 and results[2]['final_gear'] == 0
assert np.sqrt(np.mean((signals[0] - signals[1]) ** 2)) > .005
(root / 'docs/reports/drive-metrics.json').write_text(json.dumps(results, indent=2), encoding='utf8')
print('PCM, signal, exported RPM limits, shifts, output resistance and neutral: PASS')
