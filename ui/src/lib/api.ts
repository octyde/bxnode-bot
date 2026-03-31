import { invoke } from "@tauri-apps/api/core";
import type {
  SkillInfo,
  SkillDetail,
  SkillsConfigSummary,
  SyncResponse,
  StatsResponse,
  ProviderInfo,
  ChannelInfo,
  CronJob,
  MemoryRecord,
  MemorySearchResult,
  MemoryStats,
  AppConfig,
  AppInfo,
  AppDefinition,
} from "./types";

// ── Skills ──────────────────────────────────────────────────────

export const initSkills = () =>
  invoke<SkillsConfigSummary>("init_skills");

export const listSkills = () =>
  invoke<SkillInfo[]>("list_skills");

export const getSkill = (name: string) =>
  invoke<SkillDetail>("get_skill", { name });

export const enableSkill = (name: string) =>
  invoke<string>("enable_skill", { name });

export const disableSkill = (name: string) =>
  invoke<string>("disable_skill", { name });

export const syncSkills = () =>
  invoke<SyncResponse>("sync_skills");

export const getSkillsConfig = () =>
  invoke<SkillsConfigSummary>("get_skills_config");

export const refreshSkills = () =>
  invoke<number>("refresh_skills");

// ── General ─────────────────────────────────────────────────────

export const getStatus = () =>
  invoke<{ status: string; version: string }>("get_status");

export const getStats = () =>
  invoke<StatsResponse>("get_stats");

export const loadConfig = (path?: string) =>
  invoke<string>("load_config", { path: path ?? null });

// ── Providers ───────────────────────────────────────────────────

export const listProviders = () =>
  invoke<ProviderInfo[]>("list_providers");

export const getProviderConfig = () =>
  invoke<Record<string, unknown>>("get_provider_config");

export const updateProviderConfig = (
  providerId: string,
  apiKey?: string,
  baseUrl?: string,
  organization?: string,
  defaultModel?: string,
) =>
  invoke<string>("update_provider_config", {
    providerId,
    apiKey: apiKey ?? null,
    baseUrl: baseUrl ?? null,
    organization: organization ?? null,
    defaultModel: defaultModel ?? null,
  });

export const removeProviderConfig = (providerId: string) =>
  invoke<string>("remove_provider_config", { providerId });

// ── Channels ────────────────────────────────────────────────────

export const listChannels = () =>
  invoke<ChannelInfo[]>("list_channels");

export const getChannelStatus = (channelId: string) =>
  invoke<Record<string, unknown>>("get_channel_status", { channelId });

export const updateChannelConfig = (
  channelId: string,
  fields: Record<string, string>,
) =>
  invoke<string>("update_channel_config", { channelId, fields });

export const removeChannelConfig = (channelId: string) =>
  invoke<string>("remove_channel_config", { channelId });

export const getChannelConfig = (channelId: string) =>
  invoke<Record<string, string>>("get_channel_config", { channelId });

export const testChannelConnection = (channelId: string, fields: Record<string, string>) =>
  invoke<{ success: boolean; name?: string; username?: string; message: string }>(
    "test_channel_connection",
    { channelId, fields },
  );

// ── Cron ────────────────────────────────────────────────────────

export const listCronJobs = () =>
  invoke<CronJob[]>("list_cron_jobs");

export const addCronJob = (
  id: string,
  schedule: string,
  payload: unknown,
  description?: string,
) =>
  invoke<string>("add_cron_job", { id, schedule, payload, description: description ?? null });

export const removeCronJob = (id: string) =>
  invoke<boolean>("remove_cron_job", { id });

export const runCronJob = (id: string) =>
  invoke<string>("run_cron_job", { id });

// ── Memory ──────────────────────────────────────────────────────

export const listMemories = (limit?: number) =>
  invoke<MemoryRecord[]>("list_memories", { limit: limit ?? 50 });

export const searchMemories = (query: string, limit?: number) =>
  invoke<MemorySearchResult[]>("search_memories", { query, limit: limit ?? 10 });

export const deleteMemory = (id: string) =>
  invoke<boolean>("delete_memory", { id });

export const memoryStats = () =>
  invoke<MemoryStats>("memory_stats");

export const compactMemory = () =>
  invoke<string>("compact_memory");

// ── Config ──────────────────────────────────────────────────────

export const getConfig = () =>
  invoke<AppConfig>("get_config");

// ── Chat ────────────────────────────────────────────────────────

export const sendMessage = (message: string, model: string) =>
  invoke<string>("send_message", { message, model });

export const listModels = () =>
  invoke<string[]>("list_models");

// ── Apps ────────────────────────────────────────────────────────

export const initApps = () =>
  invoke<AppInfo[]>("init_apps");

export const listApps = () =>
  invoke<AppInfo[]>("list_apps");

export const getApp = (name: string) =>
  invoke<AppDefinition>("get_app", { name });

// ── Server ───────────────────────────────────────────────────

export interface ServerStatus {
  running: boolean;
  port: number;
  url: string | null;
  log: string[];
  error: string | null;
}

export const startServer = () =>
  invoke<ServerStatus>("start_server");

export const stopServer = () =>
  invoke<ServerStatus>("stop_server");

export const getServerStatus = () =>
  invoke<ServerStatus>("get_server_status");
