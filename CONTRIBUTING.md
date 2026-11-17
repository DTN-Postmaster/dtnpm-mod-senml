# Contributing Guide — `dtnpm-mod-senml`

This guide outlines the development rules and security requirements for the `dtnpm-mod-senml` SenML telemetry module.

---

## 1. Core Principles

- **Zero Panic / Zero Unwrap**: No `.unwrap()` or `.expect()` calls in production code.
- **RFC 8428 / RFC 8949 Compliance**:
  - Always strictly validate SenML data types and reject non-finite values (`NaN`, `Infinity`).
  - Always bound the size of inbound packs and insert time-series data transactionally.
- **Architectural Traceability (ADR)**:
  - Any change to the format or persistence layer must be documented in an ADR file under `adr/` and indexed in `adr/README.md`.

---

## 2. Local Verification

```bash
cargo fmt --all -- --check
cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic
cargo test --offline --all-features
./scripts/check-security.sh --strict
```
