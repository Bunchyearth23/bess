# Validation intégrée du moteur physique — 28 septembre 2026

Outils livrés : `tests/physical_engine.rs` et `examples/physical_engine.rs`.

## État final après corrections et optimisation

`cargo test --release --test physical_engine` : **8 tests réussis**, 2,39 s. Le test de ralenti qui échouait initialement passe désormais sans modification de sa tolérance. `cargo test --release --lib physical::cycle` : **9 tests réussis**, 0,04 s.

La correction porte sur la conservation des constituants lors des réaspirations, la consommation des réactifs du collecteur et le transport axial des constituants dans le volume d'échappement aval. Le calage VVT au ralenti est également commandé progressivement. Le bilan initial ne suffit donc pas à caractériser la version corrigée ; il est conservé ci-dessous pour expliquer les régressions détectées.

Audits finaux exécutés séquentiellement, sans autre benchmark annoncé : `output/physical-engine-final-20260928/audit-48k/` et `audit-96k/`. À 1 200 tr/min et 35 % de papillon, 0,5 s de préconditionnement et 1 s de mesure :

| Moteur | RMS brut 96 kHz | Crête brute 96 kHz | Temps par pas 96 kHz | Part du temps réel | Correction numérique / chaleur |
|---|---:|---:|---:|---:|---:|
| I4 | −34,95 dBFS | 0,07737 | 2,881 µs | 27,7 % | 0,000007 J / 49 809 J |
| V8 croisé | −34,38 dBFS | 0,08983 | 4,404 µs | 42,3 % | 0,000013 J / 92 225 J |
| V12 | −33,74 dBFS | 0,09135 | 5,846 µs | 56,1 % | 0,000020 J / 133 934 J |

**Le critère V12 de 5 µs par pas et la marge CPU de 50 % ne sont pas encore atteints dans cet audit.** À 48 kHz, avec deux pas physiques par sortie, le V12 demande 10,982 µs par sortie, soit 52,7 % du temps réel. Ces durées moyennes incluent la télémétrie et ne sont pas un p99 de callback.

Le scénario froid d'une seconde applique déjà 16 N·m après 0,5 s : il descend alors vers 243 tr/min et ne constitue pas un test de ralenti chaud établi. La validation de retour au ralenti attend huit secondes sans charge, et passe. L'audit thermique long est géré séparément : ne pas substituer ce premier transitoire d'une seconde à sa fenêtre stabilisée.

Optimisations du cylindre : suppression des calculs d'orifice lorsque la soupape est fermée, cache des constantes de came/soupape/paroi, partage du calcul trigonométrique volume/dérivée et réutilisation du volume courant pour la surface. L'empreinte du cas cylindre déterministe est restée **0342c018b4b78ee3** avant et après ces optimisations ; les équations énergétiques et leurs protections n'ont pas été modifiées.

L'exemple fonctionne **hors ligne**, sans ouvrir de périphérique audio. Il écrit le signal brut d'échappement en WAV IEEE float, les états à 1 kHz, les puissances aux ordres 0,5–12 et le travail gazeux intégré par cycle de 720°. Les diagnostics de chaleur, injection, air externe et by-pass sont moyennés sur la fenêtre de sortie CSV, et les totaux de chaleur/carburant prennent tous les pas : aucune décimation ponctuelle ne sert à estimer ces débits. Il ne passe ni par l'écouteur, ni par le gain d'écoute, ni par le limiteur final. Aucun fichier n'est normalisé.

Commande :

```
cargo run --release --example physical_engine -- <dossier> <secondes_mesurées> <fréquence> <cas>
```

Cas : `all`, `i4`, `v8`, `v12`, `i4-dfco`, `i4-idle-load`, `i4-wrong-wiring`. Le préconditionnement dure toujours 0,5 s, en plus de la durée mesurée. Les durées sont configurables jusqu'à 600 s. Une panne interne produit une erreur du programme après conservation des preuves.

## Première mesure avant corrections de gain et optimisation

Artefacts conservés : `outputs/physical-engine-audit-20260928/` (48 kHz), `outputs/physical-engine-audit-96k-20260928/` (96 kHz). Ces chiffres décrivent cette première compilation, pas nécessairement l'état final après corrections.

Régime imposé 1 200 tr/min, papillon 35 %, 0,5 s de préconditionnement + 1 s de mesure :

| Moteur | RMS brut à 48 kHz | Crête brute | Ordre dominant parmi 0,5–12 | Chaleur/correction numérique cumulées | Temps réel consommé |
|---|---:|---:|---:|---:|---:|
| I4 | −33,08 dBFS | 0,1024 | 2 | 52 004 J / 0,000007 J | 28,1 % |
| V8 croisé | −29,75 dBFS | 0,1448 | 4 | 97 376 J / 0,000013 J | 53,2 % |
| V12 | −28,67 dBFS | 0,1436 | 6 | 141 384 J / 0,000020 J | 70,6 % |

Le temps mural inclut les calculs et l'écriture des lignes CSV, exclut la création du moteur et l'analyse spectrale finale. Ce n'est ni un p99 de callback ni un benchmark isolé. À 96 kHz, le V12 a demandé 7,85 µs par pas en moyenne, soit 75,3 % du temps réel. Le critère de marge 50 % n'était donc pas satisfait dans cette première mesure.

DFCO : le test vérifie chaleur nulle et pompe acoustique encore présente. La pression finale d'admission du scénario 3 000 tr/min fermé était 14,31 kPa. Mauvais câblage I4 (ordre 1-1-1-1) : chaleur réduite à 13 142 J contre 52 004 J, états finis, sans moteur déclaré en panne.

## Défaut de ralenti identifié avant correction

Artefacts : `outputs/physical-engine-idle20-20260928/`. Le scénario maintient d'abord 1 200 tr/min pendant 0,5 s, passe en rotation libre papillon fermé pendant 20 s et ajoute 16 N·m après 10 s de mesure.

- Sans charge : oscillation approximative **984–1 534 tr/min**, avec coupure DFCO cyclique. Exemple de fenêtre 5–7 s : 978 des 2 000 observations à 1 kHz étaient en coupure.
- Avec 16 N·m : 18–20 s à **1 002–1 025 tr/min**, moyenne environ **1 016 tr/min**, toujours au-dessus de la consigne de 850 tr/min.
- Test automatique de retour au ralenti : moyenne finale **1 309,57 tr/min** contre 850 ; échec explicite, conservé comme test de régression.
- Travail gazeux total moteur : COV roulant des 20 derniers cycles **0,519 %**. Cette grandeur n'est pas le COV d'IMEP **par cylindre** du rapport (6–12 %) ; elle ne valide pas cette cible.

Cause suspectée et transmise au parent : le contrôleur cessait de corriger le ralenti au-dessus de 1,5 fois la consigne et réappliquait son by-pass de base, avant la coupure à 1,8 fois la consigne. Les preuves de sortie sont plus importantes que cette hypothèse ; toute correction doit repasser le test sans en élargir la tolérance.

## Tests présents

1. I4/V8/V12 finis, combustion significative, correction d'énergie <1 %, fondamental d'allumage >4 fois les bandes adjacentes de ±0,5 ordre.
2. Deux moteurs identiques et mêmes commandes : sortie, chaleur et admission identiques au bit près.
3. DFCO : chaleur nulle après la commande, pompage conservé.
4. Cylindres manquants dans l'ordre électrique : chaleur totale réduite.
5. Rotation libre : effet mesurable d'une charge accessoire de 22 N·m, sans NaN ni panne.
6. Retour au ralenti : moyenne finale à moins de 10 % de la consigne, aucune coupure DFCO dans la dernière seconde. **Échec initial reproduit, puis réussite après correction sans assouplir le test.**
7. Papillon : variation de MAP >20 kPa et chaleur multipliée par plus de deux.
8. Turbo 0,8 bar, 3 000 tr/min, plein papillon : arbre >10 000 tr/min et MAP moyenne finale >110 kPa après deux secondes. **Réussi.**

La qualité sonore subjective, la stabilité audio longue durée, les températures réelles et l'identification sur moteur mesuré ne sont pas établies par ces tests.
