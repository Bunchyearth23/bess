"""Build a local acceptance desk from fresh shared-engine demonstration files.

Original files are kept intact. Blind comparisons use separately level-matched
copies; the ordinary audition section links to unmodified demonstration WAVs.
Requires numpy. No network access or listener results are fabricated.
"""
from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import wave

import numpy as np


def read_wave(path: Path) -> tuple[int, np.ndarray]:
    with wave.open(str(path), "rb") as wav:
        if wav.getnchannels() != 1 or wav.getsampwidth() != 3:
            raise ValueError(f"Expected mono PCM24: {path}")
        rate = wav.getframerate()
        b = np.frombuffer(wav.readframes(wav.getnframes()), dtype=np.uint8).reshape(-1, 3)
    n = b[:, 0].astype(np.int32) | b[:, 1].astype(np.int32) << 8 | b[:, 2].astype(np.int32) << 16
    n = (n ^ 0x800000) - 0x800000
    return rate, n.astype(np.float64) / 8388608


def write_wave(path: Path, rate: int, samples: np.ndarray) -> None:
    if not np.isfinite(samples).all() or np.max(np.abs(samples)) >= 1:
        raise ValueError("Invalid blind-test audio")
    n = np.rint(samples * 8388607).astype(np.int32)
    b = np.column_stack((n & 255, n >> 8 & 255, n >> 16 & 255)).astype(np.uint8)
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(3)
        wav.setframerate(rate)
        wav.writeframes(b.tobytes())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("workshop", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--advanced", type=Path)
    parser.add_argument("--quality", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    assets = args.output / "blind"
    assets.mkdir(exist_ok=True)
    document = json.loads((args.workshop / "validation.json").read_text(encoding="utf-8"))
    cases, raw, measures = [], [], []
    for row in document["auditions"]:
        name = row["name"]
        source = args.workshop / "listening" / f"{name}-A-original.wav"
        generated = args.workshop / "listening" / f"{name}-B-imported.wav"
        rate, a = read_wave(source)
        other_rate, b = read_wave(generated)
        if rate != other_rate or len(a) != len(b):
            raise ValueError("Blind pair must share rate and duration")
        # Deliberately simple harmonic anchor with the same comparison-cycle RPM.
        t = np.arange(len(a)) / len(a) * 16
        idle = np.clip(row["baseline"]["idle_rpm"], 500, 1500)
        high = min(row.get("source_rpm_range", [0, row["baseline"]["redline_rpm"]])[1], 8000)
        rpm = np.select([t < 2, t < 4, t < 9, t < 12, t < 15],
                        [idle, idle + (2200-idle)*(t-2)/2,
                         2200+(high-2200)*(t-4)/5,
                         high-(high-2400)*(t-9)/3,
                         2400+(high-2400)*(t-12)/3], default=idle)
        phase = np.cumsum(rpm * row["baseline"]["design"]["cylinders"] / 120 * 2*np.pi / rate)
        anchor = sum(np.sin(phase*i)/i**1.5 for i in range(1, 9))
        matched = [x - x.mean() for x in (a, b, anchor)]
        rms = [float(np.sqrt(np.mean(x*x))) for x in matched]
        if min(rms) <= 1e-9:
            raise ValueError("Silent comparison source")
        common = min(0.09, *(0.85*r/np.max(np.abs(x)) for x, r in zip(matched, rms)))
        outputs = {}
        for kind, x, r in zip(("source", "physical", "anchor"), matched, rms):
            target = assets / f"{name}-{kind}.wav"
            write_wave(target, rate, x * common/r)
            outputs[kind] = target.relative_to(args.output).as_posix()
        cases.append({"name": name, **outputs})
        measures.append({"case": name, "matched_rms": common,
                         "original_rms": rms,
                         "fixed_gains": dict(zip(("source", "physical", "anchor"), [common/r for r in rms])),
                         "files": outputs,
                         "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                         "physical_sha256": hashlib.sha256(generated.read_bytes()).hexdigest()})
        tracks = []
        for suffix, label in (("A-original", "A · Automation"), ("B-imported", "B · physique"), ("B-edited", "B · réglages modifiés")):
            file = args.workshop / "listening" / f"{name}-{suffix}.wav"
            link = Path(os.path.relpath(file, args.output)).as_posix()
            tracks.append(f'<label>{label}<audio controls preload="none" src="{html.escape(link)}"></audio></label>')
        project = args.workshop / "listening" / f"{name}.bess.json"
        project_link = Path(os.path.relpath(project, args.output)).as_posix()
        raw.append(f'<article><h3>{html.escape(name)}</h3><div class="tracks">{"".join(tracks)}</div><a href="{project_link}" download>Projet à ouvrir dans BESS</a></article>')
    data = json.dumps(cases).replace("<", "\\u003c")
    extra = []
    for label, folder in [("Cames et admission indépendantes", args.output / "tuning"),
                          ("Acoustique native, couplage et VVT", args.advanced),
                          ("Qualité 1D — charge, coupure et reprise", args.output / "quality"),
                          ("Calibration linéaire — références synthétiques", args.quality)]:
        if folder is None or not folder.exists():
            continue
        tracks = []
        for file in sorted(folder.rglob("*.wav")):
            link = html.escape(Path(os.path.relpath(file, args.output)).as_posix())
            title = html.escape(file.relative_to(folder).as_posix())
            project = file.with_suffix(".bess.json")
            project_link = (f'<a href="{html.escape(Path(os.path.relpath(project, args.output)).as_posix())}" download>Projet BESS</a>' if project.exists() else "")
            tracks.append(f'<label>{title}<audio controls preload="none" src="{link}"></audio>{project_link}</label>')
        if tracks:
            extra.append(f'<article><h3>{label}</h3><div class="tracks">{"".join(tracks)}</div></article>')
    variant = ""
    manifest = args.workshop / "beamng-variant" / "manifest.json"
    if manifest.exists():
        archive = manifest.parent / json.loads(manifest.read_text(encoding="utf-8"))["zip_file"]
        if not archive.is_file():
            raise ValueError(f"Missing BeamNG demonstration archive: {archive}")
        link = html.escape(Path(os.path.relpath(archive, args.output)).as_posix())
        variant = f'<p><a href="{link}" download>Configuration BeamNG de démonstration (ZIP)</a> — conserve le véhicule original actif ; cette copie a été vérifiée hors jeu et n’est pas installée automatiquement.</p>'
    page = TEMPLATE.replace("__CASES__", data).replace("__RAW__", "".join(raw)).replace("__EXTRA__", "".join(extra)).replace("__VARIANT__", variant)
    (args.output / "TESTER.html").write_text(page, encoding="utf-8")
    (args.output / "listening-protocol.json").write_text(json.dumps({
        "cases": measures, "method": "Separate fixed AC-RMS matched files for blind tests; ordinary clips unchanged. Synthetic 8-harmonic anchor follows approximate comparison RPM. No listener answers generated.",
        "listening_accepted": False, "beamng_accepted": False,
    }, indent=2) + "\n", encoding="utf-8")
    print(args.output / "TESTER.html")


TEMPLATE = '''<!doctype html><html lang="fr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>BESS — essais de réception</title><style>
:root{color-scheme:dark;font:16px system-ui;background:#10181d;color:#e5efef}body{max-width:1180px;margin:auto;padding:35px 24px 80px}h1{font-size:38px;margin:4px 0 12px}h2{margin:0 0 12px}p{line-height:1.6;color:#bdcccc}section,article{background:#19262d;border:1px solid #32464e;border-radius:12px;padding:22px;margin:22px 0}article{background:#122027}a{color:#79dbcb}button,select{font:inherit;background:#247e73;color:white;border:1px solid #72bbaf;border-radius:7px;padding:10px 18px;margin:6px 8px 6px 0;cursor:pointer}button:disabled{opacity:.45;cursor:default}label{display:block;margin:12px 0}input[type=range]{width:210px}audio{width:100%;display:block;margin-top:10px}.tracks{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:20px}.tag{color:#75d4c2;text-transform:uppercase;letter-spacing:2px;font-size:12px}small{color:#9bb3ba}.check label{padding:6px}.score{font-size:22px}textarea{width:96%;min-height:80px;background:#10181d;color:inherit;border:1px solid #57717a;padding:12px;font:inherit}</style>
<div class="tag">Atelier unifié · réception utilisateur</div><h1>Tout tester depuis un même endroit</h1>
<p>Le moteur physique est commun aux imports Automation et à la création libre. Cette page rassemble les écoutes et les vérifications à faire à ton retour. Les réponses restent dans ce navigateur ; le bouton final télécharge ton compte rendu.</p>
<section class="check"><h2>1. Parcours dans l’application</h2><p>Ouvre <b>target/release/bess.exe</b>, importe un ZIP ou ouvre un projet ci-dessous. Pour juger la réponse de l’accélérateur, utilise une sortie filaire.</p>
<label><input type="checkbox" data-check="import"> Import : origine des paramètres, pièces actives et valeurs du banc compréhensibles.</label>
<label><input type="checkbox" data-check="edit"> Atelier : cames admission/échappement indépendantes ; modification, courbe de couple, référence épinglée et remises à zéro.</label>
<label><input type="checkbox" data-check="save"> Sauvegarde puis réouverture : réglages, rapports et référence A conservés.</label>
<label><input type="checkbox" data-check="drive"> Direct, cycle et conduite : démarrage, ralenti, charge, frein, rapports, coupure et reprise.</label></section>
<section><h2>2. Écoutes libres</h2><p>Volumes tels que rendus : aucune égalisation de niveau ajoutée sur ces fichiers. Compare identité, bruit d’admission, mécanique et échappement.</p>__RAW____EXTRA__</section>
<section><h2>3. Qualité comparée, de 0 à 100</h2><p>Écoute la référence puis les quatre versions anonymisées. Note la qualité naturelle de chacune : 0 très artificiel, 100 excellent. Le lot comprend une référence cachée et un témoin harmonique simplifié. Ce protocole exploratoire n’est pas une certification MUSHRA.</p>
<div id="quality-cases" class="tracks" aria-label="Moteur de comparaison"></div><button id="quality-start">Préparer une comparaison</button><div id="quality"></div><button id="quality-save" disabled>Enregistrer ces notes</button><p id="quality-status"></p></section>
<section><h2>4. Reconnaissance en aveugle — 16 essais</h2><p>Pour chaque paire, indique quelle version est la source Automation. Les niveaux moyens sont rapprochés sur des copies distinctes. Les essais réutilisent quatre véhicules : le score reste exploratoire, sans prouver le réalisme ni l’équivalence à un moteur réel.</p>
<button id="blind-start">Commencer les 16 essais</button><div id="blind"></div><p class="score" id="blind-score"></p></section>
<section class="check"><h2>5. Options avancées et export</h2>__VARIANT__
<label><input type="checkbox" data-check="coupling"> Couplage pression/cylindres : mesurer le niveau, comparer le couple et écouter avant/après.</label>
<label><input type="checkbox" data-check="native"> Option d’écoute à fréquence native : comparer le timbre et les décrochages affichés.</label>
<label><input type="checkbox" data-check="fv"> Primaires en volumes finis : comparer un WAV exporté, avec le temps de calcul nécessaire.</label>
<label><input type="checkbox" data-check="calibration"> Calibration WAV : deux enregistrements stationnaires comparables, filtre et résultat dans un nouveau dossier.</label>
<label><input type="checkbox" data-check="beamng"> BeamNG : créer la configuration BESS, conserver le mod original, choisir BESS et vérifier ralenti, charge, 5 200 tr/min, transitions et caméras.</label></section>
<section><h2>6. Ton retour</h2><p>Indique le véhicule, le régime, la charge et le réglage concernés lorsqu’un résultat te déplaît.</p><textarea id="notes" aria-label="Observations" placeholder="Observations…"></textarea><button id="download">Télécharger mes résultats JSON</button><p id="saved"></p></section>
<script>
const cases=__CASES__, results={version:1,created:new Date().toISOString(),quality:[],blind:[],checks:{},notes:""};
let q=null, trial=0, sequence=[];
const $=id=>document.getElementById(id); const random=()=>crypto.getRandomValues(new Uint32Array(1))[0]/4294967296;
function shuffled(items){let a=items.slice();for(let i=a.length-1;i>0;i--){let j=Math.floor(random()*(i+1));[a[i],a[j]]=[a[j],a[i]]}return a}
function audio(src){return `<audio controls preload="none" src="${src}"></audio>`}
document.addEventListener('play',event=>{for(const a of document.querySelectorAll('audio'))if(a!==event.target)a.pause()},true);
cases.forEach((c,i)=>{let label=document.createElement('label'),input=document.createElement('input');input.type='radio';input.name='quality-case';input.value=i;input.checked=i===0;label.append(input,document.createTextNode(' '+c.name));$('quality-cases').append(label)});
$('quality-start').onclick=()=>{let c=cases[+document.querySelector('[name="quality-case"]:checked').value];q={case:c.name,items:shuffled(['source','physical','anchor','source'])};$('quality').innerHTML='<h3>Référence</h3>'+audio(c.source)+q.items.map((k,i)=>`<label>Version ${i+1}${audio(c[k])}<input type="range" min="0" max="100" value="50" data-rating="${i}" aria-label="Note version ${i+1}"><output>50</output></label>`).join('');document.querySelectorAll('[data-rating]').forEach(el=>el.oninput=()=>el.nextElementSibling.value=el.value);$('quality-save').disabled=false;$('quality-status').textContent='Les identités restent cachées jusqu’à l’enregistrement des notes.'};
$('quality-save').onclick=()=>{let scores=[...document.querySelectorAll('[data-rating]')].map((el,i)=>({kind:q.items[i],score:+el.value}));results.quality.push({case:q.case,scores});$('quality-status').textContent='Notes enregistrées. Identités : '+scores.map((s,i)=>`${i+1} = ${s.kind}`).join(', ');$('quality-save').disabled=true;save()};
function showTrial(){if(trial===16){let correct=results.blind.filter(t=>t.correct).length;$('blind').innerHTML='16 réponses enregistrées.';$('blind-score').textContent=`${correct}/16 sources identifiées. Le seuil historique ≤11/16 ne prouve pas l’équivalence sonore.`;save();return}let pair=sequence[trial],c=cases[pair.case];$('blind').innerHTML=`<h3>Essai ${trial+1} / 16</h3><div class="tracks"><label>Version 1${audio(c[pair.order[0]])}</label><label>Version 2${audio(c[pair.order[1]])}</label></div><button data-answer="0">La source est la version 1</button><button data-answer="1">La source est la version 2</button>`;document.querySelectorAll('[data-answer]').forEach(button=>button.onclick=()=>{let answer=+button.dataset.answer;results.blind.push({trial:trial+1,case:c.name,sourcePosition:pair.order.indexOf('source')+1,answer:answer+1,correct:pair.order[answer]==='source'});trial++;showTrial()})}
$('blind-start').onclick=()=>{if(results.blind.length&&!confirm('Recommencer les 16 essais et remplacer ce score ?'))return;results.blind=[];trial=0;sequence=shuffled(Array.from({length:16},(_,i)=>({case:i%cases.length,order:shuffled(['source','physical'])})));$('blind-score').textContent='';showTrial()};
function save(){results.notes=$('notes').value;document.querySelectorAll('[data-check]').forEach(el=>results.checks[el.dataset.check]=el.checked);try{localStorage.setItem('bess-readiness-20261003',JSON.stringify(results))}catch(_){} }
document.querySelectorAll('[data-check]').forEach(el=>el.onchange=save);$('notes').oninput=save;
try{let previous=JSON.parse(localStorage.getItem('bess-readiness-20261003'));if(previous){Object.assign(results,previous);$('notes').value=results.notes||'';document.querySelectorAll('[data-check]').forEach(el=>el.checked=!!results.checks[el.dataset.check]);$('saved').textContent='Compte rendu local restauré ; tu peux le télécharger ou poursuivre.'}}catch(_){}
$('download').onclick=()=>{save();let blob=new Blob([JSON.stringify(results,null,2)],{type:'application/json'}),url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download='bess-retour-utilisateur.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);$('saved').textContent='Compte rendu téléchargé. Aucun résultat envoyé automatiquement.'};
</script></html>'''

if __name__ == "__main__":
    main()
