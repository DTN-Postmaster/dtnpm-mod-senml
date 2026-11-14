# dtnpm-mod-senml

Fullstack **DTN SenML Telemetry** (RFC 8428) plugin for **DTN Postmaster** (`dtnpm`), implementing `dtnpm_web_core::WebModule`.

## Features

- **SenML Parser (`parser`)**: Decodes RFC 8428 CBOR and JSON SenML packs and extracts measurements.
- **BPA Service (`SenmlService`)**: Registers `dtn://<node>/senml`, verifies inbound bundles via directional BPsec policies, and stores telemetry packs.
- **SQLite Storage (`SenmlStorage`)**: Provides `migrations/0001_senml_schema.sql` over the shared SQLite connection.
- **HTTP API (`/api/senml/v1/...`)**: Query historical telemetry, inspect metadata/sources, publish SenML bundles, stream live updates over SSE, and serve Open MCT telemetry views.
- **Standalone Mode**: Includes `dtnpm-mod-senml` daemon (`cli` feature) and `examples/test_server.rs`.
- **WebUI Component (`ui/`)**: Preact `<SenmlView />` component and Playwright E2E spec.

## Build and Test

Requires sibling checkouts of `../dtnpm-core`, `../dtnpm-web-core`, and `../../hardy/*`.

```bash
cargo test --all-targets
```

Run the standalone mock server example:

```bash
cargo run --example test_server
```

## License

Dual-licensed under the Apache License, Version 2.0 or the MIT License (crate metadata: `Apache-2.0`). See [LICENSE.md](LICENSE.md).
