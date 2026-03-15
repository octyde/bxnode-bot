import * as api from "$lib/api";
import type { ServerStatus } from "$lib/api";
import { appStore } from "./app.svelte";

export type ServerState = "stopped" | "starting" | "running" | "stopping" | "error";

export interface ActivityMessage {
  id: string;
  type: "incoming" | "outgoing" | "error";
  channel: string;
  chatId: string;
  userId?: string;
  userName?: string;
  content: string;
  timestamp: string;
  replyTo?: string;
  error?: string;
  sessionId?: string;
}

export interface PendingApproval {
  id: string;
  userId: string;
  userName?: string;
  channel: string;
  chatId: string;
  firstMessage: string;
  timestamp: string;
}

const MAX_MESSAGES = 200;

let _state = $state<ServerState>("stopped");
let _port = $state(3000);
let _url = $state<string | null>(null);
let _log = $state<string[]>([]);
let _error = $state<string | null>(null);
let _channelStatuses = $state<Record<string, { connected: boolean; error?: string }>>({});
let _messages = $state<ActivityMessage[]>([]);
let _pendingApprovals = $state<PendingApproval[]>([]);
let _attached = $state(false); // true when connected to an external server (not spawned by Tauri)
let _pollTimer: ReturnType<typeof setInterval> | null = null;
let _wsConnection: WebSocket | null = null;
let _statusRefreshTimer: ReturnType<typeof setInterval> | null = null;

// Pending RPC response callbacks
const _rpcCallbacks = new Map<string, (result: unknown, error?: { code: number; message: string }) => void>();

function applyStatus(status: ServerStatus) {
  _port = status.port;
  _url = status.url;
  _log = status.log;
  _error = status.error;
}

function startPolling() {
  stopPolling();
  _pollTimer = setInterval(async () => {
    try {
      const status = await api.getServerStatus();
      applyStatus(status);
      // Only disconnect if we are NOT attached to an external server.
      // When _attached is true, the Tauri process tracker doesn't know
      // about the external process, so status.running will be false — but
      // the server is still there.
      if (!status.running && _state === "running" && !_attached) {
        _state = "stopped";
        disconnectWs();
        appStore.toast("Server stopped", "info");
      }
    } catch {
      // Ignore polling errors
    }
  }, 3000);
}

function stopPolling() {
  if (_pollTimer) {
    clearInterval(_pollTimer);
    _pollTimer = null;
  }
}

function connectWs(port: number) {
  disconnectWs();
  try {
    const ws = new WebSocket(`ws://localhost:${port}/ws`);
    ws.onopen = () => {
      ws.send(JSON.stringify({
        type: "request",
        id: "channels-status",
        method: "channels.status",
        params: {},
      }));
      ws.send(JSON.stringify({
        type: "request",
        id: "approvals-list",
        method: "approvals.list",
        params: {},
      }));
    };
    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);

        // Handle RPC responses
        if (msg.type === "response") {
          // Check for registered RPC callbacks first
          const cb = _rpcCallbacks.get(msg.id);
          if (cb) {
            _rpcCallbacks.delete(msg.id);
            cb(msg.result, msg.error);
          }

          if (msg.id === "channels-status" && msg.result) {
            const channels = msg.result.channels as Array<{
              id: string;
              connected: boolean;
              error?: string;
            }>;
            const statuses: Record<string, { connected: boolean; error?: string }> = {};
            for (const ch of channels) {
              statuses[ch.id] = { connected: ch.connected, error: ch.error };
            }
            _channelStatuses = statuses;
          }
          if (msg.id === "approvals-list" && msg.result) {
            const raw = (msg.result.approvals ?? []) as Array<Record<string, unknown>>;
            _pendingApprovals = raw.map(a => ({
              id: a.id as string,
              userId: a.user_id as string,
              userName: a.user_name as string | undefined,
              channel: a.channel as string,
              chatId: a.chat_id as string,
              firstMessage: a.first_message as string,
              timestamp: a.timestamp as string,
            }));
          }
        }

        // Handle broadcast events pushed by server
        if (msg.type === "event" && msg.event) {
          handleBroadcastEvent(msg.event, msg.payload);
        }
      } catch {
        // Ignore parse errors
      }
    };
    ws.onclose = () => {
      _wsConnection = null;
      // Clear all pending callbacks on disconnect
      for (const [, cb] of _rpcCallbacks) {
        cb(null, { code: -1, message: "WebSocket disconnected" });
      }
      _rpcCallbacks.clear();
      // Auto-reconnect if we're supposed to be running
      if (_state === "running" && _port) {
        setTimeout(() => {
          if (_state === "running" && !_wsConnection) {
            connectWs(_port);
          }
        }, 3000);
      }
    };
    _wsConnection = ws;
  } catch {
    // WebSocket connection failed — non-fatal
  }
}

function handleBroadcastEvent(eventName: string, payload: Record<string, unknown>) {
  if (!payload) return;
  // The payload is the serialized BroadcastEvent which has { event: "...", data: { ... } }
  const d = (payload.data as Record<string, unknown>) ?? payload;

  switch (eventName) {
    case "message.incoming":
      addMessage({
        id: (d.id as string) ?? crypto.randomUUID(),
        type: "incoming",
        channel: d.channel as string,
        chatId: d.chat_id as string,
        userId: d.user_id as string,
        userName: d.user_name as string | undefined,
        content: d.content as string,
        timestamp: d.timestamp as string,
        sessionId: d.session_id as string | undefined,
      });
      break;
    case "message.outgoing":
      addMessage({
        id: crypto.randomUUID(),
        type: "outgoing",
        channel: d.channel as string,
        chatId: d.chat_id as string,
        content: d.content as string,
        timestamp: d.timestamp as string,
        replyTo: d.reply_to as string | undefined,
        sessionId: d.session_id as string | undefined,
      });
      break;
    case "message.error":
      addMessage({
        id: crypto.randomUUID(),
        type: "error",
        channel: d.channel as string,
        chatId: d.chat_id as string,
        content: "",
        timestamp: d.timestamp as string,
        error: d.error as string,
        sessionId: d.session_id as string | undefined,
      });
      break;
    case "approval.request":
      _pendingApprovals = [..._pendingApprovals, {
        id: d.id as string,
        userId: d.user_id as string,
        userName: d.user_name as string | undefined,
        channel: d.channel as string,
        chatId: d.chat_id as string,
        firstMessage: d.first_message as string,
        timestamp: d.timestamp as string,
      }];
      break;
    case "approval.resolved":
      _pendingApprovals = _pendingApprovals.filter(a => a.id !== (d.id as string));
      break;
  }
}

function addMessage(msg: ActivityMessage) {
  _messages = [..._messages, msg].slice(-MAX_MESSAGES);
}

function disconnectWs() {
  if (_wsConnection) {
    _wsConnection.close();
    _wsConnection = null;
  }
  if (_statusRefreshTimer) {
    clearInterval(_statusRefreshTimer);
    _statusRefreshTimer = null;
  }
  _channelStatuses = {};
}

function refreshChannelStatus() {
  if (_wsConnection && _wsConnection.readyState === WebSocket.OPEN) {
    _wsConnection.send(JSON.stringify({
      type: "request",
      id: "channels-status",
      method: "channels.status",
      params: {},
    }));
  }
}

export const serverStore = {
  get state() { return _state; },
  get port() { return _port; },
  get url() { return _url; },
  get log() { return _log; },
  get error() { return _error; },
  get channelStatuses() { return _channelStatuses; },
  get running() { return _state === "running"; },
  get attached() { return _attached; },
  get messages() { return _messages; },
  get pendingApprovals() { return _pendingApprovals; },
  get pendingApprovalCount() { return _pendingApprovals.length; },

  /** Send a generic RPC request and return a Promise for the result */
  sendRpc<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    return new Promise((resolve, reject) => {
      if (!_wsConnection || _wsConnection.readyState !== WebSocket.OPEN) {
        reject(new Error("WebSocket not connected"));
        return;
      }
      const id = `rpc-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
      _rpcCallbacks.set(id, (result, error) => {
        if (error) {
          reject(new Error(error.message));
        } else {
          resolve(result as T);
        }
      });
      _wsConnection.send(JSON.stringify({
        type: "request",
        id,
        method,
        params,
      }));
      // Timeout after 15s
      setTimeout(() => {
        if (_rpcCallbacks.has(id)) {
          _rpcCallbacks.delete(id);
          reject(new Error("RPC timeout"));
        }
      }, 15000);
    });
  },

  async start() {
    if (_state === "running" || _state === "starting") return;
    _state = "starting";
    _error = null;
    _attached = false;
    try {
      const status = await api.startServer();
      applyStatus(status);
      _state = status.running ? "running" : "error";
      if (status.running) {
        appStore.toast(`Server started on port ${status.port}`, "success");
        startPolling();
        setTimeout(() => {
          connectWs(status.port);
          _statusRefreshTimer = setInterval(refreshChannelStatus, 3000);
        }, 1500);
      }
    } catch (e) {
      _state = "error";
      _error = e instanceof Error ? e.message : String(e);
      appStore.toast(`Failed to start server: ${_error}`, "error");
    }
  },

  async stop() {
    if (_state !== "running") return;
    // If attached to external server, just detach
    if (_attached) {
      this.detach();
      return;
    }
    _state = "stopping";
    try {
      const status = await api.stopServer();
      applyStatus(status);
      _state = "stopped";
      _attached = false;
      disconnectWs();
      stopPolling();
      appStore.toast("Server stopped", "info");
    } catch (e) {
      _state = "error";
      _error = e instanceof Error ? e.message : String(e);
      appStore.toast(`Failed to stop server: ${_error}`, "error");
    }
  },

  async restart() {
    if (_state === "running") {
      _state = "stopping";
      try {
        const status = await api.stopServer();
        applyStatus(status);
        disconnectWs();
        stopPolling();
      } catch (e) {
        _state = "error";
        _error = e instanceof Error ? e.message : String(e);
        appStore.toast(`Failed to stop server: ${_error}`, "error");
        return;
      }
    }
    await new Promise(r => setTimeout(r, 500));
    _state = "starting";
    _error = null;
    try {
      const status = await api.startServer();
      applyStatus(status);
      _state = status.running ? "running" : "error";
      if (status.running) {
        appStore.toast(`Server restarted on port ${status.port}`, "success");
        startPolling();
        setTimeout(() => {
          connectWs(status.port);
          _statusRefreshTimer = setInterval(refreshChannelStatus, 3000);
        }, 1500);
      }
    } catch (e) {
      _state = "error";
      _error = e instanceof Error ? e.message : String(e);
      appStore.toast(`Failed to restart server: ${_error}`, "error");
    }
  },

  async attach(port: number) {
    if (_state === "running") return;
    _state = "starting";
    _error = null;
    _port = port;
    try {
      await new Promise<void>((resolve, reject) => {
        const ws = new WebSocket(`ws://localhost:${port}/ws`);
        const timeout = setTimeout(() => {
          ws.close();
          reject(new Error(`Connection timed out on port ${port}`));
        }, 5000);
        ws.onopen = () => {
          clearTimeout(timeout);
          ws.close();
          resolve();
        };
        ws.onerror = () => {
          clearTimeout(timeout);
          reject(new Error(`Cannot connect to server on port ${port}`));
        };
      });
      _state = "running";
      _attached = true;
      _url = `http://localhost:${port}`;
      appStore.toast(`Attached to server on port ${port}`, "success");
      startPolling();
      connectWs(port);
      _statusRefreshTimer = setInterval(refreshChannelStatus, 3000);
    } catch (e) {
      _state = "stopped";
      _error = e instanceof Error ? e.message : String(e);
      appStore.toast(`Failed to attach: ${_error}`, "error");
    }
  },

  async restartRemote() {
    if (_state !== "running" || !_attached) return;
    const port = _port;

    // Send restart RPC to the server — it will spawn a new process and shut down
    try {
      await this.sendRpc("server.restart");
    } catch {
      // Expected: the server shuts down, so the RPC may not get a response
    }

    // Disconnect and wait for the new server to come up
    disconnectWs();
    stopPolling();
    _channelStatuses = {};
    _state = "starting";

    // Poll until the new server is reachable (up to 15 seconds)
    const maxAttempts = 30;
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      await new Promise(r => setTimeout(r, 500));
      try {
        await new Promise<void>((resolve, reject) => {
          const ws = new WebSocket(`ws://localhost:${port}/ws`);
          const timeout = setTimeout(() => { ws.close(); reject(new Error("timeout")); }, 2000);
          ws.onopen = () => { clearTimeout(timeout); ws.close(); resolve(); };
          ws.onerror = () => { clearTimeout(timeout); reject(new Error("unreachable")); };
        });
        // Server is up — reconnect
        _state = "running";
        _attached = true;
        connectWs(port);
        startPolling();
        _statusRefreshTimer = setInterval(refreshChannelStatus, 3000);
        appStore.toast(`Server restarted on port ${port}`, "success");
        return;
      } catch {
        // Not ready yet, keep polling
      }
    }

    // Timed out
    _state = "stopped";
    _attached = false;
    _error = "Server did not come back up after restart";
    appStore.toast("Server restart timed out", "error");
  },

  detach() {
    if (_state !== "running") return;
    disconnectWs();
    stopPolling();
    _state = "stopped";
    _attached = false;
    _url = null;
    _error = null;
    _channelStatuses = {};
    appStore.toast("Detached from server", "info");
  },

  async refresh() {
    try {
      const status = await api.getServerStatus();
      applyStatus(status);
      if (status.running && _state !== "running") {
        _state = "running";
        _attached = false;
        startPolling();
        connectWs(status.port);
        _statusRefreshTimer = setInterval(refreshChannelStatus, 3000);
      } else if (!status.running && _state !== "running") {
        // Tauri-managed process not running — probe the configured port
        // to auto-detect an externally running server
        const port = status.port || _port;
        try {
          await new Promise<void>((resolve, reject) => {
            const ws = new WebSocket(`ws://localhost:${port}/ws`);
            const timeout = setTimeout(() => {
              ws.close();
              reject(new Error("timeout"));
            }, 2000);
            ws.onopen = () => {
              clearTimeout(timeout);
              ws.close();
              resolve();
            };
            ws.onerror = () => {
              clearTimeout(timeout);
              reject(new Error("unreachable"));
            };
          });
          // External server detected — auto-attach
          _state = "running";
          _attached = true;
          _port = port;
          _url = `http://localhost:${port}`;
          startPolling();
          connectWs(port);
          _statusRefreshTimer = setInterval(refreshChannelStatus, 3000);
        } catch {
          // No server found on the port — stay stopped
          if (_state === "running") {
            _state = "stopped";
            disconnectWs();
            stopPolling();
          }
        }
      } else if (!status.running && _state === "running" && !_attached) {
        _state = "stopped";
        _attached = false;
        disconnectWs();
        stopPolling();
      }
    } catch {
      // Ignore
    }
  },

  getChannelStatus(channelId: string): { connected: boolean; error?: string } | null {
    return _channelStatuses[channelId] ?? null;
  },

  clearMessages() {
    _messages = [];
  },

  approveUser(id: string) {
    if (_wsConnection && _wsConnection.readyState === WebSocket.OPEN) {
      _wsConnection.send(JSON.stringify({
        type: "request",
        id: `approve-${id}`,
        method: "approvals.approve",
        params: { id },
      }));
    }
  },

  rejectUser(id: string) {
    if (_wsConnection && _wsConnection.readyState === WebSocket.OPEN) {
      _wsConnection.send(JSON.stringify({
        type: "request",
        id: `reject-${id}`,
        method: "approvals.reject",
        params: { id },
      }));
    }
  },

  refreshApprovals() {
    if (_wsConnection && _wsConnection.readyState === WebSocket.OPEN) {
      _wsConnection.send(JSON.stringify({
        type: "request",
        id: "approvals-list",
        method: "approvals.list",
        params: {},
      }));
    }
  },

  cleanup() {
    stopPolling();
    disconnectWs();
  },
};
