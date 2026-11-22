# Agent Guide — `dtnpm-mod-senml`

## 🎯 Mission
Provide the complete fullstack SenML telemetry (RFC 8428) application module (`WebModule`) for **DTN Postmaster**, including the CBOR / JSON decoder, the BPA ingestion service (`SenmlService`), transactional SQLite persistence (`SenmlStorage`), the `/api/senml/v1/...` REST/SSE API, and the Preact UI component `<SenmlView />`.

---

## 📝 Documentation & User Interface Language

- **English by Default (Docs & UI)**:
  - **Documentation**: English must be used for all documentation, unless the file is suffixed with `.fr.md` or another language is explicitly requested in the prompt.
  - **User Interface (100% English)**: Every user-facing string in `ui/src/` and `ui/tests/` (headings, telemetry labels, chart legends, buttons, `SecurityBadge` text, `title` tooltips, `aria-label`s, dialogs, and UI error messages) MUST be in English. Keep shared components synchronized with `dtnpm-web-core/ui`.
  - **Navigation Scope**: `SenML` is an operational top-level `Navbar` tab in `dtnpm-web-core` (`NavTabId: 'senml'`); station-level configuration, routing, security, and filtering panels belong under `Station` sub-tabs (`ConfigView.tsx`), never added as new global navbar tabs.

---

## 🛡️ Security Frameworks and Requirements

### 1. Cyber Resilience Act (CRA) & RFC 8428 / RFC 8949 Compliance
- **Strict Validation of SenML Packs**:
  - **Version (`bver`)**: If the `bver` field is specified, it MUST equal `10` (RFC 8428). Any other value must be rejected with an explicit error.
  - **Sums (`s` / `bs`)**: Accumulator values must not be artificially overwritten to `0.0` when absent.
  - **Non-finite Numbers**: Strict rejection of `NaN`, `Infinity`, and `-Infinity` floats in CBOR and JSON encoding to prevent poisoning calculations and databases.
- **Resource Bounding & DoS**:
  - Every SenML pack is strictly bounded: maximum size of 2 MiB and maximum of 1,000 records per payload.
- **Transactional and Atomic Writes**:
  - All record insertions for a single pack in the SQLite database execute within a single transaction (`conn.transaction()`), guaranteeing atomicity and preventing partial inconsistencies.
- **Supply-Chain & VEX**:
  - Uses `yaml_serde` (without libyaml).
  - `not_affected` VEX recorded for `RUSTSEC-2026-0459`. CI actions pinned by full 40-character SHA-1.
- **Security Policy**: Documented in [SECURITY.md](SECURITY.md).

### 2. BPSec & Frontend Robustness
- **Signature Policy**: Mandatory rejection of unsigned or invalid packets when `VerifyPolicy::Strict` is active.
- **UI Timer Lifecycle (`SenmlView.tsx`)**:
  - All blink handles (`setTimeout`) triggered upon receiving SSE events are tracked in a `Set` and systematically cleared (`clearTimeout`) on component unmount to prevent browser memory leaks.

---

## 🦀 Rust Engineering Standards

1. **Zero Panic / Zero Unwrap Rule**:
   - All code under `src/` must be 100% free of `.unwrap()`, `.expect()`, and the `panic!` macro.
   - Strict linter: `-D warnings -D clippy::unwrap_used -D clippy::panic`.
2. **Mandatory ADRs**:
   - Any SenML architectural decision must be recorded under `adr/` and indexed in [adr/README.md](adr/README.md).

---

## ⚡ Critical Verification Commands

- `cargo fmt --all -- --check`
- `cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic`
- `cargo test --offline --all-features`
- `cd ui && pnpm test:e2e` (and `cd ../dtnpm-web-core/ui && pnpm build && pnpm test:e2e` when updating shared UI views)
- `./scripts/check-security.sh --strict`
- `./scripts/check-coverage.sh --summary-only`
