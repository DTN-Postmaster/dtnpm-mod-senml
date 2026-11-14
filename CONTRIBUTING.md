# Guide de Contribution — `dtnpm-mod-senml`

Ce guide présente les règles de développement et les exigences de sécurité pour le module de télémétrie SenML de `dtnpm-mod-senml`.

---

## 1. Principes Fondamentaux

- **Zéro Panic / Zéro Unwrap** : Aucun appel `.unwrap()` ou `.expect()` en code de production.
- **Conformité RFC 8428 / RFC 8949** :
  - Toujours valider strictement les types de données SenML et refuser les valeurs non finies (`NaN`, `Infinity`).
  - Toujours borner la taille des packs entrants et insérer les séries temporelles de manière transactionnelle.
- **Traçabilité Architecturale (ADR)** :
  - Tout changement de format ou de persistance doit faire l'objet d'un fichier ADR sous `adr/` et être indexé dans `adr/README.md`.

---

## 2. Vérification Locale

```bash
cargo fmt --all -- --check
cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic
cargo test --offline --all-features
./scripts/check-security.sh --strict
```
