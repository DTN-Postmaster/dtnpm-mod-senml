# Security Policy — `dtnpm-mod-senml`

## Supported Versions

| Version | Supported | Security Updates |
|---------|-----------|------------------|
| 0.2.x   | Yes       | Active           |
| < 0.2   | No        | End of Life      |

## Security Architecture & EU Cyber Resilience Act (CRA) Alignment

`dtnpm-mod-senml` implements the DTN BPv7 SenML Telemetry service (`dtn://<station>/senml`), RFC 8428 JSON/CBOR parser, SQLite time-series storage, SSE/Open MCT API routes, and Preact UI for `dtn-postmaster`:

- **Memory Safety & Panic-Free Production Targets**: Built in Rust (`edition = "2024"`) and gated in CI with `-D warnings -D clippy::unwrap_used -D clippy::panic`.
- **BPSec Enforcement (`src/service.rs`)**: Inbound telemetry bundles are verified against `DirectionalKeyStore`. Under `VerifyPolicy::Strict`, only bundles with `AuthStatus::Valid` are accepted (`Unsigned`, `MissingKey`, and `Invalid` are dropped with a warning). Explicit opt-in policies (`VerifyPolicy::Warn`, `VerifyPolicy::Ignore`) remain available for unauthenticated telemetry testing.
- **RFC 8428 Input Validation & Resource Bounds (`src/service.rs`, `src/parser.rs`)**:
  - `on_deliver` drops bundles exceeding `MAX_SENML_BUNDLE_BYTES` (1 MiB payload + 64 KiB envelope margin) before calling `buffer_stream`.
  - `parse_senml_payload` enforces `MAX_SENML_PAYLOAD_BYTES` (1 MiB) and `MAX_SENML_RECORDS` (4,096 records).
  - Enforces `bver == 10` for both JSON and CBOR (`-1`), rejects non-finite CBOR/JSON floats (`NaN`, `+Inf`, `-Inf`), supports SenML Sum (`s` / `bs`), and rejects records lacking a value field (`v`, `vs`, `vb`, `vd`, `s`).
- **Transactional Persistence (`src/storage.rs`)**: Telemetry batch ingestion wraps dictionary updates and time-series inserts inside a single SQLite transaction (`conn.transaction()`), ensuring atomic all-or-nothing commits.
- **Standalone Secret Handling (`src/main.rs`)**: Supports `--token-file` (`DTN_HDY_TOKEN_FILE`) alongside `--token` (`DTN_HDY_TOKEN`) and Unix Domain Sockets (`--socket`).
- **Supply Chain & VEX**: Replaced `serde_yaml` (`unsafe-libyaml`, `RUSTSEC-2024-0449`) with pure-Rust `yaml_serde = "0.10"` (and `toml = "0.8"` for `.toml` config persistence). `RUSTSEC-2026-0459` (`libsqlite3-sys` SQLite `session` extension) is documented in `.cargo/audit.toml` and `deny.toml` as VEX Not Affected because `dtnpm-mod-senml` and `dtnpm-core` do not enable the SQLite `session` extension.
- **SBOM & CI Hardening**: CI runs with `permissions: contents: read`, SHA-pinned GitHub Actions, and CycloneDX SBOM generation.

## Reporting a Vulnerability

Do **not** open a public GitHub issue for security vulnerabilities.
Report vulnerabilities privately via GitHub Security Advisories for `dtn-postmaster/dtnpm-mod-senml` or by contacting the maintainers directly. Please include:

- Component version and commit hash
- Configuration flags (`verify_policy`, standalone CLI options, etc.)
- Minimal proof-of-concept or reproduction steps

We aim to acknowledge reports within 72 hours and coordinate a fix before public disclosure.
