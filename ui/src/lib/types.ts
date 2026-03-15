// TypeScript types matching Rust structs

// ── Skills ──────────────────────────────────────────────────────

export interface SkillInfo {
  name: string;
  description: string;
  license?: string;
  is_active: boolean;
  reference_count: number;
  script_count: number;
}

export interface SkillDetail extends SkillInfo {
  instructions?: string;
}

export interface SkillsConfigSummary {
  enabled: boolean;
  total_skills: number;
  active_skills: number;
  directories: string[];
}

export interface SyncResponse {
  success: boolean;
  synced: string[];
  skipped: string[];
  errors: [string, string][];
  total: number;
}

// ── Providers ───────────────────────────────────────────────────

export interface ProviderInfo {
  id: string;
  name: string;
  configured: boolean;
  has_api_key: boolean;
  base_url?: string;
  default_model?: string;
}

// ── Channels ────────────────────────────────────────────────────

export interface ChannelInfo {
  id: string;
  name: string;
  configured: boolean;
  status: "running" | "stopped" | "error" | "not_configured";
}

// ── Cron ────────────────────────────────────────────────────────

export interface CronJob {
  id: string;
  schedule: string;
  payload: unknown;
  enabled: boolean;
  description?: string;
}

// ── Memory ──────────────────────────────────────────────────────

export interface MemoryRecord {
  id: string;
  scope: MemoryScope;
  content: string;
  summary?: string;
  tags: string[];
  importance: number;
  created_at: number;
  updated_at: number;
  ttl_days?: number;
  deleted_at?: number;
  provenance?: string;
}

export interface MemoryScope {
  agent_id: string;
  channel_id?: string;
  user_id?: string;
  session_id?: string;
}

export interface MemorySearchResult {
  id: string;
  score: number;
  summary?: string;
  content_preview: string;
  tags: string[];
  created_at: number;
  importance: number;
}

export interface MemoryStats {
  total_records: number;
  total_size_bytes: number;
}

// ── Config ──────────────────────────────────────────────────────

export interface AppConfig {
  server: {
    host: string;
    port: number;
    cors: boolean;
  };
  skills: {
    enabled: boolean;
    directories: string[];
  };
  cron: {
    enabled: boolean;
    store_path: string;
  };
  memory: {
    enabled: boolean;
    store_path: string;
    max_results: number;
    ttl_days: number;
  };
}

// ── Stats ───────────────────────────────────────────────────────

export interface StatsResponse {
  version: string;
  providers: number;
  skills_enabled: boolean;
  total_skills: number;
  active_skills: number;
}

// ── Sessions ────────────────────────────────────────────────────

export interface SessionInfo {
  id: string;
  channel: string;
  chatId: string;
  project: string;
  title: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface TranscriptEntry {
  timestamp: string;
  type: "user" | "assistant" | "tool_use" | "tool_result" | "system";
  content?: string;
  user_id?: string;
  model?: string;
  tool?: string;
  input?: unknown;
  output?: string;
  success?: boolean;
  message?: string;
}

export interface ProjectInfo {
  name: string;
  sessionCount: number;
}

// ── Chat ────────────────────────────────────────────────────────

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  timestamp: number;
}

// ── UI ──────────────────────────────────────────────────────────

export interface Toast {
  id: string;
  message: string;
  type: "success" | "error" | "warning";
}

export type Route =
  | "dashboard"
  | "skills"
  | "providers"
  | "channels"
  | "activity"
  | "cron"
  | "memory"
  | "chat"
  | "settings";
