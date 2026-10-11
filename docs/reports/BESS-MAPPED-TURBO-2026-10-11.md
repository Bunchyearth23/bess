# W-014 / P5 — Turbo cartographié

La qualification du deuxième lot et les corrections de débit/topologie sont détaillées dans [BESS-MAPS-TORSION-QUALIFICATION-2026-10-11.md](BESS-MAPS-TORSION-QUALIFICATION-2026-10-11.md).

Le lot technique ajoute un turbo optionnel cartographié dans la voix physique commune. Les projets sans carte gardent la caractéristique analytique historique ; aucune carte mesurée n'est livrée ou présumée.

## Livraison

- Import JSON lisible de cartes rectangulaires compresseur et turbine, en SI corrigé : vitesse RPM corrigée, débit kg/s corrigé, rapport de pression sans unité et rendement. Les références de température/pression, matériel, source et statut mesuré sont obligatoires. Stockage compilé `Copy`, sans tableau dynamique sur le chemin audio : 8 lignes de vitesse et 16 coordonnées au maximum. Chaque texte de provenance accepte 256 octets UTF-8.
- Interpolation bilinéaire continue aux lignes de vitesse. Maintien à la frontière hors domaine, avec diagnostic explicite `map_outside_domain`. Au-dessous de la première vitesse, la hauteur compresseur décroît en vitesse au carré ; cette prolongation de démarrage est une estimation explicite. Les cartes positives ne mesurent pas le flux inverse/surge : un passage hors domaine reste non qualifié.
- La hauteur cartographiée agit sur l'inertance du conduit et le rendement sur le travail de compression. Le rendement turbine agit sur le travail récupéré ; son débit corrigé fournit une surface de tuyère équivalente pour le réseau gaz. Le réseau transporte la masse une seule fois.
- Actionneurs wastegate/BOV du premier ordre, pertes de palier proportionnelles à la vitesse, chaleur échangée avec une frontière de paroi à température prescrite. Les pertes arbre et la chaleur de paroi sont déclarées dans les comptes de chaque pas ; la paroi prescrite représente un réservoir thermique extérieur, pas une température de métal prédite.
- `ExperimentalSpec.turbo_model` enregistre l'option et toute la carte. Le défaut reste `None`. Les fixtures `TurboModel::synthetic_example()` sont explicitement synthétiques.

## Validation et démonstration

Les tests dédiés couvrent import/provenance, dimensions/rendements invalides, continuité aux lignes, conversions corrigées inversibles, spool/lever de pied/BOV, comptes arbre/charge/paroi et identité du chemin sans carte. Les deux tests de cartes passent en release. Le test de spool/BOV/paroi et celui d'identité passent dans le binaire release déjà compilé ; la suite intégrée doit reconfirmer l'ensemble sur les dernières modifications communes. La démonstration passe la vérification de compilation et Clippy strict.

`cargo run --release --example mapped_turbo_mechanics -- work/simulation-p5-p6` produit traces CSV, comparaisons de la même configuration/graine, fichiers de configuration synthétique et métriques de temps. Le scénario comprend un échelon de charge, libération du banc imposé et lever de pied. Les WAV de sources mécaniques sont des moyens d'écoute, pas des preuves de fidélité du turbo.

La démonstration a été exécutée en profil **dev** à 48 kHz, 2 s par référence, graine commune `11079183697511781973`. Les artefacts sont dans `work/simulation-p5-p6/`. Pics de charge : 184 040 Pa baseline, 213 437 Pa cartographié ; pics de vitesse arbre : 171 887 et 114 696 RPM. 20,25 % des lignes CSV cartographiées signalent une requête hors carte (notamment démarrage) : ces points ne sont pas calibrés. Les temps dev observés, environ 7,6 et 7,2 s avec production des fichiers, ne constituent pas un benchmark release ni une qualification temps réel.

## Réception encore ouverte

W-010 doit fournir des cartes et références réelles de pression, débit, vitesse et température. Les extrapolations de démarrage et de flux inverse restent estimées ; aucun résultat synthétique ne ferme X-024/X-027. Les cartes plus grandes doivent être réduites avant import et leur erreur de réduction documentée. La géométrie équivalente et les conditions thermiques du matériel doivent être calibrées. Les réponses de choke/surge au-delà du domaine des cartes, l'endurance temps réel et l'écoute/BeamNG restent à qualifier séparément.

## Validation commune finale

Le lot intégré passe 459 tests release, Clippy strict et formatage. Les exemples sont reconstruits et rejoués en release après le correctif de persistance exacte des coefficients JSON; tous les rendus terminent sans erreur. Les preuves et répertoires courants sont dans le [bilan commun](BESS-SIMULATION-PARALLEL-2026-10-11.md). Les critères mesurés, sonores et de performance indiqués ci-dessus restent ouverts.
