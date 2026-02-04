# Nanobot Lessons (HKUDS/nanobot)

Date: 2026-02-04

## Executive Summary

Nanobot is positioned as an ultra-lightweight personal AI assistant with a small, readable codebase (about ~4,000 LOC) and a narrow, opinionated feature set. The repo emphasizes ease of onboarding, OpenAI-compatible provider support, minimal configuration surface, and a clean modular layout that makes the agent loop and tool execution easy to follow. The design shows a practical path to shipping a usable assistant by minimizing dependencies and keeping the core control loop and toolchain explicit.

## What We Can Learn

1. **Minimal, explicit agent loop**
   The README surfaces the core agent loop components (`loop.py`, `context.py`, `memory.py`, `skills.py`, `tools/`) in a small, flat tree. This makes the execution flow and reasoning chain easy to read and modify, a useful pattern for research or rapid iteration.

2. **Config-first UX**
   A single JSON config file (`~/.nanobot/config.json`) drives providers, channels, tools, and model selection. This reduces hidden magic and makes troubleshooting straightforward.

3. **OpenAI-compatible provider strategy**
   The project defaults to OpenRouter for LLMs and supports other providers (Anthropic, OpenAI, Groq, Gemini). It also supports local inference via vLLM or any OpenAI-compatible server, reinforcing a portability-first approach.

4. **Fast onboarding path**
   The install + quick-start is three steps: `onboard`, edit config, run agent. This is a good template for making assistants approachable for non-experts.

5. **Lean channels for real-world utility**
   Telegram and WhatsApp are the initial chat channels. This keeps the channel surface narrow while still enabling real-world usage. The WhatsApp flow is explicit (QR linking + two terminals).

6. **Scheduling as a first-class primitive**
   The CLI includes cron support for recurring jobs, which is a simple but high-leverage feature for “assistant” use cases.

7. **Readable module boundaries**
   The project structure splits responsibilities into agent, skills, channels, bus, cron, heartbeat, providers, session, config, and CLI. This is a good example of a small but complete system decomposition.

8. **Shipping with Docker from day one**
   The README documents Docker workflow with config persistence, which is useful for repeatable deployment without deep local setup.

## Practical Takeaways For Our Codebase

- If we want a smaller, more teachable core, consider carving out a minimal “agent loop + tools” layer with clear entry points, mirroring nanobot’s `agent/` layout.
- Keep config surface as a single JSON file (or a single directory) so that onboarding and debugging remain predictable.
- Favor OpenAI-compatible provider abstractions and optional local-model support to keep the deployment story flexible.
- Make cron/scheduling part of the core CLI rather than an add-on.

## References (Repo Signals)

- README highlights: ultra-lightweight scope, ~4k LOC, core features, quick start.
- README config examples: providers, model, web search, channels.
- README project structure: core loop + tools + skills + channels + bus + cron + heartbeat + providers + sessions + config + CLI.
- README deployment: Docker workflow and config persistence guidance.
