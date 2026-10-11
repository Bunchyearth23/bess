# BESS — W-013 / P4 : combustion dépendante de la charge

11 octobre 2026. Modèle réduit expérimental ; aucune calibration moteur réelle
ou qualification du cliquetis n'est déduite de cette livraison.

## Comportement livré

`experimental.predictive_combustion = Some(PredictiveCombustionConfig)` remplace
le réglage empirique de durée/CA50 au déclenchement d'étincelle. `None` garde le
calcul historique. La configuration `Copy` et sérialisable suit le chemin commun
de la voix physique, avec ou sans `fuel_transients`.

Une vitesse de flamme laminaire réduite dépend de pression, température,
richesse et dilution. Les exposants et références sont visibles et bornés ;
ils restent des estimations. Une vitesse turbulente ajoute une contribution
estimée à partir de la vitesse moyenne du piston. Le rayon de propagation
utilise la dimension de l'alésage ; le délai de développement dépend du même
état de charge. La variation corrélée et la graine propre au cylindre perturbent
ces temps au seul événement d'étincelle. Une probabilité de stabilité tient
compte du froid, de la dilution et de la richesse lorsque la variation est active.

Cette fermeture ne représente ni chimie détaillée, ni champ de turbulence, ni
front spatial. Le moteur demeure une zone thermodynamique unique, et la courbe
normalisée de Wiebe distribue l'énergie du carburant réellement disponible.
Le délai positif place toujours le début du dégagement après l'étincelle.
L'ouverture d'échappement termine la réaction ; le carburant restant reste
transportable. L'enveloppe angulaire de Wiebe demeure 0,001–720° ; des charges
trop lentes sont tronquées par cette enveloppe et l'ouverture d'échappement,
et ne deviennent pas des combustions complètes artificielles.

## Bilans et observation

La réaction consomme au plus le carburant vapeur et l'air frais disponibles.
Le bilan chimique reste `chaleur = masse_brûlée × 43 MJ/kg`. Le film liquide ne
brûle pas directement et ne se transforme pas en une source d'énergie sonore.
Les champs `predicted_burn_duration_s` et `predicted_flame_speed_m_s` exposent
l'estimation lors des événements d'allumage, avant la limitation éventuelle par
l'angle admissible et la sortie d'échappement.

Le nouveau modèle ne retouche pas l'auto-inflammation, les seuils de cliquetis,
l'octane ni la commande anticliquetis existante. Sans traces réelles d'avance
limitée par le cliquetis, X-025 reste ouvert.

## Bancs et réception

`examples/fuel_combustion.rs` produit un témoin et trois variants enregistrés,
avec WAV au même gain et traces de couple/MAP/chaleur/ratés/correction. Les CSV
cylindre exposent pression/angle, richesse et inventaires pour les départs
froid/chaud et les reprises. Tous sont explicitement synthétiques.

Les tests vérifient le sens des sensibilités (température et turbulence
accélèrent, dilution et richesse pauvre ralentissent), bornes de paramètres,
variabilité, absence de chaleur avant étincelle, énergie réellement consommée,
déterminisme après redémarrage et comparaison de pas 48/96/192 kHz.

Résultats observés sur machine locale : les 16 tests cylindre,
3 tests film et 2 tests flamme passent en dev et release. La démonstration
fonctionnelle est rendue en dev sans périphérique audio. Le test de dilution choisit une charge
à 40 % d'air frais, en dessous du plateau de stabilité (la probabilité nominale
reste 1 au-dessus de 52 % dans cette corrélation estimée).

| Variante physique, même commande | Chaleur cumulée J | Carburant injecté kg | Pic WAV au gain commun 0,25 |
|---|---:|---:|---:|
| Historique | 88433,589561 | 0,002227523 | 0,029370 |
| Film seul | 80275,597713 | 0,002385469 | 0,029717 |
| Flamme seule | 63788,293032 | 0,002195900 | 0,032999 |
| Film + flamme | 51390,554599 | 0,002380630 | 0,032831 |

Ces écarts sont des effets du modèle et de ses coefficients estimés, sans
garantie d'amélioration perceptuelle ou de fidélité. Le banc mixte expose moins
de combustion complète avec sa propagation plus lente ; ce résultat n'est pas
normalisé pour retrouver le couple du témoin. Les fichiers sont dans
[artifacts/simulation-p3-p4](../../artifacts/simulation-p3-p4/), avec des projets
v4 enregistrés/rechargés, des traces et quatre WAV float à 48 kHz de 1,2 s.

Le replay après reset est bit-identique à graine fixée. À 48/96/192 kHz, chaleur
combinée `3674,190352 / 3680,199301 / 3683,701351 J` (écart `<0,164 %` au nominal).
La correction gazeuse active après injection vaut `4,688e-10 / 9,561e-10 /
1,883e-9 J`. Le premier cycle avant admission de carburant conserve la garde
200 K historique : `1,480164839 / 1,513175757 / 1,529391508 J`. Chaque pas non
négligeable doit égaler la référence historique et avoir chaleur chimique et
latente nulles. Le coût n'est pas effacé du bilan : voir l'audit de
[W-012](BESS-FUEL-TRANSIENTS-2026-10-11.md). La garde de démarrage et son effet
avec le pas de temps restent une limite héritée, sans confusion avec la nouvelle
propagation. La suite globale/Clippy finale est portée par le lot intégré.

## Reste ouvert

Références pression/angle et CA50 mesurées, points réservés de W-010, paramètres
par moteur, dispersion physique de turbulence, auto-inflammation/end-gas et
anticliquetis identifiés, coût/endurance temps réel sur machine cible, écoute
utilisateur. Les tendances des tests ne constituent pas une meilleure précision
démontrée contre un moteur réel. La qualification complète de W-013 reste ouverte.

## Validation commune finale

Le lot intégré passe 459 tests release, Clippy strict et formatage. Les exemples sont reconstruits et rejoués en release après le correctif de persistance exacte des coefficients JSON; tous les rendus terminent sans erreur. Les preuves et répertoires courants sont dans le [bilan commun](BESS-SIMULATION-PARALLEL-2026-10-11.md). Les critères mesurés, sonores et de performance indiqués ci-dessus restent ouverts.
