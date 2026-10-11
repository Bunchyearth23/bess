"""Source-bound BESS -> BABM clean fleet with verified, reversible installation.

Run prepare, render, merge, then install. Each original receives one BESS profile.
This verifies packages and installation; it does not claim BeamNG/listening QA.
"""
import argparse
import array
import hashlib
import json
import math
import re
import shutil
import subprocess
import sys
import time
import wave
import zipfile
from pathlib import Path


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding='utf-8')


def snapshot(folder):
    return {str(p.relative_to(folder)): digest(p) for p in sorted(folder.rglob('*')) if p.is_file()}


def command(run, *args):
    started = time.monotonic()
    process = subprocess.run([str(a) for a in args], capture_output=True,
                             text=True, encoding='utf-8-sig', errors='replace', timeout=1800)
    log = run / 'commands.jsonl'
    with log.open('a', encoding='utf-8') as stream:
        stream.write(json.dumps({'args': [str(a) for a in args], 'exit_code': process.returncode,
            'seconds': round(time.monotonic()-started, 3),
            'stdout': process.stdout, 'stderr': process.stderr}, ensure_ascii=False)+'\n')
    if process.returncode:
        raise RuntimeError(f'Failed {args}: {process.stderr}\n{process.stdout}')
    return process.stdout


def profile(slug):
    # Editorial choices based on the archived year, output, mass and induction.
    # Berlingo 1000Lingo is a 1012 kW turbo special, not a stock utility vehicle.
    settings = {
        'advent_tc': ('Sport Classic', .98, .85, 1., 1., 1., 12000., 55.),
        'archo_coupe': ('Classic Coupe', .92, .72, 1., .8, -.5, 8500., 60.),
        'b5_a': ('Economy', .80, .60, 0., .3, -2., 6000., 45.),
        'b5_c': ('Turbo Sport', .95, .80, 1., 1., .5, 10000., 55.),
        'b5_gt': ('Turbo Road', .92, .75, .7, .8, .2, 9500., 55.),
        'berlingo_1000lingo': ('Turbo Extreme', 1., 1., 1., 2., 2., 14000., 55.),
        'cerberus_a': ('Sport Road', .97, .85, 1.5, 1.5, .5, 11000., 60.),
        'genesis_phantom': ('Modern Sport', .92, .80, .5, .8, .2, 10000., 50.),
        'nord_optis': ('Classic Economy', .82, .65, .8, .5, -2., 5200., 50.),
        'pulsar_opti': ('Modern Turbo', .85, .70, .5, .5, -1., 7500., 45.),
        'thunderhawk_zero': ('Classic Road', .94, .75, 1.2, 1., -.5, 7000., 60.),
        'volk_icarus': ('Historic', .86, .72, 1.5, 1., -2., 5000., 50.),
        'volk_icarus_ii': ('Historic', .86, .72, 1.2, .8, -1.5, 5200., 50.),
    }
    name, master, engine, bass, body, rasp, cutoff, decay = settings[slug]
    return {'name': name, 'master_gain': master, 'engine_gain': engine, 'sound': {
        'exhaust_bass_db': bass, 'exhaust_body_db': body, 'exhaust_rasp_db': rasp,
        'exhaust_high_cut_hz': cutoff, 'exhaust_decay_ms': decay}}


def original_info(source):
    with zipfile.ZipFile(source) as archive:
        names = archive.namelist()
        root_info = [n for n in names if re.fullmatch(r'vehicles/[^/]+/info.json', n)]
        configs = [n for n in names if re.fullmatch(r'vehicles/[^/]+/[^/]+\.pc', n)]
        assert len(root_info) == len(configs) == 1, (source, root_info, configs)
        info = json.loads(archive.read(root_info[0]))
        root = root_info[0][:-len('info.json')]
        config = json.loads(archive.read(root+'info_'+Path(configs[0]).stem+'.json'))
        return {'name': info['Name'], 'root': root, 'configuration': configs[0],
                'specs': {key: config.get(key) for key in ['Configuration', 'Power', 'Weight',
                    'Years', 'Induction Type', 'Fuel Type', 'PowerPeakRPM', 'TorquePeakRPM']}}


def prepare(args):
    run = args.run
    run.mkdir()  # Never overwrite a previous fleet or its receipts.
    mods = args.mods.resolve()
    before = snapshot(mods)
    write(run/'installed-before.json', before)
    catalogue = json.loads(args.catalogue.read_text(encoding='utf-8-sig'))
    assert len(catalogue['vehicles']) == 13
    (run/'sources').mkdir()
    (run/'projects').mkdir()
    fleet = {'mods': str(mods), 'vehicles': [], 'bess_exporter': str(args.exporter.resolve()),
        'bess_exporter_sha256': digest(args.exporter), 'babm_merger': str(args.merger.resolve()),
        'babm_merger_sha256': digest(args.merger), 'status': 'prepared',
        'boundary': 'Archive/installation verification; BeamNG playback and listening remain unverified'}
    for item in catalogue['vehicles']:
        assert item['unavailable_reason'] is None
        old = Path(item['path'])
        stem = old.name.removesuffix('.merged_backup').removesuffix('.zip')
        slug = stem.removeprefix('bunchyearth23_')
        source = run/'sources'/(stem+'.zip')
        shutil.copy2(old, source)
        sha = digest(source)
        assert sha == digest(old)
        info = original_info(source)
        chosen = profile(slug)
        profile_path = run/'projects'/(slug+'-profile.json')
        write(profile_path, chosen)
        project_folder = run/'projects'/slug
        command(run, args.preparer.resolve(), source.resolve(), profile_path.resolve(), project_folder.resolve())
        metadata = json.loads((project_folder/'metadata.json').read_text())
        active, baseline = metadata['active'].copy(), metadata['baseline'].copy()
        active.pop('sound'); baseline.pop('sound')
        assert active == baseline, 'Sound pass changed engine hardware'
        fleet['vehicles'].append({'slug': slug, 'source': str(source.resolve()),
            'installed_source': str(old), 'source_sha256': sha, **info,
            'profile': chosen, 'engine': metadata['engine'],
            'project': str((project_folder/'project.bess.json').resolve())})
        print('PREPARED', info['name'], metadata['engine']['layout'], metadata['engine']['cylinders'],
              metadata['engine']['displacement_l'], chosen['name'], flush=True)
    assert snapshot(mods) == before, 'Read-only preparation changed installed files'
    write(run/'fleet.json', fleet)


def render(args, fleet):
    for index, vehicle in enumerate(fleet['vehicles'], 1):
        out = args.run/vehicle['slug']
        print(f'RENDER {index}/13 {vehicle["name"]}', flush=True)
        command(args.run, fleet['bess_exporter'], vehicle['project'], out.resolve(),
                '--workers', '8', '--format', 'complete-variant')
        manifest = json.loads((out/'manifest.json').read_text())
        export = out/manifest['zip_file']
        vehicle['export'] = str(export.resolve())
        with zipfile.ZipFile(export) as archive, zipfile.ZipFile(vehicle['source']) as original:
            assert archive.testzip() is None
            assert len(archive.namelist()) == len(set(archive.namelist()))
            for name in original.namelist():
                assert archive.read(name) == original.read(name), ('Original modified', name)
            marker = json.loads(archive.read('bess-export.json'))
            assert marker['kind'] == 'bess-variant-vehicle'
            assert marker['source_archive_sha256'] == vehicle['source_sha256']
            assert len([n for n in archive.namelist() if n.endswith('.pc')]) == 2
            vehicle['generated_wavs'] = len([n for n in marker['added_files'] if n['path'].endswith('.wav')])
        vehicle['export_sha256'] = digest(export)
        write(args.run/'fleet.json', fleet)
        print('RENDERED', vehicle['name'], vehicle['generated_wavs'], 'WAVs', flush=True)
    assert snapshot(Path(fleet['mods'])) == json.loads((args.run/'installed-before.json').read_text())
    fleet['status'] = 'rendered'
    write(args.run/'fleet.json', fleet)


def merge(args, fleet):
    stage = args.run/'staged'
    stage.mkdir(exist_ok=True)
    for vehicle in fleet['vehicles']:
        mods = stage/vehicle['slug']/'mods'
        if not mods.exists():
            mods.mkdir(parents=True)
            shutil.copy2(vehicle['source'], mods/Path(vehicle['source']).name)
            shutil.copy2(vehicle['export'], mods/Path(vehicle['export']).name)
            command(args.run, fleet['babm_merger'], mods.resolve(), vehicle['name'])
        packs = list(mods.glob('*.zip'))
        assert len(packs) == 1
        vehicle['pack'] = str(packs[0].resolve())
        vehicle['pack_sha256'] = digest(packs[0])
        # BeamNG recursively mounted plain ZIPs in the user's .babm_backup.
        # Use its already-supported grouping-backup suffix in the manifest too.
        for manifest_path in (mods/'.babm_backup').glob('*/manifest.json'):
            manifest = json.loads(manifest_path.read_text())
            for item in manifest['original_files']:
                old = manifest_path.parent/item['backup_filename']
                if old.name.endswith('.zip.merged_backup'):
                    assert old.is_file()
                    continue
                assert old.is_file() and old.suffix == '.zip'
                disabled = old.with_name(old.name+'.merged_backup')
                assert not disabled.exists()
                old.rename(disabled)
                item['backup_filename'] = disabled.name
            write(manifest_path, manifest)
        inspection = json.loads(command(args.run, args.babm.resolve(), 'bess-inspect',
            vehicle['export'], '--path', mods.resolve(), '--json'))
        assert inspection['status'] == 'variant_installed' and inspection['selectable_variant'], inspection
        print('MERGED', vehicle['name'], packs[0].name, flush=True)
    fleet['status'] = 'merged'
    write(args.run/'fleet.json', fleet)
    audit(args, fleet)


def wav_metrics(data):
    import io
    with wave.open(io.BytesIO(data)) as wav:
        assert wav.getnchannels() == 1 and wav.getframerate() == 48000 and wav.getsampwidth() == 3
        frames = wav.readframes(wav.getnframes())
    try:
        import numpy as np
    except ImportError:
        np = None
    if np is not None:
        octets = np.frombuffer(frames, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        samples = octets[:, 0] | (octets[:, 1] << 8) | (octets[:, 2] << 16)
        samples = (samples ^ 0x800000) - 0x800000
        peak = float(np.max(np.abs(samples))) / 8388608
        rms = float(np.sqrt(np.mean((samples.astype(np.float64)/8388608)**2)))
        assert 0 < peak < 1 and rms > 0
        return {'peak': peak, 'rms': rms}
    values = array.array('i', (int.from_bytes(frames[i:i+3], 'little', signed=True)
                              for i in range(0, len(frames), 3)))
    assert values and max(abs(x) for x in values) < 8388607
    rms = math.sqrt(sum((x/8388608)**2 for x in values)/len(values))
    assert rms > 0, 'Silent exported sound'
    return {'peak': max(abs(x) for x in values)/8388608, 'rms': rms}


def audit(args, fleet):
    roots, sound_paths, results = set(), set(), []
    for vehicle in fleet['vehicles']:
        pack = Path(vehicle['pack'])
        assert digest(pack) == vehicle['pack_sha256']
        with zipfile.ZipFile(pack) as archive, zipfile.ZipFile(vehicle['source']) as original:
            names = archive.namelist()
            assert len(names) == len(set(names)) and archive.testzip() is None
            pcs = [n for n in names if re.fullmatch(r'vehicles/[^/]+/[^/]+\.pc', n)]
            assert len(pcs) == 2, (pack, pcs)
            these_roots = {n.split('/')[1] for n in names if n.startswith('vehicles/')}
            assert len(these_roots) == 1 and not roots.intersection(these_roots)
            roots.update(these_roots)
            root = 'vehicles/'+next(iter(these_roots))+'/'
            original_sounds = [n for n in original.namelist() if n.endswith('.wav')]
            mapped_original_sounds = set()
            for name in original_sounds:
                mapped = root+name[len(vehicle['root']):] if name.startswith(vehicle['root']) else name
                assert archive.read(mapped) == original.read(name), ('Original audio modified', name)
                mapped_original_sounds.add(mapped)
            assert 'babm-bess-variants.json' in names
            state = json.loads(archive.read('babm-bess-variants.json'))
            # Every path declared by either original or generated blends must exist.
            for name in names:
                if name.endswith('.sfxBlend2D.json'):
                    blend = json.loads(archive.read(name))
                    def paths(value):
                        if isinstance(value, str) and value.endswith('.wav'):
                            yield value.lstrip('/')
                        elif isinstance(value, dict):
                            for item in value.values():
                                yield from paths(item)
                        elif isinstance(value, list):
                            for item in value: yield from paths(item)
                    for path in paths(blend): assert path in names, (pack, name, path)
            configs = [json.loads(archive.read(n)) for n in pcs]
            engines = [config['parts']['Camso_Engine'] for config in configs]
            assert len(set(engines)) == 2, 'Both configurations select the same engine part'
            jbeams = '\n'.join(archive.read(n).decode('utf-8-sig') for n in names if n.endswith('.jbeam'))
            for engine in engines:
                assert re.search(r'"'+re.escape(engine)+r'"\s*:\s*\{', jbeams), ('Missing engine part', engine)
            generated = [n for n in names if n.endswith('.wav') and n not in mapped_original_sounds]
            assert len(generated) == vehicle['generated_wavs']
            assert not sound_paths.intersection(generated)
            sound_paths.update(generated)
            metrics = [wav_metrics(archive.read(n)) for n in generated]
            results.append({'name': vehicle['name'], 'pack': str(pack), 'root': root,
                'configurations': pcs, 'original_wavs_unchanged': len(original_sounds),
                'distinct_engine_parts': engines,
                'generated_wavs': len(generated), 'max_peak': max(m['peak'] for m in metrics),
                'min_rms': min(m['rms'] for m in metrics), 'variant_state_version': state.get('version'),
                'passed': True})
        assert digest(Path(vehicle['source'])) == vehicle['source_sha256']
        print('AUDITED', vehicle['name'], '2 configurations', flush=True)
    assert len(roots) == 13
    report = {'vehicles': results, 'vehicle_count': 13, 'configuration_count': 26,
        'generated_wav_count': len(sound_paths), 'passed': True, 'boundary': fleet['boundary']}
    write(args.run/'audit.json', report)


def game_closed():
    result = subprocess.run(['powershell', '-NoProfile', '-Command',
        "Get-Process | Where-Object { $_.ProcessName -match 'BeamNG|BeamMP' } | Select-Object -ExpandProperty ProcessName"],
        capture_output=True, text=True, timeout=30)
    assert result.returncode == 0 and not result.stdout.strip(), 'Close BeamNG/BeamMP before installation'


def restore(args, fleet):
    vehicle = fleet['vehicles'][0]
    source_mods = args.run/'staged'/vehicle['slug']/'mods'
    mods = args.run/'restore-check'/'mods'
    assert not mods.exists()
    shutil.copytree(source_mods, mods)
    # Force restoration from the non-ZIP backup filenames, not the root copies.
    for path in mods.glob('*.merged_backup'):
        assert path.resolve().parent == mods.resolve()
        path.unlink()
    command(args.run, args.babm.resolve(), 'unmerge', vehicle['name'], '--path', mods.resolve())
    original = mods/Path(vehicle['source']).name
    exported = mods/Path(vehicle['export']).name
    assert digest(original) == vehicle['source_sha256']
    assert digest(exported) == vehicle['export_sha256']
    assert len(list(mods.glob('*.zip'))) == 2
    catalogue = json.loads(command(args.run, args.catalogue_tool.resolve(), mods.resolve()))
    assert len(catalogue['vehicles']) == 1
    assert digest(Path(catalogue['vehicles'][0]['path'])) == vehicle['source_sha256']
    write(args.run/'restore-check.json', {'vehicle': vehicle['name'],
        'restored_original_sha256': digest(original), 'restored_export_sha256': digest(exported),
        'non_zip_backups_supported': True, 'bess_original_rediscovered': True, 'passed': True})
    print('RESTORE VERIFIED', vehicle['name'], flush=True)


def install(args, fleet):
    assert fleet['status'] == 'merged'
    assert json.loads((args.run/'audit.json').read_text())['passed']
    assert json.loads((args.run/'restore-check.json').read_text())['passed']
    mods = Path(fleet['mods']).resolve()
    assert mods == args.mods.resolve(), 'Wrong target mods folder'
    if not args.allow_running_game:
        game_closed()
    before = json.loads((args.run/'installed-before.json').read_text())
    current = snapshot(mods)
    if current != before:
        changed = {name for name in set(before)|set(current) if before.get(name) != current.get(name)}
        # Live game repository downloads and registry maintenance are unrelated
        # to this fleet. Preserve their current state, never an older snapshot.
        assert all(name == 'db.json' or Path(name).parts[0] == 'repo' for name in changed), changed
        shutil.copy2(args.run/'installed-before.json', args.run/'installed-at-prepare.json')
        before = current
        write(args.run/'installed-before.json', before)
    # Keep a full copy, including BABM history and game metadata, outside mods.
    backup = mods.parent/('BESS-clean-backup-'+args.run.name)
    if backup.exists():
        assert args.resume, 'Backup already exists; use --resume after inspecting a prior rollback'
        stored = snapshot(backup)
        differences = {name for name in set(before)|set(stored) if before.get(name) != stored.get(name)}
        assert all(name == 'db.json' or Path(name).parts[0] == 'repo' for name in differences), differences
        retry = args.run/'before-retry'
        retry.mkdir(exist_ok=True)
        shutil.copy2(mods/'db.json', retry/'db.json')
        for name in differences:
            source = mods/name
            if name != 'db.json' and source.is_file():
                destination = retry/name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
    else:
        shutil.copytree(mods, backup)
        assert snapshot(backup) == before
    active = sorted(mods.glob('babm_*.zip'))
    assert len(active) == 10, 'Unexpected active Automation packs'
    assert set(p.name for p in mods.glob('*.zip')) == set(p.name for p in active)
    staged = [Path(v['pack']) for v in fleet['vehicles']]
    assert len(set(p.name for p in staged)) == 13
    prepared = mods.parent/('BESS-clean-ready-'+args.run.name)
    prepared.mkdir(exist_ok=args.resume)
    for vehicle, pack in zip(fleet['vehicles'], staged):
        assert digest(pack) == vehicle['pack_sha256']
        shutil.copy2(pack, prepared/pack.name)
    if not args.allow_running_game:
        game_closed()
    # Only explicitly inventoried archive paths are removed. On any error restore
    # the prior archives; preserved .merged_backup files and db.json stay untouched.
    removed, installed_new = [], []
    database_replaced = False
    try:
        for pack in active:
            pack.unlink()
            removed.append(pack)
        for pack in staged:
            (prepared/pack.name).replace(mods/pack.name)
            installed_new.append(mods/pack.name)
        history = mods/'.babm_backup'
        old_history = mods.parent/('BABM-history-before-'+args.run.name)
        history.rename(old_history)
        history.mkdir()
        for vehicle in fleet['vehicles']:
            src = Path(vehicle['pack']).parent/'.babm_backup'
            for folder in src.iterdir(): shutil.copytree(folder, history/folder.name)
        assert not list(history.rglob('*.zip')), 'A backup can be activated by BeamNG'
        # Preserve unrelated repository mods and their activation preferences.
        database_path = mods/'db.json'
        database = json.loads(database_path.read_text())
        old_database = json.loads((backup/'db.json').read_text())
        unrelated = {key: entry for key, entry in database['mods'].items()
                     if not entry.get('fullpath', '').startswith('/mods/.babm_backup/')
                     and entry.get('filename') not in {p.name for p in active+staged}}
        database['mods'] = unrelated.copy()
        for pack in staged:
            installed = mods/pack.name
            stat = installed.stat()
            database['mods'][pack.stem.lower()] = {
                'active': True, 'dateAdded': int(time.time()), 'dirname': '/mods/',
                'filename': pack.name, 'fullpath': '/mods/'+pack.name,
                'modType': 'vehicle', 'modname': pack.stem.lower(),
                'stat': {'accesstime': int(stat.st_atime), 'createtime': int(stat.st_ctime),
                         'filesize': stat.st_size, 'filetype': 'file',
                         'modtime': int(stat.st_mtime), 'readonly': False}}
        assert len(database['mods']) == len(unrelated)+13
        temporary_database = mods/'db.json.clean-staged'
        write(temporary_database, database)
        temporary_database.replace(database_path)
        database_replaced = True
        fleet['database'] = {'old_entries': len(old_database['mods']),
            'new_entries': len(database['mods']), 'unrelated_entries_preserved': len(unrelated),
            'active_fleet_packs': 13, 'active_backup_archives': 0}
        # Re-read all installed pack hashes, not only the staging copies.
        for vehicle in fleet['vehicles']:
            installed = mods/Path(vehicle['pack']).name
            assert digest(installed) == vehicle['pack_sha256']
            vehicle['installed_pack'] = str(installed)
        assert len(list(mods.glob('*.zip'))) == 13
        after = snapshot(mods)
        for name, sha in before.items():
            if name.startswith('.babm_backup') or name in {p.name for p in active} or name == 'db.json': continue
            assert after[name] == sha, ('Unrelated installed file changed', name)
    except Exception:
        for pack in installed_new:
            if pack.exists(): pack.unlink()
        for pack in removed: shutil.copy2(backup/pack.name, mods/pack.name)
        if database_replaced: shutil.copy2(backup/'db.json', mods/'db.json')
        if 'old_history' in locals() and old_history.exists():
            assert history.resolve() == mods/'.babm_backup'
            if history.exists(): shutil.rmtree(history)
            old_history.rename(history)
        restored = snapshot(mods)
        differences = {name for name in set(before)|set(restored) if before.get(name) != restored.get(name)}
        assert not differences or (args.allow_running_game and all(
            name == 'db.json' or Path(name).parts[0] == 'repo' for name in differences)), differences
        raise
    fleet['status'] = 'installed'
    fleet['backup'] = str(backup)
    fleet['old_babm_history'] = str(old_history)
    fleet['installed_while_game_running'] = args.allow_running_game
    write(args.run/'installed-after.json', after)
    write(args.run/'fleet.json', fleet)
    print('INSTALLED 13 vehicles / 26 configurations; backup:', backup, flush=True)


def verify(args, fleet):
    assert fleet['status'] == 'installed'
    mods = Path(fleet['mods'])
    baseline = json.loads((args.run/'installed-after.json').read_text())
    current_paths = {str(path.relative_to(mods)) for path in mods.rglob('*') if path.is_file()}
    changed_paths = set(baseline).symmetric_difference(current_paths)
    assert all(Path(name).parts[0] == 'repo' for name in changed_paths), changed_paths
    catalogue = json.loads(command(args.run, args.catalogue_tool.resolve(), mods))
    assert len(catalogue['vehicles']) == 13
    assert {digest(Path(v['path'])) for v in catalogue['vehicles']} == {
        v['source_sha256'] for v in fleet['vehicles']}
    write(args.run/'installed-catalogue.json', catalogue)
    inspections = []
    for vehicle in fleet['vehicles']:
        installed = Path(vehicle['installed_pack'])
        assert digest(installed) == vehicle['pack_sha256']
        with zipfile.ZipFile(installed) as archive:
            assert archive.testzip() is None, ('Installed CRC failure', installed)
        inspection = json.loads(command(args.run, args.babm.resolve(), 'bess-inspect',
            vehicle['export'], '--path', mods, '--json'))
        assert inspection['status'] == 'variant_installed' and inspection['selectable_variant']
        assert Path(inspection['target_path']).resolve() == Path(vehicle['installed_pack']).resolve()
        inspections.append(inspection)
    write(args.run/'installed-inspections.json', inspections)
    database = json.loads((mods/'db.json').read_text())
    expected = {Path(v['installed_pack']).stem.lower() for v in fleet['vehicles']}
    assert {key for key, value in database['mods'].items() if key.startswith('babm_')
            and value['active']} == expected
    assert not any('/.babm_backup/' in item['fullpath'] for item in database['mods'].values())
    assert not list((mods/'.babm_backup').rglob('*.zip'))
    fleet['installed_verified'] = True
    write(args.run/'fleet.json', fleet)
    print('VERIFIED installed hashes, 13 active packs, 13 originals and 13 BABM variant receipts', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['prepare', 'render', 'merge', 'audit', 'restore', 'install', 'verify'])
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--mods', type=Path, required=True)
    parser.add_argument('--catalogue', type=Path)
    parser.add_argument('--preparer', type=Path, default=Path('target/release/examples/fleet_project.exe'))
    parser.add_argument('--exporter', type=Path, default=Path('target/release/examples/export_project_probe.exe'))
    parser.add_argument('--merger', type=Path, default=Path('../babm/target/release/examples/fleet_merge.exe'))
    parser.add_argument('--babm', type=Path, default=Path('../BABM.exe'))
    parser.add_argument('--catalogue-tool', type=Path, default=Path('target/release/examples/beamng_folders.exe'))
    parser.add_argument('--allow-running-game', action='store_true',
                        help='Explicit user-authorized live installation; no game processes are stopped')
    parser.add_argument('--resume', action='store_true', help='Reuse verified backups after a prior rollback')
    args = parser.parse_args()
    args.run = args.run.resolve()
    if args.stage == 'prepare': prepare(args)
    else:
        fleet = json.loads((args.run/'fleet.json').read_text(encoding='utf-8'))
        globals()[args.stage](args, fleet)


if __name__ == '__main__':
    main()
