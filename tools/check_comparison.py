"""Validate exported PCM and compare equal-level audition renders."""
import pathlib,wave,json
import numpy as np
root=pathlib.Path(__file__).resolve().parents[1]
signals=[];reports=[]
folder = root / 'output/hybrid-ab'
enhanced = sorted(folder.glob('02-*.wav'))
assert len(enhanced) == 1
for path in (folder / '01-source-automation.wav', enhanced[0]):
    name = path.name
    with wave.open(str(path)) as w:
        assert w.getsampwidth()==3 and w.getframerate()==48000 and w.getnchannels()==1
        raw=np.frombuffer(w.readframes(w.getnframes()),dtype=np.uint8).reshape(-1,3).astype(np.int32)
        values=raw[:,0]+(raw[:,1]<<8)+(raw[:,2]<<16)
        values=np.where(values>=8388608,values-16777216,values)/8388608.
        signals.append(values)
        segments=[float(np.sqrt(np.mean(x*x))) for x in np.array_split(values,16)]
        reports.append(dict(name=name,frames=len(values),rms=float(np.sqrt(np.mean(values**2))),
            peak=float(max(abs(values))),max_step=float(max(abs(np.diff(values)))),segment_rms=segments))
delta=signals[0]-signals[1]
result=dict(files=reports,rms_difference_db=float(20*np.log10(reports[0]['rms']/reports[1]['rms'])),
    audio_delta_rms=float(np.sqrt(np.mean(delta**2))))
assert abs(result['rms_difference_db'])<0.01
assert all(x['peak']<0.96 and x['frames']==16*48000 for x in reports)
(root/'docs/reports/hybrid-comparison-metrics.json').write_text(json.dumps(result,indent=2),encoding='utf8')
print(json.dumps(result,indent=2))
