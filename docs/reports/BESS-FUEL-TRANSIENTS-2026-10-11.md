# BESS — W-012 / P3 : carburant transitoire

11 octobre 2026. Livraison technique expérimentale ; les coefficients et bancs
sont synthétiques, sans identification d'injecteur ni de moteur réel.

## Comportement livré

`experimental.fuel_transients = Some(FuelTransientConfig)` active un inventaire
liquide à deux compartiments (film et gouttelettes) dans le volume de contrôle
de la charge admise. `None` conserve le chemin historique. Le réglage est
enregistrable, composé de scalaires `Copy`, validé avant création de la voix et
utilisé par `physical::engine::Engine` pour tous les cylindres. Le contrôleur
existant fournit la commande de carburant ; son DFCO coupe immédiatement le
nouvel apport, mais le film peut encore s'évaporer et brûler si une étincelle
et de l'oxygène sont présents. Une coupure demandée ne devient pas un raté.

Une impulsion répartie en angle commence par défaut à 330° après PMH combustion
et dure 120°. La masse commandée utilise une estimation vitesse/densité du
volume balayé et soustrait le carburant déjà présent dans le réservoir donneur.
Le modèle évite ainsi le redosage des poches réaspirées. Le début/la durée et
le dépôt sont configurables : un essai direct doit explicitement les choisir
(par exemple début 550°, durée 120°, dépôt 0,05). Aucune conversion automatique
d'un carburateur ou identification d'injecteur direct n'est annoncée.

Le dépôt initial, l'entraînement du film vers les gouttelettes et l'évaporation
sont conservatifs. L'évaporation suit une constante de temps et une dépendance
thermique explicites avec intégration exponentielle positive. Sa chaleur latente
provient de la charge gazeuse ; un budget au-dessus de 200 K limite l'évaporation
en conservant le liquide non évaporé. Le modèle ne transporte pas un film
spatial dans le runner et ne prédit ni atomisation ni impact de jet.

## Bilans et diagnostics

Le liquide emporte l'enthalpie sensible du carburant vapeur à la température
d'injection moins la chaleur latente. Cette référence commune peut donner une
enthalpie liquide négative ; la vapeur entre dans `GasState` avec une enthalpie
positive, et le gaz finance sa chaleur latente.

- `injector − vapeur − Δ(liquide) = 0` : `FuelTransfer::mass_residual_kg`.
- `enthalpie_injecteur + chaleur_latente − enthalpie_vapeur − Δénergie_liquide = 0` :
  `FuelTransfer::energy_residual_j`.
- Dans le bilan du gaz, utiliser `CycleOutput::gas_fuel_added_kg` et
  `gas_fuel_enthalpy_j`. `injected_fuel_kg` représente l'apport externe total.
- L'inventaire carburant comprend `fuel_mass_kg + film_kg + droplets_kg`.
- `EnergyLedger::wall_heat_j` inclut la chaleur latente indiquée séparément par
  `fuel_transfer.latent_heat_j`. Seul l'échange paroi traditionnel chauffe le
  métal ; la chaleur chimique `heat_j` reste exactement carburant brûlé × PCI.
- Les corrections numériques du gaz et du métal restent explicitement visibles.

Un changement qui jetterait un film en cours est refusé par le setter ; le
redémarrage remet l'inventaire à zéro et conserve les options expérimentales.
Le pas de simulation n'alloue pas.

## Bancs et réception

`cargo run --release --example fuel_combustion -- artifacts/simulation-p3-p4`
produit quatre voix physiques au gain d'observation commun : témoin, carburant,
combustion et combinaison. Chaque voix écrit WAV/CSV, sa définition Scratch
JSON et un projet v4 `.bess.json`. Le banc recharge le projet par l'API standard,
vérifie les options puis rend cette définition rechargée. Deux CSV supplémentaires
décrivent les charges initiales 280 K et 450 K,
échelon de MAP, coupure et reprise, avec richesse, température, couple,
inventaires, chaleur latente et résidus. Ces températures initiales sont un banc
cylindre explicitement configuré, pas une mesure ni un démarrage véhicule validé.

Les tests ciblés couvrent film froid/chaud, impulsion, queue d'évaporation après
coupure, budgets latents, paramètres invalides, transport inverse prémélangé,
bilans combinés, replay de graine et comparaison à 48/96/192 kHz.

Résultats observés sur la machine locale, bancs dev sans périphérique audio :

- 16 tests cylindre, 3 tests film et 2 tests combustion réussis en dev **et release**
  (9 nouveaux tests). Après la dernière compilation release ciblée, les deux
  filtres restants ont été rejoués sur son exécutable de tests, sans nouveau build.
  La suite globale/Clippy finale est portée par le lot intégré.
- À 280 K / 450 K, résidu maximal de carburant total : `1,416e-17 / 1,158e-18 kg` ;
  résidu énergétique liquide/vapeur : `2,008e-15 / 4,814e-16 J` ; résidu du gaz :
  `2,776e-17 J`. Chaleur latente cumulée : `96,622606 / 64,260970 J`.
- Sur le banc combiné à 48/96/192 kHz, masse injectée `0,000183343512431 kg` ;
  dispersion absolue inférieure à `3e-17 kg`. Chaleur dégagée :
  `3674,190352 / 3680,199301 / 3683,701351 J`, écart maximal `<0,164 %` vs 96 kHz.
- Les 4 projets v4 sont enregistrés/rechargés et les options préservées ; 4 WAV
  float 48 kHz de 1,2 s sont livrés avec un gain fixe commun `0,25`, sans écrêtage
  (pics `0,029370–0,032999`). Traces/projets/WAV :
  [artifacts/simulation-p3-p4](../../artifacts/simulation-p3-p4/).

L'audit des corrections conserve un phénomène hérité : le cylindre initialisé
au PMH combustion avec du gaz d'admission atteint 200 K à sa première détente,
avant toute injection. À 96 kHz, les **612 pas de garde** entre `0,006625` et
`0,0129896 s` coûtent **1,513175757 J** de correction gazeuse de démarrage ; la
correction absolue gaz + arrondis métal vaut `1,513176580 J` dans le banc combiné.
Les 4 variantes montrent ces mêmes pas (aucune garde 3500 K), chaleur chimique et
latente nulles. Le test compare chaque correction au cylindre historique avec
un écart `<1e-12 J`, exige zéro chaleur/évaporation à ces pas et ne les masque
pas dans une annonce de « correction nulle ». Après la première injection,
correction gazeuse cumulée `9,561e-10 J` à 96 kHz ; seuil strict `<1e-6 J`.
Le bilan énergie reste fermé en incluant toutes les corrections. La garde de
démarrage héritée reste une limite du domaine et ne devient pas de la physique.

## Reste ouvert

Cartographie d'injecteur, enrichissement froid/accélération identifié, film réel
du collecteur, commande de pression/atomisation directe, démarrage libre à froid,
et confrontation aux références W-010. Le chemin utilise déjà la commande du
contrôleur, mais n'ajoute aucune cartographie mesurée. L'écoute et BeamNG restent
des réceptions séparées. W-012 ne peut pas être déclaré entièrement qualifié
sur les seuls bancs synthétiques.

## Validation commune finale

Le lot intégré passe 459 tests release, Clippy strict et formatage. Les exemples sont reconstruits et rejoués en release après le correctif de persistance exacte des coefficients JSON; tous les rendus terminent sans erreur. Les preuves et répertoires courants sont dans le [bilan commun](BESS-SIMULATION-PARALLEL-2026-10-11.md). Les critères mesurés, sonores et de performance indiqués ci-dessus restent ouverts.
