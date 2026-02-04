# Nanobot Adoption Tasks (Actionable)

Date: 2026-02-04

## Summary

This is an actionable, sequenced task list derived from `plan/002-nanobot-adoption-plan.md`, with owners, estimates, and concrete diff sketches.

## Owners (Suggested)

- Core: agent loop, module boundaries
- Gateway: HTTP/WS surface
- CLI: command additions
- Config: config schema and example updates
- QA: tests and validation

## Task List (With Estimates)

### 1) Publish a Module Map (Owner: Core, 0.5 day)

Deliverable: `docs/module-map.md`

Diff sketch:

```diff
+++ b/docs/module-map.md
+---
+summary: "Module boundaries and public entrypoints"
+title: "Module Map"
+---
+
+# Module Map
+
+## agent
+Owner: Agent loop and tool execution
+Public entrypoint: `bxnode_bot::agent`
+
+## channels
+Owner: channel adapters and registry
+Public entrypoint: `bxnode_bot::channels`
+
+## cli
+Owner: CLI surface and subcommands
+Public entrypoint: `bxnode_bot::cli`
+
+## config
+Owner: config schema and loading
+Public entrypoint: `bxnode_bot::config`
+
+## gateway
+Owner: HTTP and WS server
+Public entrypoint: `bxnode_bot::gateway`
+
+## plugins
+Owner: plugin discovery and lifecycle
+Public entrypoint: `bxnode_bot::plugins`
+
+## providers
+Owner: LLM client registry
+Public entrypoint: `bxnode_bot::providers`
+
+## session
+Owner: session storage and persistence
+Public entrypoint: `bxnode_bot::session`
+```

### 2) Enforce Module Boundary Imports (Owner: Core, 1 day)

Deliverable: `tests/module_boundaries.rs`

Diff sketch:

```diff
+++ b/tests/module_boundaries.rs
+#[test]
+fn no_deep_imports_outside_modules() {
+    // Scan for `crate::module::...` imports that bypass `mod.rs` public APIs.
+    // This is a simple regex-based guardrail.
+    // NOTE: Implementation intentionally small and fast.
+}
+```

### 3) Add Cron Module (Owner: Core, 1.5 days)

Deliverables:
- `src/cron/mod.rs`
- `src/cron/scheduler.rs`
- `src/cron/store.rs`

Diff sketch:

```diff
+++ b/src/cron/mod.rs
+pub mod scheduler;
+pub mod store;
+
+pub use scheduler::{CronScheduler, CronJob};
+pub use store::{CronStore, CronStoreEntry};
+```

```diff
+++ b/src/cron/scheduler.rs
+pub struct CronScheduler { /* ... */ }
+
+impl CronScheduler {
+    pub fn new() -> Self { /* ... */ }
+    pub fn schedule(&mut self, job: CronJob) -> anyhow::Result<()> { /* ... */ }
+    pub fn run_now(&self, job_id: &str) -> anyhow::Result<()> { /* ... */ }
+}
+
+pub struct CronJob { /* id, schedule, payload */ }
+```

```diff
+++ b/src/cron/store.rs
+pub struct CronStore { /* path + jobs */ }
+
+impl CronStore {
+    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> { /* ... */ }
+    pub fn save(&self) -> anyhow::Result<()> { /* ... */ }
+}
+
+pub struct CronStoreEntry { /* id, schedule, payload, last_run */ }
+```

### 4) Add Config For Cron (Owner: Config, 1 day)

Deliverables:
- Update `src/config/mod.rs`
- Update `config.example.yaml`

Diff sketch:

```diff
--- a/src/config/mod.rs
+++ b/src/config/mod.rs
@@
 pub struct Config {
@@
     #[serde(default)]
     pub plugins: PluginsConfig,
+
+    #[serde(default)]
+    pub cron: CronConfig,
 }
+
+#[derive(Debug, Clone, Serialize, Deserialize, Default)]
+pub struct CronConfig {
+    pub enabled: bool,
+
+    #[serde(default = "default_cron_store")]
+    pub store_path: String,
+}
+
+fn default_cron_store() -> String {
+    "./data/cron.json".to_string()
+}
```

```diff
--- a/config.example.yaml
+++ b/config.example.yaml
@@
+cron:
+  enabled: true
+  store_path: ./data/cron.json
```

### 5) Add Cron CLI (Owner: CLI, 1.5 days)

Deliverable: extend `src/cli/mod.rs`

Diff sketch:

```diff
--- a/src/cli/mod.rs
+++ b/src/cli/mod.rs
@@
 pub enum Commands {
     /// Start the gateway server
     Serve(ServeArgs),
@@
     /// Print version information
     Version,
+
+    /// Manage cron jobs
+    Cron(CronArgs),
 }
+
+#[derive(Parser, Debug)]
+pub struct CronArgs {
+    #[command(subcommand)]
+    pub action: CronAction,
+}
+
+#[derive(Subcommand, Debug)]
+pub enum CronAction {
+    List,
+    Add { id: String, schedule: String, payload: String },
+    Update { id: String, schedule: Option<String>, payload: Option<String> },
+    Remove { id: String },
+    Run { id: String },
+    Runs { id: Option<String> },
+}
```

### 6) Wire Cron Into Gateway (Optional, Owner: Gateway, 2 days)

If remote control is required, add endpoints in `src/gateway/http.rs` and `src/gateway/ws.rs`. Start with read-only endpoints for safety.

Diff sketch:

```diff
--- a/src/gateway/http.rs
+++ b/src/gateway/http.rs
@@
+// GET /cron -> list jobs
+// POST /cron -> add job
+// POST /cron/{id}/run -> run job
```

### 7) Tests (Owner: QA, 1.5 days)

Deliverables:
- `src/cron/tests.rs`
- `tests/cron_e2e.rs`

Diff sketch:

```diff
+++ b/src/cron/tests.rs
+#[test]
+fn cron_store_roundtrip() { /* load -> save -> load */ }
+
+#[tokio::test]
+async fn cron_scheduler_runs() { /* schedule -> run -> assert */ }
+```

```diff
+++ b/tests/cron_e2e.rs
+#[tokio::test]
+async fn cli_add_list_remove() { /* exec CLI and verify store */ }
+```

## Suggested Sequence

1. Task 1 (module map)
2. Task 2 (boundary test)
3. Task 3 (cron module)
4. Task 4 (config)
5. Task 5 (CLI)
6. Task 7 (tests)
7. Task 6 (gateway, optional)

## Estimated Total

- Core changes: 3 days
- CLI + Config: 2.5 days
- Tests: 1.5 days
- Gateway (optional): 2 days

**Total:** ~7–9 days depending on gateway scope.
