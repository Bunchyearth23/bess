"""Build a local listening page from the real Rust importer/render audit."""
import html
import json
import pathlib
import sys
import shutil

folder = pathlib.Path(sys.argv[1]).resolve()
report = json.loads((folder / "corpus.json").read_text(encoding="utf-8"))
rows = report["vehicles"]
good = [row for row in rows if row["status"] == "ok"]
baseline = pathlib.Path(sys.argv[2]).resolve() if len(sys.argv)>2 else None

def previous_render(row):
    """Find the single B render from a prior corpus, independent of its filename."""
    matches = list((baseline / row['folder']).glob('02-*.wav'))
    assert len(matches) == 1, (row['folder'], matches)
    return matches[0]
same_as_baseline = False
if baseline:
    old_report = json.loads((baseline / 'corpus.json').read_text(encoding='utf-8'))
    old_rows = {r['archive']: r for r in old_report['vehicles']}
    for row in good:
        oldrow = old_rows[row['archive']]
        assert oldrow['source']['fingerprint'] == row['source']['fingerprint']
        assert (baseline / oldrow['folder'] / '01-source-automation.wav').read_bytes() == (folder / row['folder'] / '01-source-automation.wav').read_bytes(), 'Source A changed'
    same_as_baseline = all(
        previous_render(old_rows[row['archive']]).read_bytes()
        == (folder / row['folder'] / '02-bess-enhanced.wav').read_bytes()
        for row in good
    )
    if same_as_baseline:
        for row in good:
            duplicate = folder / row['folder'] / '03-bess-previous.wav'
            if duplicate.exists():
                assert duplicate.read_bytes() == (folder / row['folder'] / '02-bess-enhanced.wav').read_bytes()
                duplicate.unlink()
        baseline = None
cards = []
for row in rows:
    name = html.escape(row.get("vehicle", row["archive"]).replace("_", " "))
    if row["status"] != "ok":
        cards.append(f'<article><h2>{name}</h2><p>Import failed: {html.escape(row["error"])}</p></article>')
        continue
    path = html.escape(row["folder"], quote=True)
    extra = ''
    if baseline:
        old = old_report
        oldrow = old_rows[row['archive']]
        shutil.copyfile(previous_render(oldrow), folder/row['folder']/'03-bess-previous.wav')
        extra = f'<button data-file="03-bess-previous.wav" aria-pressed="false">C · BESS {html.escape(old["version"])}</button>'
    diagnostic = ''
    if (folder/row['folder']/'components/components.json').exists():
        diagnostic = '<details><summary>Listen to isolated components</summary><p class="muted">A separate scenario without enrichment. The same gain is retained, so textures may sound quieter.</p><div class="switch">' + ''.join(f'<button data-file="components/{file}.wav" aria-pressed="false">{label}</button>' for file,label in [('01-original-playback','Original playback'),('02-measured-cycles','Measured cycles'),('03-pulses','Pulses only'),('04-texture','Texture only')])+'</div></details>'
    cards.append(f'''<article data-folder="{path}">
      <div class="cardhead"><h2>{name}</h2><span>{row['min_rpm']:.0f}–{row['max_rpm']:.0f} RPM</span></div>
      <p class="muted">{len(row['layers'][0])} off-load RPM points · {len(row['layers'][1])} on-load RPM points · 16 seconds</p>
      <div class="switch"><button class="selected" data-file="01-source-automation.wav" aria-pressed="true">A · Automation source</button><button data-file="02-bess-enhanced.wav" aria-pressed="false">B · BESS {html.escape(report['version'])}</button>{extra}</div>
      <audio controls preload="none" src="{path}/01-source-automation.wav"></audio>
      <p class="status" aria-live="polite">Source selected</p>
      {diagnostic}
      <div class="links"><a href="{path}/source.bess.json" download>Project A</a><a href="{path}/bess.bess.json" download>Project B</a><a href="{path}/comparison.txt">Measurements</a></div>
      <details><summary>Provenance</summary><p>{html.escape(row['archive'])}</p><code>{row['source']['fingerprint']}</code></details>
    </article>''')
page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>BESS · Engine listening references</title><style>
*{box-sizing:border-box}body{margin:0;background:#10151c;color:#e5edf7;font:16px/1.5 system-ui,sans-serif}main{max-width:1200px;margin:auto;padding:40px 24px}h1{font-size:38px;line-height:1.15;margin:12px 0}h2{font-size:20px;margin:0;text-transform:capitalize}.eyebrow{color:#7adebb;letter-spacing:.1em;font-size:13px}.intro{max-width:820px;color:#afbdcc}.summary{display:flex;gap:14px;flex-wrap:wrap;margin:24px 0}.summary span{background:#1d2b38;border:1px solid #304456;padding:8px 14px;border-radius:30px}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(340px,1fr));gap:18px}article{background:#19222d;border:1px solid #304050;border-radius:14px;padding:22px}.cardhead{display:flex;justify-content:space-between;gap:10px;align-items:baseline}.cardhead span{font-size:13px;white-space:nowrap;color:#7adebb}.muted,details{font-size:13px;color:#a5b5c6}.switch{display:flex;gap:8px;margin:18px 0 12px}button{border:1px solid #4e6072;border-radius:8px;padding:10px;background:#202e3c;color:#e5edf7;cursor:pointer;font:inherit;font-size:14px}button.selected{background:#8ce9c7;color:#0f2d26;border-color:#8ce9c7}button:focus-visible,a:focus-visible{outline:3px solid #ffcf76;outline-offset:3px}audio{width:100%}.status{font-size:13px;min-height:20px;color:#a5b5c6}.links{display:flex;gap:20px}a{color:#a9d8ff;font-size:14px}details{margin-top:18px}code{word-break:break-all}footer{margin-top:32px;color:#a5b5c6;font-size:14px}@media(max-width:400px){.grid{grid-template-columns:1fr}.cardhead{display:block}}
</style><main><div class="eyebrow">BESS / SOUND COMPARISON</div><h1>Compare the engine sounds</h1>
<p class="intro">Choose a vehicle and switch between source A, the current BESS render B, and previous render C when available. Playback resumes at the same position.</p>
<div class="summary">SUMMARY</div><p class="intro">Each comparison follows the same sequence: idle, rev-up, full load, and lift-off. RMS levels are matched within each pair; initial playback volume is 35%. A is a BESS playback of the source WAVs, not an Automation game capture. Switching files may cause a short interruption.</p>
<div class="grid">CARDS</div><footer>To adjust settings live, open BESS, select “Open project,” and choose a vehicle's A or B project. Projects refer to the local ZIP files; the comparison WAVs have additional final level matching.<br>VERSION · <a href="corpus.json">Full report</a> · User listening assessment is still pending. No BeamNG sound result is claimed.</footer></main>
<script>
document.querySelectorAll('article[data-folder]').forEach(card=>{
 const audio=card.querySelector('audio'),status=card.querySelector('.status');audio.volume=.35;
 let generation=0;
 audio.addEventListener('play',()=>document.querySelectorAll('audio').forEach(other=>{if(other!==audio)other.pause()}));
 audio.addEventListener('error',()=>{status.textContent='Playback unavailable: check that the WAV files are present.'});
 card.querySelectorAll('button').forEach(button=>button.addEventListener('click',()=>{
   if(button.classList.contains('selected'))return;
   const time=Number.isFinite(audio.currentTime)?audio.currentTime:0, playing=!audio.paused, token=++generation;
   audio.pause();
   card.querySelectorAll('button').forEach(b=>{b.classList.toggle('selected',b===button);b.setAttribute('aria-pressed',String(b===button))});
   audio.onloadedmetadata=()=>{if(token!==generation)return;audio.currentTime=Math.min(time,Math.max(0,audio.duration-.001));if(playing)audio.play().catch(()=>{status.textContent='Press Play to continue.'})};
   audio.src=card.dataset.folder+'/'+button.dataset.file;
   audio.load();status.textContent=button.textContent+' selected';
 }));
});
</script></html>'''
summary = f'<span>{len(good)} / {len(rows)} successful imports</span><span>{len(set(r["audio_content_sha256"] for r in good))} distinct WAV banks</span><span>{len(good)*2} comparison tracks</span>'
page = page.replace("SUMMARY", summary).replace("CARDS", "\n".join(cards)).replace("VERSION", "BESS " + html.escape(report["version"]))
page = page.replace('.switch{display:flex;', '.switch{display:flex;flex-wrap:wrap;')
if baseline:
    page = page.replace(f'{len(good)*2} comparison tracks', f'{len(good)*3} comparison tracks')
(folder / "index.html").write_text(page, encoding="utf-8")
print(folder / "index.html")
