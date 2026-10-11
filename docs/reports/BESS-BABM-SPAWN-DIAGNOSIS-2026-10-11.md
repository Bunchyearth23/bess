# Chargement BeamNG impossible : diagnostic du 11 octobre 2026

Le défaut confirmé se situe dans la fusion BABM. Les cinq scripts Lua de chaque
véhicule sont identiques entre les sources et les exports BESS. Après fusion,
`camsoEngineThermals.lua` est vide dans neuf des treize packs actuellement installés.

Véhicules concernés : Advent TC, Archo Coupe, Berlingo 1000Lingo, Cerberus A,
Nord Optis, Pulsar Opti, Thunderhawk Zero, Volk Icarus et Volk Icarus II.
Les quatre autres packs conservent les scripts ; cela ne prouve pas leur réception en jeu.

## Cause

Dans `D:/Desktop/babm/src/merger.rs`, lignes 231–270, BABM lit entièrement le
fichier avant de tenter une conversion UTF-8. Si cette conversion échoue,
le chemin de secours copie le flux ZIP déjà épuisé : le membre final fait zéro octet.
Les neuf sources concernées contiennent un script de 92 072 octets avec un
octet `0xb0` non UTF-8 à la position 88 874. BESS conserve ce script à l'identique.
Les quatre autres sources ont une version UTF-8 de 92 074 octets.

Le journal conservé `D:/BeamMP/current/beamng.2.log`, datant du 4 octobre,
signale `attempt to index field 'thermals' (a boolean value)` dans
`camsoEngine.lua:2127`, puis désactive Thunderhawk (lignes 696/746 et 1273/1323)
et Nord Optis (1770/1820). Cela correspond au module thermique vidé.

## Second défaut observé

Le journal du 11 octobre monte aussi cinq archives de sauvegarde BABM sous
`.babm_backup`: quatre pour Pulsar et une pour Thunderhawk. Elles recouvrent des
packs actifs. Ce défaut est distinct du script vidé et reste suivi par X-033.

## Suite nécessaire et limites

Corriger le secours de lecture BABM pour préserver les octets non UTF-8 et
vérifier les transformations de chemins ; ajouter un test avec un script non UTF-8.
Restaurer les scripts dans les neuf packs depuis les sources vérifiées, avec
sauvegardes inactives, puis vérifier un chargement Original et BESS dans BeamNG.
Sécuriser aussi les futures sauvegardes BABM contre leur montage par le jeu.

Aucun pack, exécutable ou réglage du jeu n'a été modifié pendant ce diagnostic.
La vérification est une comparaison de fichiers et de journaux conservés ;
aucun nouvel essai de chargement n'a été effectué.
Les tailles, empreintes et chemins par véhicule sont dans
[la preuve JSON](BESS-BABM-SPAWN-DIAGNOSIS-2026-10-11.json).
