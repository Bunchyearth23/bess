# Physique avancée et coût — 3 octobre 2026

Implémentation et rendus livrés ; la suite release finale compte **337 tests
réussis, 0 échec et 6 ignorés**. Les options restent désactivées par défaut. **X-028, la
qualification stricte 1/L du moteur complet, reste ouverte et n'est pas annoncée
réussie.** Cela ne constitue ni une acceptation sonore ni une validation physique
sur un moteur mesuré ou dans BeamNG.

## Contrats livrés

- **W-006.27** : `experimental.native_rate_acoustics` sélectionne le taux de sortie
  pour l'acoustique, la radiation et le traitement sonore. À 48 kHz, le gaz conserve
  deux sous-pas à 96 kHz. Le mode historique garde le moteur à 96 kHz et la
  décimation FIR 2:1. `Scratch::synthesis_rate()` est commun aux origines importée
  et scratch, au rendu et aux remplacements préparés. Un changement de taux exige
  une reconnexion du producteur audio.
- **W-006.28** : `calibrate_coupled_level()` mesure quatre points du moteur choisi
  via le chemin audible à 48 kHz, avec la politique de taux sélectionnée. Chaque
  mesure laisse 0,5 s d'établissement puis intègre 0,5 s de puissance. Le gain
  proposé minimise l'erreur quadratique en dB avec un poids égal par point ; les
  offsets individuels sont retournés. `coupled_level_db` est appliqué après le
  traitement sonore de l'échappement. Il ne change ni le couple, ni les ondes, ni
  l'admission. Aucun suivi de RMS, AGC ou ajustement automatique pendant l'écoute.
- **X-030** : le délai du banc 1 est une observation après le réseau acoustique.
  Il ne rallonge plus les primaires physiques. Le délai et le niveau de banc sont
  exclus de la graine des dispersions de cylindres et de la clé du banc de couple.
  Leurs remplacements préparés conservent les états gaz, vilebrequin et thermiques.
  Le niveau de banc et le gain de calibration suivent une transition de 20 ms
  lors des retouches ; leur initialisation utilise directement le gain demandé,
  sans rampe au début d'un rendu à paramètres fixes.
  Cette correction change le son des anciens projets à délai de banc non nul.
- **X-029** : `ValveFlow` prépare les invariants de soupape hors de l'itération
  Newton ; débit et pente partagent les exponentielles du rapport de pression.
  Les constantes d'écoulement sonique ne sont calculées que lorsque nécessaires,
  puis réutilisées dans la même résolution. Les budgets de masse et les seuils de
  convergence ne sont pas relâchés.
- **X-031** : `vvt_overlap_safe` est une carte alternative explicite. L'avance en
  charge monte progressivement entre 2 500 et 5 000 tr/min ; la commande de ralenti
  et la carte au-dessus de 5 000 tr/min restent identiques. Ce n'est pas une carte
  prétendument mesurée. Les cames admission/échappement indépendantes appartiennent
  au lot parallèle.
- **Qualité 1D optionnelle** : `primary_1d` remplace seulement la propagation des
  primaires par `Primary1d`, avec référence initiale 101 325 Pa, 673 K, gamma 1,33
  et R 287. Le collecteur, les filtres et la ligne aval restent ceux du réseau
  acoustique. Les erreurs sont propagées au moteur ; les amplitudes hors domaine
  ne sont pas écrêtées pour cacher un échec. Le changement de longueur utilise
  l'échange d'inventaire explicite du module, pas un modèle de paroi mobile.
  L'ouverture d'un périphérique audio refuse cette option : qualification hors
  ligne seulement, avec rendu WAV disponible. La combinaison 1D + couplage est
  rejetée explicitement : leurs références thermodynamiques ne sont pas encore
  partagées. La maille par défaut reste 4 mm ; la maille 20 mm est un diagnostic,
  pas un autre réglage retenu. Le lot qualité dispose de son propre rapport.

## Mesures de référence locale

Machine : Ryzen 7 3800X, 8 cœurs / 16 threads, Windows, compilation release
`x86-64-v3`. Ce ne sont pas les anciennes mesures du portable dev17. Mesure
monothread, minimum de trois passages alternés couplé/non couplé, rampe de 1 s
1 000→6 000 tr/min à charge 0,6. Les valeurs sont par échantillon du moteur ; à
96 kHz, la sortie à 48 kHz en consomme deux. Elles excluent le callback audio.

Référence avant optimisation de la soupape et déplacement du délai de banc, avec
la graine déjà neutralisée pour les contrôles sonores de banc :

| Moteur | Taux acoustique | Non couplé, µs | Couplé, µs |
|---|---:|---:|---:|
| I4 | 48 000 | 5,036 | 6,442 |
| I4 | 96 000 | 3,058 | 3,862 |
| V12 | 48 000 | 11,227 | 15,648 |
| V12 | 96 000 | 6,337 | 8,819 |

Snapshot final, après optimisation de soupape, déplacement du délai de banc et
lissage des gains, sans compilation ni autre calcul BESS concurrent :

| Moteur | Taux acoustique | Non couplé, µs | Couplé, µs |
|---|---:|---:|---:|
| I4 | 48 000 | 4,946 | 6,084 |
| I4 | 96 000 | 2,974 | 3,638 |
| V12 | 48 000 | 11,093 | 14,703 |
| V12 | 96 000 | 6,278 | 8,319 |

Mesure isolée `final/cpu.csv` ; le passage intermédiaire reste disponible dans
`pre-mean-fix/cpu.csv`. Sur l'I4, dont le délai de banc est nul,
le chemin couplé coûte 5,6–5,8 % de moins. Sur le V12, le lot complet coûte
5,7–6,0 % de moins ; cette comparaison inclut aussi le déplacement du délai de
banc et ne prétend donc pas isoler parfaitement l'optimisation algébrique.
Le surcoût couplé V12 reste environ 32,5 %. Le mode natif économise environ 11,6 %
par seconde de son. **La cible de 5 µs V12 n'est pas atteinte sur cette machine.**
Le V12 couplé demande ici 0,706 s CPU par seconde de son en natif, contre
0,799 s dans le mode historique à deux échantillons acoustiques par sortie.
Ces minima de microbanc ne remplacent pas l'endurance du périphérique réel.

Fichiers : `output/advanced-physics-20261003/baseline/cpu.csv`,
`calibration.csv`, `vvt.csv`. Le binaire de référence est conservé dans ce dossier.
Les fichiers d'écoute de ce dossier sont des diagnostics intermédiaires et ne
sont pas la livraison finale.

La mesure brute sur I4/V8/V12 × 850/2 500/4 500/6 500 tr/min donne un offset moyen
global de **+1,264 dB**, mais des offsets individuels de **−3,863 à +3,175 dB**.
Une constante universelle ne peut donc pas égaliser tous les points. La commande
de mesure par moteur donne une recommandation fixe et expose sa dispersion.
Après déplacement du délai de banc, la même grille finale de 12 points donne
**+2,541 dB** en moyenne (extrêmes +0,157 à +5,667 dB) : le gain dépend bien de la
configuration acoustique. Les clips utilisent les mesures propres au moteur et
à quatre régimes de sa plage : **I4 +2,833575 dB**, **V12 +2,405032 dB**.
Les quatre offsets I4 vont de +1,189 à +4,927 dB, ceux du V12 de +0,648 à
+4,553 dB ; après compensation, leur dispersion ne disparaît pas.

## X-028 : contrôle strict de la loi 1/L

Le test historique a été exécuté explicitement malgré son ancien `ignore` :
facteur de régime 0,623822 pour une longueur 0,7, soit −10,88 %, échec au seuil
10 %. Passer uniquement le moteur couplé de 8 à 48 kHz donne 0,620971 : ce n'est
pas la correction recherchée.

Le banc suit maintenant `scratch.synthesis_rate(48000)` lorsque les ondes
alimentent les soupapes : 96 kHz historique ou 48 kHz natif. Le choix entre dans
la clé du banc uniquement sous couplage. Les longueurs/dissipations aval y sont
également conservées car leurs réflexions atteignent les soupapes ; EQ et gains
d'observation sont exclus. La propagation 1D sans feedback ne déclenche aucun
nouveau calcul de couple coûteux.
L'API `dyno::sweep_at_rate()` permet de comparer les deux côtés au même taux ;
le test et la nouvelle grille de diagnostic le font. Le seuil 10 % est conservé.
La grille au même taux, avec le fond de pression conservé dans la livraison, distingue :

| Carte | Longueur | Facteur ajusté | Écart relatif | Résidu RMS | Gain maximal de couple |
|---|---:|---:|---:|---:|---:|
| Historique, came 0,3 | 0,7 | 0,620971 | −11,29 % | 0,04585 | 11,03 % |
| Historique, came 0,3 | 1,4 | 1,212181 | −13,42 % | 0,03753 | 23,48 % |
| VVT alternatif, came 0,3 | 0,7 | 0,644153 | −7,98 % | 0,03637 | 11,15 % |
| VVT alternatif, came 0,3 | 1,4 | 1,212181 | −13,42 % | 0,03289 | 17,32 % |
| Fixe, came 0,3 | 0,7 | 0,641208 | −8,40 % | 0,03310 | 11,97 % |
| Fixe, came 0,3 | 1,4 | 1,212181 | −13,42 % | 0,03306 | 16,51 % |
| Fixe, came douce | 0,7 | 0,832553 | +18,94 % | 0,01325 | 5,31 % |
| Fixe, came douce | 1,4 | 1,184729 | −15,38 % | 0,01275 | 8,95 % |

Source : `output/advanced-physics-20261003/pre-mean-fix/lengths.csv`.
La carte VVT seule ne résout donc pas X-028. La qualification stricte reste ouverte.

L'essai de fond pression/densité filtré à 12 Hz a donné un facteur **1,095958**
pour la longueur 0,7, encore plus éloigné du critère. Cet essai a été **rejeté et
retiré** ; les pressions conservées du collecteur et la relation de port
antérieure restent utilisées.

Un deuxième diagnostic garde le port non linéaire mais absorbe toutes les ondes
revenant aux soupapes (`sweep_anechoic_reference`). Il ne remplace pas la
référence historique. Un test permanent couvre son invariance physique à la longueur. Pour
la carte historique, le fit donne **0,832553 (+18,94 %) / 1,212181 (−13,42 %)** :
ce changement de référence ne suffit pas non plus. La grille complète à deux
références est dans `final/lengths.csv`. Le test de qualification historique
reste explicitement ignoré en CI avec sa raison et peut être lancé séparément ;
**son seuil de 10 % n'a pas été élargi et son échec n'est pas compté comme une
réussite**. Le couplage reste expérimental et désactivé par défaut.

Les sept tests de `physical::wave_junction` passent dans cette révision, dont
l'équivalence du débit/pente préparés, la passivité sur 40 000 perturbations et
les bilans masse/énergie/carburant d'un cylindre couplé. Gain incrémental maximal
mesuré : 1,0000000000001876 (arrondi numérique, tolérance du test 1e-6).

La loi simple 1/L suppose notamment une vitesse d'onde fixe. Dans ce modèle,
température, composition, débit moyen et calage changent avec le régime ; une
mesure sur le moteur complet ne doit pas être confondue avec la loi d'un tube
isolé à état constant. Cette observation n'est pas utilisée pour élargir le seuil.

## Reproduction et réception

La comparaison de treize régimes par moteur (`final/rates.csv`) mesure un écart
maximal absolu de couple 48→96 kHz de **0,40 % I4 couplé** et **0,13 % V12
couplé** ; les cas non couplés restent sous 0,01 %. Le banc suit néanmoins le
taux sélectionné, au lieu de présenter l'approximation comme une identité.

Sur la grille VVT de 144 points (`final/vvt.csv`), I4 à primaires égaux et came
0,3 : à 2 000 tr/min, le couple passe de **150,52 à 157,93 Nm** sans couplage
(+4,92 %) et de **146,86 à 152,48 Nm** avec couplage (+3,83 %). À 3 500 tr/min,
le lobe couplé descend de **183,43 à 177,60 Nm** (−3,18 %) ; à 3 000 tr/min,
le couple couplé baisse légèrement de **151,58 à 150,87 Nm** (−0,47 %).
La carte alternative n'est donc ni une disparition de tous les creux, ni un
gain universel. Elle reste une variante à comparer à une référence mesurée.

`examples/advanced_physics.rs` fournit les modes `cpu`, `calibration`, `vvt`,
`lengths`, `rates` et `listening`. Le dernier livre des WAV mono 48 kHz PCM 24 bits,
échappement seul, avec un gain de présentation fixe commun de 0,25 (−12,04 dB),
jamais une normalisation par clip. Il écrit les niveaux RMS/crête et les gains
de calibration mesurés. Les signaux hors pleine échelle sont refusés.

Scène commune : 1 s à 850 tr/min, montée de 3 s vers 5 850 tr/min à charge 0,7,
puis lever pendant 2 s. Variantes I4 et V12 : référence 2×, acoustique native,
couplage brut, couplage calibré, carte VVT alternative. Écoute utilisateur,
préférence de niveau et essai BeamNG restent à effectuer.

Les **10 WAV finaux de 6 secondes** sont dans
`output/advanced-physics-20261003/final/`. Leurs en-têtes ont été vérifiés
(mono, 48 000 Hz, PCM 24 bits) ; aucun dépassement pleine échelle n'a été masqué.
Après le dernier correctif de transition des gains, les dix rendus ont été
recalculés avec le binaire final : **10/10 SHA-256 sont identiques**. Les fichiers
finaux sont donc conservés. Preuve :
`output/advanced-physics-20261003/post-slew-check/waveform-preservation.json`.
La calibration est enregistrée dans `i4-calibration.json` et
`v12-calibration.json`, et chaque clip a son fichier `.scratch.json`.

| Moteur | Variante | RMS dBFS | Crête dBFS |
|---|---|---:|---:|
| I4 | Référence 2× | −31,552 | −16,020 |
| I4 | Natif | −32,051 | −16,415 |
| I4 | Couplé brut | −35,411 | −21,477 |
| I4 | Couplé calibré | −32,578 | −18,643 |
| I4 | VVT alternatif, calibré | −32,554 | −18,643 |
| V12 | Référence 2× | −33,880 | −19,161 |
| V12 | Natif | −34,312 | −19,664 |
| V12 | Couplé brut | −37,577 | −23,732 |
| V12 | Couplé calibré | −35,172 | −21,327 |
| V12 | VVT alternatif, calibré | −35,147 | −21,281 |

Source complète : `final/listening-levels.csv`. Ces signaux isolent l'échappement
et gardent une marge de présentation commune ; ce n'est pas le mix complet de
l'atelier avec la position d'écoute et les couches admission/mécanique.

Tests ciblés ajoutés : invariance physique du délai/niveau de banc et du gain
couplé, gain d'observation exact, transition de gain sur 20 ms sans saut ni
altération des états physiques, chemin natif identique au moteur direct,
compatibilité des anciens projets, carte VVT haute vitesse inchangée, équivalence
débit/pente de la soupape préparée. Les 10 tests `advanced_physics` passent en
release, y compris le contrôle de transition sur 20 ms et de préservation exacte
du couple, de la pression d'admission, de la chaleur, du carburant et des
pressions de banc. Les 3 tests `physical_realtime` passent aussi, dont l'absence
d'allocation/destruction de buffers pendant les retouches préparées. Source :
`output/final-readiness-20261003/tests-release.log`. La validation globale et
l'endurance sur périphérique sont consignées par le lot principal.
