# W-010 / P1 — Référence publique et identification hors ligne

L'utilisateur a choisi une référence publique documentée le 11 octobre 2026. Le banc est reproductible et conserve une comparaison non ajustée. La calibration réelle reste ouverte : deux valeurs constructeur ne qualifient ni les transitoires, ni la combustion, ni le son.

## Référence retenue

La [fiche officielle Honda GX160](https://www.honda-engines-eu.com/en/products/power-units/gx160), consultée le 11 octobre 2026, indique un monocylindre essence quatre temps OHV, alésage/course 68/45 mm, 163 cm³, compression 9:1, couple net maximal 10,3 Nm à 2500 tr/min et puissance nette 3,6 kW à 3600 tr/min. Honda rattache la puissance nette à SAE J1349 et précise la variabilité des exemplaires et installations. Le second couple est **dérivé** : `3600 W / (3600 × 2π/60) = 9,5493 Nm`. Les puissances continues publiées ne sont pas utilisées comme points de pleine charge.

`examples/references/honda-gx160.json` enregistre la source, les canaux manquants et les hypothèses du banc. La géométrie, la compression et l'architecture connues sont fixées. Cam 0,15, échappement 35 mm (minimum du modèle), géométries internes, températures, pertes, silencieux, inertie et limites de régime restent estimés. Papillon 1 est une hypothèse de comparaison, pas une mesure Honda.

Le point 2500 est désigné identification et le point 3600 validation réservée avant le premier rendu. Aucune identification n'est effectuée sur ces valeurs publiées. Le seuil exploratoire est déclaré auparavant : 1,03 Nm absolu aux deux points, soit 10 % du couple maximal publié. C'est un objectif d'ingénierie, pas une incertitude constructeur ni un critère de réception utilisateur.

Les [séries publiques Bosch envisagées](https://github.com/boschresearch/Bosch-Engine-Datasets) sont standardisées sans facteurs permettant de retrouver les unités physiques; elles n'ont pas été utilisées comme couple réel. Une [archive de pression à mélange méthane/CO₂ ultra pauvre](https://data.mendeley.com/datasets/rg4tyrxv2j/1) dépasserait le domaine essence actuel. Aucun canal absent n'est remplacé par une donnée synthétique portant l'étiquette mesurée.

## Outils livrés

`measured_reference` fournit un schéma versionné borné, canaux SI optionnels, import validé, séparation identification/validation, comparaison par canal, recherche par coordonnées bornée, alignement par corrélation avec décalage maximal et analyse des ordres vilebrequin à régime constant avec fenêtre Hann. Les observations réservées ne participent jamais à l'objectif d'ajustement. Les entrées synthétiques et valeurs constructeur sont distinguées des acquisitions instrumentées. Une réussite sur un seul sous-ensemble ne suffit pas à `measured_qualification`.

`examples/measured_engine.rs` rend le moteur physique commun, estime le couple frein comme couple gaz moyen moins frottement, conserve MAP et laisse lambda absent. Aucun gain sonore ne remplace une variation de paramètre physique. Les résultats ne prouvent pas l'équivalence des accessoires et de la correction atmosphérique avec le banc normalisé Honda.

Commandes reproductibles :

```powershell
cargo run --release --example measured_engine -- output/simulation-parallel-20261011/p1-gx160 examples/references/honda-gx160.json
cargo run --release --example measured_engine -- output/simulation-parallel-20261011/p1-synthetic
```

Le mode sans fichier démontre l'identification bornée de compression sur une fixture I4 explicitement synthétique, avec un point réservé. Le mode constructeur conserve l'EngineDefinition non ajustée; la sensibilité porte sur la came estimée (±0,1), jamais sur la géométrie connue. Les deux modes produisent JSON de comparaison, CSV de sensibilité, WAV exhaust brut et ordres du signal **simulé**. L'absence d'audio public synchronisé interdit une conclusion de fidélité sonore.

## Premier résultat public

Banc imposé 96 kHz, stabilisation 2 s puis moyenne 2 s par point, options nouvelles absentes. Rendu dev sur Ryzen 7 3800X :

| Régime | Couple public / dérivé | Couple BESS non ajusté | Écart | Seuil fixé avant rendu |
|---|---:|---:|---:|---:|
| 2500 tr/min | 10,3000 Nm | 13,7470 Nm | +3,4470 Nm (+33,5 %) | 1,03 Nm, échec |
| 3600 tr/min réservé | 9,5493 Nm | 13,9897 Nm | +4,4404 Nm (+46,5 %) | 1,03 Nm, échec |

Ces erreurs sont conservées dans `output/simulation-parallel-20261011/p1-gx160/before.json`. La came 0,05/0,15/0,25 produit respectivement 13,947/13,747/13,549 Nm à 2500 : cette seule sensibilité ne justifie pas un ajustement arbitraire jusqu'au couple cible. La normalisation des conditions, la géométrie admission/échappement, les cames et pertes doivent être mieux renseignées avant identification réelle.

## Réception encore ouverte

Validation finale : cinq tests dédiés et suite commune **459 tests release réussis**, Clippy strict et formatage. Les deux démonstrations sont rejouées en release après activation de la persistance JSON exacte; la comparaison publique conserve les mêmes erreurs et le point réservé échoue. La fixture récupère compression 9,984375 contre 10,0 connue, en 24 évaluations, avec point réservé réussi mais `measured_qualification:false`. Résultats courants dans `output/simulation-parallel-20261011/p1-gx160-release/` et `p1-synthetic-release/`.

Le format stationnaire actuel couvre couple/MAP/lambda, et l'alignement concerne des signaux déjà acquis en mémoire; import WAV synchronisé, traces pression-angle/CA50, thermique, enrichissement, cartes turbo et réponses structurelles réelles restent à ajouter quand des références compatibles existent. Les tolérances finales doivent tenir compte de leur incertitude. X-018/X-025, écoute et BeamNG restent ouverts. Les sorties de ce banc n'activent aucun modèle expérimental par défaut.
