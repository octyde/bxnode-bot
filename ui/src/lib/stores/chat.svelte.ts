import type { ChatMessage } from "$lib/types";

export interface ChatSession {
  id: string;
  name: string;
  messages: ChatMessage[];
  model: string;
  createdAt: number;
  updatedAt: number;
}

interface ChatStoreState {
  sessions: ChatSession[];
  currentSessionId: string | null;
  selectedModel: string;
}

class ChatStore {
  private state = $state<ChatStoreState>({
    sessions: [],
    currentSessionId: null,
    selectedModel: "",
  });

  get sessions() {
    return this.state.sessions;
  }

  get currentSessionId() {
    return this.state.currentSessionId;
  }

  get currentSession(): ChatSession | null {
    if (!this.state.currentSessionId) return null;
    return this.state.sessions.find(s => s.id === this.state.currentSessionId) || null;
  }

  get messages(): ChatMessage[] {
    // Access currentSessionId to establish reactivity dependency
    const id = this.state.currentSessionId;
    if (!id) return [];
    const session = this.state.sessions.find(s => s.id === id);
    return session?.messages || [];
  }

  get selectedModel() {
    return this.state.selectedModel || this.currentSession?.model || "";
  }

  set selectedModel(value: string) {
    this.state.selectedModel = value;
    if (this.currentSession) {
      this.currentSession.model = value;
    }
    this.saveToLocalStorage();
  }

  createSession(name?: string): string {
    const id = `session-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const now = Date.now();
    const session: ChatSession = {
      id,
      name: name || `Chat ${new Date(now).toLocaleDateString()} ${new Date(now).toLocaleTimeString()}`,
      messages: [],
      model: this.state.selectedModel,
      createdAt: now,
      updatedAt: now,
    };
    this.state.sessions = [...this.state.sessions, session];
    this.state.currentSessionId = id;
    this.saveToLocalStorage();
    return id;
  }

  switchSession(sessionId: string) {
    const session = this.state.sessions.find(s => s.id === sessionId);
    if (session) {
      this.state.currentSessionId = sessionId;
      this.state.selectedModel = session.model;
      this.saveToLocalStorage();
    }
  }

  deleteSession(sessionId: string) {
    this.state.sessions = this.state.sessions.filter(s => s.id !== sessionId);
    if (this.state.currentSessionId === sessionId) {
      // Switch to the most recent session, or create a new one
      if (this.state.sessions.length > 0) {
        this.state.currentSessionId = this.state.sessions[this.state.sessions.length - 1].id;
      } else {
        this.createSession();
      }
    }
    this.saveToLocalStorage();
  }

  renameSession(sessionId: string, newName: string) {
    const session = this.state.sessions.find(s => s.id === sessionId);
    if (session) {
      session.name = newName;
      this.saveToLocalStorage();
    }
  }

  addMessage(message: ChatMessage) {
    if (!this.currentSession) {
      this.createSession();
    }
    if (this.currentSession) {
      this.currentSession.messages = [...this.currentSession.messages, message];
      this.currentSession.updatedAt = Date.now();
      this.saveToLocalStorage();
    }
  }

  clearMessages() {
    if (this.currentSession) {
      this.currentSession.messages = [];
      this.currentSession.updatedAt = Date.now();
      this.saveToLocalStorage();
    }
  }

  loadFromLocalStorage() {
    try {
      const saved = localStorage.getItem("chat-sessions");
      if (saved) {
        const parsed = JSON.parse(saved) as ChatStoreState;
        const sessions = parsed.sessions || [];
        const currentSessionId = parsed.currentSessionId || null;
        const selectedModel = parsed.selectedModel || "";

        if (sessions.length === 0) {
          this.createSession();
        } else {
          // Reassign entire state to ensure Svelte 5 $state reactivity picks up all nested data
          this.state = {
            sessions,
            currentSessionId,
            selectedModel,
          };
        }
      } else {
        // First time - create a default session
        this.createSession();
      }
    } catch (e) {
      console.error("Failed to load chat sessions:", e);
      this.createSession();
    }
  }

  private saveToLocalStorage() {
    try {
      localStorage.setItem("chat-sessions", JSON.stringify(this.state));
    } catch (e) {
      console.error("Failed to save chat sessions:", e);
    }
  }
}

export const chatStore = new ChatStore();
