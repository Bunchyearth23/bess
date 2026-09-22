"""Read-only analysis of the user's Automation archive."""
import io, json, zipfile, pathlib, struct
import numpy as np

root = pathlib.Path(__file__).resolve().parents[1]
archive = root / 'bunchyearth23_thunderhawk_zero.zip'
rows = []
with zipfile.ZipFile(archive) as z:
    blend_name = next(n for n in z.namelist() if n.endswith('.sfxBlend2D.json'))
    blend = json.loads(z.read(blend_name))
    for layer, samples in enumerate(blend['samples']):
        for name, rpm in samples:
            raw=z.read(name); offset=12; chunks={}
            while offset+8<=len(raw):
                key=raw[offset:offset+4]; size=struct.unpack_from('<I',raw,offset+4)[0]
                chunks[key]=raw[offset+8:offset+8+size]; offset+=8+size+(size%2)
            fmt,channels,rate=struct.unpack_from('<HHI',chunks[b'fmt ']); assert fmt==3
            audio=np.frombuffer(chunks[b'data'],dtype='<f4').astype(float).reshape(-1,channels).mean(axis=1)
            spectrum = abs(np.fft.rfft(audio * np.hanning(len(audio))))
            freq = np.fft.rfftfreq(len(audio), 1/rate)
            indices = np.where((spectrum[1:-1] > spectrum[:-2]) & (spectrum[1:-1] > spectrum[2:]))[0]+1
            indices = sorted(indices, key=lambda i:spectrum[i], reverse=True)[:6]
            rows.append(dict(layer=layer, name=name, rpm=rpm, rate=rate, frames=len(audio),
                rms=float(np.sqrt(np.mean(audio**2))), peak=float(max(abs(audio))),
                dc=float(audio.mean()), seam=float(abs(audio[-1]-audio[0])),
                peaks_hz=[round(float(freq[i]),2) for i in indices]))
result=dict(blend=blend_name, rows=rows)
(root/'docs/reports/automation-source-analysis.json').write_text(json.dumps(result,indent=2),encoding='utf8')
print(json.dumps(dict(layers=len(blend['samples']), counts=[len(x) for x in blend['samples']],
    rpm=[min(x['rpm'] for x in rows),max(x['rpm'] for x in rows)],
    selected=[x for x in rows if x['rpm'] in (803,1690,3324,4989)]),indent=2))
