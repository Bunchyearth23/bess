# Flotte BESS → BABM, 4 octobre 2026

## État

Les 13 véhicules installés ont été repris depuis leurs archives originales
préservées, traités par BESS puis fusionnés individuellement avec BABM.
Les packs sont **installés et vérifiés**. L'utilisateur a explicitement demandé
l'installation pendant que BeamNG fonctionne (« installe, le jeu gère »).
Les 13 nouveaux packs et le registre propre sont en place. Les empreintes et CRC
des archives installées, les 13 sources retrouvées par BESS et les 13 reçus de
variante BABM ont été relus avec succès après installation.

La demande confirmée est **13 entrées séparées et deux configurations par entrée** :
la configuration originale et une seule configuration BESS. B5 A/C/GT et Volk
Icarus I/II restent séparés.

## Profils

Les caractéristiques moteur importées restent identiques à la référence BESS
issue de chaque archive. Seuls le son d'échappement et les niveaux sont ajustés.
Les géométries estimées par BESS et les hypothèses d'import sont conservées dans
les métadonnées ; il ne s'agit pas de reconstructions mesurées de vrais moteurs.

| Véhicule | Moteur importé | Profil BESS |
| --- | --- | --- |
| Advent TC | V8 4,50 L | Sport Classic |
| Archo Coupe | V8 4,00 L | Classic Coupe |
| B5 A | 4 cylindres en ligne 1,00 L | Economy |
| B5 C | 4 cylindres en ligne 1,06 L turbo | Turbo Sport |
| B5 GT | 4 cylindres en ligne 1,07 L turbo | Turbo Road |
| Berlingo 1000Lingo | 4 cylindres en ligne 1,73 L turbo | Turbo Extreme |
| Cerberus A | V6 3,59 L | Sport Road |
| Genesis Phantom | 6 cylindres en ligne 3,00 L | Modern Sport |
| Nord Optis | 4 cylindres en ligne 1,30 L | Classic Economy |
| Pulsar Opti | 3 cylindres en ligne 1,30 L turbo | Modern Turbo |
| Thunderhawk Zero | V8 2,79 L | Classic Road |
| Volk Icarus | 4 cylindres en ligne 1,40 L | Historic |
| Volk Icarus II | 4 cylindres en ligne 1,40 L | Historic |

Le profil du Berlingo est fondé sur sa déclaration de 1 012 kW, turbo et
nitrométhane, plutôt que sur son nom de carrosserie. Les anciennes peu puissantes
reçoivent moins d'aigus et de niveau ; les versions sportives gardent davantage
de présence. Le temps de décroissance de l'échappement est réglé entre 45 et
60 ms. Les réglages exacts sont sauvegardés dans les projets individuels.

## Vérifications observées

- 13 archives d'origine copiées et contrôlées par SHA-256.
- Chaque export complet BESS conserve tous les membres d'origine byte pour byte.
- 13 packs BABM, 13 racines de véhicule distinctes et 26 configurations.
- Deux pièces moteur distinctes résolues par véhicule ; références WAV des blends
  présentes, chemins audio générés sans collision entre véhicules.
- 1 496 WAV générés : mono, 48 kHz, PCM 24 bits, non silencieux et sans saturation
  numérique, crête maximale 0,94. Les 748 WAV d'origine sont conservés à
  l'identique après fusion.
- BABM reconnaît chaque profil comme `variant_installed`, y compris après le
  changement de suffixe des sauvegardes.
- Les treize archives réellement installées passent SHA-256 et CRC ; les treize
  reçus BABM associent chaque export à son pack installé, et BESS retrouve toutes
  les sources pristine. Preuves : `installed-after.json`, `installed-catalogue.json`
  et `installed-inspections.json`.
- Restauration compilée sur copie de l'Advent : original et export restaurés aux
  empreintes attendues depuis les sauvegardes inactives ; BESS retrouve l'original.
- Trois tests BESS d'export complet et deux tests BABM de fusion initiale réussis ;
  Clippy strict, formatage Rust et compilation Python des nouveaux outils réussis.
- Génération des 13 véhicules : 425,8 secondes au total.

## Nettoyage préparé

Le journal réel de BeamNG montre le montage de ZIP sous `.babm_backup`, et le
registre contient 30 sauvegardes actives, dix anciens packs et dix autres mods.
Une sauvegarde cachée portant l'extension ZIP reste donc un mod pour ce jeu.

La flotte préparée utilise `.zip.merged_backup` dans les sauvegardes et leurs
manifestes. La restauration BABM et la redécouverte BESS ont été vérifiées avec
ce suffixe. L'installation conserve les dix autres mods et leurs préférences,
retire les anciennes entrées de sauvegarde du registre et active seulement les
13 packs de la flotte. Une copie complète de `mods` est exigée avant remplacement.

Le registre final vérifié contient 23 entrées : 13 packs de la flotte et dix
autres mods conservés. Les 26 sauvegardes de fusion portent un suffixe inactif.
Le journal du jeu confirme la détection et l'activation à chaud des 13 nouveaux
packs ; cela ne vérifie pas leur affichage dans le sélecteur ni leur écoute.

Le premier contrôle du registre a tenu compte à tort des nouveaux packs déjà
inscrits par le jeu parmi les autres mods. Le remplacement a été annulé et le
contrôle corrigé pour la reprise. L'ancien `babm_genesis.zip` présente également
un CRC invalide sur `camso_t_alcantara_r_rbgmaskg.dds` ; son contenu observé est
conservé dans la sauvegarde. La source Genesis pristine et son remplacement
passent les contrôles. Détails : `prior-genesis-corruption.json`.

Les futures opérations BABM peuvent encore créer des historiques portant
l'extension ZIP : la protection de cette passe est réalisée par le nouvel outil
de livraison, et ne modifie pas tous les chemins de sauvegarde internes de BABM.

## Artefacts et limite

- Dossier de travail : `D:\BeamMP\current\BESS-exports\clean-20261004`.
- `fleet.json` : sources, réglages, exports, packs et empreintes.
- `audit.json` : vérification des 13 packs et des fichiers audio.
- `restore-check.json` : restauration depuis les sauvegardes inactives.
- `commands.jsonl` : commandes exécutées, résultats et durées.
- Sauvegarde complète : `D:\BeamMP\current\BESS-clean-backup-clean-20261004`.
- Projets BESS modifiables : sous-dossier `projects` ; exports : un dossier par véhicule.

Ces preuves portent sur les fichiers et le passage entre applications. La
présence effective des 13 entrées dans le sélecteur BeamNG, le comportement en
conduite et la pertinence sonore restent à vérifier en jeu par écoute.
