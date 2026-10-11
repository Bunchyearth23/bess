# W-015 / P6 — Transfert structurel et torsion réduite

La qualification du deuxième lot et le remplacement de la projection d'arrêt par des événements/contraintes statiques sont détaillés dans [BESS-MAPS-TORSION-QUALIFICATION-2026-10-11.md](BESS-MAPS-TORSION-QUALIFICATION-2026-10-11.md).

Le lot technique ajoute des réponses modales importées et une torsion vilebrequin/volant optionnelles. Les nouveaux projets gardent la mécanique à zéro dans le mix ; aucune acceptation sonore ni réponse réellement mesurée n'est annoncée.

## Livraison

- `StructuralResponse` importe jusqu'à 32 modes : fréquence Hz, rapport d'amortissement, masse kg, participations tête/bloc et transfert de vitesse vers pression Pa/(m/s). La provenance, le matériel, les positions capteur/écoute et le statut mesuré restent enregistrés.
- Les sources BESS existantes sont des excitations relatives. Leur conversion en force possède des facteurs N/unité et son propre statut `excitation_scale_measured` : importer une fréquence réelle ne transforme pas ces sources estimées en forces mesurées.
- Le solveur modal utilise le point milieu implicite et un compte travail/énergie/amortissement. L'observation remplace le transfert perceptuel uniquement si `ExperimentalSpec.mechanical_transfer` est présent. Les fréquences dépassant 0,4 fois la fréquence de synthèse du moteur sont refusées; le chemin de sortie conserve sa décimation habituelle. Les transferts importés décrivent la position documentée ; une autre position nécessite une autre réponse.
- `TorsionConfig` partage l'inertie totale existante entre vilebrequin et volant, avec raideur Nm/rad et amortissement Nm·s/rad explicites. Le gaz, les masses alternatives et le démarreur agissent côté vilebrequin ; la charge agit côté volant. La vitesse des cylindres suit le vilebrequin en rotation libre. Un banc imposant RPM demeure rigide.
- Le point milieu implicite ferme le travail avec énergie cinétique des deux inerties, énergie du ressort et dissipation de l'amortisseur. Les projections au repos sont déclarées dans `TorsionLedger.stop_loss_j`. Les coefficients par défaut du modèle activé sont estimés, jamais présentés comme ceux d'un moteur identifié.
- Les configurations sont des structures bornées `Copy`, avec états fixes ; aucun tableau dynamique n'est créé pendant un pas audio. Les défauts des trois nouvelles options sont absents ; les chemins historiques restent conservés.

## Validation et démonstration

Les tests dédiés couvrent un transitoire de couple/charge et son bilan de travail, convergence en pas de temps, pertes de repos déclarées, réponse modale excitée puis dissipative, compte modal fermé, import/sauvegarde et domaine de fréquence. Les deux tests structurels et les deux tests de torsion passent dans le binaire release déjà compilé ; la suite intégrée doit reconfirmer les dernières modifications communes. La démonstration compile, passe Clippy strict et s'exécute sans échec moteur.

La démonstration `mapped_turbo_mechanics` génère références baseline/importées, CSV, WAV mécanique complet, WAV contacts/combustion/slap séparés et mix commun avec gain mécanique zéro. Les sorties brutes sont conservées. Des copies séparées `*-audition.wav` offrent une écoute à RMS mécanique 0,05 : leur gain constant hors ligne est enregistré explicitement dans `summary.json`, commun aux trois sources de chaque référence, et n'entre jamais dans un projet ni dans la simulation. Les fichiers de réponse joints portent `measured: false` et des conversions de force non mesurées.

Exécution dev à 48 kHz, 2 s par référence, même graine : torsion maximale 0,07089 rad, énergie de ressort maximale 45,23 J pendant la phase libre ; RPM libres cartographiés 2938–5809. Le transfert synthétique produit un RMS brut de `6,511e-7`, contre `4,991e-4` pour la référence perceptuelle : cette différence de sensibilité est visible, sans interprétation de fidélité. Les gains d'écoute séparés valent environ 76 790 et 100,17. Les artefacts et traces sont dans `work/simulation-p5-p6/` ; les temps dev ne qualifient pas la cible de performance.

## Réception encore ouverte

La torsion est un modèle réduit à deux inerties ; elle ne remplace pas une identification du vilebrequin, du volant et des accouplements. Les passages au repos utilisent une projection inélastique explicitement comptée ; la qualification des démarrages/stalls exige des mesures et des scénarios supplémentaires. Les contacts restent des sources estimées tant que leur conversion force/impulsion n'est pas mesurée. L'import de modes et les comptes numériques ne ferment pas X-019 : la réception sonore des sources isolées puis du mix reste une décision d'écoute utilisateur. Mesures du bloc/culasse, coefficients de rayonnement et coût/endurance sur le portable réel restent ouverts.

## Validation commune finale

Le lot intégré passe 459 tests release, Clippy strict et formatage. Les exemples sont reconstruits et rejoués en release après le correctif de persistance exacte des coefficients JSON; tous les rendus terminent sans erreur. Les preuves et répertoires courants sont dans le [bilan commun](BESS-SIMULATION-PARALLEL-2026-10-11.md). Les critères mesurés, sonores et de performance indiqués ci-dessus restent ouverts.
