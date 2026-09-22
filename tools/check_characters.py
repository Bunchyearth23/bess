"""Validate the real-vehicle 0.3 listening set without judging subjective realism."""
import json
import pathlib
import sys
import wave
import numpy as np

root = pathlib.Path(__file__).resolve().parents[1]
folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'output/acoustics-0.3'
signals = []
reports = []
for prefix in ['00-', '01-', '02-', '03-']:
    candidates = sorted(folder.glob(f'{prefix}*.wav'))
    assert len(candidates) == 1, (folder, prefix)
    path = candidates[0]
    name = path.stem
    with wave.open(str(path)) as wav:
        assert (wav.getsampwidth(), wav.getframerate(), wav.getnchannels()) == (3, 48000, 1)
        raw = np.frombuffer(wav.readframes(wav.getnframes()), dtype=np.uint8).reshape(-1, 3).astype(np.int32)
    value = raw[:, 0] + (raw[:, 1] << 8) + (raw[:, 2] << 16)
    value = np.where(value >= 8388608, value - 16777216, value) / 8388608.
    assert len(value) == 16 * 48000 and np.isfinite(value).all()
    rms = np.sqrt(np.mean(value ** 2))
    peak = np.max(np.abs(value))
    assert peak < .96 and rms > .0001
    # A steady full-load window avoids conflating transient loudness with timbre.
    segment = value[6 * 48000:8 * 48000]
    blocks = segment[:len(segment) // 2048 * 2048].reshape(-1, 2048)
    spectrum = np.mean(abs(np.fft.rfft(blocks * np.hanning(2048))) ** 2, axis=0)
    frequencies = np.fft.rfftfreq(2048, 1 / 48000)
    report = dict(name=name, frames=len(value), rms=float(rms), peak=float(peak),
                  max_step=float(np.max(np.abs(np.diff(value)))),
                  mean=float(np.mean(value)),
                  high_band_fraction=float(spectrum[frequencies >= 2000].sum() / spectrum.sum()),
                  segment_rms=[float(np.sqrt(np.mean(s*s))) for s in np.array_split(value, 16)])
    if signals:
        report['level_difference_db'] = float(20 * np.log10(rms / reports[0]['rms']))
        assert abs(report['level_difference_db']) < .01
        report['difference_from_source_rms'] = float(np.sqrt(np.mean((value-signals[0]) ** 2)))
        assert report['difference_from_source_rms'] > .01 * rms
    signals.append(value)
    reports.append(report)

result = dict(files=reports, pairwise_difference_rms={
    f'{reports[i]["name"]}/{reports[j]["name"]}': float(np.sqrt(np.mean((signals[i]-signals[j]) ** 2)))
    for i in range(4) for j in range(i+1, 4)
})
target = root / 'docs/reports/acoustics-0.3-metrics.json'
target.write_text(json.dumps(result, indent=2), encoding='utf8')
for r in reports:
    print(f'{r["name"]}: RMS={r["rms"]:.8f}, peak={r["peak"]:.5f}, high band={r["high_band_fraction"]:.4f}')
print('PCM, finite signal, duration, equal RMS and distinct signals: PASS')
