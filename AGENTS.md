# Agent Guide — `dtnpm-mod-senml`

## 🎯 Mission
Fournir le module applicatif complet (Fullstack WebModule) de télémétrie SenML (RFC 8428) pour **DTN Postmaster**, comprenant le décodeur CBOR / JSON, le service d'ingestion BPA (`SenmlService`), la persistance transactionnelle SQLite (`SenmlStorage`), l'API REST/SSE `/api/senml/v1/...` et le composant d'interface Preact `<SenmlView />`.

---

## 🛡️ Référentiels et Exigences de Sécurité

### 1. Cyber Resilience Act (CRA) & Conformité RFC 8428 / RFC 8949
- **Validation Rigoureuse des Packs SenML** :
  - **Version (`bver`)** : Si le champ `bver` est spécifié, il DOIT être égal à `10` (RFC 8428). Toute autre valeur doit être rejetée avec une erreur explicite.
  - **Sommes (`s` / `bs`)** : Les valeurs d'accumulation ne doivent pas être artificiellement écrasées à `0.0` si absentes.
  - **Nombres non finis** : Rejet strict des flottants `NaN`, `Infinity` et `-Infinity` en encodage CBOR et JSON afin de prévenir l'empoisonnement des calculs et bases de données.
- **Bornage des Ressources & DoS** :
  - Tout pack SenML est strictement borné : taille maximale de 2 Mio et 1 000 enregistrements maximum par payload.
- **Écritures Transactionnelles et Atomiques** :
  - Toutes les insertions d'enregistrements d'un même pack dans la base SQLite s'exécutent au sein d'une unique transaction (`conn.transaction()`) garantissant l'atomicité et évitant les incohérences partielles.
- **Supply-Chain & VEX** :
  - Utilisation de `yaml_serde` (sans libyaml).
  - VEX `not_affected` consigné pour `RUSTSEC-2026-0459`. Actions CI épinglées par SHA-1 à 40 caractères.
- **Politique de sécurité** : Consignée dans [SECURITY.md](SECURITY.md).

### 2. BPSec & Robustesse Frontend
- **Politique de Signature** : Rejet impératif des paquets non signés ou invalides lorsque `VerifyPolicy::Strict` est active.
- **Cycle de Vie des Timers UI (`SenmlView.tsx`)** :
  - Tous les handles de clignotement (`setTimeout`) sur réception d'événements SSE sont enregistrés dans un ensemble (`Set`) et systématiquement libérés (`clearTimeout`) lors du démontage du composant pour prévenir les fuites de mémoire dans le navigateur.

---

## 🦀 Standards d'Ingénierie Rust

1. **Règle Zéro Panic / Zéro Unwrap** :
   - Tout le code sous `src/` doit être 100% exempt de `.unwrap()`, `.expect()` et de macro `panic!`.
   - Linter strict : `-D warnings -D clippy::unwrap_used -D clippy::panic`.
2. **ADRs Obligatoires** :
   - Toute décision architecturale SenML doit être consignée sous `adr/` et indexée dans [adr/README.md](adr/README.md).

---

## ⚡ Commandes Critiques de Vérification

- `cargo fmt --all -- --check`
- `cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic`
- `cargo test --offline --all-features`
- `./scripts/check-security.sh --strict`
- `./scripts/check-coverage.sh --summary-only`
