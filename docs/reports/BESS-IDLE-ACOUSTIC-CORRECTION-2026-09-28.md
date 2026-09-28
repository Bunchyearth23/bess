# Correction du caquètement au ralenti — 28 septembre 2026

L'utilisateur décrit un « bruit de poule » au ralenti. Les validations
précédentes de stabilité, d'énergie thermique et de temps réel n'établissaient
pas la qualité sonore. Ce retour invalide l'acceptation sonore du lot précédent.

## Diagnostic reproductible

`examples/physical_idle_probe.rs` produit huit secondes à 850 tr/min, papillon
0,05, pour I4, V8 croisé et V12 : échappement/admission/mécanique séparés à
96 kHz, véritable écoute Orbit à 48 kHz et états à 1 kHz. Mesures sur les
secondes 2–8, sans normalisation. Référence préservée dans
`output/physical-idle-chirp-before-20260928/`.

Le signal brut d'échappement concentrait 85–93 % de sa puissance entre 500 et
2 000 Hz et environ 0,11 % sous 250 Hz. Le défaut précède donc le mélange
d'écoute. Pendant cette mesure, le régime imposé et le by-pass étaient constants,
sans nouveau raté. L'admission présentait les cadences attendues de combustion
28,33 / 56,67 / 85 Hz ; la décimation n'effectuait aucune transposition.

Le premier essai, convection sur débit moyen plutôt qu'instantané, réduisait
certains écarts d'enveloppe mais déplaçait les pics vers 2–2,3 kHz. Il ne
résolvait pas le défaut. Preuves conservées dans
`output/physical-idle-chirp-meanflow-20260928/`.

L'ablation du retour de pression acoustique vers l'orifice non linéaire est
discriminante : le signal brut perd 31–36 dB par rapport à cet essai, tandis
que la chaleur de combustion varie de moins de 1,8 %, sans nouveau raté.
Les pics Orbit retombent à 28,33 / 56,67 / 85 Hz et 90–95 % de la puissance
passe sous 250 Hz. Le couplage explicite retardé entretenait donc une tonalité
parasite, malgré des états thermiques finis. Le bornage de sa pression à ±20 %
ne garantissait pas sa passivité. Résultats d'ablation :
`output/physical-idle-chirp-no-feedback-20260928/`.

## Correction retenue

- Le système 0D conservatif calcule les échanges de gaz et leur contre-pression.
  Ces débits physiques excitent l'acoustique. L'onde retardée n'est plus ajoutée
  directement à la pression de l'orifice. Une interaction bidirectionnelle
  entre les ondes et les soupapes exigera une résolution de frontière passive ;
  elle n'est pas revendiquée par ce correctif.
- La frontière acoustique à débit prescrit suit `p+ = Z Q + p−`. Son ancienne
  réflexion arbitraire de 0,15 absorbait artificiellement les modes bas.
- La convection utilise un débit moyen lissé sur 120 ms. Les pulsations restent
  dans l'excitation ; elles ne déplacent plus brutalement la tête de lecture du
  retard pour toutes les ondes déjà présentes dans le tuyau.
- Le réseau aval prépare ses arrivées une fois avant la jonction. La même onde
  sert au calcul de la jonction et à sa propagation ; il n'existe plus de retard
  supplémentaire d'un échantillon à cette frontière.
- La conversion fixe pression/amplitude passe de `1/24000` à `1/3000`, soit
  +18,06 dB, après suppression de l'oscillation parasite. Ce gain ne dépend ni
  du régime, ni de la charge, ni du RMS observé. Le gain d'écoute 0–1 et le
  limiteur existants restent actifs.

La relation de frontière utilise les identités de pression et de débit des
[jonctions de guides d'ondes](https://dsprelated.com/freebooks/pasp/Lossless_Scattering.html).
Ce choix constitue un modèle acoustique linéaire excité par les débits physiques,
pas un solveur spatial complet de gaz compressible.

## Vérifications

Un nouveau test I4/V8/V12 à 48 et 96 kHz rejette un ralenti dont l'énergie est
dominée par cette tonalité haute, indépendamment du gain final. Un autre compare
bit à bit la frontière couplée préparée avec la frontière autonome, y compris
préparation répétée, changement de température et décroissance. Les tests de
ralenti libre, turbo, afterfire, énergie, déterminisme et absence d'allocation
passent. La suite complète compte **232 tests release réussis**, aucun échec.
Clippy strict sur toutes les cibles passe. Journaux :
`output/physical-idle-correction-20260928/{full-tests,clippy,build}.txt`.

Exécutable reconstruit : `target/release/bess.exe`, 7 562 240 octets,
SHA-256 `60EBE23ED36F4FA38D46BDA1D48151E05A7D490EAD004A6F47A6FF52567A903A`.

Cet exécutable a calculé le V12 cyclique sur le périphérique à 48 kHz pendant
60,000 s, volume de sortie nul : **0 sous-alimentation, 0 trame manquante,
0 défaut physique/producteur, 0 dépassement du callback**. Callback maximal
0,023 ms, p99 ≤ 1 % du budget, bloc de synthèse maximal 11,033 ms. Rapport :
`output/physical-idle-correction-20260928/device-v12-60s.txt`. L'endurance de
dix minutes du lot précédent ne constitue pas une mesure de cet exécutable.

### Résultats finaux

| Ralenti Orbit, secondes 2–8 | I4 | V8 croisé | V12 |
|---|---:|---:|---:|
| RMS, écoute 0,8 | −29,83 dBFS | −28,52 dBFS | −28,10 dBFS |
| Puissance sous 250 Hz | 96,61 % | 95,10 % | 90,71 % |
| Pic spectral dominant | 141,67 Hz | 56,67 Hz | 85 Hz |

Le pic I4 est la cinquième harmonique de sa cadence de 28,33 Hz ; les deux
autres pics sont aux fondamentales d'allumage. Aucun nouveau raté dans la
fenêtre mesurée. Les mêmes métriques avant/après et les états complets sont
disponibles dans `output/physical-idle-chirp-after-20260928/`.

La paire I4 d'écoute est recadrée aux secondes 2–8, PCM24/48 kHz. L'ancien son
est **atténué** de 8,139 dB pour rejoindre le RMS du nouveau, laissé inchangé.
Ces copies comparables ne remplacent aucun original et n'introduisent aucune
normalisation dans le moteur. `tools/physical_idle_analysis.py` reproduit les
mesures et les copies ; `index.html` les présente sans lecture automatique.

Les cycles complets de douze secondes sont régénérés dans
`output/physical-listening-idlefix-20260928/`. Les trois moteurs passent les
contrôles d'échantillons finis, d'absence de défaut et d'extinction finale.
Au palier de 4 500 tr/min, leurs RMS sont −8,30 / −6,94 / −5,46 dBFS ; le
limiteur de −1 dBFS intervient. Il ne faut donc pas interpréter cette calibration
comme une grande réserve de niveau à pleine charge. Le CSV
`output/physical-idle-correction-20260928/levels-final.csv` couvre aussi volumes
0/0,8/1, Orbit/Tailpipe, I4 standard et V12 ouvert ; le mute reste exactement nul.

Les anciens WAV et les anciennes mesures de niveau restent des preuves
historiques du diagnostic. La disparition du défaut mesuré ne remplace pas le
jugement de l'utilisateur sur le nouveau son. Aucun essai BeamNG ni validation
sur un enregistrement de moteur réel n'est ajouté par ce lot.
