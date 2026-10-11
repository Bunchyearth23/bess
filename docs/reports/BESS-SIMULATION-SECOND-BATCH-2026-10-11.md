# W-010–W-016 — Deuxième lot parallèle de simulation

L'utilisateur a autorisé la poursuite du lot le 11 octobre 2026. Les sept chantiers restent **building**. Ce lot approfondit les frontières de gaz, les démarrages et les bilans mécaniques; il ne constitue pas une réception mesurée ou sonore.

## Changements concrets

- **P2 / W-011.1 :** outil exécutable séparant délai de tube, soupape à états fixes et moteur complet. Les deux échecs du critère historique sont maintenant enregistrés, aux deux cadences et avec leurs courbes brutes. Le seuil reste 10 %. [Diagnostic](BESS-WAVE-QUALIFICATION-2026-10-11.md).
- **P3–P4 :** préparation explicite d'un moteur arrêté avec températures réelles des gaz, parois, refroidissement et traitement aval. Le second appel et les appels après simulation sont rejetés sans modifier l'état. Les démarrages froids/chauds et redémarrages conservent film et chaleur, sans régime imposé ni reset. Diagnostics des inventaires et apports numériques accessibles sans allocation dans le pas commun. [Scénarios et limites](BESS-FUEL-STARTUP-2026-10-11.md).
- **P5 :** capacité de turbine corrigée pour le Cd 0,9 du réseau, topologie turbo conservée même quand la carte ferme le débit, composition commune pour surface et travail. Ces corrections concernent les options cartographiées. [Grille et bilans](BESS-MAPS-TORSION-QUALIFICATION-2026-10-11.md).
- **P6 :** événements d'arrêt torsion résolus avant les contraintes statiques : disparition du recul numérique et du travail de friction de signe incorrect. Grilles de bilans/convergence, séparation de l'observation sonore et des états physiques, import exact et compteurs de coût.
- **P7 / W-016.1 :** nouveau réseau hors ligne utilisant la même EOS variable et référence énergétique que les cylindres, avec air/carburant transportés par cellule. Le mode fixe et le chemin de production antérieurs sont conservés. [Interface et limites](BESS-COUPLED-1D-2026-10-11.md#deuxième-lot--w-0161--eos-commune-et-espèces-distribuées).

## Limites observées, conservées dans la livraison

**Résonance du moteur complet :** erreur de facteur de −11,29 % à longueur ×0,7 et −13,42 % à ×1,4, encore en échec. Doubler la cadence ou fixer la fenêtre de comparaison n'efface pas cet écart. Un délai correct et une soupape incrémentalement passive ne prouvent pas l'exactitude du moteur complet.

**Démarrage et décélération :** le plancher de gaz à 200 K ajoute de l'énergie visible, surtout au démarrage froid et pendant la coupure. Les bilans ferment en comptant cet apport; cela ne le transforme pas en chaleur physique. La convergence de la chaleur de redémarrage reste moins bonne que celle des phases stationnaires. Les mesures de température/richesse et une amélioration de ce domaine restent requises.

**Contact entre mélanges à températures différentes :** le réseau 1D conserve masse, énergie et espèces mais produit environ 1,84 % d'erreur locale de pression dans le test de contact 500/1100 K. Le nouveau solveur est d'ordre spatial 1, avec intégration temporelle RK2; ses bilans ne justifient pas une intégration en production. Les frontières de quantité de mouvement/jet, les pertes, l'admission distribuée, le turbo/traitement et l'export commun complet restent ouverts.

**Authenticité :** Honda GX160 reste une référence constructeur non ajustée, en échec sur les deux valeurs publiques. Aucun nouvel essai synthétique n'est converti en acquisition mesurée. Cartes turbo, forces de contact et réponses structurelles restent des estimations; l'endurance sur matériel cible et l'écoute/BeamNG restent ouvertes.

## Reproduction

```powershell
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --release --example wave_qualification -- output/simulation-second-20261011/waves-release
cargo run --release --example fuel_startup -- output/simulation-second-20261011/startup-release 48000
cargo run --release --example simulation_maps_torsion_grid -- output/simulation-second-20261011/maps-torsion-release
cargo run --release --example coupled_1d -- output/simulation-second-20261011/p7-mixture 0.04 4 - mixture
```

Les artefacts de ce lot sont distincts de `output/simulation-parallel-20261011`, qui conserve les preuves du premier lot. Les parents W ne changent pas d'état automatiquement.

## Validation optimisée finale

Sur le même AMD Ryzen 7 3800X, sources gelées avant compilation : **483 tests release réussis, zéro défaillance, six diagnostics ignorés**, répartis sur 62 exécutables de tests. Clippy strict tous targets, `cargo fmt --check` et `git diff --check` passent. Le test commun de modèles combinés confirme les mêmes échantillons écoute/WAV et zéro allocation/libération sur le chemin chaud. Les deux tests d'initialisation incluent le refus après première simulation sur moteur non préparé et après reset.

Le test X-028 ignoré est ensuite exécuté explicitement sur son nouveau binaire release : **échec attendu, code 101**, facteur 0,620970824 pour longueur 0,699999988. Ce diagnostic en échec n'est pas compté dans les 483 réussites. L'exemple complet conserve aussi l'échec à ×1,4 et à 96 kHz.

Les **onze exécutions release** ci-dessous terminent avec code zéro. Les durées murales incluent les écritures des fichiers; elles ne sont pas des délais audio ou une qualification temps réel.

| Démonstration | Coût mural observé |
|---|---:|
| Diagnostic résonance 48/96 kHz | 14,877 s |
| Deux démarrages de 8 s, 48 kHz | 4,951 s |
| Deux démarrages de 8 s, 96 kHz | 5,982 s |
| Deux démarrages de 8 s, 192 kHz | 11,866 s |
| 57 cas cartes/torsion/structure | 0,785 s |
| Moteur commun sans carte / cartographié, deux rendus de 2 s | 1,821 s |
| 1D EOS commune 0,04 s, 16 / 8 / 4 mm | 0,863 / 3,011 / 11,760 s |
| 1D espèces distribuées à gamma fixe, 0,04 s / 4 mm | 4,655 s |
| Référence 1D historique, 0,04 s / 4 mm | 2,350 s |

Le cas EOS commune à 4 mm comporte 1 000 cellules et 16 089 pas, sans rejet, avec résidus maximaux masse **1,144e-17 kg** / énergie **1,586e-11 J** / air **3,880e-18 kg** / carburant **2,118e-21 kg**. Ses champs de référence fixe sont nuls et ses propriétés locales variables sont explicitement enregistrées. Le solveur reste nettement plus coûteux que le temps simulé.

L'annulation est déclenchée **pendant** une demande de 0,16 s en EOS commune : arrêt à **0,01 s / 3 605 pas**, `completed:false`, WAV finalisé **1 988 octets**, dont 1 920 octets de données correspondant exactement à 480 échantillons float. Le manifeste et les comptes partiels sont conservés; énergie résiduelle maximale **8,316e-12 J**. Un fichier déjà présent avant démarrage n'est pas utilisé comme preuve d'annulation active.

Exécutable final `target/release/bess.exe` : SHA-256 **38836CF5DD87D4F60DD2876A6837A03D8AAB3EBD1ED8DDA9415C90F8340AB9BF**. Son WAV par défaut reproduit exactement la référence antérieure : **50EF7089BEFAA804DA8DC40389FCF685FD5C4757ECC7E68357AEC19E4EC95F51**. Les options nouvelles restent explicites et le gain mécanique initial reste nul. La reconstruction a lieu après fermeture de BESS, conformément au choix de l'utilisateur; aucune activation de mod, installation ou publication n'est réalisée.

Preuves : `output/simulation-second-20261011/release-tests.log`, `clippy.log`, `release-build.log`, `release-runs.json`, `strict-x028.log`, `cancellation-result.json`, les sous-répertoires de rendu et `verification.json`. `source-sha256.json` conserve les 128 fichiers Rust/Cargo de cette compilation; leurs hashes sont revérifiés après les rendus. Le commit de base reste `8ceb3496e1ad9b8e3eb8245ce9c7d670bf804be2`, avec les changements autorisés non commités et les autres travaux préexistants préservés.
