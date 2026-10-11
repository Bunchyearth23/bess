# W-016 — Pipeline hors ligne cylindre/réseau 1D conservatif

11 octobre 2026. Deux lots techniques : référence à gamma fixe puis W-016.1 avec EOS commune et espèces distribuées. Intégration au moteur de production et qualification mesurée ouvertes.

## Portée livrée

`physical::coupled_1d::Exhaust1d` assemble des primaires Euler distribués, un collecteur de volume fini et un tube aval Euler distribué. Tous les tubes utilisent le solveur existant MUSCL/SSP-RK2, HLLC avec récupération HLLE/positivité, référence 4 mm et CFL 0,4. Aucun écrêtage de cellule ou apport caché d'énergie n'est ajouté.

Chaque interface possède un seul flux partagé. Le flux du primaire vers le collecteur est appliqué une fois à la face du tube, et son transfert intégré accepté est appliqué avec le signe opposé au collecteur. La face collecteur/aval fonctionne de la même manière. Les nouveaux diagnostics `boundary_by_side` séparent les deux frontières et incluent les subdivisions adaptatives, évitant de reconstituer un échange à partir du pas nominal.

La frontière cylindre reçoit pression/température et composition donneuse du primaire, avec un budget de masse lié à sa première cellule. Le cylindre existant fournit ensuite exactement ses masses, enthalpies et espèces échangées; ces quantités entrent une fois dans le tube via `Boundary::Flux`. La pression de face et l'orifice sont liés par l'impédance caractéristique implicite. Aucun deuxième estimateur de débit ni source sonore doublée n'est ajouté.

## Limites de la référence du premier lot

- Les tubes et le collecteur utilisent un gaz calorifiquement parfait à gamma/R constants. Les cylindres conservent leur thermodynamique variable; l'interface échange exactement l'énergie fournie, mais les deux équations d'état ne constituent pas encore une référence thermodynamique variable commune.
- Les inventaires air frais/carburant sont conservatifs et parfaitement mélangés par tube/collecteur. Ils ne résolvent pas la propagation spatiale de fronts de composition.
- Le pipeline ne remplace pas `Engine` ni son export qualité. L'exemple utilise un I4 à espacement régulier des phases, admission et RPM imposés. Il ne prétend pas couvrir les architectures complètes, leur contrôleur, le turbo, le catalyseur, les pertes aux parois ou le réseau d'admission distribué.
- La garde `primary_1d + wave_coupling` du moteur commun reste présente. Cette démonstration ne justifie pas de l'enlever et ne ferme pas X-028.

## Démonstration fonctionnelle

`cargo run --release --example coupled_1d -- out/simulation-p7 0.16 4`

L'exemple raccorde quatre `CycleCylinder` réels au réseau à pas global borné par CFL. Il rend un WAV mono de pression aval, un CSV couple/pressions/résidus et un manifeste JSON : géométrie, référence de gaz, Scratch, graine, RPM, taux, gain d'observation, coût/maille et limites. Le retrait DC 12 Hz et le gain fixe 30 000 Pa/unité sont des observations sans retour physique. Les données sont explicitement synthétiques.

La progression est publiée pendant le rendu. Un quatrième argument peut désigner un fichier d'annulation; sa création arrête le calcul à une frontière de trame, finalise les fichiers partiels et inscrit `completed: false` et la durée réellement rendue. Le moteur audio temps réel reste exclu.

## Validation de cette session

`cargo test --release --lib physical::coupled_1d` : **2 tests réussis**, aucune défaillance.

1. État uniforme avec soupapes fermées : absence de source de pression/espèces et bilan global masse/énergie.
2. Impulsion sur un réseau à deux primaires : bilan masse/énergie/air frais/carburant à chaque pas; résidu énergétique maximal inférieur à 1e-7 J, résidus de masse/espèces inférieurs à 1e-13 kg. Le raffinement 16 → 8 → 4 mm réduit l'écart d'intégrale de pression aval à la référence.

La démonstration **dev** a rendu 0,16 s avec 1 000 cellules et 82 206 pas (`out/simulation-p7`) : résidus maximaux masse 2,19e-16 kg, énergie 1,22e-10 J, air frais 3,04e-17 kg et carburant 6,90e-20 kg. Le coût 282,65 s est celui d'un binaire non optimisé pendant des builds concurrents; il ne constitue pas une mesure de performance du renderer release.

Après correction de la gestion des restes de trame sous la nanoseconde, le pipeline courant a été rejoué sur 0,04 s (`out/simulation-p7-current`) : **16 272 pas**, 1 000 cellules, temps physique final **0,04 s**, résidus maximaux masse **1,36e-17 kg**, énergie **1,74e-11 J**, air frais **3,73e-18 kg**, carburant **1,34e-21 kg**. Cette correction transporte le petit reste vers la trame suivante; une contrainte CFL inférieure au domaine du cylindre produit une erreur explicite, sans saut de temps physique.

L'annulation a été déclenchée **pendant** un rendu (`out/simulation-p7-cancel`) : arrêt à 0,01 s / 3 640 pas, WAV finalisé, CSV/manifeste conservés, `completed: false` et `physical_time_s: 0.01`. L'énergie résiduelle maximale reste 9,53e-12 J. La validation finale commune reconstruira/rejouera les exemples release après la suite intégrée.

Ces tolérances et résidus mesurent la fermeture numérique des inventaires du réseau, pas les erreurs contre un moteur réel. Le test X-028 inchangé a aussi été exécuté explicitement en release et échoue encore (facteur 0,6209708 contre 0,7, écart 11,29 %); le pipeline 1D ne change pas ce statut.

## Suite après le premier lot (état historique)

La validation commune finale passe **459 tests release**, Clippy strict et formatage. Le renderer courant est reconstruit et rejoué en release sur 0,16 s : 1 000 cellules, 82 206 pas, résidus max énergie 1,46e-10 J / masse 1,41e-16 kg, coût interne 12,54 s. Une annulation active du binaire release s'arrête à 0,02 s / 7 503 pas et finalise le WAV avec `completed:false`. Ces artefacts courants sont dans `output/simulation-parallel-20261011/p7-release/` et `p7-release-cancel/`; [preuve commune](BESS-SIMULATION-PARALLEL-2026-10-11.md).

Partager une équation d'état variable et des scalaires distribués, intégrer les branches retenues au moteur commun avec suppression des volumes/échanges remplacés, ajouter pertes/températures/traitement/turbo et admission, puis qualifier convergence temporelle et spatiale, réflexions analytiques, coût/mémoire, annulation et sauvegarde dans l'export commun. La conservation de ce banc ne constitue pas à elle seule une qualification des frontières de production.

## Deuxième lot — W-016.1 : EOS commune et espèces distribuées

`SpeciesPipe` conserve cinq inconnues **par cellule** : densité, quantité de mouvement, énergie totale, densité d'air frais et densité de carburant. Les produits constituent le reste. `SpeciesExhaust1d` raccorde ces cellules à un collecteur de volume fini, seul volume parfaitement mélangé, et à un tube aval du même type. Les espèces ne sont plus instantanément mélangées dans toute une branche : un test d'injection vérifie l'apparition d'air frais à la première cellule sans apparition immédiate au collecteur.

Le mode `GasEquation::SharedMixture` utilise les mêmes `thermo::Mixture`, R(Y), cv(T,Y), énergie sensible et référence à 298,15 K que `CycleCylinder`. L'inversion énergie/température reprend la racine quadratique rationalisée à partir des fonctions caloriques publiques; aucune seconde table d'espèces n'est introduite. Gamma et célérité du port proviennent de sa température et de sa composition locales. Le collecteur ferme sa pression avec cette même EOS. Le mode `Fixed` constitue une réduction à gamma/R constants avec les scalaires distribués; `Exhaust1d` et le solveur MUSCL de production restent disponibles séparément.

Le nouveau solveur utilise **HLLC/HLLE, ordre spatial 1 et SSP-RK2 temporel**, CFL 0,4. Chaque interface transmet le même flux à cinq composantes, appliqué avec des signes opposés aux inventaires voisins. Les espèces HLLC suivent exactement le flux massique et les fractions donneuses, évitant des soustractions différentes à débit quasi nul. Les états invalides déclenchent un rejet/réduction du pas, sans réparation des masses ou de l'énergie. La validation thermodynamique tolère seulement quelques ulps aux bornes et dans la somme des fractions; les inconnues conservées ne sont jamais écrêtées ou renormalisées. Les transferts donneurs invalides sont rejetés avant la modification des tubes.

La frontière cylindre fournit toujours exactement ses masses d'espèces et son enthalpie sensible échangées. Cette enthalpie d'un volume à vitesse nulle entre dans l'énergie totale du tube; aucune énergie cinétique de jet supplémentaire n'est inventée. Le flux de quantité de mouvement imposé à la face reste la pression de sa première cellule, sans modèle complet de jet/orifice ou résolution commune de sa quantité de mouvement. Ce banc ferme ses inventaires d'énergie et d'espèces, mais ne qualifie donc pas encore une frontière cylindre/tube complète à tous les régimes. Le collecteur absorbe les forces de pression via ses parois, sans stocker la quantité de mouvement des jets.

### Preuves dev du deuxième lot

**11 nouveaux tests réussis** : 7 `physical::finite_volume::species_tests`, 4 `physical::coupled_1d::species_network_tests`. Les 2 tests du réseau de référence restent également réussis. Clippy ciblé bibliothèque/exemple passe sans avertissement. La validation release intégrée est centralisée par le chantier commun.

| Contrôle | Résultat synthétique |
|---|---|
| EOS contre `GasState`, air/carburant/produits/mélanges, 200–3500 K | Énergie à 1e-9 J, température à 1e-8 K, pression à 1e-6 Pa pour le volume de test |
| Mélange à 298,15 K | Température conservée; aucune libération artificielle de chaleur chimique |
| Réduction gamma fixe | Même énergie Euler et même flux uniforme que `EulerPipe`; scalaires transportés avec le flux de masse |
| Advection scalaire périodique, 64 / 128 / 256 cellules | Erreur L1 : 5,620891e-3 / 2,878009e-3 / 1,455727e-3; convergence spatiale d'ordre 1 |
| SSP-RK2 contre advection semi-discrète analytique, dt / dt/2 / dt/4 | Erreur L1 : 9,851259e-7 / 2,462495e-7 / 6,155784e-8; facteur voisin de 4 |
| Impulsion réseau, 16 / 8 / 4 mm | Intégrale de pression aval : 0,579686143 / 0,579646814 / 0,579613795 Pa·s; écart à la référence réduit |
| Bilans réseau de l'impulsion | Résidu énergie max 4,11e-12 J; masse et espèces < 1e-13 kg à chaque pas |

Un contact transporté à pression/vitesse identiques mais **500 / 1100 K et compositions différentes** conserve ses cinq inventaires et présente **1 864,644 Pa d'écart maximal de pression, soit 1,84 %** de 101 325 Pa. La réduction gamma fixe garde l'équilibre à moins de 1e-6 Pa. Le contact isotherme dans l'EOS variable garde aussi cet équilibre dans le test. Le cas non isotherme rend visible l'erreur de mélange conservatif entre EOS variables : aucun correctif non conservatif n'est utilisé pour cacher cette erreur. Son amélioration et sa qualification restent une gate d'exactitude avant intégration de production; les bilans fermés ne suffisent pas à la lever.

### Démonstration et coût

`target/release/examples/coupled_1d.exe output/simulation-second-20261011/p7-mixture 0.04 4 - mixture`

Le cinquième argument choisit `mixture`, `fixed-species` ou `legacy` (valeur par défaut). Le quatrième conserve le fichier d'annulation; `-` signifie aucun fichier. Les mêmes mailles 16 / 8 / 4 mm et durées 0,04–30 s sont disponibles. La création du fichier d'annulation pendant le calcul finalise le WAV et les inventaires partiels. Le CSV ajoute température, gamma et fractions de la première cellule du port 0; le manifeste final distingue les références fixes des valeurs locales de l'EOS commune et consigne les graines et les rejets de pas.

Le binaire **dev** a terminé 0,04 s à 4 mm : **1 000 cellules, 16 089 pas**, temps physique 0,04 s, aucun rejet de pas. Résidus maximaux : masse **1,056e-17 kg**, énergie **1,626e-11 J**, air frais **3,729e-18 kg**, carburant **1,483e-21 kg**. Les fichiers WAV/CSV/JSON sont dans `out/simulation-p7-mixture-dev/`. Coût interne **293,50 s**, binaire non optimisé pendant les travaux parallèles : coût release à mesurer par la validation commune, aucune revendication temps réel.

Ce rendu précède deux dernières améliorations sans changement de ses équations de flux : validation des fractions externes avant toute mutation et clarification du manifeste. Les anciens champs `gamma`/`gas_constant` de son JSON décrivent la référence fixe et ne sont pas la gamma/R utilisée par `mixture`; le renderer final les remplace par `fixed_gamma`/`fixed_gas_constant` nuls dans ce mode et des propriétés initiales locales explicites. La démonstration release commune reconstruit cette version finale.

Les quatre buffers conservatifs du nouveau solveur représentent environ **160 200 octets pour les 1 000 cellules des cinq tubes**, hors métadonnées de vecteurs, collecteur, cylindres et fichiers. Les tableaux sont alloués à la construction, sans allocation par pas. La fermeture EOS variable et ses racines répétées ajoutent du calcul; cache des états primitifs et reconstruction d'ordre supérieur demandent des mesures séparées.

### Gates restant ouvertes

W-016.1 dispose maintenant d'une interface EOS commune et d'espèces conservées/distribuées démontrées. W-016.2–4 conservent leurs gates : frontières à quantité de mouvement cohérente et équilibre des contacts non isothermes, convergence robuste sur tous les régimes, pertes thermiques/friction/réacteurs, réseau d'admission, turbo, remplacement explicite des volumes de production, export/annulation communs et qualification mesurée. La garde `primary_1d + wave_coupling` reste active; ce lot ne retire aucune protection et ne requalifie pas X-028, l'écoute ou BeamNG.

## Validation optimisée finale du deuxième lot

Les onze nouveaux tests passent dans la suite commune de **483 tests release réussis**, Clippy strict tous targets et formatage. Le renderer final est reconstruit avec métadonnées EOS explicites, et les mailles 16/8/4 mm sont rejouées en mode commun, ainsi que les deux références fixes. Le cas 4 mm rend 0,04 s / 1 000 cellules / 16 089 pas, sans rejet, en 11,751 s interne (11,760 s murale). Résidus maximaux : masse 1,144e-17 kg / énergie 1,586e-11 J / air 3,880e-18 kg / carburant 2,118e-21 kg. Les coûts muraux 16/8 mm sont 0,863/3,011 s. Le transport distribué fixe prend 4,655 s, le réseau historique 2,350 s pour 0,04 s / 4 mm.

L'annulation active du mode EOS commune s'arrête à 0,01 s / 3 605 pas, manifeste `completed:false`, WAV float finalisé et taille/sample count vérifiés; résidu énergie maximal 8,316e-12 J. Artefacts `output/simulation-second-20261011/p7-*`, comptes d'annulation et [preuve commune](BESS-SIMULATION-SECOND-BATCH-2026-10-11.md#validation-optimisée-finale). Le contact non isotherme à 1,84 % reste X-037 et l'intégration de production reste ouverte.
