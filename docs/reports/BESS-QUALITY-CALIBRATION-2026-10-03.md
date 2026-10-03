# Qualité 1D et calibration linéaire — 3 octobre 2026

## Périmètre livré

Le moteur physique commun conserve la combustion, le vilebrequin et les organes existants. L'option `primary_1d` remplace uniquement la propagation dans les primaires d'échappement par un calcul conservatif 1D. Le collecteur, les silencieux et l'admission gardent leur réseau actuel. Cette option demande une validation hors ligne avant toute qualification temps réel.

La calibration WAV est un outil distinct : elle prépare un filtre FIR linéaire à partir d'un rendu et d'une référence, puis écrit une copie traitée et un rapport. Elle ne modifie ni le moteur physique ni les fichiers source. Elle ne prouve aucune authenticité automobile.

## Calcul 1D

`src/physical/finite_volume.rs` résout les équations d'Euler à section constante pour les variables conservées densité, quantité de mouvement et énergie totale. La reconstruction MUSCL utilise des pentes primitives limitées ; le flux HLLC résout les interfaces ; l'intégration SSP-RK2 utilise deux étages. Les fondements sont la reconstruction linéaire conservative de [van Leer (1979)](https://www.sciencedirect.com/science/article/pii/0021999179901451), le solveur de contact de [Toro, Spruce et Speares (1994)](https://iris.unitn.it/handle/11572/70548) et les schémas temporels de [Gottlieb et Shu (1998)](https://www.cfm.brown.edu/people/sg/SSP1.pdf).

Les vitesses d'onde encadrent les valeurs gauche, droite et moyennes de Roe. Un état intermédiaire HLLC non physique déclenche un flux HLLE. Un étage non positif ou dépassant le plafond CFL est refusé : reprise au premier ordre avec pas divisé par deux. Aucun écrêtage de cellule ne rajoute clandestinement masse ou énergie. L'importance des faibles densités et de l'énergie interne positive est établie par [Einfeldt et al. (1991)](https://www.sciencedirect.com/science/article/pii/0021999191902113) ; le [livre numérique Clawpack](https://www.clawpack.org/riemann_book/html/Euler_approximate.html) fournit une référence vérifiable pour les solveurs approchés d'Euler.

Choix précis de cette implémentation :

- CFL cible 0,4 ; contrôle des deux étages limité à 0,48.
- Maximum 14 tentatives et 4096 sous-pas par appel ; dépassement signalé explicitement.
- Densité et pression contrôlées à chaque étage, avec rejet des valeurs non finies.
- Toutes les zones de travail sont allouées à la construction.
- Comptabilité séparée des flux aux extrémités et des échanges d'inventaire dus à une modification explicite de longueur.
- Adaptateur `Primary1d` avec invariants de Riemann exprimés en pascals équivalents et réservoirs isentropiques aux extrémités.

Le gaz de référence du primaire intégré est fixé à sa construction : 673 K, 101325 Pa, γ = 1,33 et R = 287 J/(kg·K). La combinaison avec `wave_coupling` est refusée par la validation : le port Blair suit la composition et le fond thermodynamique 0D, qui ne sont pas encore unifiés avec cette référence 1D. Le solveur ne traite pas le transfert thermique aux parois, la viscosité, les espèces chimiques, les sections variables ou le travail d'une paroi mobile. Une modification de longueur conserve les états locaux et comptabilise le changement d'inventaire ; ce n'est pas un calcul de déplacement matériel d'une paroi. Les caractéristiques entrantes sont limitées au domaine explicite `|amplitude| <= 2 × pression de référence` ; sortir du domaine provoque un arrêt signalé, pas une onde écrêtée présentée comme physique.

L'adaptateur part d'un gaz au repos et reçoit les caractéristiques acoustiques issues du débit filtré. Le débit moyen d'échappement demeure dans les volumes 0D ; il n'est pas injecté comme transport moyen dans ce conduit 1D. La conservation vérifiée concerne le conduit numérique et ses flux aux frontières, pas une nouvelle conservation globale d'un échappement moteur entièrement 1D.

## Calibration WAV

`src/calibration.rs` utilise la moyenne de périodogrammes fenêtrés, le rapport régularisé des puissances et une réalisation FIR symétrique. La réponse voulue est lissée sur une bande logarithmique ; le nombre de coefficients et les gains sont bornés. Le principe d'identification fréquentielle et les limites du modèle linéaire sont décrits dans la [documentation de référence MathWorks](https://www.mathworks.com/help/ident/gs/estimate-models-using-frequency-domain-data.html). La [régularisation FIR](https://www.mathworks.com/help/ident/ug/regularized-identification-of-dynamic-systems.html) explique le compromis entre erreur d'ajustement et sensibilité aux données.

Les deux premiers tiers de chaque WAV servent à l'ajustement spectral ; le dernier tiers sert à une mesure séparée. Le retard FIR connu est compensé pour les mesures. La norme L1 des coefficients borne le gain pour tous les signaux et toutes les fréquences, y compris entre les points de la FFT. Une atténuation fixe supplémentaire garde la copie PCM sous 0,95 de crête ; ce facteur est enregistré, sans limiteur dynamique caché.

Le résultat contient `calibrated.wav` et `calibration.json` : coefficients, retard, configuration, erreurs spectrales avant/après, RMS, empreintes SHA256 et chemins des deux WAV originaux. Les fréquences non excitées ne sont pas artificiellement amplifiées pour donner une impression de correspondance. Les fichiers doivent avoir le même taux d'échantillonnage et suffisamment de données ; silence, valeurs non finies et fichiers trop courts sont refusés.

Il faut comparer des enregistrements stationnaires au même régime, à la même charge et dans des conditions de prise de son comparables. La calibration magnitude ne synchronise pas les cycles, n'identifie pas la phase d'un moteur et ne peut pas réparer une mauvaise architecture ou une mauvaise combustion. Une réduction d'erreur sur le dernier tiers confirme uniquement le comportement du filtre sur ces fichiers.

## Reproduction et état de validation

```text
cargo test --release --lib finite_volume
cargo test --release --lib calibration
cargo run --release --example quality_validation -- <nouveau dossier>
cargo run --release --example quality_export -- <autre nouveau dossier>
```

L'exemple mesure 21 combinaisons par résolution évaluée : primaires de 0,23, 0,55 et 0,70 m ; 80, 125, 250, 500, 1000, 2000 et 4000 Hz ; excitation de 10 Pa ; 48 kHz ; 673 K ; 101325 Pa. Il compare la propagation au guide linéaire à retard fractionnaire de BDSP et enregistre l'écart exact, le critère de 2 dB, la phase, le CFL et le coût. Il produit aussi un couple WAV **synthétique**, dont la référence est une transformation FIR connue, puis exécute la véritable API de calibration sur ces fichiers.

### Première mesure : maille maximale 6 mm

Les quatre tests numériques passent en release : état uniforme, tube à choc de Sod et bilan aux frontières, transport périodique conservatif, détente forte positive. Les cinq tests sélectionnés par `calibration` passent, dont trois nouveaux tests de calibration : identité, référence FIR synthétique avec mesure séparée et relecture du profil, rejet des entrées invalides. L'exemple compile et s'exécute. Les résultats complets sont conservés dans [le rapport brut r1](BESS-QUALITY-CALIBRATION-2026-10-03-r1.json).

| Domaine comparé | Écart maximal absolu | Cas responsable |
| --- | ---: | --- |
| 80–1000 Hz, 15 combinaisons | 0,023059 dB | 0,55 m / 1000 Hz |
| 80–4000 Hz, 21 combinaisons | 3,080867 dB | 0,70 m / 4000 Hz |

Le critère prévu de moins de 2 dB sur les bas ordres est satisfait sur les fréquences 80–1000 Hz testées. L'extension à 4000 Hz échoue : −2,543433 dB à 0,55 m et −3,080867 dB à 0,70 m. Ces pertes numériques ne sont pas masquées par une normalisation. Le temps cumulé des 21 comparaisons est 10,348 s pour 3,78 s simulées par solveur ; cette mesure de développement avec d'autres travaux en cours ne qualifie pas le temps réel.

Deux essais du moteur complet commun comparent l'option 1D au guide sur 0,2 s, avec mesure des dernières 0,1 s : aucun arrêt numérique ; écart RMS d'échappement −0,219322 dB à 900 tr/min / charge 0,1 et +0,687912 dB à 3000 tr/min / charge 0,8. Ce sont des contrôles d'intégration courts, sans affirmation de régime thermique établi ni de validité à toutes les charges.

La référence WAV est explicitement synthétique : bruit déterministe traité par `0,8 x[n] + 0,45 x[n−1]`. Sur le tiers réservé, l'erreur spectrale dans la bande 40–12000 Hz passe de **1,330169 à 0,071192 dB RMS** sur 1021 points actifs. Le FIR possède 257 coefficients et un retard de 128 échantillons ; aucune atténuation supplémentaire n'est nécessaire (`output_safety_gain=1`). Les RMS pleine bande sont également publiés, mais ne constituent pas le critère d'ajustement limité à 40–12000 Hz. Aucun enregistrement automobile authentique n'a été fourni ou inventé.

Le plan initial proposait des cellules de 20–40 mm et un CFL de 0,8. Cette implémentation choisit un CFL plus conservateur de 0,4.

### Comparaison de résolution : 20 mm et 4 mm

La [deuxième mesure brute](BESS-QUALITY-CALIBRATION-2026-10-03-r2.json) ajoute `Primary1d::with_cell_size`, réservé à la préparation avant tout calcul. Elle compare les deux mailles sur les mêmes 21 cas chacune, sans autre compilation ni calcul lourd d'un agent en parallèle.

| Espacement maximal demandé | Erreur maximale 80–1000 Hz | Erreur maximale 80–4000 Hz | Calcul primaire : secondes murales par seconde simulée |
| --- | ---: | ---: | ---: |
| 20 mm | 0,555626 dB | 49,730789 dB | 0,351824 |
| 4 mm | 0,011928 dB | 0,861539 dB | 5,665469 |

Le plafond de 2 dB est donc respecté aux basses fréquences testées par les deux mailles. La maille de 20 mm dissipe fortement les aigus : à 0,70 m, −6,4373 dB à 2000 Hz et −49,7308 dB à 4000 Hz. C'est une limite numérique publiée, pas une perte acoustique physique revendiquée. La maille de 4 mm respecte 2 dB sur toute la grille testée, mais coûte environ **16,1 fois** plus sur ces primaires isolés. L'option ne constitue donc pas une amélioration gratuite de toute la bande audible.

Les chronos du moteur I4 complet en 4 mm, hors construction, valent **15,283 s/s** à 900 tr/min et **17,286 s/s** à 3000 tr/min ; le guide de référence vaut respectivement 0,233 et 0,239 s/s. Les deux runs de 0,2 s sont finis sans erreur, avec écarts RMS −0,218634 et +0,707105 dB. Ces observations justifient le classement hors ligne ; elles ne garantissent pas un coût précis pour un autre moteur ou un export de banque complet.

**Choix livré : cible de 4 mm par défaut pour l'option hors ligne.** Le constructeur de l'adaptateur borne la grille à 32–512 cellules ; les géométries du moteur testées respectent 4 mm, mais un appel direct avec un conduit dépassant 2,048 m atteint ce plafond et doit consulter `cell_size_m()` pour sa résolution effective. Le constructeur paramétrable refuse explicitement une résolution qui dépasserait son budget de 512 cellules. La dissipation de presque 50 dB à 4 kHz rend la maille de 20 mm inadaptée à cette option audio, malgré son résultat satisfaisant sur les bas ordres. Le maillage retenu est donc plus fin et plus coûteux que les 20–40 mm initialement envisagés. Le constructeur paramétrable conserve 20 mm pour les diagnostics numériques ; il n'ajoute pas un choix audio non qualifié dans l'interface.

### Qualification du chemin WAV

`examples/quality_export.rs` a été compilé et exécuté sur le snapshot final. Il produit une démonstration I4 de quatre secondes par `RenderEngine`, soit le même transport, mixage, limiteur et décimation que `scratch_wav`. La synthèse tourne à 96 kHz et le fichier sort en PCM24 mono 48 kHz. La mesure CPU a été isolée des autres calculs BESS.

| Seconde | Régime / charge imposés | Trames avec coupure physique | Crête avant fondu final | RMS avant fondu final |
| --- | --- | ---: | ---: | ---: |
| 0–1 | 1500 tr/min / 0,25 | 0 | 0,267024 | 0,075244 |
| 1–2 | 3000 tr/min / 0,8 | 0 | 0,343168 | 0,132031 |
| 2–3 | 3000 tr/min / 0 | 48000 | 0,298046 | 0,088504 |
| 3–4 | 3000 tr/min / 0,7 | 0 | 0,342508 | 0,125723 |

Aucun échec numérique, toutes les phases finies et non silencieuses, coupure effective puis reprise vérifiée. Le temps de rendu hors construction et écriture disque est **71,819174 s**, soit **17,954793 secondes murales par seconde simulée**. Ce résultat confirme un usage hors ligne sur cette machine ; il ne constitue pas une qualification temps réel.

Une relecture indépendante du PCM confirme 192000 échantillons de trois octets, 191672 non nuls et une crête entière de 2878705. Le fondu de présentation de 50 ms est celui de l'export habituel ; aucune normalisation supplémentaire n'a été ajoutée. [Rapport brut du rendu](BESS-QUALITY-CALIBRATION-2026-10-03-export.json).

- WAV : `output/final-readiness-20261003/quality/quality-1d-load-cut-recovery.wav`.
- Rapport local : `output/final-readiness-20261003/quality/quality-export.json`.
- SHA256 du WAV : `b703f1af564365aa8f90c29a3727a9cfa02c3980494c86974f5122f41838241a`.

Les trois WAV de calibration r2 (`synthetic-generated.wav`, `synthetic-reference.wav`, `calibrated/calibrated.wav`) sont également vérifiés en PCM entier 24 bits, mono 48 kHz ; leurs en-têtes WAVE_FORMAT_EXTENSIBLE déclarent le sous-format PCM, pas IEEE float. Ils restent des signaux synthétiques de validation du filtre, distincts du WAV moteur ci-dessus.
