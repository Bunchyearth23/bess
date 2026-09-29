# X-014 — Profil du moteur physique V12 (96 kHz) et plan d'optimisation

Analyse en lecture seule : aucun fichier du dépôt n'a été modifié. Tout a été construit et exécuté sur dev17
depuis une copie de `main` (`git archive main`) ; variantes expérimentales dans `srcprof/`, `srcfd/`, `srcfm/`, `srcpw/`, `srcor/`.
Résultats bruts : `dev17-out/x014*/` (ce dossier).

## 0. Méthode et limites (à lire d'abord)

- **`perf` est installé sur dev17 mais inutilisable** : `perf_event_paranoid=4`, pas de `sudo -n`. Pas de `valgrind`, `gdb`, `samply` non plus.
  Un profil échantillonné classique (`perf record -g`) n'a donc **pas** pu être produit. Pour l'obtenir : `sudo sysctl kernel.perf_event_paranoid=1` sur dev17
  puis `perf record -F 2999 --call-graph fp` sur le binaire construit avec `CARGO_PROFILE_RELEASE_DEBUG=true RUSTFLAGS="-C force-frame-pointers=yes"`.
- Substitut utilisé : **chronométrage instrumenté par `rdtsc`** (copie `srcprof/`, `patch.py` reproduit l'instrumentation : 31 « laps » dans
  `engine.rs::advance` et `cycle.rs::step`). Chaque lap coûte ~36 cycles (mesuré : section vide × 12 cylindres = 434 cycles). Les chiffres
  ci-dessous sont **corrigés** de ce surcoût (total brut 26 k cycles/pas → 19,7 k corrigés). Précision : ±10 % relatif par ligne, pas de niveau ligne de source.
- Scénario : `physical_engine <out> 5 96000 v12` = régime imposé 1200 tr/min, `imposed_rpm = Some`, 1 substep/pas à 96 kHz.
  C'est le même chemin que `AutomationVoice` (régime imposé). Le chemin vilebrequin libre ajoute un `sin_cos` par cylindre (`engine.rs` bloc « reciprocating »), non mesuré ici.
- Bruit de mesure dev17 (machine partagée, 24 cœurs) : ±3 à 5 % d'un run à l'autre, avec des pointes à +30 %. Toute comparaison ci-dessous utilise
  12 exécutions alternées (3 s) et compare **min** et médiane. Un gain < 2 % est indiscernable.

## 1. CPU : dev17 vs portable

| | CPU | coût mesuré V12 |
|---|---|---|
| dev17 | AMD Ryzen 9 3900X (Zen 2, 24 threads, boost ~4,4 GHz) | **6,1 à 6,4 µs/pas** (min 6122, médiane 6200 ns) |
| portable utilisateur | Intel i7-8565U (Whiskey Lake, 4 c/8 t, ~4,6 GHz turbo mono, thermique limité) | 5,846 µs/pas (X-014) |

Les valeurs absolues ne sont pas comparables : microarchitecture différente (Zen 2 : `divsd`/`sqrtsd` plus rapides, libm identique).
Les **proportions** sont utilisables ; les gains en µs doivent se re-mesurer sur le portable avant de fermer X-014. dev17 est ~7 % plus lent que la valeur du portable sur ce code.

## 2. Profil par section (dev17, cycles TSC par pas de 96 kHz, corrigé du surcoût des laps)

Total corrigé ≈ 19,7 k cycles/pas. Les cylindres (12) pèsent ≈ 58 %.

| % | cycles/pas | section | localisation |
|---|---|---|---|
| **23,3** | 4605 | Soupapes : `valve_lift` ×2 + `port_flow` ×2 par cylindre | `cycle.rs:259-297` (`valve_lift`, `port_flow`), `gas.rs:110-141` (`mass_flow_from_states`), `gas.rs:230-256` (`HarmonicCam::lift_m`) |
| **19,4** | 3833 | Échappement acoustique (12 primaires + 2 `ExhaustNetwork`) | `acoustic.rs:187-330` (`Acoustic::next`), `acoustics.rs:104-116, 275-320` (`ExhaustNetwork::step_boundary`) |
| **15,4** | 3031 | `Manifolds::step` (clone complet, 5 appels `rate()`, `advect_tail` ×2) | `manifolds.rs:607-830`, `advect_tail` `manifolds.rs:325-370` |
| **9,0** | 1772 | `GasState::step` du cylindre | `thermo.rs:224-275`, appelé `cycle.rs:~512` |
| **7,7** | 1526 | `IntakeAcoustic::next` (12 runners + airbox) | `intake_acoustic.rs:159-235` |
| 5,8 | 1142 | Construction de `CycleOutput` (`specific_enthalpy` ×5, divisions) | `cycle.rs:~530-560` |
| 3,6 | 708 | Validation d'entrée `CycleCylinder::step` | `cycle.rs:304-330` |
| 3,6 | 703 | `Slap::step` (`sin_cos`, `sqrt`, division par cylindre) | `mechanical.rs:67-105`, appelé `engine.rs` boucle cylindres |
| 2,9 | 574 | Construction de `CycleInput` (20 champs × 12) | `engine.rs` `self.cylinders[i].step(CycleInput{..})` |
| 2,5 | 487 | Hohenberg (1/32 pas) + chaleur paroi | `cycle.rs:~497-509`, `cycle.rs:560-580` |
| 2,2 | 433 | Wiebe / brûlage | `cycle.rs:~477-495`, `thermo.rs:317-345` |
| 1,6 | 316 | `AfterTreatment::step_resident` ×2 (clone + `exp`) | `manifolds.rs:63-135` (`exp` `manifolds.rs:114`) |
| 0,7 | 133 | Étincelle / variation cycle-à-cycle | `cycle.rs:~380-455` |
| 0,7 | 144 | `Radiation::next` + préparation `Air` | `radiation.rs:268` |
| 0,6 | 121 | Passe-haut combustion, `Slap::next`, mécanique, tone, fondu | `engine.rs` fin d'`advance`, `tone.rs`, `radiation.rs:77` |
| 0,8 | 160 | Contrôleur, induction (inactive V12 atmo), crank (inactif : régime imposé) | `controller.rs:77`, `engine.rs` |

Constats :

1. **Pas d'allocation dans le chemin chaud.** `vec!`/`collect`/`Box`/`push` n'apparaissent que dans `new()`
   (`engine.rs:175`, `acoustic.rs:87`, `intake_acoustic.rs:128`). Les `clone()` du pas sont des copies de structures `Copy`-like
   (`Manifolds::clone` ≈ 1 Ko à `manifolds.rs:651` + recopie `*self = working` ; `AfterTreatment` `manifolds.rs:76`) : coût mémoire réel mais < 1 %.
2. **Les fonctions thermo/γ(T) sont bon marché** : `specific_heat_cv`, `gamma`, `specific_enthalpy`, `temperature_from_energy` sont des polynômes/1 `sqrt`
   (`thermo.rs:26-52`). Le vrai coût transcendantal est dans **`mass_flow_from_states`** (`gas.rs:110-141`) : `powf` (`critical`, et le facteur bloqué),
   ou `ln + exp + exp_m1 + sqrt` en sous-critique, ×2 soupapes ouvertes par cylindre + 5 appels `rate()` par pas dans `manifolds.rs` (`manifolds.rs:298`).
   `HarmonicCam::lift_m` (`gas.rs:230`) fait 3 `rem_euclid` (appel `fmod`) + `cos` + `powf` par soupape même fermée (le early-return `lift == 0` de `port_flow` arrive **après** le calcul du lift).
3. **`powf(0.15)/exp` de la combustion** (`cycle.rs:420-422`) n'est exécuté qu'à chaque étincelle : négligeable (C_SPARK 0,7 %).
4. **Décimateurs** (`automation_voice.rs:80`, `PolyphaseDecimator` de bdsp, 95 taps × 3 flux, 1 sortie par 2 pas physiques) : non couverts par l'exemple `physical_engine`,
   **non mesurés**. Estimation par le code : ≈ 285 MAC en chaîne d'accumulation dépendante (`bdsp filters.rs:1103-1108`) ≈ 1,1 k cycles par sortie 48 kHz, soit ~2–3 % du coût par pas physique. À mesurer avec `automation_physical.rs`.
5. **Lignes à retard** (`bdsp delay.rs:173-203`) : `read_at` fait 2 `%` entiers par la longueur non constante et `write` 1. Hypothèse « les divisions entières coûtent cher »
   **testée et réfutée** (voir §3, expérience A) : gain nul.
6. **SVF** (`radiation.rs`, `tone.rs`, `engine.rs` passe-haut, ExhaustNetwork) : `Radiation`+tone+mécanique+passe-haut = 1,3 % ; les SVF des segments d'`ExhaustNetwork`
   sont comptés dans les 19,4 % de l'échappement. Pas de point chaud isolé.
7. Le lot récent (+7 % annoncé) se retrouve dans : `IntakeAcoustic` 7,7 %, `Slap::step` 3,6 %, `KNOCK` bloc inactif ≈ 0 (`if knock` non pris). Slap/knock/runners d'admission = ~11 % à eux trois.

## 3. Expériences de faisabilité (dev17, 12 exécutions alternées, ns/pas, min / médiane)

| Exp. | Changement (copie scratch) | Résultat | Sortie |
|---|---|---|---|
| base | `main` | 6122 / 6191 | — |
| A | `DelayLine` locale sans `%` entier (`srcfd/src/fastdelay.rs`, 3 fichiers) | 6085 / 6253 | **bit-exact** (WAV/CSV identiques), gain **nul** |
| B | `lift_m`: `rem_euclid` → `x - floor(x/P)*P` (`srcfm`) | 5990 / 6118 (−2 %) | **non bit-exact** (WAV différent) |
| C | cache de `loss()`/`powf` dans `IntakeAcoustic::next` (`srcpw`) | 6165 / 6258 | bit-exact, gain **nul** |
| D | borne haute : `mass_flow_from_states` sans transcendantales (`srcor`, sons faux) | 5802 / 6051 vs base 6260 / 6573 sur le même run (**−7 %**) | non valide, sert de plafond pour les soupapes cylindre |
| E | `CODEGEN_UNITS=1` + `LTO=fat` (aucun changement de code) | **5790 / 5909 vs 6122 / 6203 (−5 %)** | **bit-exact** (WAV/CSV identiques sur 2 s) |

Conclusion : le coût est **diffus** ; aucun micro-réglage isolé n'apporte plus de 2 %. Ce qui bouge : les flags de compilation (E) et la réduction du
travail transcendantal des orifices (D, plafond ~7 % sur les seules soupapes cylindre).

## 4. Plan d'optimisation classé

Objectif : 5,846 → < 5,0 µs sur portable = −14,5 %. Les gains sont des estimations dev17 ; à re-mesurer sur i7-8565U. Les gains ne s'additionnent pas exactement.
Fichiers actuellement modifiés par d'autres agents (couple, admission, cliquetis, couplage d'ondes) : `engine.rs`, `cycle.rs`, `intake_acoustic.rs`, `mechanical.rs`, `acoustic.rs`, `manifolds.rs` ; donc tout ce qui suit touche des zones en mouvement sauf le point 1.

| # | Action | Gain attendu | Risque bit-exact / son | Fichiers |
|---|---|---|---|---|
| 1 | **`[profile.release] codegen-units = 1`, `lto = "fat"`** (essayé : ThinLTO seul n'avait rien donné, X-014 ; ici mesuré −5 %) | **−5 %** mesuré (min et médiane) | **bit-exact** vérifié (WAV/CSV identiques) ; coût : compilation plus lente (~1 min sur dev17), à valider sur le portable et pour le CI | `Cargo.toml` uniquement. Aucun conflit avec les merges en cours : **faisable maintenant** |
| 2 | **Factoriser dans `CycleCylinder::step`** : valider `CycleInput` une fois par pas moteur au lieu de 12×, calculer `specific_enthalpy(intake.T)` et `specific_enthalpy(exhaust.T)` une fois par sous-pas (5 appels/cylindre aujourd'hui, mêmes arguments pour les 12), passer `CycleInput` par référence | −3 à −5 % (sections 3,6 + 5,8 + 2,9 %) | **bit-exact** si les mêmes expressions sont conservées ; contrat de validation à garder pour les autres appelants de `step` (tests, `physical_cylinder`) : garde-fou dans la fonction partagée, ne pas le retirer sans mettre le check au point d'entrée | `cycle.rs`, `engine.rs` (+ `examples/physical_cylinder.rs`) |
| 3 | **Formule d'orifice moins chère** : remplacer les `powf` du facteur (`critical`, facteur bloqué) par une fonction de γ tabulée/polynomiale (γ = f(T) linéaire par morceaux, `thermo.rs:40`), fusionner `exp`/`exp_m1` ; appliquer aussi aux 5 `rate()` de `manifolds.rs` | −4 à −7 % (plafond D : −7 % cylindres seuls ; ajouter la part `manifolds` 15 %) | **non bit-exact** (erreur relative ~1e-9 possible, choisir la précision au niveau de l'arrondi f32 de sortie) ; risque de dérive des tests de conservation d'énergie (`numerical_correction_j`) : exiger `absolute_correction` inchangé au dernier chiffre affiché | `gas.rs`, `cycle.rs`, `manifolds.rs`, tests `gas.rs` |
| 4 | **Soupape fermée = sortie précoce avant `lift_m`** : garder par cylindre les angles d'ouverture/fermeture (constantes de came) et ne calculer `lift_m` que dans la fenêtre ; hors fenêtre, retourner 0 sans `fmod`/`cos`/`powf` | −2 à −4 % (B mesure −2 % avec un `wrap` non exact ; une garde exacte l'améliore) | **bit-exact possible** si la garde est conservatrice (fenêtre ⊇ zone de lift non nul) ; casse la VVT (`intake_phase`) si la fenêtre n'est pas décalée de `intake_phase_rad` | `cycle.rs`, `gas.rs` |
| 5 | **`Manifolds::step`** : supprimer le `clone()` complet (~1 Ko) + recopie en travaillant sur les seuls états modifiés ; calculer `outgoing_budgets()` une fois (appelé dans `engine.rs` **et** `manifolds.rs:643`) ; `advect_tail` : sortir le calcul de composition (division) de la boucle des faces | −2 à −4 % | bit-exact si l'ordre des opérations est conservé | `manifolds.rs`, `engine.rs` |
| 6 | **`Acoustic::next` / `ExhaustNetwork`** : tableaux plats au lieu de `Vec<Vec<>>`, réciproques `1/(c±v)` et `1/(ρc)` précalculés hors boucle par primaire, une seule lecture SVF par mode | −1 à −3 % | bit-exact seulement si les divisions restent des divisions ; sinon erreur 1 ulp f32 (inaudible) | `acoustic.rs`, `acoustics.rs` |
| 7 | **Échappement/admission à 48 kHz sur moteur 96 kHz** (réseaux d'ondes seuls, la thermo reste à 96 kHz) | −8 à −13 % (les deux réseaux = 27 % du coût) | **change le son** : retards quantifiés, coupures SVF, phase ; validation A/B à l'écoute et sur les ordres (`v12-orders.csv`). Décision produit, pas une optimisation transparente | `acoustic.rs`, `intake_acoustic.rs`, `acoustics.rs`, `engine.rs` |
| 8 | Décimateurs FIR : 4 accumulateurs indépendants par branche (`bdsp filters.rs:1103`) | −1 à −2 % (estimé, non mesuré) | non bit-exact (réassociation), ~1e-7 | dépôt `bdsp` (dépendance privée pinée) ; hors BESS |
| 9 | `Slap::step` : ne pas exécuter si `block` désactivé, réutiliser `sin_cos` déjà calculé | −0,5 à −1 % | bit-exact | `mechanical.rs`, `engine.rs` |

Ordre recommandé : **1** (immédiat, indépendant des merges) → **2, 5, 4** (bit-exact, après les merges de `cycle.rs`/`manifolds.rs`) → **3** (après validation de tolérance)
→ 6 et 9 en opportunité. 1 + 2 + 5 + 4 visent ≈ −13 à −18 % en restant **bit-exact**, soit à peu près la cible < 5 µs sans toucher au son ; le point 7 n'est nécessaire que si cela ne suffit pas.

## 5. Ce qui n'est PAS établi

- Aucun échantillonnage `perf` : les pourcentages viennent de laps `rdtsc`, pas de lignes de source. La répartition « soupapes 23 % » regroupe validation de lift, `lift_m` et `mass_flow_from_states` ; la part respective n'est pas isolée (borne haute D = 7 % du total pour le seul calcul transcendantal des soupapes cylindre).
- Le coût des décimateurs, du chemin vilebrequin libre (`imposed_rpm = None`), de la suralimentation (`induction`) et d'autres régimes/charges n'a pas été mesuré (V12 atmo à 1200 tr/min imposé uniquement).
- Aucun chiffre sur le portable i7-8565U ; le pas de 6,1–6,4 µs de dev17 vs 5,846 µs annoncés n'est pas un écart de code mais de machine/charge.
- L'expérience D change la physique (débits faux) : c'est un plafond de coût, pas une variante candidate.

## 6. Reproduction

Sur cette machine (dev17 via `tools/dev17.sh`) :

```
S=/tmp/claude-1000/-home-bunchy-Desktop-bess/4d155310-9edc-4fd1-9782-368eba9e7c8c/scratchpad/x014
tools/dev17.sh x014p $S/srcprof cargo build --release --example physical_engine
tools/dev17.sh x014p $S/srcprof ~/bess-build/x014p/target/release/examples/physical_engine out/w 5 96000 v12   # PROF lignes sur stderr
$S/run_n.sh / run_or.sh / run_ab.sh / run_abc.sh   # comparaisons alternées (base vs variantes)
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 CARGO_PROFILE_RELEASE_LTO=fat cargo build --release --example physical_engine   # expérience E
```
