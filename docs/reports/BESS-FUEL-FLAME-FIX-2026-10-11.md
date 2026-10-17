# BESS 0.12 — correction des options carburant et flamme

Suite au [diagnostic](BESS-FUEL-FLAME-INVESTIGATION-2026-10-11.md) et à la nouvelle capture de l'utilisateur : quatre options visibles actives, coefficients carburant/flamme par défaut, puissance effondrée à environ 14 kW. La correction porte sur le moteur commun et le banc ; les options restent indépendantes et enregistrées dans les projets.

## Correction

**Flamme.** Le temps de traversée géométrique `0.7 × alésage / vitesse` était utilisé comme durée complète de libération de chaleur, sans référence de calibration. Le modèle conserve ses sensibilités à pression/température/richesse/dilution/turbulence et sa variation déterministe, mais exprime maintenant la durée de Wiebe par rapport à une référence explicite : 60° à 1 500 tr/min, alésage 86 mm, vitesse moyenne de piston 4,3 m/s. Le délai de développement suit également la vitesse turbulente relative. Ces trois paramètres de référence sont sérialisés/validés et ajoutés par défaut à la lecture des anciens projets.

Cette référence est **un ancrage estimé au modèle historique**, pas un ajustement sur des mesures réelles. Aucun CA50 fixe n'est imposé et aucun multiplicateur de couple n'est ajouté. Les mélanges froids, dilués ou pauvres peuvent toujours brûler lentement ou rester partiellement imbrûlés. L'énergie provient toujours du carburant et de l'oxygène disponibles ; la combustion s'arrête toujours à l'ouverture d'échappement.

**Carburant.** Après la première impulsion initialisée par vitesse/densité, les impulsions suivent les masses nettes d'air frais et de vapeur déjà portée, effectivement échangées par l'admission depuis l'impulsion précédente. Le débit inverse participe au bilan. La coupure carburant ne supprime pas la mesure d'air ; la commande de richesse/coupure est appliquée au déclenchement suivant. Le calcul reste causal, avec une mémoire d'un cycle de remplissage. Le film, les gouttelettes, l'évaporation et leurs inventaires/échanges thermiques sont conservés. La réinitialisation efface aussi cette mémoire.

**Déclenchement de l'injection.** Le nouveau test de dosage à admission restreinte a d'abord échoué à un rapport carburant/air de 0,91983 de sa consigne. La cause était une seconde comparaison flottante de l'angle d'événement reconstruit : l'indice de cycle pouvait avancer, mais l'angle reconstruit dépassait la fin du pas d'une unité d'arrondi et l'impulsion était perdue. La détection suit maintenant uniquement le franchissement d'indice de cycle, avec un angle borné à l'intervalle du pas, et le cylindre utilise exactement le même détecteur que l'injecteur. Le test restreint passe sans relâcher sa tolérance de 5 %.

Un rejeu arithmétique du détecteur d'origine sur 64 cycles, à 48/96/192 kHz, retrouve seulement **59 événements à 1 500 et 3 000 tr/min, 62 à 5 500**, contre les 64 attendus ; à 6 900 les 64 sont trouvés. Le nouveau test du composant vérifie les **64 doses effectivement injectées** sur ces douze combinaisons, avec conservation de masse. Il passe. Ce défaut partagé explique une dépendance au régime/alignement numérique sans dépendance à une architecture moteur particulière ; la reproduction exacte de la première courbe utilisateur demeure distincte de ce diagnostic de composant.

**Banc avec carburant.** Avant d'accepter deux moyennes voisines, chaque point attend au moins quatre constantes de temps de film/gouttelettes (limitées au budget existant de 4 s ; 1 s aux valeurs par défaut) et vérifie aussi la stabilité de MAP. Le chemin sans carburant conserve son critère historique. Le statut explicite de non-convergence au budget reste à livrer sous X-041 ; cette modification ne prétend pas résoudre toutes les dérives thermiques.

## Comparaison fonctionnelle

Moteur I4 de référence, 25 points de 850 à 6 999 tr/min, plein gaz, avec les valeurs par défaut de la capture. Le mode `options` utilise la politique de cadence du banc : 8 kHz sans rétroaction, 48 kHz avec rétroaction, sous-pas gaz à 96 kHz dans les deux cas.

| Options après correction | Puissance maximale | Couple minimal | Points négatifs |
| --- | ---: | ---: | ---: |
| Aucune | 103,01 kW | 134,50 Nm | 0 |
| Carburant seul | 92,46 kW | 116,55 Nm | 0 |
| Flamme seule | 106,02 kW | 139,65 Nm | 0 |
| Carburant + flamme | 94,23 kW | 120,36 Nm | 0 |
| Carburant + admission | 90,59 kW | 96,63 Nm | 0 |
| Carburant + torsion | 92,46 kW | 116,55 Nm | 0 |
| Les quatre options visibles | 92,15 kW | 100,89 Nm | 0 |
| Avec rétroaction d'échappement également | 94,67 kW | 111,33 Nm | 0 |

Le balayage final sous `output/fuel-flame-fixed-options/`, après correction du déclenchement d'injection, retrouve exactement ces chiffres. Le régime est intégré dans le moteur complet, contrairement à l'angle calculé directement dans le test de cylindre : la perte d'événements dépend de l'alignement numérique, et ne survenait pas dans ces courbes synthétiques particulières.

À 8 kHz avant correction, la flamme seule donnait 17,46 kW et le cumul carburant/flamme 13,92 kW, avec respectivement un et sept points négatifs. Le balayage sans options demeure identique. L'admission et la torsion ne doivent pas être désignées comme causes de la première capture carburant seul : l'utilisateur a confirmé que la flamme y était désactivée, et ce cas exact reste sans projet enregistré pour reproduction.

Le test du cylindre à admission restreinte vérifie que la masse de carburant suit la masse d'air mesurée, avec conservation de masse/énergie ; le test du banc vérifie séparément carburant, flamme, cumul et quatre options visibles. Ce sont des contrôles fonctionnels synthétiques, pas des mesures d'un moteur réel.

## Reproduction

```powershell
cargo run --release --locked --example fuel_flame_stability -- output/fuel-flame-fix-options options
cargo run --release --locked --example fuel_flame_stability -- output/fuel-flame-fix-final
cargo test --release --locked --all-targets --jobs 1 -- --test-threads=2
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

CSV et journaux locaux conservés dans `output/fuel-flame-fix-*` et `output/fuel-flame-fixed-*`. **176 tests physiques release passent**, avec quatre diagnostics ignorés ; les quatre tests du banc et les six tests d'intégration des options passent aussi, dont le partage exact live/WAV sans allocation sur le chemin chaud. Clippy strict sur toutes les cibles et le contrôle de format passent.

La suite générale a rencontré un manque de mémoire dans la compilation LLVM, même à concurrence réduite. Après libération effective de la mémoire, la reprise complète réussit : **488 tests release passés, zéro échec, six diagnostics ignorés, 63 suites** (`output/fuel-flame-fix-tests-verified.log`, sortie 0). Le test d'affichage du panneau réel passe aussi sur le code final. Les tentatives interrompues/échouées ne sont pas comptées comme des validations ; les six diagnostics historiques exclus restent exclus, sans nouveau passage de leurs portes de qualification.

## Exécutable du Bureau

`cargo build --release --locked --bin bess --jobs 2` réussit. L'exécutable corrigé 0.12.0 est installé dans `D:/Desktop/BESS.exe`, avec sauvegarde de l'ancien et comparaison SHA-256 du résultat de compilation et de la copie installée :

- Nouveau : `A3374E84A49A83EBFD12DB9B56EC6BEFB025FB1B39CA8505D72336D6857F485A`.
- Ancien : `003C8CA32BE817038C3D59514C37FAAB0AAFBE0CB0111137FCF5239360978648`.
- Sauvegarde et reçu : `output/fuel-flame-fixed-desktop-20261011-193226/`.

Le fichier est donc effectivement différent de la version précédente. La version déclarée reste 0.12.0 ; aucune nouvelle publication Git n'est effectuée.

La calibration authentique, les transitoires étendus et l'acceptation de l'utilisateur restent ouverts sous W-012/W-013. X-040 (graphe bornant les valeurs négatives à zéro) reste distinct et non corrigé par ce lot. Aucun commit/push supplémentaire n'est impliqué par la correction locale.
