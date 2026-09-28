# Gaz, composition, suralimentation et ralenti — 28 septembre 2026

Ce lot constitue un prototype physique mesurable, déterministe et conservatif. Les dimensions équivalentes, pertes, coefficients de combustion et cartes de compresseur sont estimés ; les résultats ci-dessous ne constituent pas une validation sur moteur réel ni une validation subjective du son.

## Modèles livrés

- `src/physical/manifolds.rs` : admission à volume fini, deux collecteurs séparés, papillon avec fuite et bypass en surfaces SI, pertes de sortie selon diamètre/catalyseur/silencieux, transport de masse et d'enthalpie avec `cv(T)` de `thermo`.
- Composition transportée explicitement : air frais, carburant imbrûlé, reste assimilé aux produits. Les contreflux reprennent les fractions du volume donneur ; aucun air frais n'est créé par une recirculation. L'oxygène disponible est estimé à 0,233 fois la masse d'air frais.
- Chaque collecteur possède un volume aval : deux mètres de tuyau et une cavité estimée de silencieux. La température et la pression de ce volume restent moyennes ; 16 cellules d'advection suivent uniquement les constituants. Une faible respiration alternée à la sortie ne mélange donc plus instantanément l'air extérieur jusqu'au collecteur.
- `AfterTreatment::step_resident` utilise le stock actuel du collecteur. Le carburant et l'oxygène effectivement brûlés sont retirés des traceurs, avec masse totale inchangée. Chaleur chimique, stockage dans la paroi et pertes vers l'ambiance sont comptabilisés. Le raccordement moteur dépose la chaleur gaz au pas suivant.
- `src/physical/induction.rs` : énergie d'arbre turbo, récupération de travail turbine, charge compressible, travail/enthalpie compresseur, pertes de paliers et limite de vitesse. Une inertance de conduit et une caractéristique cubique non monotone permettent un débit compresseur signé et le pompage. L'ouverture de BOV agit sur la pression de charge, sans oscillateur sonore.
- `src/physical/controller.rs` : PI de ralenti avec antiwindup conditionnel. La sortie du domaine de régulation à haut régime ferme le bypass au lieu de rouvrir brutalement sa valeur de base. Aucun couple artificiel de ralenti ni plancher imposé de régime.

La construction du modèle réduit de pompage suit [Greitzer, 1976](https://doi.org/10.1115/1.3446138) et les travaux de modélisation dynamique disponibles à la [NASA](https://ntrs.nasa.gov/citations/19850012807). La longueur équivalente de conduit et les coefficients cubiques sont estimés. Le modèle ne résout ni champ spatial d'énergie cinétique ni cellules de décrochage tournant ; sa fréquence et son amplitude ne sont pas une carte turbo mesurée.

## Contrats de conservation

Les budgets de sortie sont partagés entre cylindres et ports. Les contreflux emportent l'enthalpie et les fractions massiques de l'ancien état donneur. Le compresseur et le papillon échangent la même masse et la même enthalpie de part et d'autre. Les sorties de BOV et de compresseur inversé retirent les constituants présents dans la charge.

`ManifoldStep.exhaust_mass_flow_kg_s` reste le débit collecteur → volume aval pour turbine/acoustique. `external_tailpipe_mass_flow_kg_s` est le débit réel volume aval → atmosphère. Les bilans incluent l'énergie et la masse des volumes aval. Le prélèvement turbine est appliqué via `set_tailpipe_heat_j` au pas suivant ; les audits globaux doivent donc garder le terme de chaleur en attente.

La limite CFL des cellules de composition borne les débits demandés ; la masse limitée est exposée par `transport_limited_mass_kg`. Les corrections thermiques restent exposées dans les trois ensembles de bilans admission/collecteurs/volumes aval.

`set_active_bank_mask([bool; 2])` conserve l'identité des deux bancs, y compris un moteur dont tous les cylindres sont raccordés au seul banc 1. Un banc inactif ne possède ni débit ni budget et n'évolue pas. L'aire de sortie est divisée par le nombre de bancs actifs. Le masque vide est rejeté ; `set_active_banks` reste compatible. Les états inactifs stockés demeurent constants dans les sommes d'inventaire et n'échangent aucune masse/énergie.

## Diagnostic et correction du ralenti

Le premier défaut identifié n'était pas le volume sonore : avec bypass fermé, l'admission apportait environ 0,127 g/s d'air mais le collecteur directement relié à l'atmosphère pouvait réaspirer environ 19,65 g/s d'air frais. La recirculation fournissait alors artificiellement assez d'oxygène pour maintenir environ 1250 tr/min. Un volume aval parfaitement mélangé ne suffisait pas à supprimer le raccourci spatial ; l'advection des constituants dans le tuyau a corrigé cette réaspiration immédiate.

La fuite de papillon est maintenant de 0,0003 de la surface principale, soit environ 0,713 mm² à 2 L (diamètre équivalent 0,95 mm). Le bypass garde une plage de 0,035 de la surface principale. Ce sont des calibrations estimées bornées, pas des mesures constructeur.

Le moteur intégré applique ensuite un retard admission de +20° au voisinage du ralenti fermé, fondu vers l'avance sous charge, avec une transition de 50 ms. Cette modification parent dans `engine.rs` réduit le recouvrement et la dilution. Le réglage est une hypothèse de commande VVT, pas une calibration mesurée ; il est distinct du gain d'écoute.

## Mesure intégrée finale

Commande reproductible :

```text
cargo run --release --example physical_engine -- outputs/physical-idle-final-vvt 20 48000 i4-idle-load
```

Les données se trouvent dans `outputs/physical-idle-final-vvt/i4-idle-load.csv`, `i4-idle-load-cycles.csv` et `i4-idle-load-summary.txt`. Les lignes de trace couvrent des fenêtres de 1 ms ; les flux et apports de chaleur sont des moyennes de fenêtre.

| Fenêtre | Charge | Régime moyen | Min–max | MAP moyenne | Bypass moyen | DFCO | Nouveaux ratés |
|---|---:|---:|---:|---:|---:|---:|---:|
| 8–10 s | 0 Nm | 849,47 tr/min | 840,70–855,47 | 23,10 kPa | 0,08290 | 0 | 0 |
| 18–20 s | 16 Nm | 849,25 tr/min | 832,53–860,72 | 32,71 kPa | 0,16619 | 0 | 0 |

Consigne : 850 tr/min. Les deux moyennes sont à moins de 0,1 % de la consigne. Dix ratés ont été comptés pendant le démarrage/transitoire, aucun dans ces fenêtres stabilisées. Le COV du travail final chargé est de 0,336 % sur 20 cycles ; ce n'est ni une mesure d'IMEP réelle ni une validation de la variabilité perceptive.

Sur les 20 s : aucun échec numérique, correction absolue de 0,000131 J pour 109912,471 J de chaleur, régime final 847,53 tr/min. Coût observé : 5520,4 ns par pas de sortie, ratio temps calcul/temps simulé 0,265 sur cette machine et ce scénario. Cette mesure n'est pas extrapolée au V12 ni présentée comme preuve audio temps réel.

Le WAV produit par l'exemple appartient à la chaîne intégrée du parent. Son niveau mesuré est −38,11 dBFS RMS, crête 0,06683 ; ces nombres ne valent pas acceptation subjective du timbre ou de l'écoute.

## Pompage et BOV : mesure ciblée

Après 0,5 s de mise en charge avec soutirage de 0,07 kg/s, pression avant fermeture : 180121,2 Pa. Sur 0,208333 s après fermeture, sans BOV : 0,001009444 kg de débit inversé cumulé et 16 changements de signe. Les variantes de BOV testées suppriment ce débit inversé dans ce scénario et réduisent l'amplitude de pression à moins de la moitié. La masse écrêtée par le garde de débit est nulle, correction thermique inférieure à 1e-7 J.

Sur une fermeture de 0,2 s, comparaison 96/192 kHz : masses inversées 0,001008529823 / 0,001008539331 kg et pressions moyennes 123352,56 / 123353,46 Pa. Ce contrôle établit une convergence numérique locale ; il ne valide pas les coefficients estimés. La BOV recirculée retourne actuellement à une frontière d'entrée idéale, sans volume de conduit amont résolu.

## Vérifications et limites restantes

- Tests ciblés release : manifolds **12/12**, induction **5/5**, controller **8/8**.
- Conservation masse/enthalpie en réseau fermé et ouvert ; réaction résidente sans double combustion ; absence de régénération d'air frais dans une recirculation ; advection de tuyau ; récupération turbine comptabilisée ; absence de port fantôme au seul banc 1.
- Le test miroir des bancs compare pression, débits extérieurs, espèces et bilans après 2000 pas ; le banc inactif reste strictement constant.
- Aucun changement de calibration runtime après l'audit VVT. Le masque bancs ajouté ensuite préserve le cas nominal banc 0 de cet audit.
- Admission ITB actuellement représentée par un volume équivalent commun, pas par des conduits indépendants. Thermodynamique moyenne ; la propagation acoustique est un autre sous-système. Les compositions sont des traceurs simplifiés, pas une chimie multi-espèces. Pas de carte constructeur, validation sur banc moteur ou acceptation sonore fournie par ce lot.

Économie graft communiquée au parent : supplément de **7495 tokens sur 3 appels** depuis le précédent relevé de 87248 tokens/3 appels ; cumul du sous-agent **94743 tokens/6 appels**. Aucun montant monétaire estimé.
