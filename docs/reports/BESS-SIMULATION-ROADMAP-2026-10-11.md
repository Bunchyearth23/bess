# BESS — Chantiers de simulation P1 à P7

Date : 11 octobre 2026. Demande : créer les sept chantiers proposés après examen de la simulation actuelle. D-047 les a enregistrés comme `planned`; D-048 les passe à `building` et lance leur exécution parallèle sur demande explicite de l'utilisateur. Ce document définit les travaux et leur réception.

P1 à P7 désignent les rangs de la proposition, indépendamment des niveaux de gravité P1/P2/P3 du registre des risques. Les identifiants durables sont W-010 à W-016.

## Cadre commun

- Améliorer le moteur physique B commun à Automation et à la création libre (D-036/D-038), avec les mêmes paramètres enregistrés pour écoute, WAV et export. Conserver original A, baseline importée, provenance et variantes originales (D-043).
- Préparer les migrations et comparaisons avant de modifier les comportements. Les options expérimentales existantes restent explicites; leur activation par défaut demande une décision distincte. La mécanique reste à zéro dans les nouveaux projets selon W-006.36.
- Conserver un ensemble de référence versionné : I4/I6/V8/V12, ralenti, charges partielles, pleine charge, démarrage, coup de gaz, coupure et reprise; variantes atmosphérique/turbo selon le phénomène étudié. Chaque comparaison indique configuration, graine, taux, commande et gain de présentation.
- Vérifier les bilans masse/énergie/carburant, la convergence en pas de temps et les limites du domaine. Ne pas masquer une instabilité par écrêtage ni transformer une donnée synthétique en mesure réelle.
- Pour chaque lot livré : tests release adaptés puis suite intégrée, Clippy strict tous targets, formatage, démonstration fonctionnelle et mesures avant/après. Contrôler la détermination live/WAV et les remplacements préparés sans allocation lorsqu'ils sont affectés.
- Suivre coût par pas, temps de rendu et endurance audio sur une machine identifiée. X-014 reste ouvert jusqu'à qualification de la cible V12 historique et du portable réel; les compromis de performance ne doivent pas relâcher les bilans ou masquer des différences sonores.
- Séparer réception numérique, confrontation aux mesures, écoute utilisateur et BeamNG. Les boucles WAV ne transportent pas automatiquement toute la dynamique transitoire; une intégration audio dynamique dans BeamNG constitue un périmètre ultérieur.

## P1 / W-010 — Calibration sur un moteur réel

**Objectif :** remplacer les estimations déterminantes par des paramètres identifiés et vérifier la fidélité hors des points utilisés pour l'ajustement.

**État de départ :** W-006.31, X-005/X-018/X-025 restent ouverts. La calibration WAV de W-006.11 ne constitue pas une identification du moteur. Source : [readiness](BESS-READINESS-2026-10-03.md).

**Travaux :**

1. Définir le format des références avec unités, moteur/configuration, géométrie connue, conditions thermiques, RPM/charge/MAP/richesse, couple et provenance. Indiquer explicitement les canaux manquants et la synchronisation audio/télémétrie; OBD ne mesure pas la pression cylindre.
2. Préparer import, alignement, comparaison stationnaire et analyse des ordres moteur. Fournir un exemple synthétique clairement identifié pour vérifier les outils.
3. Sélectionner un moteur réel documenté et inventorier les données accessibles; arrêter avant ajustement les points d'identification, points de validation et tolérances adaptées à l'incertitude des mesures.
4. Identifier les paramètres physiques influents avec bornes et analyse de sensibilité; conserver un jeu témoin, la provenance et les résultats non ajustés.
5. Comparer sur les points réservés couple/MAP/richesse, ordres et enveloppes sonores, puis organiser l'écoute à niveau comparable.

**Sortie :** outils et jeu de référence reproductibles; calibration réelle acceptée uniquement avec provenance, critères fixés avant ajustement et résultats sur points réservés. Sans mesures, la préparation technique peut être livrée, mais la réception réelle et X-018/X-025 restent ouverts.

## P2 / W-011 — Couplage admission–cylindres–échappement

**Objectif :** rendre les retours d'ondes et leurs effets sur le remplissage/couple cohérents aux deux soupapes.

**État de départ :** prototype échappement opt-in, retour admission absent de ce prototype; X-028 échoue toujours au seuil strict 10 %. Source : [physique avancée](BESS-ADVANCED-PHYSICS-2026-10-03.md).

**Travaux :**

1. Reproduire X-028 et distinguer propagation d'un tube à état constant, frontière soupape et réponse du moteur complet; conserver le test historique et son seuil.
2. Identifier les écarts de référence moyenne/perturbation, timing et transport; corriger les frontières avec passivité et bilans conjoints vérifiés.
3. Relier les ondes d'admission aux soupapes avec impédances, composition, flux inverse et budgets compatibles avec les volumes de gaz.
4. Comparer longueurs, croisement et calage de cames à 48/96 kHz sur le même moteur, sans mêler gains d'observation et états physiques.
5. Livrer dynos et WAV couplés/non couplés, coût CPU/endurance et confrontation aux références W-010.

**Sortie :** qualification historique 10 % réellement réussie, frontières passives/conservatives et absence d'oscillation parasite sur la grille; convergence et effets admission documentés. Une autre référence diagnostique ne ferme pas X-028. Le couplage demeure expérimental tant que ses critères ne sont pas satisfaits.

## P3 / W-012 — Carburant et transitoires

**Objectif :** faire dépendre la richesse et la chaleur libérée du trajet réel du carburant lors des démarrages, reprises et coupures.

**État de départ :** `CycleCylinder::step_shared_checked` conserve du carburant gazeux mais injecte un équivalent dosé avec l'air entrant; son commentaire indique l'absence de film liquide/évaporation. Les propriétés de gaz variables sont déjà présentes : [lot gaz](BESS-GAS-BATCH-2026-09-29.md).

**Travaux :**

1. Définir des inventaires liquide/film/vapeur et une injection temporisée; distinguer admission indirecte/directe uniquement si le type est connu ou choisi explicitement.
2. Implémenter dépôt, entraînement et évaporation avec chaleur latente et bilans carburant/énergie; éviter le redosage du carburant réaspiré.
3. Relier températures, commande d'injection et enrichissement transitoire au contrôleur, sans inventer une cartographie mesurée.
4. Vérifier démarrage froid/chaud, échelon de papillon, DFCO/reprise et ratés alimentés; distinguer coupure volontaire et défaut de combustion.
5. Livrer traces richesse/couple/température et WAV comparables, avec sauvegarde/migration et chemin commun live/offline.

**Sortie :** bilans fermés incluant film et chaleur latente, convergence et inventaires bornés; scénarios reproductibles et confrontation W-010 lorsqu'elle est disponible. Les constantes non mesurées restent étiquetées estimées.

## P4 / W-013 — Combustion plus prédictive

**Objectif :** mieux relier développement de flamme, durée de combustion et variabilité à l'état de la charge.

**État de départ :** loi de Wiebe avec richesse, résiduels, dépendance RPM et dispersion corrélée; paroi thermique déjà dynamique. Le cliquetis/sa commande restent à calibrer (X-025); aucune nouvelle authenticité n'est déduite de ces composants.

**Travaux :**

1. Mesurer la sensibilité du modèle actuel et établir des références pression/angle, chaleur libérée et CA50, en distinguant traces synthétiques et mesures W-010.
2. Proposer puis implémenter un modèle réduit de développement/propagation dépendant de pression, température, richesse, dilution et turbulence estimée; conserver le bilan d'énergie du carburant.
3. Approfondir les variations cycle/cylindre et ratés à partir de l'état physique, avec graines stables et corrélations contrôlées.
4. Calibrer délai d'auto-inflammation, seuils et commande anticliquetis sur des références d'avance limitée par le cliquetis, lorsqu'elles existent.
5. Comparer pression, couple, chaleur, coût CPU et son sur points réservés, puis livrer une démonstration fonctionnelle.

**Sortie :** pas de chaleur créée artificiellement ni combustion avant l'étincelle; convergence, bilans et amélioration sur points réservés vérifiés. La qualification du cliquetis exige des références réelles; X-025 reste ouvert sans elles.

## P5 / W-014 — Turbo fondé sur des cartes mesurées

**Objectif :** prédire plus fidèlement débit, rendement, montée en pression et pompage du groupe turbo.

**État de départ :** `Induction::step_with_species` utilise une caractéristique générique, des rendements fixes et une wastegate proportionnelle sans dynamique d'actionneur; X-024/X-027 ne constituent pas une calibration mesurée.

**Travaux :**

1. Définir/importer des cartes compresseur et turbine avec débit/vitesse corrigés, unités, conditions de référence et provenance; définir le comportement hors domaine et les données manquantes.
2. Implémenter interpolation et rendements dépendants du point de fonctionnement, sans discontinuité aux limites des lignes de vitesse.
3. Ajouter dynamique d'actionneur wastegate/BOV, pertes mécaniques et échanges thermiques pertinents, en fermant les bilans arbre/gaz/parois.
4. Vérifier spool, échelons de charge, choke, lever de pied et surge; conserver une référence explicite pour les projets sans carte réelle.
5. Comparer pression, vitesse arbre, débit, température et son avec W-010; tester import, sauvegarde et restitution dans la voix commune.

**Sortie :** cartes correctement référencées, bilans fermés et scénarios stables/convergents; fidélité déclarée uniquement pour le matériel et le domaine réellement mesurés. Une carte synthétique ne ferme pas X-024/X-027.

## P6 / W-015 — Mécanique et rayonnement sonore

**Objectif :** rendre les vibrations et leur rayonnement moins dépendants de coefficients perceptuels arbitraires.

**État de départ :** contacts, combustion et slap excitent déjà des modes bloc/culasse; le vilebrequin est à un degré de liberté. X-019 reste ouvert après les rejets utilisateur. Source : [excitation mécanique](BESS-MECHANICAL-EXCITATION-2026-10-03.md).

**Travaux :**

1. Constituer une référence de sources isolées et définir/importer des réponses mesurées bloc/culasse avec position de capteur, unités et provenance.
2. Améliorer le modèle réduit modal/transfert et l'excitation physique des contacts, sans substituer une hausse de gain à la correction de la source.
3. Ajouter une dynamique torsionnelle réduite vilebrequin/volant, avec inerties, raideurs et amortissements explicites; vérifier le bilan de travail avec la dynamique cylindre.
4. Relier modes mécaniques et position d'écoute en préservant la séparation physique/observation et le coût temps réel.
5. Livrer sources isolées et mix complet à niveau comparable; vérifier les moteurs importés/libres et recueillir l'écoute utilisateur avant toute remise en avant de cette couche.

**Sortie :** réponse stable/dissipative, dynamique conservatrice ou pertes expliquées, migration et contrôles fonctionnels; réception sonore explicite pour fermer X-019. Le défaut mécanique zéro des nouveaux projets reste inchangé sans instruction distincte.

## P7 / W-016 — Mode 1D complet et couplé hors ligne

**Objectif :** étendre le mode qualité aux collecteurs/conduits et partager l'état thermodynamique avec le moteur couplé.

**État de départ :** le 1D optionnel remplace seulement les primaires; aval acoustique conservé; combinaison 1D + feedback refusée car les références ne sont pas partagées. Maillage retenu 4 mm/CFL 0,4. Sources : [qualité](BESS-QUALITY-CALIBRATION-2026-10-03.md), [physique avancée](BESS-ADVANCED-PHYSICS-2026-10-03.md).

**Travaux :**

1. Définir une interface commune d'état moyen, perturbations, composition et flux; cartographier les échanges 0D/1D avant leur remplacement.
2. Étendre les volumes finis aux collecteurs et tronçons retenus, puis à l'admission selon les résultats W-011; traiter jonctions, pertes et températures avec bilans explicites.
3. Coupler les frontières soupapes/cylindres au solveur 1D, sans double comptage de débit, énergie ou bruit.
4. Vérifier tubes/jonctions analytiques, conservation, convergence spatiale/temporelle, réflexions et réponse moteur; conserver 4 mm/CFL 0,4 comme référence tant qu'aucune nouvelle décision ne la remplace.
5. Livrer le rendu hors ligne, progression/annulation et mesures temps/mémoire; confronter à W-010 et aux diagnostics W-011, avec projets/migrations reproductibles.

**Sortie :** partage thermodynamique et frontières couplées qualifiés, convergence/bilans démontrés et rendu exploitable. Le mode reste hors ligne; aucun support audio temps réel n'est promis par ce chantier. Le seuil X-028 reste inchangé.

## Ordre et dépendances

P1 à P7 restent l'ordre de priorité enregistré. La préparation W-010 (formats, référence et métriques) précède les comparaisons des autres lots; l'arrivée de mesures réelles ne conditionne pas les bancs synthétiques et la préparation technique.

W-011 et W-012 sont les premières évolutions du cœur; W-013 réutilise leurs interfaces physiques et les métriques W-010. W-014 utilise les bilans et références communs. W-015 exploite l'excitation W-013. W-016 s'appuie sur les frontières W-011 et les échanges gaz/carburant W-012/W-013; le turbo cartographié est intégré aux cas turbo après W-014.

Chaque chantier conserve deux jalons séparés : livraison technique démontrable, puis qualification mesurée/sonore applicable. Une donnée externe absente laisse ce second jalon ouvert, sans transformer un test synthétique en preuve réelle ni marquer automatiquement le parent `blocked`/`landed`.
