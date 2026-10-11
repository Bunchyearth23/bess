# W-011 — Frontières admission et échappement conjointes

11 octobre 2026. Lot technique expérimental, réception mesurée/sonore ouverte.

## Changement livré

`experimental.intake_wave_coupling` ajoute une frontière admission implicite indépendante du prototype échappement existant. Elle est désactivée par défaut et migre à `false` depuis les anciens projets. Les options d'échappement et d'admission peuvent être comparées séparément.

Le réseau d'admission lit ses arrivées avant le pas cylindre (`begin`), fournit une relation caractéristique pression/débit (`port`), puis lance une seule onde à partir du débit effectivement accepté par les volumes de gaz (`finish`). Le cylindre résout conjointement l'orifice et cette impédance; le signe de l'impédance s'oppose au débit entrant. Le filtre retirant le débit moyen utilise exactement la même mémoire à la frontière et au lancement. Les masses/enthalpies/espèces restent transférées une fois par les volumes conservatifs existants, avec leurs budgets de donneur et leurs flux inverses.

La vitesse des ondes et leur impédance utilisent gamma/R, pression et température du mélange du runner. La jonction plenum pondère les runners par leurs admittances physiques, ramenées à l'impédance de l'air ambiant du conduit/airbox. Les interfaces couplées refusent une amplitude non finie ou hors du domaine de ±200 kPa; elles ne masquent pas cette erreur par l'écrêtage du chemin historique.

Le `next` historique garde ses constantes et son arithmétique lorsque la nouvelle option est désactivée. Les gains d'observation, le niveau échappement et le bruit d'air ne participent pas au retour physique. Les adaptations de température et la composition sont celles du modèle réduit; elles ne constituent pas un calcul distribué des espèces ou une calibration de pertes.

## Références exécutables

- `tests/joint_intake.rs` : migration désactivée, admission seule/conjointe à 48/96 kHz, influence physique de la longueur, invariance du couple/MAP/chaleur aux gains d'observation.
- `physical::intake_acoustic::tests::joint_intake_impedance_opposes_flow_and_unforced_network_decays` : signe dissipatif de l'impédance et décroissance après une excitation finie à 48/96 kHz.
- `examples/joint_intake.rs` : douze comparaisons 48/96 kHz, runners 0,25/0,75 m, chemins historique/échappement/conjoint. WAV stéréo admission/échappement, CSV de transitoires et définition Scratch pour chaque comparaison. RPM imposé 3200, papillon 0,3 → 0,85 → 0,03, même graine et gain fixe; statistique sur 0,3–0,4 s. Ces valeurs sont synthétiques et ne sont pas des points stationnaires mesurés.
- Le test historique `primary_length_moves_the_tuned_torque_peak_as_one_over_length` reste intact, ignoré par défaut et exécutable explicitement. Son seuil reste 10 %; une réussite sur un tube ne fermerait pas X-028.

## Validation de cette session

`cargo test --test joint_intake` : **4 tests réussis**, aucune défaillance. La grille couvre I4 puis I6/V8/V12, 48/96 kHz, longueurs extrêmes 0,15/1,5 m, charge puis lever de pied. Trois tests avaient également réussi en release; le quatrième a été corrigé après découverte d'une incohérence du fixture (preset changé sans `apply_design`), puis les quatre ont été rejoués en dev. La suite intégrée release appartient à la validation finale commune.

`cargo run --example joint_intake -- out/simulation-p2` : **douze rendus réussis**. Couple gaz moyen dans la fenêtre transitoire 0,3–0,4 s :

| Chemin | Runner | 48 kHz | 96 kHz |
|---|---:|---:|---:|
| Historique | 0,25/0,75 m | 175,8342 Nm | 175,8335 Nm |
| Échappement seul | 0,25/0,75 m | 170,5406 Nm | 170,7196 Nm |
| Conjoint | 0,25 m | 181,5508 Nm | 181,6484 Nm |
| Conjoint | 0,75 m | 209,8581 Nm | 210,0057 Nm |

L'écart entre taux est 0,054 % et 0,070 % sur les deux cas conjoints. La longueur affecte maintenant les états physiques avec le retour admission; sans ce retour, elle reste une observation sonore. Ces chiffres attestent de l'effet/convergence sur ces cas, pas de la fidélité du couple. WAV/CSV et définitions sont dans `out/simulation-p2`; les coûts enregistrés lors de cette première démonstration sont en **dev**, sans valeur de réception temps réel.

**X-028 demeure en échec.** Le test historique inchangé a été exécuté explicitement sur le binaire release construit dans cette session (`wave_coupling-7c5d03401596fc32.exe`, 14:13:29, heure locale), en 4,05 s : pour longueur x0,7, facteur ajusté **0,6209708** contre **0,69999999**, soit **11,29 %** d'écart; RMS de ratio 0,046, gain maximal 0,110. Le seuil 10 % échoue et le test s'arrête avant x1,4. Aucun seuil ni référence n'a été remplacé pour annoncer une réussite.

## Réception et suite

Le retour admission est raccordé au chemin moteur commun utilisé par l'écoute et les WAV. Le lot ne ferme pas W-011 : X-028, confrontation W-010, grille complète calage/croisement/régime/charge, endurance audio sur matériel identifié et écoute restent requis. La jonction airbox reste une approximation linéaire à air ambiant; une variation du gaz moyen ne vaut pas une preuve de passivité du système complet.

Les sous-pas gaz utilisent une arrivée acoustique commune par trame, comme le prototype échappement. Le lancement repose sur le débit moyen accepté dans cette trame; sa convergence temporelle sur une grille plus large reste à qualifier. Le mode 1D de production et le retour échappement restent incompatibles tant que leurs références thermodynamiques ne sont pas partagées.

## Validation commune finale

Le lot intégré passe 459 tests release, Clippy strict et formatage. Les exemples sont reconstruits et rejoués en release après le correctif de persistance exacte des coefficients JSON; tous les rendus terminent sans erreur. Les preuves et répertoires courants sont dans le [bilan commun](BESS-SIMULATION-PARALLEL-2026-10-11.md). Les critères mesurés, sonores et de performance indiqués ci-dessus restent ouverts.
