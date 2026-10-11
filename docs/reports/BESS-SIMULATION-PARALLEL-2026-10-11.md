# BESS — Premier lot parallèle P1 à P7

Exécution autorisée le 11 octobre 2026, sur AMD Ryzen 7 3800X. Trois agents ont traité P2/P7, P3/P4 et P5/P6; le coordinateur a livré P1, les contrôles visibles et la vérification commune. W-010 à W-016 restent `building` : le premier lot technique est distinct de leurs critères complets de réception.

## Livraison et limites

| Chantier | Premier lot livré | Réception encore ouverte |
|---|---|---|
| [P1 — référence publique](BESS-PUBLIC-REFERENCE-2026-10-11.md) | Schéma SI/provenance, import, alignement, ordres, recherche bornée, sensibilité, comparaison Honda GX160 non ajustée | Le seuil public exploratoire échoue; données thermiques, pression-angle, son et identification réelle manquent |
| [P2 — ondes conjointes](BESS-JOINT-INTAKE-2026-10-11.md) | Retour admission indépendant, frontière implicite, dyno correctement indexé et à la fréquence choisie, comparaisons 48/96 kHz | Passivité/énergie du système complet, grille cames/régimes, X-028 et référence mesurée |
| [P3 — carburant](BESS-FUEL-TRANSIENTS-2026-10-11.md) | Injection temporisée, film/gouttelettes, évaporation/chaleur latente, DFCO, inventaires et sauvegarde commune | Film spatial, spray/DI, cartographie d'enrichissement et vrai démarrage libre froid |
| [P4 — flamme](BESS-PREDICTIVE-COMBUSTION-2026-10-11.md) | Corrélation pression/température/richesse/dilution/turbulence, variations semées, énergie du carburant conservée | Pression/CA50 mesurées, chimie/auto-inflammation et cliquetis identifié |
| [P5 — turbo](BESS-MAPPED-TURBO-2026-10-11.md) | Cartes corrigées importables, rendements, turbine reliée au réseau gaz, wastegate/BOV, pertes explicites | Cartes livrées synthétiques; domaine réel, flux inverse/surge, thermique prédictive et endurance |
| [P6 — mécanique](BESS-STRUCTURAL-TORSION-2026-10-11.md) | Réponse modale importable, positions/provenance, observation dissipative, torsion à deux inerties | Forces/transferts mesurés et réception sonore; la couche mécanique reste à zéro dans les nouveaux projets |
| [P7 — réseau 1D](BESS-COUPLED-1D-2026-10-11.md) | Banc hors ligne cylindres/primaires/collecteur/tube aval, flux uniques, conservation, WAV/CSV, progression et annulation | Gaz parfait constant, espèces mélangées par branche; EOS variable, espèces distribuées et intégration production/export restent à réaliser |

## Utilisation

Dans l'éditeur du moteur, la section visible **Simulation models** permet d'activer le retour admission, le carburant liquide, la flamme prédictive et la torsion, avec leurs paramètres bornés. Elle propose l'import et le retrait des cartes turbo et réponses bloc/culasse. Les cartes s'appliquent aux moteurs turbo. Les fichiers JSON lisibles produits par `mapped_turbo_mechanics` servent de templates synthétiques.

Ces cinq modèles physiques/observateurs utilisent les options enregistrées dans `EngineDefinition` pour moteurs importés et libres, lecture, WAV et boucles d'export. Le format projet reste v4 et les clés absentes des anciens projets gardent les options désactivées. Le banc couplé P7 est accessible par son exemple hors ligne; il n'est pas encore une option du mode qualité de production. La garde `primary_1d + wave_coupling` reste présente.

Les constantes livrées et les fixtures de cartes/rayonnement restent estimées ou synthétiques. Les démos à gain fixe ne prouvent pas une amélioration de fidélité sonore. P6 conserve les WAV bruts et propose des copies séparées d'écoute avec les gains déclarés, sans modification de la simulation.

## Preuves communes

- Cinq tests d'intégration couvrent la migration sans nouvelles clés, la persistance des deux origines avec baseline/source importées préservées, le cache dyno, le rejet des paramètres invalides et le rendu combinant toutes les nouvelles options. Ce dernier vérifie 48 000 échantillons lecture/WAV identiques (après application de la seule fin de fichier) et zéro allocation/libération dans la boucle de rendu.
- Cinq tests P1 couvrent la séparation des observations réservées, les erreurs d'import/canaux, les ordres fractionnaires et l'alignement. Valeurs constructeur et fixtures synthétiques ne donnent jamais une qualification instrumentée.
- Un test supplémentaire reproduit la perte d'un bit d'un coefficient décimal dans l'ancien décodeur JSON. L'option `serde_json/float_roundtrip` conserve désormais exactement les doubles sérialisés; elle concerne l'import/sauvegarde et n'ajoute aucun calcul au pas physique. Le test compare les bits du coefficient et les options complètes, sans tolérance élargie.
- Rendu par défaut de l'exécutable reconstruit identique au témoin sauvegardé avant modification : SHA256 `50EF7089BEFAA804DA8DC40389FCF685FD5C4757ECC7E68357AEC19E4EC95F51`.
- X-028 a été exécuté explicitement sur le test historique inchangé en release : facteur obtenu `0.6209708` contre `0.69999999` attendu, soit environ 11,29 % d'écart. Le seuil reste 10 %, échec conservé.
- `cargo test --release --all-targets` : **459 réussis, zéro échec, six diagnostics exclus**, après le correctif de persistance exacte des décimaux. Les premières assertions de fixture (origine de projet/architecture), de saturation de probabilité et de correction héritée avant injection ont été corrigées sur preuves explicites; `initial-release.log` conserve l'ancienne exécution et `integrated-release.log` la réussite finale. Les modèles actifs ne relâchent pas leurs bilans pour faire passer ces assertions.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check` et le contrôle des espaces du diff réussissent. Les exclusions de diagnostics ne ferment ni X-014, ni le test historique X-028 exécuté séparément.

La comparaison publique Honda, les outils P1 et les journaux communs sont dans `output/simulation-parallel-20261011/`. Les premiers artefacts P2, P3/P4, P5/P6 et P7 sont respectivement dans `out/simulation-p2/`, `artifacts/simulation-p3-p4/`, `work/simulation-p5-p6/` et `out/simulation-p7/`. Les rapports propres aux chantiers donnent leurs domaines et bilans numériques.

Les cinq exemples sont également reconstruits et exécutés en **release** après le dernier correctif, dans `output/simulation-parallel-20261011/{p1-gx160,p1-synthetic,p2,p3-p4,p5-p6,p7}-release/`. Les six exécutions réussissent : 6,54 / 7,97 / 2,86 / 2,04 / 2,01 / 12,65 s de temps mural respectivement. Ce sont des temps de démonstration incluant les fichiers, pas une qualification audio temps réel. P2 conserve ses douze paires de configurations/WAV; P3/P4 ses quatre projets rechargés/WAV; P5/P6 ses sources brutes et copies d'écoute distinctes.

P7 release rend 0,16 s / 1 000 cellules / 82 206 pas en 12,54 s internes, avec résidu énergétique maximal 1,46e-10 J et masse 1,41e-16 kg. L'annulation a aussi été déclenchée pendant le binaire release : arrêt à 0,02 s / 7 503 pas, WAV finalisé, manifeste `completed:false` dans `p7-release-cancel/`. Ce coût et la portée physique réduite confirment le périmètre hors ligne.

Exécutable final : SHA256 `2D0C95E3F0F32637445C9DCFBF4207921041ABE08500A1E1150F9CFAAC6032AA`. Le témoin WAV est recontrôlé sur ce binaire et garde le hash indiqué ci-dessus. `verification.json`, `release-runs.json` et les journaux dans le répertoire commun enregistrent version de base, machine, tests, artefacts et limites.

## Suite

Les tâches cochées de l'index désignent les livraisons vérifiées; les tâches plus larges restent ouvertes quand seulement une partie est livrée. Aucun parent n'est passé automatiquement à `landed`. Les performances réelles sur portable/V12, écoute utilisateur et BeamNG restent à qualifier. Aucun commit, push ou installation en jeu n'a été demandé pour ce lot.
