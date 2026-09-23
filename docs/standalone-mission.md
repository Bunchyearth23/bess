# Mission : développer un synthétiseur sonore de moteurs thermiques

Tu interviens dans ce dépôt pour développer un logiciel de synthèse sonore de moteurs thermiques à pistons, paramétrable et utilisable en temps réel.

L’objectif est un instrument de synthèse cohérent et extensible, pas un outil certifié de prédiction acoustique ni un simulateur thermodynamique complet. Il doit produire du son à partir d’une configuration et de commandes, sans dépendre obligatoirement d’enregistrements.

## 1. Commence par examiner le projet

Inspecte le dépôt, ses instructions, son langage, son système de build, ses dépendances audio et graphiques, ses tests et son état actuel. Préserve les modifications existantes et ne remplace pas une fonctionnalité opérationnelle sans raison.

Respecte la stack présente. Si le projet utilise C++, Xmake, raylib, SDL ou une autre technologie, exploite d’abord ces choix. Rust n’est ni obligatoire ni privilégié. Si le dépôt est vide, choisis une stack adaptée au DSP temps réel, justifie brièvement ce choix et commence par une application minimale.

Évite les refontes inutiles, les dépendances lourdes et les abstractions anticipant des fonctionnalités non demandées.

## 2. Direction technique

Construis un noyau procédural inspiré de la physique :

Commandes → phase moteur → événements par cylindre → excitations → admission/bloc/échappement → mixage → sortie audio.

Sépare trois responsabilités :
- La description du moteur et ses commandes.
- Le traitement DSP, indépendant de l’interface et du périphérique audio.
- L’application : lecture audio, contrôles, presets et export.

Dans la première version, le régime est une commande externe. Ne prétends pas déduire correctement le régime de l’accélérateur sans modèle de couple, d’inertie et de charge mécanique.

Ne commence pas par du machine learning, une simulation CFD, une thermodynamique complète ou une banque massive d’échantillons. Une calibration à partir d’enregistrements pourra être ajoutée ensuite.

## 3. Configuration et commandes

Prévois une configuration sérialisable, versionnée et validée contenant au minimum :
- Le nombre de cylindres et le type de cycle ; implémente d’abord le quatre-temps.
- Les phases des événements par cylindre, exprimées avec une convention angulaire explicite.
- Le routage des cylindres vers les bancs ou collecteurs.
- Les paramètres simplifiés des excitations, conduits, pertes et résonances.
- Les niveaux relatifs admission/bloc/échappement.

Sépare les paramètres décrivant le moteur des réglages de calibration sonore. Ne présente pas un coefficient de filtre comme une dimension mécanique exacte.

Les commandes continues doivent distinguer le régime en tr/min, la charge normalisée et le volume utilisateur. La charge doit pouvoir modifier l’intensité et le timbre, pas uniquement le volume final.

Représente explicitement l’état de combustion. Un couple négatif, une pédale relâchée et une coupure d’injection ne doivent pas être considérés automatiquement comme le même état.

Valide les unités, les bornes, les indices de routage, les valeurs non finies et les configurations impossibles. Documente toute approximation.

## 4. Horloge et événements

Utilise une phase continue conservée entre les blocs audio.

Pour un moteur quatre temps :
- rotation_hz = rpm / 60 ;
- cycle_hz = rpm / 120 ;
- un cycle complet correspond à 720 degrés de vilebrequin.

Pour N cylindres avec une combustion par cylindre et par cycle, la cadence moyenne est N × rpm / 120. Cette cadence n’est pas automatiquement la fréquence fondamentale dominante du signal.

À 3 000 tr/min, les tests doivent retrouver environ 25 événements/s pour un monocylindre et 100 événements/s pour un quatre-cylindres, indépendamment de la taille des blocs.

Détecte correctement tous les franchissements d’événements, y compris aux frontières de blocs et pendant les variations de régime. Lisse les commandes sans réinitialiser arbitrairement la phase.

À régime nul, ne crée pas de nouvelles pulsations de cycle. Laisse décroître l’énergie déjà présente dans les résonateurs au lieu de couper brutalement la sortie.

Distingue le calage de combustion des événements d’admission et d’ouverture d’échappement. Une première approximation est acceptable, mais elle doit être nommée et documentée.

## 5. Excitations et chemins acoustiques

Représente les cylindres individuellement, avec leur phase, leur excitation et leur routage acoustique.

Point important : permuter uniquement les noms des cylindres ne doit pas changer le son si leurs excitations, instants et chemins restent identiques. Pour rendre un ordre d’allumage audible, il faut représenter les différences temporelles ou de propagation correspondantes.

Génère des excitations pulsées réglables plutôt qu’une simple sinusoïde. Sépare autant que possible :
- Les composantes synchronisées avec le cycle.
- Le bruit ou résidu irrégulier.
- Les vibrations et résonances du bloc.

Prévois des variations faibles, reproductibles avec une graine fixe, notamment entre cycles. Évite une modulation aléatoire indépendante de chaque paramètre à chaque échantillon.

Commence avec des chemins admission et échappement simplifiés, mais distincts. Le bloc peut initialement utiliser une petite banque de résonateurs clairement identifiée comme approximation.

Pour les conduits, privilégie une représentation explicite par retards, réflexions et pertes. Une relation de départ est :

delay_samples = sample_rate × length_m / sound_speed_m_s

La célérité est une hypothèse du modèle à documenter ; ne traite pas une valeur pour l’air ambiant comme une vérité universelle pour les gaz d’échappement.

Gère les retards fractionnaires lorsque nécessaire, les bornes des lignes à retard et la stabilité des boucles de rétroaction. Ne transforme pas le réseau acoustique en simple égaliseur en prétendant simuler une géométrie complète.

Prévois une stratégie explicite contre l’aliasing des excitations et des traitements non linéaires. Un filtrage de sortie ne répare pas l’aliasing déjà créé. Choisis une solution proportionnée au prototype et mesure son coût.

## 6. Contraintes audio temps réel

Le moteur DSP doit pouvoir être rendu hors ligne et utilisé par le backend audio avec le même chemin de traitement.

Dans le callback audio : aucune allocation dynamique, aucun accès disque, aucune journalisation, aucun verrou bloquant et aucune opération de préparation lourde.

Préalloue les buffers et les états. Transmets les commandes depuis l’interface avec un mécanisme adapté au temps réel. Prépare les changements de topologie hors du callback ; ne reconstruis pas les collecteurs dans le thread audio.

Préviens les clics dus aux changements de paramètres, les NaN, les valeurs infinies et les instabilités. Vérifie aussi les niveaux, l’écrêtage et l’éventuelle composante continue.

Utilise 48 kHz par défaut, avec une fréquence configurable. Le noyau peut être mono au départ. Si la sortie mono est dupliquée sur deux canaux, ne présente pas cela comme une spatialisation stéréo.

Conserve des résultats déterministes à configuration, commandes et graine identiques. Ne normalise pas indépendamment chaque rendu de comparaison : cela masquerait les différences de niveau liées aux commandes.

## 7. Première version à livrer

Priorité 1 : un noyau compilable et testé, capable de générer un WAV hors ligne pour un régime constant et une rampe de régime.

Priorité 2 : lecture audio en temps réel, avec commandes minimales du régime, de la charge et des niveaux des trois sources. Réutilise l’interface existante ; sinon, une interface minimale suffit.

Priorité 3 : chargement/sauvegarde des presets et comparaison de plusieurs configurations.

Fournis au moins trois presets illustratifs, dont deux permettant de comparer un même nombre de cylindres avec un phasage ou un routage différent. Ne les présente pas comme des reproductions validées de moteurs commerciaux.

Prévois des scénarios reproductibles : régime stabilisé, montée et descente en régime, variation de charge à régime constant, extinction de la combustion et décroissance acoustique.

Ne développe pas encore les turbocompresseurs, explosions à l’échappement, boîte de vitesses, Doppler, habitacle détaillé, plugins audio ou modèles neuronaux. Ne prépare pas une architecture de plugins uniquement pour anticiper ces possibilités.

## 8. Tests et validation

Ajoute des tests couvrant :
- La conversion régime/phase et le comptage des événements.
- Les événements aux frontières de blocs et l’indépendance à leur taille, à tolérance numérique justifiée.
- Le déterminisme avec graine fixe et le rejet des configurations invalides.
- L’absence de nouvelles impulsions à régime nul et la décroissance des résonateurs.
- L’absence de sorties non finies, de dépassements de buffers et d’instabilités sur les plages autorisées.
- L’invariance lors d’un simple renommage des cylindres et les différences attendues lors d’un vrai changement temporel ou acoustique.

Ajoute un benchmark du coût de rendu, avec fréquence d’échantillonnage, taille de bloc et configuration. Distingue les mesures hors ligne des observations dans le callback audio. Ne transforme pas un benchmark moyen en garantie de temps réel.

Produis des WAV courts de validation et, si l’outillage le permet, une analyse simple du niveau et du spectre. Ne confonds pas le pic spectral maximal avec la cadence des combustions.

Distingue clairement : cohérence mécanique simplifiée, stabilité numérique, plausibilité perceptive et fidélité à un moteur réel. Sans enregistrement de référence ni test d’écoute, ne prétends pas avoir démontré le réalisme.

## 9. Références à consulter et vérifier

Utilise comme points de départ, sans reprendre aveuglément leurs conclusions :

1. Baldan et al., « Physically informed car engine sound synthesis for virtual and augmented environments », 2015.
   DOI : 10.1109/SIVE.2015.7361287.

2. Andy Farnell, « Designing Sound », sections et exemples consacrés aux moteurs automobiles.

3. Julius O. Smith III, « Physical Audio Signal Processing », pour les guides d’ondes, jonctions, pertes et retards fractionnaires.

4. Jagla, Maillard et Martin, « Sample-based engine noise synthesis using an enhanced pitch-synchronous overlap-and-add method », 2012.
   DOI : 10.1121/1.4754663.
   À garder pour une éventuelle branche basée sur des enregistrements.

5. M. L. Munjal, « Acoustics of Ducts and Mufflers », pour approfondir les conduits et silencieux.

6. Dépôts à examiner :
   https://github.com/DasEtwas/enginesound
   https://github.com/ange-yaghi/engine-sim

Pour chaque source réellement consultée, consigne le lien, l’idée utilisée, les limitations et la licence lorsqu’il s’agit de code. Vérifie la compatibilité des licences avant toute réutilisation. Une publication accessible n’accorde pas automatiquement les mêmes droits que son code ou ses données.

Si l’accès réseau ou un document manque, indique-le. N’invente pas le contenu d’une publication et ne bloque pas le prototype sur une source inaccessible.

## 10. Méthode de travail et livrables

Après un bref état des lieux, présente un plan court puis implémente effectivement le premier jalon. Ne t’arrête pas à un document d’architecture ou à des classes vides.

Travaille par incréments compilables. Exécute les tests et les commandes disponibles. En cas de blocage environnemental, distingue ce qui a été exécuté de ce qui a seulement été préparé.

Livre le code, les presets, les tests, les scénarios audio et une documentation concise comprenant :
- README : compilation, lancement, contrôles et export.
- docs/design.md : architecture, unités, conventions et approximations.
- docs/references.md : sources effectivement consultées et licences.
- docs/validation.md : tests, mesures, résultats et limites connues.

Termine par un compte rendu factuel : fonctionnalités opérationnelles, commandes exécutées, résultats, fichiers audio produits et limites restantes. N’annonce pas une qualité acoustique ou une performance que tu n’as pas vérifiée.