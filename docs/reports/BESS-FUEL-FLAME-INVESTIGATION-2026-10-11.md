# BESS 0.12 — investigation carburant / propagation de flamme

## État et périmètre

Investigation du code de `main` (`de72eb4`) et du moteur commun de BESS 0.12.0. Le défaut de propagation est reproduit ; les trous signalés avec le carburant seul restent à reproduire. L'utilisateur confirme explicitement que la flamme était **désactivée** dans la première capture et que le problème ne dépend pas du moteur choisi. Ce cas reste distinct du cumul des deux options.

Aucune correction de physique, de banc ou de graphe n'est livrée ici et aucune publication supplémentaire n'est effectuée. Un exemple de diagnostic reproductible et un contrôle automatique de l'affichage des coefficients sont ajoutés.

Après l'investigation, à la demande de l'utilisateur, `cargo build --release --locked --bin bess` réussit et `D:/Desktop/BESS.exe` est remplacé par le résultat vérifié. SHA-256 : `003C8CA32BE817038C3D59514C37FAAB0AAFBE0CB0111137FCF5239360978648`, identique au précédent exécutable : les ajouts de diagnostic ne modifient pas le programme livré. Sauvegarde et reçu conservés dans `output/desktop-refresh-20261011-163049/`. Version 0.12.0, défauts physiques toujours ouverts.

## Résultats du moteur de référence

I4 standard, plein gaz, 25 points de 850 à 6 999 tr/min, configurations identiques hors options. Puissances maximales du banc à 48 kHz :

| Options | Puissance maximale | Couple minimal | Points de couple négatif |
| --- | ---: | ---: | ---: |
| Aucune | 103,23 kW | 134,27 Nm | 0 |
| Carburant seul | 99,15 kW | 126,37 Nm | 0 |
| Flamme seule | 17,02 kW | −0,72 Nm | 1 |
| Carburant + flamme | 14,09 kW | −10,09 Nm | 7 |

Les balayages à 8 kHz donnent respectivement 103,01 / 99,12 / 17,46 / 13,92 kW. La baisse majeure de puissance persiste aux deux cadences. Ces deux cadences utilisent cependant le même pas de gaz à 96 kHz ; elles excluent une cause liée à la seule cadence audio, sans démontrer la convergence temporelle de la physique.

Un second balayage de 13 points, avec **au moins 2 s de stabilisation puis 1 s de moyenne sur des cycles complets**, donne 103,52 / 92,68 / 17,22 / 13,50 kW. Attendre davantage ne résout pas la combustion prédictive. Le carburant seul change davantage avec cette procédure, ce qui justifie une qualification spécifique du critère d'arrêt du banc.

Sur les dernières fenêtres des essais stationnaires de 5 s, la flamme seule donne environ 47,51 / 23,44 / 0,54 Nm à 3 000 / 5 500 / 6 900 tr/min. Le cumul donne 44,18 / 8,00 / −4,77 Nm. Aucun nouveau raté compté dans ces dernières fenêtres : une combustion trop tardive ou incomplète peut perdre son travail utile sans être comptée comme un échec d'allumage. Ces fenêtres de 0,1 s ne sont pas toutes des cycles complets.

## Cause reproduite : phasage de la flamme

`src/physical/combustion_model.rs:63` estime la durée par `0.7 × alésage / vitesse turbulente`. `src/physical/cycle.rs:747` convertit cette durée en angle et l'utilise comme durée complète de la loi de Wiebe ; son CA50 est construit à partir de l'étincelle fixe, du délai de développement et de la fraction de durée correspondant à 50 % de combustion. L'ouverture de l'échappement termine ensuite la combustion.

Le cylindre isolé de diagnostic, à réservoirs imposés de 101 kPa, sans variation stochastique, mesure après 2 s :

| Options | CA50 à 3 000 tr/min | CA50 à 5 500 tr/min | CA50 à 6 900 tr/min |
| --- | ---: | ---: | ---: |
| Aucune | 11,25° après PMH | 14,62° | 16,03° |
| Carburant seul | 12,94° | 16,25° | 17,60° |
| Flamme seule | 88,07° | 100,78° | Seuil de 50 % non atteint |
| Carburant + flamme | 96,47° | Seuil non atteint | Seuil non atteint |

La durée prédite avec la flamme seule vaut environ 215–220° de vilebrequin ; avec les deux options, 229–234°. La chaleur arrive très tard dans la détente et peut rester partiellement non libérée à l'ouverture de l'échappement. C'est cohérent avec l'effondrement du couple dans le moteur complet. **Les conditions aux limites du cylindre isolé diffèrent du moteur complet** : ses CA50 ne sont pas des mesures directement extraites des courbes de puissance.

La corrélation et ses coefficients sont estimés. Les tests existants vérifient les tendances et la conservation de la chaleur effectivement consommée, sans imposer une enveloppe fonctionnelle de durée/CA50/couple. Ils ne qualifient donc pas le comportement montré par les captures. Corriger nécessite de revoir la relation vitesse de flamme / fraction brûlée / durée, puis de contrôler le phasage et le carburant imbrûlé sur une plage de régime et de charge. Forcer arbitrairement le CA50 ou multiplier le couple masquerait cette erreur.

## Carburant seul : signalement confirmé, reproduction manquante

Sans flamme, six variantes de géométrie/came/compression, sept variantes d'admission/rétroaction/turbo et les **14 architectures prédéfinies** ont été balayées avec et sans carburant liquide. Aucun de ces 54 balayages supplémentaires ne contient de couple négatif. Cela ne réfute pas le signalement : les réglages enregistrés et les autres interactions de son projet restent nécessaires pour reproduire exactement le cas.

Le dosage liquide est calculé à partir du déplacement géométrique et de l'état instantané de l'admission (`cycle.rs`, calcul de `dose`), alors que la voie vapeur historique suit la masse d'air réellement admise. Cette différence est une piste à qualifier sur le remplissage, la richesse et les transitoires ; elle n'est pas une cause démontrée des trous de la première capture.

Le contrôle automatique `displaying_simulation_models_preserves_saved_coefficients` affiche dix images du panneau réel, carburant actif avec/sans flamme, sans interaction. Les coefficients sérialisés restent strictement identiques : aucun changement involontaire par affichage/arrondi n'est reproduit pour ces valeurs par défaut. Ce contrôle ne simule pas les actions natives de l'utilisateur ni tous les réglages personnalisés.

## Deux défauts auxiliaires constatés dans le code

1. **Graphe** : `src/main.rs:1601` ramène toute valeur négative à zéro par un bornage de la coordonnée verticale. Des plateaux visuels à zéro peuvent donc correspondre à un couple réellement négatif. Les valeurs négatives existent dans les CSV ; l'affichage devrait les montrer ou les signaler.
2. **Banc** : `src/dyno.rs`, `sweep_at_rate_with`, peut terminer dès que deux moyennes de couple consécutives sont proches ; en atmosphérique chaque fenêtre ne couvre qu'un cycle. Le film, la richesse, la température et la MAP ne participent pas au critère. Après 4 s, le résultat est retourné sans statut de non-convergence. Ajouter une durée minimale adaptée aux états lents et une qualification de convergence est nécessaire. Cela ne résout pas à lui seul le défaut de flamme, qui persiste dans les essais prolongés.

## Reproduction et traces

Exemple : `examples/fuel_flame_stability.rs`, sans périphérique audio. Depuis la racine du dépôt :

```powershell
cargo run --release --locked --example fuel_flame_stability -- output/fuel-flame-investigation-final
cargo run --release --locked --example fuel_flame_stability -- output/fuel-flame-metering-final metering
cargo run --release --locked --example fuel_flame_stability -- output/fuel-flame-presets presets
# Après réception du chemin du projet, conserver ses réglages et désactiver
# séparément chaque option pour attribuer le défaut :
cargo run --release --locked --example fuel_flame_stability -- output/fuel-flame-project project 'CHEMIN_DU_PROJET.json'
cargo test --release --locked --bin bess displaying_simulation_models_preserves_saved_coefficients
cargo clippy --locked --example fuel_flame_stability --bin bess --tests -- -D warnings
cargo fmt --all -- --check
```

Les trois modes synthétiques ont été exécutés ; le mode projet compile mais attend le projet utilisateur pour une exécution représentative. Les traces locales ignorées par Git sont conservées sous `output/` : courbes, balayages prolongés, fenêtres stationnaires, étincelles, CA50 et matrices de réglages. Le rapport conserve les résultats principaux indépendamment de ces fichiers générés. Le test ciblé, Clippy strict et le contrôle de format passent. Aucun nouvel essai matériel, écoute ou contrôle BeamNG n'est revendiqué.

Prochaine étape : rejouer un projet montrant les trous avec **carburant actif et flamme désactivée**, puis corriger et qualifier séparément le phasage prédictif, le banc et son affichage. W-012/W-013 et leurs portes de qualification restent ouverts.
