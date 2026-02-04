---
summary: "Assessment of Rust rewrite feasibility and Z.AI plan sufficiency"
read_when:
  - Considering a Rust rewrite of OpenClaw core services
  - Validating Z.AI plan requirements for OpenClaw model usage
title: "Rust Rewrite Assessment"
---

# Rust Rewrite Assessment

Date: 2026-02-04

## Scope And Current Architecture

- Core CLI and gateway services are TypeScript and run on Node. Entry flow starts in `openclaw.mjs` and `src/entry.ts`.
- Gateway server logic is in `src/gateway/*` with a large surface area for channels, tools, sessions, and control UI.
- Plugin system and extension loader live in `src/plugins/*` with extensions under `extensions/*`.
- Web control UI is a separate Vite app under `ui/` (`ui/package.json`).
- Native apps are Swift for iOS and macOS under `apps/ios` and `apps/macos`, and Kotlin for Android under `apps/android`.

## Feasibility Of A Rust Rewrite

Full rewrite is possible in principle but would be a multi program replacement:

- Replace the Node TypeScript core (CLI, gateway, plugin runtime, channel integrations).
- Rebuild or wrap a large set of Node SDKs and native modules used by the gateway and channels.
- Redesign the plugin ABI or keep a JavaScript runtime bridge to preserve existing extensions.
- The Swift and Kotlin apps are separate codebases and would remain in their current languages.

Given this scope, a full rewrite would be a long running, high risk project unless requirements are narrowed.

## Potential Benefits Of Rust

These are potential advantages and would need profiling to confirm on OpenClaw workloads:

- Lower memory overhead for long running gateway processes.
- More predictable latency under load by avoiding GC pauses.
- A single native binary for the core CLI and gateway could simplify distribution.
- Stronger compile time guarantees in core services.

## Costs And Risks

- Loss of compatibility with the current Node extension ecosystem unless a JS bridge is maintained.
- Large reimplementation effort for channel SDKs and gateway integrations.
- Increased maintenance burden during a multi year migration, with behavior parity risk across channels.

## Recommended Path If Rust Is A Goal

Based on the current architecture, a smaller and lower risk approach is:

- Keep the Node TypeScript gateway as the primary runtime.
- Introduce Rust for a constrained subsystem that has clear performance or stability pressure.
- Preserve the plugin system and extension compatibility while Rust is phased in.

This approach minimizes user visible breakage and reduces the scope of rework.

## ZAI Coding Plan Sufficiency

External verification (2026-02-04) indicates two relevant constraints in Z.AI docs:

- The "Other tools" guidance says any OpenAI protocol tool can integrate GLM-4.7 by using the Z.AI coding endpoint and an API key.
- The FAQ and overview state the Coding Plan quota only applies inside supported coding tools and cannot be used for general API calls. API calls are billed separately.

Z.AI lists OpenCode among supported tools, so using the coding endpoint and key inside OpenCode should be covered by the Coding Plan. OpenClaw uses the Z.AI API directly and requires a `ZAI_API_KEY` for the built in `zai` provider. The gateway defaults to the Z.AI coding endpoint and fails at runtime without a valid API key.

Conclusion: The Z.AI Coding Plan alone is not sufficient to run OpenClaw features that depend on the `zai` provider unless Z.AI explicitly includes OpenClaw as a supported tool. A valid API key and billable API access are required for direct API use.

## Feature → Model Map (Examples)

Examples below are drawn from provider docs already in this repo. Treat them as
illustrative and swap in equivalent models from your chosen providers.

| Feature or capability | Config knob | Example models |
| --- | --- | --- |
| Default chat + tools (gateway, routing, browser/canvas tools) | `agents.defaults.model.primary` (or per-agent override) | `anthropic/claude-opus-4-5`, `openai/gpt-5.2`, `zai/glm-4.7`, `venice/llama-3.3-70b` |
| Coding-focused sessions | `agents.defaults.model.primary` (or per-agent override, `/model` switch) | `openai-codex/gpt-5.2`, `opencode/claude-opus-4-5`, `zai/glm-4.7`, `qwen-portal/coder-model` |
| Vision / image analysis (`image` tool) | `agents.defaults.imageModel.primary` (fallbacks optional) | `qwen-portal/vision-model`, `venice/qwen3-vl-235b-a22b`, `venice/gemini-3-pro-preview` |
| Complex reasoning | `agents.defaults.model.primary` (or per-agent override) | `anthropic/claude-opus-4-5`, `venice/claude-opus-45`, `venice/deepseek-v3.2` |
| Fast + cheap defaults | `agents.defaults.model.primary` | `venice/qwen3-4b`, `venice/llama-3.2-3b` |
| Local or offline inference | `models.providers` + local runtime | `ollama/llama3.3` |
| Talk mode speech output (TTS) | `talk.modelId` | `eleven_v3` |

## Evidence

- CLI bootstrap: `openclaw.mjs`, `src/entry.ts`
- Gateway surface: `src/gateway/`
- Plugin runtime: `src/plugins/`, `extensions/`
- Web UI: `ui/package.json`
- Native apps: `apps/ios`, `apps/macos`, `apps/android`
- Z.AI provider details: `docs/gateway/configuration.md`, `docs/providers/zai.md`
