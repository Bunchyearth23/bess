# W-006 — cylindre piloté par vilebrequin externe

`src/physical/cycle.rs` ajoute un cylindre quatre temps utilisable par le moteur multicylindre. L'ancien prototype à régime imposé reste disponible séparément.

## Comportement livré

- L'angle global déroulé pilote volume, soupapes et couple gazeux. La phase mécanique est distincte du décalage électrique de l'étincelle. Une étincelle décalée de 360° tombe sur le croisement et ne produit aucune combustion.
- Le régime et le temps de pas viennent du moteur parent. Le cylindre ne contient aucun oscillateur ni impulsion sonore : la sortie acoustique exploitable est son débit réel à l'échappement.
- Flux signés d'admission et d'échappement avec enthalpie du donneur ; budgets de masse entrants fournis par le parent pour partager les réservoirs. Les masses et enthalpies retournées correspondent aux transferts effectivement appliqués.
- Injection : ajout réel de masse et d'enthalpie, rapport air/carburant de référence 14,7, PCI 43 MJ/kg. La masse de carburant et la masse d'air frais sont suivies dans le mélange. La combustion transforme ces traceurs en produits sans retirer leur masse du gaz. Le carburant non brûlé est évacué proportionnellement au mélange et rapporté séparément aux deux ports.
- Wiebe avec CA50 nominal à 8° après PMH, fenêtre d'allumage de compression, contrôle de richesse et dilution. La coupure de carburant annule immédiatement injection et chaleur ; le pompage continue. Un brûlage très lent décale le CA50 plutôt que commencer avant l'étincelle. La combustion s'arrête à l'ouverture de l'échappement.
- Variabilité corrélée déterministe, optionnelle, évaluée à chaque étincelle. Elle agit sur retard/durée et réussite de combustion, pas sur un gain audio aléatoire. La graine est restaurée par `reset()`, sans allocation.
- Échange thermique Hohenberg, évalué tous les 32 pas, puis échange de Newton à chaque pas. Paroi métallique avec capacité thermique et liaison au liquide de refroidissement. Les corrections énergétiques numériques du gaz et de la paroi restent visibles séparément.

## Hypothèses explicites

Le modèle conserve les propriétés thermodynamiques air-like du socle (`cv=650+0,16T`) pour air, carburant vaporisé et produits. C'est un suivi de masses agrégées, sans chimie détaillée, évaporation ni film liquide. Les fractions d'air non consommé et de carburant sont désormais fournies par chaque réservoir : une réaspiration transporte leur composition réelle, sans recréer de l'oxygène. Le carburant déjà mélangé dans une poche réaspirée est déduit de la nouvelle injection. Les quatre transferts de traceurs aux ports sont signés et concordent avec la convention des transferts massiques.

Les lois de durée, dilution, ratés, variabilité, coefficients de décharge et constantes de paroi sont des estimations à calibrer. Le COV d'IMEP cible du rapport n'est pas encore établi par mesure dans ce lot.

Hohenberg utilise **p en bar**, V en m³, T en K et vitesse moyenne du piston en m/s, coefficient 130 et constante de vitesse 1,4. Cette convention a été vérifiée dans l'[équation 49 de cette publication de modélisation](https://www.nature.com/articles/s41598-025-15819-7). Pour V=0,0005 m³, p=1 bar, T=400 K et vitesse=5 m/s, le coefficient vaut environ 82,44 W/(m² K). Le multiplicateur de calibration est explicite et peut être mis à zéro pour un test adiabatique.

## Vérification observée

`cargo test --release --lib physical::cycle` : **9 tests réussis**, environ 0,07 s hors compilation.

- Combustion avec dégagement de chaleur, absence totale de chaleur sans étincelle, pompage et évacuation du carburant toujours présents.
- Conservation cumulative du carburant injecté = brûlé + évacué + restant ; cohérence des flux massiques et énergétiques de ports avec le bilan du gaz.
- Coupure carburant immédiate et mauvais câblage électrique sans combustion.
- Cylindre fermé adiabatique : masse constante et retour de pression après compression/détente, erreur relative inférieure à `1e-8`.
- Respect des budgets entrants des réservoirs.
- Reproductibilité bit à bit, équivalence après déplacement commun des phases, variabilité reproductible après reset et différente avec une autre graine.
- Corrections énergétiques du cas nominal de combustion inférieures à 1 % de la chaleur dégagée ; le test conserve explicitement ce seuil, il ne les masque pas.
- Réaspiration de produits : des flux réels dans les soupapes ne créent ni air frais, ni nouvelle injection, ni chaleur.
- Conservation des traceurs air/fuel sur les deux sens de chaque port, erreur absolue inférieure à `1e-12` kg, y compris consommation par combustion.
- Un mélange déjà dosé et réaspiré ne reçoit pas une seconde dose identique de carburant.

`cargo clippy --release --lib -- -D warnings` : réussi sur l'état du dépôt lors du contrôle. Ces preuves portent sur le calcul local ; elles ne prouvent ni l'absence de sous-alimentation audio en conditions réelles, ni l'acceptation sonore en jeu.
