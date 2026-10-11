# BESS 0.12.0 — publication et exécutable bureau

La demande utilisateur du 2026-10-11 porte sur le commit et le push de BESS 0.12, puis la mise à jour de `D:\Desktop\BESS.exe`. La version Cargo et le verrou de dépendances passent à **0.12.0**; la révision BDSP reste fixée à `b2981d431d321d29224752a2f855495da965d4bb`. La cible de publication est `origin/main`, avec le tag annoté `v0.12.0`.

## Périmètre livré

Les deux lots P1–P7 et leurs exemples/tests/reports sont inclus, ainsi que les outils et comptes rendus de flotte déjà présents dans le checkout. Les sorties de calcul `output/`, `artifacts/`, `out/` et `work/` restent locales. Les [notes de version](../../RELEASE_NOTES_0.12.0.md) décrivent les fonctionnalités, la compatibilité et les limites. Les états W-010–W-016 restent `building`; une publication ne clôt pas les critères physiques ou d'écoute.

## Preuves de validation

Avant le changement de version, le bilan effectivement relu dans `output/simulation-second-20261011/release-tests.log` est **483 réussites, zéro échec, six diagnostics ignorés sur 62 suites**. Le manifeste des 128 fichiers de cette validation est comparé au checkout: seules les deux métadonnées de version `Cargo.toml` et `Cargo.lock` diffèrent. Aucun fichier Rust testé n'a changé. La suite complète n'est donc pas rejouée uniquement pour le numéro de version.

Après le passage en 0.12.0, `cargo fmt --check`, `git diff --check` et `cargo clippy --locked --all-targets -- -D warnings` passent. La compilation Windows utilise `cargo build --release --locked --bin bess` et la cible AVX2/FMA déjà choisie (`x86-64-v3`). Les logs et le manifeste courant sont conservés dans `output/release-0.12.0/`.

Le test strict de résonance X-028 reste explicitement en échec; X-035 (chaleur numérique froide), X-036 (convergence du redémarrage) et X-037 (pression du contact 1D non isotherme) restent ouverts. Calibration réelle, écoute, endurance audio et réception BeamNG ne sont pas certifiées par cette publication. [Résultats détaillés du second lot](BESS-SIMULATION-SECOND-BATCH-2026-10-11.md).

## Reconstruction et livraison bureau

La compilation release 0.12.0 termine avec code zéro en 3 min 21 s. Un lancement explicitement attendu du binaire reconstruit (`--render`) termine avec code zéro et reproduit le WAV par défaut précédent, SHA-256 **50EF7089BEFAA804DA8DC40389FCF685FD5C4757ECC7E68357AEC19E4EC95F51**.

`target/release/bess.exe` et la copie livrée `D:\Desktop\BESS.exe` ont le même SHA-256 : **003C8CA32BE817038C3D59514C37FAAB0AAFBE0CB0111137FCF5239360978648**.

L'ancien exécutable bureau est conservé et vérifié dans `output/release-0.12.0/desktop-before.exe`, SHA-256 **38836CF5DD87D4F60DD2876A6837A03D8AAB3EBD1ED8DDA9415C90F8340AB9BF**. Le reçu `output/release-0.12.0/verification.json` conserve les codes de retour, hashes et preuves; la vérification des références distantes complète ce reçu après publication. Aucun mod n'est réinstallé dans cette opération.

## Publication des sources

Le commit **`2e54a4087ea262c3f6108d773b7f8e32aa0fd96a`**, `Release BESS 0.12.0 with optional simulation models and qualification benches`, publie les 56 fichiers de la livraison sur `origin/main`. `git ls-remote origin refs/heads/main` confirme exactement ce SHA après le push. Le compte rendu et l'index sont ensuite enregistrés dans un commit documentaire; le tag annoté de livraison **`v0.12.0`** identifie cet état final. Le reçu local conserve la vérification finale distincte de `main`, de l'objet tag et de son commit cible.
