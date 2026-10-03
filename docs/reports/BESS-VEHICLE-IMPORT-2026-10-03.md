# Import du véhicule vers le banc — 3 octobre 2026

Le banc peut maintenant reprendre la masse déclarée, les rapports avant (1 à 12), le rapport de pont, le rayon géométrique des pneus et le couple déclaré servant de référence à l'embrayage. Le couple qui entraîne le véhicule provient toujours du moteur physique. L'import lit l'archive en place, sans extraction ni réécriture.

## Sélection et résolution

`vehicle_setup::inspect` exige une configuration `.pc` unique et une pièce JBeam `main` unique. Il suit ses slots récursivement : choix de `pc.parts` prioritaire, chaîne vide = slot retiré, sinon pièce par défaut. Une pièce présente dans le ZIP mais inaccessible depuis cette racine ne contribue pas aux paramètres. Un choix périmé et détaché dans `pc.parts` n'active donc pas une pièce.

Une pièce choisie doit aussi déclarer un `slotType` correspondant au slot accessible ; une pièce incompatible ou un format de type non pris en charge reste ignoré. Un choix erroné d'une transmission dans un slot de roues ne peut donc pas fournir les rapports du banc.

Le parseur lit uniquement des objets/tableaux/chaînes/nombres/booléens/null. Il accepte les commentaires JBeam, les virgules finales et les virgules omises. Les variables simples `$nom` sont résolues avec les valeurs par défaut des seules pièces actives puis `pc.vars` ; les expressions `$=…` restent des données non évaluées. Une expression inverse avant le neutre ne bloque pas les rapports avant ; une expression ou variable inconnue dans ces rapports annule toute la boîte importée afin de ne décaler aucun numéro de vitesse.

La masse vient exclusivement de `info_<configuration>.json: Weight`, jamais d'une somme partielle des nœuds. `Torque` du même fichier dimensionne seulement la référence d'embrayage. Les rapports de pont sont attribués aux dispositifs différentiels identifiés, les rayons aux tables `pressureWheels`. Des valeurs actives contradictoires, une pièce dupliquée, plusieurs configurations, une boîte incohérente, des variables non résolues ou des valeurs hors plage conservent les estimations existantes du banc. Les notes exposent les sources et les champs manquants.

Limites de lecture : 20 000 entrées ZIP, 4 Mo par document, 64 Mo cumulés de métadonnées, profondeur 64, un million de valeurs par document. Les clés d'objet dupliquées ou une syntaxe non prise en charge invalident le document concerné ; une deuxième configuration illisible ne rend pas artificiellement la première unique. Le parseur ne charge aucune ressource externe.

## Intégration et limites

`beamng::Vehicle.setup` porte le résultat ; `VehicleSetup::apply` applique uniquement les champs résolus et valides. L'interface appelle cette méthode à l'import neuf ou à la réinitialisation explicite des paramètres véhicule. Les projets rechargés conservent leur conduite enregistrée. Les six anciens rapports gardent leur format JSON ; six rapports supplémentaires et un nombre de vitesses permettent les boîtes jusqu'à douze rapports.

Ce sous-ensemble ne simule pas le système de pièces BeamNG complet : expressions Lua/JBeam, CVT, hybrides, réduction de transfert complexe et ressources externes ne sont pas évalués. Un rayon déclaré est géométrique et ne modélise pas l'écrasement du pneu. Le banc utilise une masse et un rayon équivalents uniques ; si les valeurs actives diffèrent, il conserve ses estimations. Les forces aérodynamiques, l'adhérence et la dynamique détaillée du véhicule restent hors de cet import.

## Vérification reproductible

Les tests couvrent la sélection active, les slots par défaut et leurs suppressions, les choix détachés, les variables et leur priorité, les expressions refusées, les ambiguïtés, les bornes et la masse déclarée. `examples/vehicle_setup.rs` lit les archives de `cars/`, valide les paramètres appliqués et compare les SHA-256 avant/après chaque lecture. Il vérifie explicitement Genesis Phantom : 1 372 kg, sept rapports `2.17, 1.73, 1.34, 1.05, 0.85, 0.71, 0.60`, pont `5.62`, rayon `0.365 m`, référence d'embrayage `278 Nm`.

Premier passage release : huit tests d'import réussis ; exemple compilé et exécuté sur les douze archives. Les cinq paramètres sont résolus sur les douze véhicules, tous les réglages appliqués sont valides et les douze SHA-256 avant/après sont identiques. L'assertion Genesis complète passe. Manifeste : `output/final-readiness-20261003/vehicle-import/corpus.json`. Seize documents de structure/suspension sortent du sous-ensemble syntaxique accepté et sont explicitement signalés ; les paramètres ci-dessus proviennent des documents déclaratifs pris en charge.

Après ce premier passage, le contrôle de compatibilité `slotType` et son neuvième test négatif ont été ajoutés. Les neuf tests passent dans la suite release intégrée finale de 337 tests ; le corpus a été régénéré avec le contrôle `slotType`, toujours 12/12 et sans changement des archives. Une course entre noms de fichiers temporaires révélée par les tests parallèles a été corrigée par réservation exclusive d'un nom unique. Ce rapport ne revendique ni essai BeamNG, ni conformité d'un véhicule réel.
