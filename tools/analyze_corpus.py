"""Read-only periodicity diagnostic. Local correlation is not cylinder identification."""
import json
import pathlib
import struct
import sys
import zipfile
import numpy as np


def decode(raw):
    chunks = {}
    offset = 12
    while offset + 8 <= len(raw):
        key = raw[offset:offset + 4]
        size = struct.unpack_from('<I', raw, offset + 4)[0]
        chunks[key] = raw[offset + 8:offset + 8 + size]
        offset += 8 + size + size % 2
    fmt, channels, rate = struct.unpack_from('<HHI', chunks[b'fmt '])
    if fmt != 3:
        raise ValueError(f'Analysis only supports float WAV, got {fmt}')
    x = np.frombuffer(chunks[b'data'], '<f4').astype(float).reshape(-1, channels).mean(axis=1)
    if not np.isfinite(x).all():
        raise ValueError('Non-finite samples')
    return rate, x - x.mean()


def periodicity(x, rate, rpm):
    nominal = rate * 120 / rpm
    n = len(x)
    fft_size = 1 << (2*n-1).bit_length()
    f = np.fft.rfft(x, fft_size)
    corr = np.fft.irfft(f * f.conj(), fft_size)
    energy = np.r_[0., np.cumsum(x*x)]
    low, high = max(1, int(nominal*.97)), min(n-2, int(nominal*1.03)+1)
    lags = np.arange(low-1, high+2)
    denom = np.sqrt(energy[n-lags]*(energy[n]-energy[lags]))
    scores = corr[lags] / np.maximum(denom, 1e-20)
    k = 1 + np.argmax(scores[1:-1])
    a,b,c = scores[k-1:k+2]
    delta = .5*(a-c)/(a-2*b+c) if abs(a-2*b+c)>1e-12 else 0.
    lag = float(lags[k]+np.clip(delta,-.5,.5))
    nominal_score = float(np.interp(nominal,lags,scores))
    return dict(nominal_period=nominal, fitted_period=lag,
                nominal_correlation=nominal_score, fitted_correlation=float(b),
                change_percent=100*(lag/nominal-1), boundary=bool(k==1 or k==len(scores)-2))


root = pathlib.Path(__file__).resolve().parents[1]
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding='utf-8'))
result = []
for vehicle in report['vehicles']:
    if vehicle['status'] != 'ok':
        continue
    rows = []
    with zipfile.ZipFile(root/'cars'/vehicle['archive']) as archive:
        blend = json.loads(archive.read(vehicle['source']['blend']))
        for layer, samples in enumerate(blend['samples']):
            for name, rpm in samples:
                if archive.getinfo(name).file_size > 16_000_000:
                    raise ValueError('WAV too large')
                rate,x = decode(archive.read(name))
                rows.append(dict(layer=layer,name=name,rpm=rpm,**periodicity(x,rate,rpm)))
    reliable = [r for r in rows if not r['boundary'] and r['fitted_correlation']>=.65]
    summary = dict(archive=vehicle['archive'],samples=len(rows),
                   nominal_median=float(np.median([r['nominal_correlation'] for r in rows])),
                   fitted_median=float(np.median([r['fitted_correlation'] for r in rows])),
                   locally_reliable=len(reliable),
                   median_abs_change_percent=float(np.median([abs(r['change_percent']) for r in reliable])) if reliable else None)
    print(json.dumps(summary),flush=True)
    result.append(dict(**summary,rows=rows))
path = root/'docs/reports/corpus-periodicity.json'
path.write_text(json.dumps(dict(method='Normalized autocorrelation near nominal 720-degree period, +/-3%; diagnostic only',vehicles=result),indent=2),encoding='utf-8')
