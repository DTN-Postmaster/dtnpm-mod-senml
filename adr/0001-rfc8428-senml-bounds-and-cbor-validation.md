# ADR 0001: RFC 8428 Input Bounds, CBOR Validation, and Strict BPSec Policy

- **Status**: Accepted
- **Scope**: `dtnpm-mod-senml` (`src/parser.rs`, `src/service.rs`)

## Context
Audit of SenML bundle ingestion identified several parser and authentication gaps:
1. Incoming payloads lacked size and record-count caps, exposing the service to OOM/CPU exhaustion from huge streams or packs.
2. `bver` was not validated, non-finite CBOR floats (`NaN`, `Inf`) could enter telemetry storage/JSON serialization, SenML Sum (`s` / `bs`) was unhandled, and value-less records were coerced to `0.0` / `base_value`.
3. `SenmlService` under `VerifyPolicy::Strict` only dropped `AuthStatus::Invalid`, accepting `Unsigned` and `MissingKey` telemetry.

## Decision
1. Enforce a pre-stream bound in `SenmlService::on_deliver` (`1 MiB + 64 KiB` bundle cap), a `1 MiB` payload cap (`MAX_SENML_PAYLOAD_BYTES`), and a `4,096`-record cap (`MAX_SENML_RECORDS`) in `parse_senml_payload`.
2. Enforce `bver == 10` (RFC 8428) across JSON and CBOR (`-1`), reject all non-finite CBOR/JSON numeric values, support `bs` (`-6`) and `s` (`5`), and reject any record missing `v`, `vs`, `vb`, `vd`, and `s`.
3. Enforce `auth_status == AuthStatus::Valid` under `VerifyPolicy::Strict` while preserving opt-in `VerifyPolicy::Warn` and `VerifyPolicy::Ignore`.

## Consequences
- Malformed, non-finite, unsupported-version, or oversized telemetry bundles are rejected cleanly before database insertion.
- Strict BPSec stations reject unauthenticated/unknown-key sensor injections while retaining explicit opt-in modes for legacy/open telemetry nets.
