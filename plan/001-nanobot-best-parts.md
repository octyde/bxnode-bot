---
summary: "Best parts learned from HKUDS/nanobot (module boundaries, cron as first-class)"
title: "Nanobot Best Parts"
date: "2026-02-04"
source: "https://github.com/HKUDS/nanobot"
---

# Nanobot Best Parts

## Why This Matters

Nanobot keeps a small, readable codebase and highlights design choices that make it easy to understand, extend, and operate. Two standout areas are its clear module boundaries and its first-class cron/scheduling support.

## Evidence From Nanobot

Module boundaries are explicit in the README project structure and separate core concerns into dedicated packages such as agent, skills, channels, bus, cron, heartbeat, providers, session, config, and CLI. citeturn1view0

Cron is treated as a first-class feature with explicit CLI commands for add, list, and remove, documented alongside the main CLI reference. citeturn1view0

## Best Parts To Emulate

1. Clear module boundaries
2. Cron as a first-class primitive
3. Minimal, explicit agent loop
4. Config-first UX

## How To Apply These Ideas

1. Keep the top-level package structure stable and small.
2. Ensure every subsystem owns a narrow responsibility and avoids hidden coupling.
3. Treat scheduling as a core capability with a stable CLI surface.
4. Document module boundaries and cron usage in a single, discoverable place.

## Adoption Checklist (Concrete Steps)

1. Define and publish a module map
   - One-line responsibility for each subsystem.
   - Single entrypoint per subsystem.
2. Reduce cross-module imports
   - Replace deep internal imports with public interfaces.
   - Add lightweight adapters where needed.
3. Make cron a first-class CLI surface
   - Stable commands: `list`, `add`, `update`, `remove`, `run`, `runs`.
   - Store cron definitions in a single, documented location.
4. Codify config as the single source of truth
   - Prefer one config file or one directory with clear schema.
   - Ensure the CLI can validate and print effective config.
5. Keep the agent loop explicit
   - Single loop module with clear inputs/outputs.
   - Separate context assembly, tool execution, and memory updates.
6. Audit and prune dependencies
   - Remove unused libraries.
   - Prefer standard library where possible.
7. Tighten onboarding
   - One command to bootstrap.
   - One command to run.
   - Minimal required environment variables.

## Notes

This doc is derived from the public README and repo structure in HKUDS/nanobot. citeturn1view0
