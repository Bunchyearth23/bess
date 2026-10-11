# W-012.3 — Démarrage libre froid/chaud, film et flamme

Date : 2026-10-11. Lot technique synthétique P3/P4, après W-012/W-013.

## Livrable

`Engine::initialize_stopped_with_thermal(StartupThermalState)` prépare les **inventaires réels** avant le premier `next` : gaz à pression atmosphérique dans chaque volume, cylindres suivant leur phase mécanique, température métal/coolant explicite, vilebrequin à zéro, traitement et induction cohérents avec la température choisie. Le constructeur historique reste inchangé tant que cette API n'est pas appelée.

Tous les scalaires de configuration/diagnostic sont Copy. `Engine::next` n'alloue rien de nouveau : un observateur scalaire optionnel conserve les flux du dernier pas ; l'inventaire complet est scanné par `transient_diagnostics()` hors de `next`. Un deuxième appel d'initialisation est refusé **avant toute mutation**, tout comme un appel après le premier pas, même après `reset`. Un redémarrage physique utilise la commande starter sans remettre à zéro les films ni les températures.

Les températures froid/chaud, starter, friction, film et flamme sont des hypothèses synthétiques. Il n'existe ici aucune identification sur démarrage enregistré.

## Scénario reproductible

Exemple `examples/fuel_startup.rs`, moteur commun Scratch sauvegardé puis rechargé, options fuel_transients + predictive_combustion, graines identiques, **imposed_rpm=None à tous les pas**. Le contrôleur commun décide richesse, étincelle, bypass et DFCO. Aucun couple fictif de combustion n'est ajouté.

| Temps | Commandes |
|---|---|
| 0–1,2 s | Starter, papillon 0,2, charge zéro |
| 1,2–3 s | Starter relâché, tip-in papillon 0,75 |
| 3–4 s | Papillon zéro, DFCO par le contrôleur |
| 4–4,8 s | Reprise papillon 0,4 |
| 4,8–5,8 s | Charge résistante explicite 300 Nm, injection/ignition coupées par overrun=0 |
| 5,8–7 s | Second starter, papillon 0,2 |
| 7–8 s | Starter relâché, papillon 0,2 |

Cas froid : gaz, métal cylindre, coolant et traitement 280 K. Cas chaud : gaz d'admission 300 K, métal cylindre 450 K, coolant 360 K, traitement 450 K. L'ambiance d'admission extérieure reste 300 K dans les deux cas : le cas froid s'échauffe réellement à l'entrée. Le coolant est une frontière thermique prescrite, pas un circuit d'eau dynamique. L'exhaust initial conserve l'estimation de produits résiduels du moteur commun ; cette préparation ne prétend pas reconstruire la purge réelle après une longue immobilisation. Une induction équipée d'une carte turbo conserve la température de paroi prescrite par la carte, pas un métal dynamique.

Commandes exécutées en développement (aucun périphérique audio requis) :

```powershell
cargo test --lib startup_tests -- --nocapture
cargo build --example fuel_startup
target\debug\examples\fuel_startup.exe artifacts/simulation-startup 48000
target\debug\examples\fuel_startup.exe artifacts/simulation-startup 96000
target\debug\examples\fuel_startup.exe artifacts/simulation-startup 192000
```

Pour chaque cas/cadence : projet `.bess.json`, trace CSV à 1 ms, WAV float au gain commun fixe 0,25, journal texte, CSV de **tous** les pas avec correction >1e-8 J. Les options sont contrôlées après sauvegarde/rechargement. Le projet stocke la définition du moteur ; l'état thermique initial est indiqué dans le journal et appliqué par l'exemple, pas persisté comme commande de lecture automatique du projet. 30 artefacts attendus dans `artifacts/simulation-startup`.

## Résultats et convergence

| Mesure | Froid 48/96 kHz | Froid 192 kHz | Chaud 48/96 kHz | Chaud 192 kHz |
|---|---:|---:|---:|---:|
| Pic régime (tr/min) | 4418,237 | 4419,182 | 4429,686 | 4430,755 |
| Chaleur totale (J) | 315743,015 | 318478,061 | 323930,658 | 326480,688 |
| Injection totale (kg) | 0,013058523 | 0,013161096 | 0,013380119 | 0,013507794 |
| Chaleur latente (J) | 4548,416 | 4584,326 | 4674,439 | 4719,174 |
| DFCO (s) | 1,000 | 1,000 | 1,000 | 1,000 |
| Chaleur pendant second starter (J) | 28983,914 | 32034,906 | 27329,313 | 28990,068 |

Le moteur atteint réellement zéro tr/min pendant la phase de charge, puis tourne et brûle du carburant au second starter. Les températures, film et vapeur sont conservés au redémarrage.

48 et 96 kHz utilisent tous deux un pas gaz de 1/96000 s (deux sous-pas à 48 kHz). Les états à exactement 1…8 secondes et intégrales sont identiques, à l'arrondi de sommation près : ce résultat teste le regroupement audio, pas deux pas physiques indépendants. 192 kHz réduit réellement le pas gaz.

192 versus 96 : chaleur totale +0,866 % froid / +0,787 % chaud ; injection +0,785 % froid / +0,954 % chaud ; pic régime +0,0214 % froid / +0,0241 % chaud. La chaleur **pendant le second starter** varie +10,53 % froid / +6,08 % chaud. Le régime froid à 7 s varie +4,42 % (1989,758 → 2077,738 tr/min). La convergence de l'événement redémarrage reste donc **ouverte** malgré une faible dispersion de l'intégrale totale. Aucun seuil n'a été élargi pour qualifier cette dispersion.

## Bilan et apport numérique explicitement séparés

Volume de contrôle observé : somme des cylindres, liquide et métal cylindre. Il ne s'agit pas du bilan global de tous les collecteurs, traitement, turbo et arbre.

```text
Δcarburant_cyl = injecteur + carburant_entrant_ports − carburant_sortant_ports − brûlé
Δ(Ugaz + Eliquide + Emétal) = chaleur_chimique + enthalpie_ports
                          + enthalpie_injecteur_liquide − travail_piston
                          − chaleur_coolant + corrections_gaz/métal
```

La chaleur latente et les transferts métal/gaz sont internes à ce bilan : ils ne sont pas ajoutés une seconde fois. Le carburant résiduel du film continue à s'évaporer pendant DFCO, alors que l'injecteur est coupé.

Sur les six rendus : résidu carburant maximal <1,8e-17 kg, résidu énergie inventaire maximal <3,2e-7 J, résidu local gaz maximal <1,2e-16 J. Les tests imposent respectivement 1e-12 kg / 1e-5 J / 1e-7 J sur 0,5 s de démarrage froid. L'exemple impose 1e-11 kg / 1e-4 J / 1e-7 J sur huit secondes. **Cette fermeture inclut les corrections explicites ; elle ne signifie pas conservation physique sans apport numérique.**

Froid 48/96 kHz : correction gaz totale 4,920586808 J ; 3,558718428 J pendant le premier starter **avant injection**, 1,361868332 J pendant DFCO après première injection. Les autres phases restent sous 2,1e-8 J. À 192 kHz : total 4,949015508 J dont 1,387936223 J pendant DFCO. Le garde de gaz à **200 K** reste actif ; il n'a pas été abaissé.

Chaud 48/96 kHz : 0,078565843 J au total, presque entièrement en DFCO. À 192 kHz : 0,062047130 J. L'apport gaz représente environ 0,00156 % de la chaleur totale froide et 0,0000243 % de la chaleur chaude à 48/96 kHz. Ce faible ratio ne qualifie pas le modèle à basse température.

Les premiers événements froids apparaissent à 0,055020833 s, gaz minimal 200 K, injecteur et chaleur latente nuls. Maximum de correction par pas audio 48 kHz froid : 0,013736749 J à 3,064375 s, en DFCO ; chaud : 0,005503472 J à 3,179541667 s. Les événements chauds sont entre 3,143791667 et 3,179687500 s. Certains pas DFCO conservent une faible chaleur latente issue du film ; le CSV distingue cet échange de la correction. Les corrections de métal sont de l'ordre de 5,23e-5 J cumulés à 48/96 kHz, 1,05e-4 J à 192 kHz, presque compensées en signe : elles sont aussi publiées.

La qualification physique cold-start / DFCO reste ouverte, ainsi que la convergence redémarrage. Les bilans quantifient précisément ces limites au lieu de les absorber dans une validation physique.

## Tests et intégration

Deux tests startup ciblés dev puis release passent : initialisation thermique effective, rejet atomique du second init sans changement des masses/énergies, rejeu interdit après premier pas et reset ; démarrage physique au starter avec clôture gaz/liquide/métal et carburant. L'assertion supplémentaire couvre également le moteur historique déjà tourné sans préparation préalable.

Correctif P5 coordonné : la topologie turbo des collecteurs dépend désormais d'un booléen initial fixé à la construction ; une carte donnant aire turbine zéro ne reroute plus silencieusement l'échappement comme un moteur atmosphérique. La composition de gaz turbine est fournie à la carte et au travail arbre avec la même pondération que leur pression/température (nombre cylindres pour aire prédite, débit sortant pour travail), exclusivement quand l'option carte est active. C'est une approximation de collecteur partagé ; aucun mélange parfaitement résolu des banques n'est revendiqué. Les tests carte/espèces du chantier P5 vérifient le correctif.

Qualification encore ouverte : relevés cold/hot démarrage et tip-in, enrichment/deposition/film réellement identifiés, contrôle idle identifié, refroidissement dynamique complet, phase et méthode de pulse initial partiel, convergence de restart, écoute et BeamNG. Les points publics Honda GX160 couple/puissance ne mesurent ni démarrage, ni température, ni film ni flamme.

Fichiers du lot : `src/physical/engine.rs`, `cycle.rs`, `fuel_transient.rs`, `manifolds.rs` (init/topologie seulement), nouvel exemple et ce rapport. Le hook induction est livré par P5. Les déclarations/persistence des options P3/P4 étaient déjà communes au lot précédent. Aucun commit, push ou installation. Graft seconde passe : **123407 jetons économisés sur 6 appels** (ask + callers + skeleton avant édition).

## Rejeu optimisé final

Suite commune **483 tests release réussis**, Clippy strict et formatage. Les six rendus de huit secondes sont reconstruits/rejoués en release dans `output/simulation-second-20261011/startup-release`, avec les 30 artefacts et les mêmes limites thermiques/convergence. Les deux cas prennent ensemble 4,951 / 5,982 / 11,866 s murales à 48 / 96 / 192 kHz, écritures comprises. Les modèles combinés du moteur commun restent sans allocation dans leur test de chemin chaud et partagent exactement l'écoute/WAV. Le WAV historique par défaut reste byte-identique. Ces résultats ne ferment ni X-035 ni X-036. [Preuves finales](BESS-SIMULATION-SECOND-BATCH-2026-10-11.md#validation-optimisée-finale).
