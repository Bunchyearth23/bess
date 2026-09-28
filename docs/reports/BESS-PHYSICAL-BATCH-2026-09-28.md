# W-006 — premier lot parallèle, 28 septembre 2026

Lancement explicitement demandé par l'utilisateur. Trois branches de travail
dans le même espace, avec fichiers distincts : infrastructure audio, cylindre
thermodynamique et échanges gazeux. Intégration et revue croisées ensuite.
W-006 passe de `planned` à `building`. Ce lot ne remplace pas encore la voix
scratch à événements par un moteur physique complet.

## Ce qui est intégré

- Producteur audio dédié : synthèse, changements de voix et destruction hors
  callback CPAL ; anneaux SPSC `rtrb` pour les échantillons et commandes. Les
  échanges de modèles préparés restent entre interface et producteur.
- Scratch calculé à deux fois la fréquence de sortie, puis décimé par le
  `PolyphaseDecimator` de BDSP. Le même chemin sert au rendu WAV ; le fondu final
  de 50 ms est propre au fichier. Imports conservés à leur fréquence native.
- Fondu bref en cas de manque d'échantillons, compteurs de coupures visibles,
  diagnostic silencieux incluant producteur et callback. FTZ/DAZ limité aux
  calculs audio x86-64, puis environnement flottant restauré.
- `physical::thermo` : cinématique bielle-manivelle, premier principe avec
  enthalpies et travail, chaleur spécifique variable, Wiebe à CA50 exact,
  bilan séparé des corrections numériques. Voir [rapport thermo](PHYSICAL-THERMO-2026-09-28.md).
- `physical::gas` : cames sur 720°, jeu, aire de rideau plafonnée, Cd interpolé,
  orifices compressibles bidirectionnels et volumes conservatifs.
- `physical::config` relie alésage, course, compression, soupapes et came du
  constructeur à des grandeurs SI. Bielle, diamètres de soupapes, levée et
  relation came/durée sont des estimations explicites, pas des mesures véhicule.
- `physical::cylinder` combine ces éléments dans un monocylindre à régime
  imposé, avec réservoirs admission/échappement fixes. L'exemple
  `cargo run --release --example physical_cylinder` produit deux traces CSV et
  leurs bilans dans `target/physical-cylinder`.

## Mesure du prototype

Cas nominal : cylindre 86 × 86 mm, compression 10,5, 1 500 tr/min, admission
80 kPa / 310 K, échappement 105 kPa / 650 K, 96 kHz pendant une seconde.
Le premier cycle comprend une transition d'initialisation.

| Mesure | Sans combustion | Avec combustion |
| --- | ---: | ---: |
| Pression maximale | 18,904 bar | 43,489 bar |
| Température maximale | 822,20 K | 2 371,36 K |
| Travail gazeux cumulé | −174,38 J | 4 676,74 J |
| Chaleur ajoutée | 0 J | 9 636,86 J |
| Résidu du bilan énergétique | 2,8e−12 J | 2,7e−11 J |
| Somme absolue des corrections numériques | 1,3e−9 J | 2,4e−9 J |

Ces résultats mesurent la cohérence du prototype, pas sa fidélité à un moteur
réel. La combustion est un apport de chaleur idéal avec une estimation fixe
de charge fraîche ; il n'y a pas de transport d'espèces ni de masse de carburant.

Passer de 96 à 192 kHz change le travail cumulé de **0,001421 %** et la pression
maximale de **0,005074 %**. Un passage séparé sans écriture CSV, après chauffe,
mesure **0,364 µs/pas** avec combustion sur cette machine. Il s'agit d'un seul
cylindre avec ses diagnostics, sans réseau acoustique ; aucune extrapolation
de performance V12 n'en est déduite.

## Validation du lot intégré

- Suite complète `cargo test --release` : **159 tests réussis**, aucun échec.
- Après l'ajustement final de tampon, les **8 tests temps réel et 4 tests audio**
  sont repassés ; l'analyse stricte de toutes les cibles et le build release aussi.
- `cargo clippy --release --all-targets -- -D warnings` : réussi.
- Exécutable habituel reconstruit : `target/release/bess.exe`.
- Formatage vérifié pour les modules audio/physiques et l'exemple ajoutés ;
  `git diff --check` réussi. Le contrôle global `cargo fmt --check` reste en
  échec sur des zones déjà non formatées du dépôt ; aucune remise en forme
  générale n'a été appliquée à ces travaux antérieurs.
- Logs : `output/physical-batch-tests-20260928.txt`,
  `output/physical-batch-clippy-20260928.txt`,
  `output/physical-batch-format-20260928.txt`.

La relecture a corrigé une incompatibilité entre l'anneau de 20 ms et les gros
callbacks de certains périphériques. La sortie demande environ 10 ms puis
dimensionne l'anneau pour au moins deux callbacks réellement négociés. Les
périphériques imposant davantage de latence affichent cette valeur réelle.
Des tests consomment deux rafales de 480, 1 024 et 2 048 échantillons sans manque.

La parité bit à bit du rendu direct/WAV est testée avec les mêmes commandes et
le même état initial ; le fondu propre au fichier est appliqué explicitement
dans la comparaison. Cela ne compare pas un enregistrement de la sortie Windows.

Premier contrôle réel silencieux de 60 s, à 48 kHz / calcul 96 kHz et tampon
20 ms : 6 000 callbacks, maximum 0,019 ms, p99 ≤ 1 % du délai, aucun dépassement
du callback, mais **1 sous-alimentation / 162 échantillons manquants**. Le maximum
de calcul d'un bloc de 5 ms était 2,783 ms. Ce résultat ne satisfait pas le
critère zéro coupure ; le dimensionnement du producteur est revu ensuite.
Preuve initiale conservée : `output/physical-batch-device-60s-20260928.txt`.

Correction retenue : **3 blocs de 10 ms, soit 30 ms** au périphérique courant,
avec capacité adaptée aux gros callbacks si nécessaire. Pas d'attente active
ni de réinitialisation des compteurs. La seconde mesure silencieuse de 60 s
donne **0 sous-alimentation, 0 échantillon manquant**, 6 001 callbacks,
maximum callback **0,009 ms**, p99 **≤ 1 %** du délai, 0 dépassement et maximum
producteur **2,259 ms par bloc de 10 ms**. Preuve :
`output/physical-batch-device-30ms-60s-20260928.txt`.

Cette minute observée ne vaut pas le critère de dix minutes ni une écoute
subjective ; ces deux validations restent ouvertes. Le contrôle utilise
`bess.exe --audio-check <rapport> scratch 60` et accepte jusqu'à 600 secondes.

SHA256 de l'exécutable final :
`6068E92363B12C425A66509BFE719147674FA0D6067886035F8EF77B5EEBD8AD`.

## Limites et suite

La capacité calorifique est une approximation linéaire cohérente avec son
intégrale énergétique, différente de Brunt. L'échange thermique utilise un
coefficient de Newton fourni ; Hohenberg n'est pas intégré. CA50 est imposé à
8° après PMH, sans recherche automatique du meilleur couple.

Restent le plénum et collecteur dynamiques, papillon, vilebrequin libre avec
frictions/inertie/démarreur, couplage débit/ondes d'échappement, multicylindre,
ratés et ralenti/overrun physiques. Le contrôle de 10 000 géométries porte sur
le noyau thermodynamique, pas sur ce moteur complet encore à construire.
L'écoute et la qualification BeamNG restent distinctes de ces preuves.
