# W-006.1 — fondation thermodynamique, 28 septembre 2026

Implémentation : `src/physical/thermo.rs`. Ce lot est un socle de calcul ; il ne remplace pas à lui seul la source sonore scratch.

## Grandeurs et bilan

- Cinématique bielle-manivelle exacte en `f64`, volume et dérivée analytique par radian ; angle zéro au PMH. Géométries impossibles rejetées à la construction.
- État gazeux conservatif `(m, U, V)`, pression idéale `p = mRT/V`, entrées et sorties d'enthalpie, chaleur de combustion, échange thermique signé et travail de piston.
- Travail `p dV` trapézoïdal ; couplage pression/température résolu analytiquement sans allocation ni itération. L'enthalpie sortante utilise la température au début du pas.
- Masse sortante limitée à 90 % du donneur ; le receveur doit reprendre la masse réellement retournée dans le bilan. Température protégée entre 200 et 3500 K. Toute énergie ajoutée ou retirée par ces protections est comptée séparément dans `numerical_correction_j`.
- Wiebe normalisée sur une durée finie : le CA50 demandé correspond exactement à la moitié de l'énergie totale. L'intégration par différence de fractions conserve la chaleur lorsque le pas traverse le début ou la fin de combustion. Les angles de combustion sont déroulés, sans remise à zéro au PMH.
- Échange thermique de Newton avec coefficient fourni explicitement par l'appelant ; Hohenberg et sa calibration restent à intégrer.

## Différence délibérée avec le rapport

Le modèle de capacité calorifique utilise `cv(T) = 650 + 0,16 T` J/(kg K), `R = 287` J/(kg K), et son intégrale `u(T) = 650 T + 0,08 T²` J/kg. C'est une approximation d'ingénierie d'un gaz de type air, pas la corrélation de Brunt ni un mélange chimique validé. Le gamma est variable et cohérent avec l'énergie : `gamma = 1 + R/cv(T)`.

Le test de compression vérifie donc la conservation d'entropie de cette loi (`650 ln(T/T0) + 0,16(T−T0) + R ln(V/V0) = 0`) et non une égalité artificielle avec `p0 CR^1,3`. Pour le cas testé CR=10, la pression au PMH reste entre `p0 CR^1,35` et `p0 CR^1,42`. Le remplacement par des propriétés de mélange plus précises reste un travail distinct ; conserver `u(T)` comme intégrale de `cv(T)` est nécessaire.

## Vérification observée

`cargo test --release --lib physical::thermo` : **8 tests réussis**, 0 échec, environ 0,17 s pour les tests sur cette machine (temps de compilation exclu).

- Volume aux points morts et dérivée comparée à une différence centrée sur 1 000 angles.
- Cohérence dérivée de l'énergie / capacité calorifique et inversion énergie/température.
- Compression et détente entraînées sur 7 200 pas : retour d'énergie relatif inférieur à `1e-8`, sans correction thermique significative.
- Cycle fermé avec combustion et paroi : travail net positif, erreur d'énergie relative inférieure à `1e-8`, aucune énergie de protection significative.
- Apport de chaleur isochore et transfert de masse/enthalpie.
- Protections, bilan explicite de correction, refus des entrées invalides sans mutation.
- CA50 exact et totalité de l'énergie de combustion retrouvée.
- **10 000 géométries déterministes**, chacune sur 128 pas, balayage exécuté deux fois avec empreinte bit à bit identique et états finis. Ce test inclut des conceptions extrêmes où les protections thermiques peuvent intervenir ; il prouve la robustesse numérique dans cette enveloppe, pas leur réalisme physique.

La durée de toute la suite n'est pas un benchmark du futur moteur V12 ni une mesure du callback audio. Admission/échappement couplés, évolution libre du régime, propriétés brûlé/frais, stabilité acoustique, CPU sous charge et écoute restent à vérifier dans les lots d'intégration.
