# ADR 0002: Transactional SQLite Ingest, Pure-Rust `yaml_serde`, and Token File Auth

- **Status**: Accepted
- **Scope**: `dtnpm-mod-senml` (`Cargo.toml`, `src/storage.rs`, `src/main.rs`)

## Context
- Batch ingestion of telemetry records needs atomic consistency across `metric_dictionary` and `telemetry_history` tables.
- Direct dependency `serde_yaml = "0.9"` pulled in unmaintained C bindings (`unsafe-libyaml`, `RUSTSEC-2024-0449`), and `persist_to_yaml_if_needed` only handled YAML configs rather than `.toml` station configs.
- Standalone binary authentication lacked `--token-file` support.

## Decision
1. Keep batch record insertion inside a single SQLite transaction (`conn.transaction()` -> `tx.commit()`) in `SenmlStorage::insert_records`.
2. Replace `serde_yaml = "0.9"` with pure-Rust `yaml_serde = "0.10"` (matching `dtnpm-core`), add `toml = "0.8"` for atomic `.toml` config persistence alongside YAML, and remove `RUSTSEC-2024-0449` from `.cargo/audit.toml` and `deny.toml`.
3. Add `--token-file <PATH>` (`DTN_HDY_TOKEN_FILE`) alongside `--token` in `src/main.rs`.

## Consequences
- `unsafe-libyaml` and `RUSTSEC-2024-0449` are eliminated from the dependency tree, `.toml` and `.yaml` operator config updates persist atomically, and batch telemetry writes remain transactional.
