# W-014.3 / W-015.2–3 — Qualification cartes, actionneurs et torsion

Deuxième lot exécuté le 11 octobre 2026 sur Windows, AMD Ryzen 7 3800X. Toutes les cartes et réponses utilisées restent synthétiques et non calibrées. Les sources relatives, conversions de force, raideurs et amortissements restent estimés.

## Défauts reproduits puis corrigés

1. **Capacité turbine :** le mapper utilisait un Cd de 0,8 et le réseau gaz 0,9. Une surface ainsi calculée produisait 12,5 % de débit supplémentaire. Le mapper utilise désormais 0,9 ; la reconstruction du débit est vérifiée sur 81 points pression/température/vitesse/composition.
2. **Turbine fermée :** une carte de débit nul à pression positive fournissait une surface nulle ; le réseau utilisait cette valeur comme commutateur vers sa restriction historique. Le réseau conserve désormais explicitement sa topologie turbo. Un test injecte du gaz dans le collecteur et vérifie pression croissante et transfert aval exactement nul avec turbine et wastegate fermées. Aucun plancher artificiel de surface n'a été ajouté.
3. **Arrêt torsion intra-pas :** la projection terminale des vitesses pouvait produire un déplacement négatif et un travail de friction de signe négatif avant l'arrêt. Le pas résout désormais le premier événement zéro-vitesse, puis les contraintes statiques de chacun des deux rotors. Le déplacement reste vers l'avant et un rotor maintenu au repos ne reçoit aucun travail de sa charge. Le modèle reste explicitement celui d'un moteur tournant vers l'avant, sans kickback inverse.

Les comportements sans cartes/torsion gardent leurs chemins historiques. Un banc imposant RPM avec torsion activée reste bit-identique au banc rigide pour gaz, couple, carburant, admission, échappement et mécanique.

## Interfaces et bilans

`Induction::set_turbine_composition` partage les propriétés du gaz avec le calcul de surface et le travail turbine. La voix commune l'appelle uniquement pour une carte activée ; les anciens appelants compatibles conservent leur hypothèse PRODUCTS. La préparation de surface utilise une composition moyenne des banques, puis le calcul de travail utilise leurs débits sortants réels ; cette réduction à un turbo commun demeure une approximation lorsque les banques diffèrent fortement.

`Induction::initialize_temperature` prépare la charge froide/chaude à pression atmosphérique avec masse d'air cohérente et arbre/débit/actionneurs nuls. La paroi cartographiée reste une frontière thermique prescrite, pas une température de métal prédite ; un scénario uniforme doit choisir explicitement sa température de paroi.

Les actionneurs wastegate et BOV suivent leur réponse analytique du premier ordre à 96/192 kHz. Le test de wastegate utilise une pression prescrite pour isoler l'actionneur ; son reset de réservoir ne fait pas partie des comptes gaz des autres scénarios.

L'intégration modale conserve exactement les mêmes états, travail et dissipation quand seul le gain de rayonnement change. Un test moteur compare également la réponse importée et la réponse historique : la mécanique sonore change, tandis que rotation libre, gaz, carburant, admission et échappement restent bit-identiques. Les positions de la fixture sont capteur `[0,0,0]` m et écoute `[1,0,0]` m ; ces positions documentent les coefficients importés, sans prédire un déplacement arbitraire du microphone.

## Grilles exécutées et convergence

`cargo run --example simulation_maps_torsion_grid -- work/simulation-p5-p6-grid` produit une synthèse reproductible, avec profil de compilation, CPU d'exécution, temps mural et diagnostics d'événement. Un garde de dénormalisation est activé comme dans les autres rendus BESS. Le root peut rejouer cette même commande en release pour la suite commune.

- **27 cas turbo :** 96/192/384 kHz, températures initiales 260/300/450 K, demandes 0/0,04/0,12 kg/s, échelon et lever de pied. Résidus maximaux : masse `7,12e-15` kg, arbre `4,04e-14` J ; corrections thermodynamiques cumulées au plus `6,01e-9` J. Le compte d'énergie charge intégré ferme dans la précision observée. Les puissances turbine utilisent le débit réel de leur tuyère équivalente et de la wastegate du banc, pas un débit imposé indépendant de la carte.
- **Convergence turbo 96→192 puis 192→384 kHz :** erreur relative maximale de pression terminale `7,88e-6` puis `3,93e-6`, vitesse arbre `2,38e-5` puis `1,19e-5`, chaleur paroi `2,12e-4` puis `1,07e-4` (normalisation `max(abs(référence),1)`). Les écarts décroissent ; cela ne valide aucune mesure matérielle.
- **27 cas torsion :** 48/96/192 kHz, RPM initiaux 0/40/3000, charges 0/40/400 Nm, couple 40 Nm puis coupure. Résidu de travail maximal `9,17e-10` J ; différence maximale RPM terminale 48→96 kHz `0,00055` RPM.
- **3 cas structure :** excitation relative sinusoïdale 200 Hz, 48/96/192 kHz. RMS `0,000277928`, `0,000277908`, `0,000277904` ; résidu modal instantané inférieur à `3,19e-18` J et compte intégré inférieur à `4,36e-15` J.

Le banc torsion déclenche 30 événements sur 604 800 pas, avec un maximum de 1 événement par pas observé et 1560 itérations de recherche au total. Chaque événement utilise 52 itérations bornées ; les compteurs `stop_events` et `event_solver_iterations` restent accessibles. Les cas au repos statique n'effectuent aucune recherche. Le chemin normal n'alloue pas et utilise un seul solveur implicite ; la recherche ajoute un coût local pendant les arrêts. Ces comptes et temps **dev** ne constituent pas une qualification de la cible temps réel V12 ou du portable.

## Vérification actuelle et reste

Les tests dev ciblés passent : cartes/nozzle/actionneurs/init/spool, quatre tests torsion incluant grille de 36 stalls et rotor bloqué, trois tests structurels et trois tests d'intégration moteur/topologie. La suite release commune est exécutée après gel des sources; résultats finaux ci-dessous.

La confrontation matérielle des cartes et vibrations, le flux inverse/surge hors domaine, les transitoires thermiques du métal, une identification torsionnelle du matériel, l'endurance temps réel et l'écoute restent ouverts. Les comptes numériques, la convergence et les WAV n'établissent ni fidélité acoustique ni réception BeamNG.

## Rejeu optimisé final

**483 tests release réussis**, Clippy strict tous targets et formatage. Les 57 cas sont rejoués en release dans `output/simulation-second-20261011/maps-torsion-release`, en 0,785 s murale. Résidus maximaux : masse turbo 7,118e-15 kg, arbre 4,031e-14 J, torsion 9,164e-10 J; 30 événements/1560 itérations, maximum 1 événement par pas. Le banc commun cartographié/sans carte est reconstruit et rejoué, avec projets et WAV dans `mapped-common-release`; le cas cartographié atteint 213 628 Pa / 113 761 tr/min d'arbre sur cette fixture transitoire. Le modèle sans carte conserve 184 040 Pa / 171 887 tr/min. Les gains d'audition restent hors de la simulation. Le WAV par défaut du nouvel exécutable est byte-identique à la référence antérieure. [Preuves finales](BESS-SIMULATION-SECOND-BATCH-2026-10-11.md#validation-optimisée-finale).
