# Nanobot Adoption Plan For BXNode Bot

Date: 2026-02-04

## Goal

Adopt the best parts of HKUDS/nanobot by tightening module boundaries and promoting cron scheduling to a first-class feature, while keeping the current Rust architecture intact.

## Current Module Map (From `src/lib.rs`)

- `agent`: agent loop, context, tool execution (`src/agent/*`)
- `channels`: channel adapters and registry (`src/channels/*`)
- `cli`: CLI entrypoints and config commands (`src/cli/*`)
- `config`: config loading and schema (`src/config/*`)
- `gateway`: HTTP and WS server (`src/gateway/*`)
- `plugins`: plugin registry (`src/plugins/*`)
- `providers`: LLM clients and registry (`src/providers/*`)
- `session`: session handling (`src/session/*`)

## Plan (Sequenced And Concrete)

### Phase 1: Publish Clear Module Boundaries

1. Create a module map doc that declares ownership and public entrypoints for each subsystem.
   Proposed file: `docs/module-map.md`.
2. Ensure each module has a single public interface surface.
   Example: re-export only from `src/agent/mod.rs`, `src/channels/mod.rs`, `src/providers/mod.rs`.
3. Audit cross-module imports and replace deep imports with module-level APIs.
   Target files: `src/agent/*`, `src/gateway/*`, `src/providers/*`, `src/channels/*`.
4. Add a lightweight “module boundary” test that scans for disallowed imports.
   Proposed location: `tests/module_boundaries.rs`.

### Phase 2: Cron As A First-Class Primitive

1. Add a dedicated cron module with explicit scheduler types.
   Proposed files: `src/cron/mod.rs`, `src/cron/scheduler.rs`, `src/cron/store.rs`.
2. Extend config with a `cron` section and defaults.
   Update: `src/config/mod.rs`, `config.example.yaml`, `src/config/tests.rs`.
3. Add CLI commands for cron management.
   Update: `src/cli/mod.rs` with `cron` subcommands (`list`, `add`, `update`, `remove`, `run`, `runs`).
4. Add gateway endpoints for cron operations if remote control is required.
   Update: `src/gateway/http.rs`, `src/gateway/ws.rs`, `src/gateway/protocol.rs`.
5. Add tests for cron scheduling and persistence.
   Proposed files: `src/cron/tests.rs`, `tests/cron_e2e.rs`.

### Phase 3: Config-First UX

1. Ensure a single config file controls all subsystems (including cron).
   Update: `config.example.yaml` and `src/config/mod.rs`.
2. Add a CLI “show effective config” output.
   Update: `src/cli/config.rs` to print resolved values.
3. Document config schema and defaults in one place.
   Proposed file: `docs/configuration.md` (or update existing).

### Phase 4: Make The Agent Loop Explicit

1. Document the agent loop in `src/agent` as a single flow: context → tools → execution.
   Update: `src/agent/mod.rs` and `docs/module-map.md`.
2. Ensure tools and context are explicit boundaries.
   Target files: `src/agent/context.rs`, `src/agent/tools.rs`, `src/agent/execution.rs`.
3. Add a focused test for the loop ordering and tool execution lifecycle.
   Update: `src/agent/tools_tests.rs` or new `src/agent/execution_tests.rs`.

## Definition Of Done

1. Module map doc exists and is referenced from `docs/`.
2. No deep cross-module imports outside public interfaces.
3. Cron is configurable, manageable by CLI, and covered by tests.
4. Config is the single source of truth and easy to inspect.
5. Agent loop is documented and has a basic lifecycle test.

## Risks And Mitigations

1. Risk: Refactors may cause subtle behavior changes.
   Mitigation: Add module boundary tests and keep changes scoped by phase.
2. Risk: Cron storage design becomes a migration burden.
   Mitigation: Use a simple JSON or YAML store with versioned schema.
3. Risk: Gateway surface grows too quickly.
   Mitigation: Implement cron CLI first, gateway endpoints second if needed.

## Suggested Execution Order

1. Phase 1 (Module boundaries)
2. Phase 2 (Cron core + CLI)
3. Phase 3 (Config UX)
4. Phase 4 (Agent loop clarity)
