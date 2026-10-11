# W-011.1 — Diagnostic séparé de X-028

Deuxième lot du 11 octobre 2026. Le test historique, sa référence 0D et son seuil de **10 %** restent inchangés. L'exemple `wave_qualification` enregistre les configurations complètes, les courbes brutes CSV et les résultats JSON; ses diagnostics ne remplacent pas la qualification.

## Trois niveaux distincts

1. **Propagation isolée à état constant.** Deux `bdsp::delay::DelayLine`, les primitives utilisées par les primaires acoustiques, sont lus avant écriture. Une impulsion parcourt une longueur puis revient après réflexion à un bout à pression imposée. Le barycentre du premier retour est comparé à `2 L / sqrt(gamma R T)`, pour 0,49/0,70/0,98 m à 48/96 kHz. Ceci isole le délai géométrique; les filtres, collecteurs, pertes, variations de température et convection du moteur complet sont absents. Cela ne mesure pas un pic de couple.
2. **Soupape à états constants.** `ValveFlow` et `WavePort` de production résolvent ensemble débit et pression pour 24 combinaisons de pression cylindre, impédance et superposition linéaire/non linéaire. Le coefficient de réflexion incrémental est calculé autour de l'arrivée nulle; les deux relations doivent fermer. La passivité incrémentale ainsi vérifiée n'est pas un budget énergétique de tout le moteur.
3. **Moteur complet.** Le banc commun réalise les mêmes 31 points que le test historique, longueurs ×0,7/1/1,4, à 48 et 96 kHz. Le ratio est comparé à la référence 0D originale et, séparément, à une référence anéchoïque conservant l'impédance locale. Les données des trois longueurs sont exportées même quand la première échoue; le test historique s'arrête à son premier échec.

Le diagnostic supplémentaire de fenêtre fixe cherche seulement entre `0,8 × longueur` et `1,2 × longueur` et utilise les mêmes points pour tous les facteurs de cette recherche. Il teste la sensibilité à la fenêtre variable de l'estimateur; sa plage restreinte et la référence anéchoïque n'autorisent aucune substitution au critère accepté.

## Première exécution observée

`cargo test --example wave_qualification` : trois tests réussis, y compris récupération d'un décalage synthétique connu par l'estimateur historique. `cargo run --example wave_qualification -- output/simulation-second-20261011/waves-dev` termine avec JSON et CSV valides.

L'erreur maximale de délai est **0,00002334 %**; résidu de débit maximal **2,265e-12 kg/s**, résidu de pression nul dans l'arrondi f64 enregistré. Les réflexions incrémentales des 24 cas sont dans [-1,1] à la tolérance du test. Ce sont des résultats numériques synthétiques.

| Cadence | Référence historique 0D | Longueur | Erreur de facteur | Fenêtre fixe diagnostique |
|---|---|---:|---:|---:|
| 48 kHz | inchangée | ×0,7 | −11,29 % | −11,38 % |
| 48 kHz | inchangée | ×1,4 | −13,42 % | −13,07 % |
| 96 kHz | inchangée | ×0,7 | −11,29 % | −11,38 % |
| 96 kHz | inchangée | ×1,4 | −13,42 % | −13,42 % |

La référence anéchoïque donne environ +18,94 % à ×0,7 et −13,42 % à ×1,4 aux deux cadences; elle ne résout pas le problème. Le doublement de cadence et la fenêtre fixe ne suffisent donc pas à franchir le seuil. Ces expériences écartent deux explications simples sur cette grille, sans attribuer causalement toute l'erreur à un composant précis.

## Suite

X-028 reste ouvert et les retours restent désactivés par défaut. Il faut encore isoler les effets de l'état moyen variable, des filtres/collecteurs et de la superposition non linéaire sur le moteur complet, puis confronter les résultats au réseau 1D et à une référence authentique. Aucun coefficient physique ni seuil n'est modifié pour faire passer ce diagnostic.

Commande reproductible optimisée :

```powershell
cargo run --release --example wave_qualification -- output/simulation-second-20261011/waves-release
```

## Rejeu release final

Les trois tests dédiés passent dans la suite commune de **483 réussites**, Clippy strict et formatage. L'exemple optimisé termine en 14,877 s, avec les mêmes facteurs/échecs et les configurations complètes dans `output/simulation-second-20261011/waves-release`. Le test historique ignoré est aussi rejoué séparément sur le nouveau harness : code 101, premier échec ×0,7 inchangé; il n'est pas compté comme une réussite. [Preuves finales](BESS-SIMULATION-SECOND-BATCH-2026-10-11.md#validation-optimisée-finale).
