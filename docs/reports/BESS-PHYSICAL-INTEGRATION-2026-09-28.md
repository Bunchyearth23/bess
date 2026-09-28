# Moteur physique intégré — 28 septembre 2026

Ce lot poursuit W-006 après les premiers noyaux isolés. Le chemin scratch joué
dans l'application utilise désormais le moteur physique. L'ancien générateur
événementiel reste accessible uniquement comme référence hors ligne explicite ;
le « Descriptor voice » n'est pas réintroduit. Les banques Automation et leur
export BeamNG conservent leur chemin distinct.

## Ce qui est relié

- Cylindres 0D en double précision, géométrie bielle-manivelle, soupapes pilotées
  par les cames, écoulements étranglés/réversibles, combustion Wiebe et pertes
  Hohenberg. La loi calorifique est une approximation air `cv=650+0,16T`, intégrée
  de façon cohérente ; ce n'est pas la corrélation Brunt du rapport de recherche.
- Air frais, carburant imbrûlé et produits sont transportés entre cylindres,
  admission, échappement et charge turbo. Les gaz refoulés ne sont plus
  automatiquement transformés en air frais ; une charge prémélangée n'est pas
  dosée deux fois. Le catalyseur consomme les réactifs du volume réel.
- Le vilebrequin reçoit les couples gazeux, inertiels, de frottement et de
  transmission. La rotation libre n'utilise ni courbe sinus de couple, ni maintien
  artificiel du régime. Le mode direct reste un banc à régime imposé.
- Régulateur de ralenti à pas de 30 ms, réserve d'allumage, coupure d'injection et
  reprise, démarreur et calage. Charges A/C 16 Nm et direction 22 Nm, persistées et
  modifiables sans reconstruction du moteur.
- Afterfire optionnel au lever de pied : une fenêtre de 200–300 ms agit sur le
  carburant retenu et l'allumage. La réaction utilise les réactifs et températures
  des volumes d'échappement ; aucune impulsion sonore indépendante n'est ajoutée.
  Reprise, limiteur et démarreur annulent la commande. Désactivé, les trois WAV
  de référence restent identiques octet pour octet après cette modification.
- Une conduite primaire par cylindre, jonctions acoustiques à admittances,
  deux lignes et connexions H/X passives. La vitesse du son suit les températures,
  les délais des primaires suivent `L/(c ± u)`. Les soupapes reçoivent l'onde de
  retour avec une borne provisoire de 20 % de la pression moyenne.
- Arbre turbo, travail turbine/compresseur, volume de charge, caractéristique de
  compresseur non monotone et inertance permettant le reflux de pompage. BOV
  atmosphérique/recirculée/absente ; les cartes et dimensions restent estimées.
- Les sources échappement, admission et mécanique traversent la chaîne d'écoute
  et le limiteur. Conversion fixe de pression en amplitude, aucune normalisation
  RMS adaptative. Le volume d'écoute reste entre 0 et 1.

Les pièces agissent sur les dimensions, débits, cames et inertie. Les anciens
réglages de pulses et la température d'échappement manuelle ne pilotent plus le
scratch physique. Le calage mécanique sur les deux tours est séparé du câblage
électrique ; un ordre incorrect peut rater ses combustions.

## Défauts découverts par l'intégration

Le premier ralenti libre oscillait entre environ 984 et 1 534 tr/min. Le contrôle
rouvrait son by-pass en sortant de la zone de ralenti : ce saut a été supprimé.
Un deuxième défaut transformait les gaz refoulés dans l'admission en air frais ;
les bilans de constituants ont été ajoutés et testés dans les deux sens.

Après ces corrections, le ralenti sans charge restait à environ 1 250 tr/min,
by-pass exactement fermé. Le diagnostic a montré **19,65 g/s d'air frais entrant
par l'échappement**, contre **0,127 g/s au papillon**. Le collecteur était relié
directement à une atmosphère infinie : le gaz du tuyau et du silencieux ne
séparait pas les pulsations des cylindres de l'air extérieur. Cette mesure a
motivé un volume aval conservatif, au lieu de masquer l'erreur par un couple
ou une réduction de volume sonore.

Un seul volume parfaitement mélangé restait trop diffusif pour la composition.
Le modèle final transporte donc les constituants dans **16 cellules axiales**
du volume aval ; sa thermodynamique reste 0D. L'air entré à l'embouchure doit
être transporté jusqu'au collecteur. Les limites CFL sont explicites et les
tests vérifient les inventaires. Le travail de turbine est retiré de la chaleur
du volume aval au pas suivant. Les bancs inactifs ne disposent d'aucun débit ;
un moteur placé entièrement sur le second banc fonctionne sans banc fantôme.

Avec VVT, l'admission est retardée de 20° au ralenti pour réduire le croisement
et les gaz résiduels, avec une transition de 50 ms vers l'avance sous charge.
Ce réglage fait varier le débit physique. Mesure après 0,5 s de conditionnement
puis 20 s de rotation libre, charge de 16 Nm appliquée après 10 s :

| Fenêtre | Moyenne | Étendue | MAP moyenne | DFCO / nouveaux ratés |
|---|---:|---:|---:|---:|
| Sans charge, 8–10 s | 849,47 tr/min | 840,70–855,47 | 23,10 kPa | 0 / 0 |
| Avec 16 Nm, 18–20 s | 849,25 tr/min | 832,53–860,72 | 32,71 kPa | 0 / 0 |

Consigne 850 tr/min. Dix ratés restent présents pendant le démarrage/transitoire.
Correction énergétique absolue 0,000131 J pour 109 912 J de chaleur. Le COV du
travail **total** des vingt derniers cycles chargés est 0,336 % ; il ne valide
pas le COV par cylindre. Voir [le rapport gaz](PHYSICAL-GAS-2026-09-28.md).

## Preuves et outils

- `tests/physical_engine.rs` : ordres dominants I4/V8/V12, énergie numérique,
  déterminisme, coupure, câblage incorrect, charge, ralenti, papillon et turbo.
- `tests/physical_realtime.rs` : aucune allocation pendant un V12 biturbo à
  connexion X, changements de commande et redémarrage compris, avec et sans
  afterfire optionnel.
- `tests/physical_afterfire.rs` : réaction accrue, injection réellement retenue,
  fenêtres bornées, annulation et erreurs de géométrie lisibles. L'excitation
  `afterfire_heat_j` représente 80 % de la chaleur chimique réagie ; ce test
  n'est pas une preuve de bilan énergétique global.
- Une coupure volontaire de carburant ne gonfle plus le compteur de ratés.
  Le pompage demeure présent, la chaleur et l'injection sont nulles ; les
  vrais défauts d'allumage avec carburant restent comptabilisés.
- Test acoustique : transmission entre bancs pour H/X, isolation pour les lignes
  séparées, décroissance de l'impulsion à 48 et 96 kHz.
- Test V8 croisé : demi-ordres présents par banc et annulation partielle dans
  la somme ; changer le premier câble d'allumage ne déplace plus les cames.
- `examples/physical_engine.rs` : WAV brut, états, travail par cycle et spectres
  par ordre. Les valeurs brutes ne sont pas les niveaux de l'écouteur final.
- `examples/scratch_levels.rs` : véritable chaîne 2× et décimation, mute,
  volumes 0,8/1, vues Orbit/Tailpipe et mesure du limiteur.
- `bess --audio-check REPORT scratch 600 "V12 60°" cycle` : calcul réel silencieux
  sur le périphérique, commandes cycliques, rapport des défauts physiques,
  sous-alimentations de la file et budget du callback.

Démo prête : `output/physical-listening-20260928/index.html`, trois WAV mono
PCM24/48 kHz de 12 s (I4, V8 croisé, V12). Les séquences utilisent un régime
imposé et passent par le moteur de rendu partagé avec l'application, sans
normalisation. Formats, échantillons, fin silencieuse et liens sont vérifiés.
L'aperçu visuel dans le navigateur intégré n'a pas été possible : sa politique
a refusé le protocole `file:` ; aucun contournement n'a été tenté.

Le CSV final de niveau (`output/physical-bench-levels-20260928.csv`) mesure
l'I4 stock au ralenti, volume 0,8, Orbit : **−21,85 dBFS RMS**, crête −9,54 dBFS.
Le volume nul est exactement silencieux. Au même ralenti, la vue Tailpipe
atteint le limiteur de −1 dBFS à 0,8 et 1. Ces mesures remplacent les premières
mesures provisoires de ce lot ; elles ne constituent pas une appréciation à
l'oreille. [Détails des clips et niveaux](BESS-PHYSICAL-CRANK-CONTROLLER-2026-09-28.md).

Les premières mesures, conservées comme références historiques, sont dans
[l'audit du moteur](PHYSICAL-ENGINE-AUDIT-2026-09-28.md). Elles ne doivent pas être
présentées comme des résultats de la dernière compilation.

Validation avant le dernier raccordement de l'afterfire : **226 tests release réussis**, Clippy strict sur
toutes les cibles, `cargo fmt --all -- --check`, `git diff --check` et graphe
Graft synchronisé. Journaux : `output/physical-final-{tests,clippy,fmt,build}-20260928.txt`.
Le formatage a également corrigé les écarts présents dans les fichiers concernés
avant ce lot, sans changement de comportement.

Exécutable de cette validation et de l'endurance ci-dessous : `target/release/bess.exe`, 7 561 216 octets,
SHA-256 `903A45970C9F5B9C7E170D7B09742E12BCF78E623D5FE52571006CB45D8BE741`.

La mesure isolée avant essai de liaison optimisée donne **5,846 µs/pas** à
96 kHz pour le V12 (56,1 % du temps réel). ThinLTO avec une unité de génération
a donné 5,987 µs/pas sur un essai de deux secondes, sans amélioration mesurée ;
ces options n'ont pas été conservées. La cible de 5 µs demeure ouverte. Les
calculs tiennent le temps réel dans cette mesure, ce qui ne suffit pas à établir
l'absence d'interruptions du périphérique.

Le précontrôle V12 de 60 s avec 30 ms de file, pendant des compilations, a
observé 3 sous-alimentations / 1 122 échantillons manquants, sans panne du moteur
ni dépassement du callback. Le bloc producteur le plus lent a duré 26,639 ms.
La file réserve désormais quatre blocs de 10 ms et conserve les compteurs
d'erreur. L'essai suivant a réellement ouvert la sortie audio à 48 kHz et
calculé le V12 à 96 kHz pendant **600,000 secondes**, avec 5 976 changements
de commande et un volume de sortie nul :

| Mesure de l'endurance | Résultat |
|---|---:|
| Sous-alimentations / trames manquantes | 0 / 0 |
| Dépassements du budget du callback | 0 |
| Temps maximal du callback | 0,179 ms |
| p99 du callback, borne supérieure du seau | ≤ 1 % du budget |
| Temps maximal d'un bloc de synthèse de 10 ms | 11,635 ms |
| Défaut du producteur / moteur physique | aucun / aucun |
| Callbacks / blocs produits | 60 000 / 60 004 |

Rapport brut : `output/physical-v12-soak-600s-20260928.txt`. Exécution du
28 septembre, 20:59:24–21:09:25 (Europe/Brussels), sur l'exécutable identifié
ci-dessus. L'essai prouve la tenue observée de ce scénario sur ce périphérique,
avec cette file de 40 ms. Il n'établit ni la latence acoustique totale, ni
l'écoute subjective, ni l'absence d'interruptions sous toute charge système.
Le raccordement optionnel de l'afterfire a été ajouté après cette compilation ;
sa validation doit rester distinguée de cette endurance.

La suite complète a ensuite été rejouée après le raccordement de l'afterfire,
la correction du compteur DFCO et l'extension du test d'allocation : **230 tests
release réussis**, aucun échec. Clippy strict sur toutes les cibles et le
formatage passent. Le graphe Graft est synchronisé (96 fichiers, 1 320 nœuds).
Journaux finaux : `output/physical-final-afterfire-{tests,clippy,build}-20260928.txt`.

Exécutable livré : `target/release/bess.exe`, **7 562 752 octets**,
SHA-256 `1E42590DF9BE52ABEB0E1BD964C195CDB5740B8956DB05EF6AF071BD312915D7`.
Les mesures d'afterfire, conventions énergétiques et empreintes des trois WAV
inchangés figurent dans [le rapport des commandes](BESS-PHYSICAL-CRANK-CONTROLLER-2026-09-28.md).

Ce dernier exécutable a repassé le scénario V12 cyclique sur le périphérique
pendant **60,000 s**, le 28 septembre de 21:15 à 21:16 : aucun défaut physique
ou producteur, aucune sous-alimentation, aucune trame manquante, aucun
dépassement du callback ; p99 ≤ 1 %, callback maximal 0,046 ms, bloc de synthèse
maximal 10,393 ms. Rapport : `output/physical-v12-final-60s-20260928.txt`.
L'afterfire optionnel est désactivé dans ce scénario appareil ; son comportement
activé est couvert par les tests numériques et d'absence d'allocation, pas par
une endurance longue en rotation libre.

## Limites qui ne se résolvent pas par une compilation

La stabilité numérique ne prouve pas la fidélité à un moteur mesuré. La loi
calorifique, les coefficients d'écoulement, les cartes turbo, les masses mobiles
et les pertes acoustiques sont des estimations. L'admission à papillons
individuels utilise encore un volume équivalent commun. Les raccords H/X
couplent les perturbations acoustiques ; ils ne constituent pas un solveur CFD
spatial. La dissipation T60 et les limites de pression restent des protections
provisoires.

Les cibles exactes de COV par cylindre, de respiration du ralenti, de MAP et de
résonance pendant une longue coupure demandent une calibration séparée. Le COV
du travail total moteur n'est pas celui d'un cylindre. L'identification 2AFC et
la note MUSHRA nécessitent de vrais auditeurs et une référence enregistrée.

Le solveur 1D MUSCL–HLLC et la calibration différentiable restent la phase 6
optionnelle. Les extensions de cliquetis et de convolution d'une réponse de
pièce ne sont pas implémentées par ce lot. Aucun de ces points n'est marqué
réussi par les seuls essais automatisés, et aucun essai en jeu BeamNG n'est
revendiqué pour le scratch.
