import type { SessionInfo, TranscriptEntry, ProjectInfo } from "$lib/types";
import { serverStore } from "./server.svelte";

// Raw shapes from the Rust backend (snake_case)
interface RawSession {
  id: string;
  channel: string;
  chat_id: string;
  project: string;
  title: string | null;
  created_at: string;
  updated_at: string;
}

interface RawProject {
  name: string;
  session_count: number;
}

function mapSession(raw: RawSession): SessionInfo {
  return {
    id: raw.id,
    channel: raw.channel,
    chatId: raw.chat_id,
    project: raw.project,
    title: raw.title,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
  };
}

function mapProject(raw: RawProject): ProjectInfo {
  return {
    name: raw.name,
    sessionCount: raw.session_count,
  };
}

let _sessions = $state<SessionInfo[]>([]);
let _selectedSessionId = $state<string | null>(null);
let _transcript = $state<TranscriptEntry[]>([]);
let _projects = $state<ProjectInfo[]>([]);
let _loading = $state(false);
let _transcriptLoading = $state(false);

export const sessionsStore = {
  get sessions() { return _sessions; },
  get selectedSessionId() { return _selectedSessionId; },
  get transcript() { return _transcript; },
  get projects() { return _projects; },
  get loading() { return _loading; },
  get transcriptLoading() { return _transcriptLoading; },

  get selectedSession(): SessionInfo | null {
    return _sessions.find(s => s.id === _selectedSessionId) ?? null;
  },

  async loadSessions() {
    _loading = true;
    try {
      const result = await serverStore.sendRpc<{ sessions: RawSession[] }>("sessions.list");
      _sessions = (result?.sessions ?? []).map(mapSession).sort(
        (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()
      );
      // Auto-select first session if none selected
      if (!_selectedSessionId && _sessions.length > 0) {
        _selectedSessionId = _sessions[0].id;
        await this.loadTranscript(_sessions[0].id);
      }
    } catch {
      // Ignore — server might not be connected
    } finally {
      _loading = false;
    }
  },

  async loadTranscript(sessionId: string) {
    _transcriptLoading = true;
    try {
      const session = _sessions.find(s => s.id === sessionId);
      const project = session?.project ?? "default";
      const result = await serverStore.sendRpc<{ entries: TranscriptEntry[] }>(
        "sessions.transcript",
        { session_id: sessionId, project }
      );
      _transcript = result?.entries ?? [];
    } catch {
      _transcript = [];
    } finally {
      _transcriptLoading = false;
    }
  },

  async selectSession(id: string) {
    _selectedSessionId = id;
    await this.loadTranscript(id);
  },

  async loadProjects() {
    try {
      const result = await serverStore.sendRpc<{ projects: RawProject[] }>("projects.list");
      _projects = (result?.projects ?? []).map(mapProject);
    } catch {
      // Ignore
    }
  },

  /** Append a real-time transcript entry for the currently viewed session */
  appendLiveEntry(entry: TranscriptEntry) {
    _transcript = [..._transcript, entry];
  },

  clear() {
    _sessions = [];
    _selectedSessionId = null;
    _transcript = [];
    _projects = [];
  },
};
