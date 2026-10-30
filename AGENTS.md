# wayfind development rules

- Keep this kernel place independent. No real location or organization names in
  code, fixtures, tests or documentation. Graph data is supplied by callers.
- Routing, snapping, geometry, maneuvers, instructions and navigation semantics
  live only in `core`. WASM, UniFFI and TypeScript are thin protocol adapters.
- Accept only `wayfind-graph/1`; ignore unknown top-level fields. Validate inputs
  and return errors across bindings. Never panic or unwrap on user input paths.
- Use PowerShell 7 on Windows. Keep generated WASM, Android libraries, Kotlin,
  JS output and Cargo build artifacts out of Git. Commit lockfiles.
- Keep synthetic unit tests beside the code they test. Optional external graph
  checks use `WAYFIND_GRAPH_FILE`; never commit external graph data or identifiers.
- Run Rust workspace tests, release integration checks, WASM/JS tests and the
  Android build before shipping changes that affect their shared protocol.
- All source is AGPL-3.0-only. Do not put routing policy in a platform adapter.
