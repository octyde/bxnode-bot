import type { InstalledApp, AppInfo } from "$lib/types";

interface AppsStoreState {
  installedApps: InstalledApp[];
  activeAppId: string | null;
  availableApps: AppInfo[];
  loading: boolean;
}

class AppsStore {
  private state = $state<AppsStoreState>({
    installedApps: [],
    activeAppId: null,
    availableApps: [],
    loading: false,
  });

  get installedApps() { return this.state.installedApps; }
  get activeAppId() { return this.state.activeAppId; }
  set activeAppId(id: string | null) { this.state.activeAppId = id; }
  get availableApps() { return this.state.availableApps; }
  get loading() { return this.state.loading; }

  async init() {
    this.state.loading = true;
    try {
      const { initApps } = await import("$lib/api");
      this.state.availableApps = await initApps();
    } catch (e) {
      console.error("Failed to init apps:", e);
    } finally {
      this.state.loading = false;
    }
  }

  isInstalled(appName: string): boolean {
    return this.state.installedApps.some(a => a.id === appName);
  }

  install(appName: string) {
    if (this.isInstalled(appName)) return;
    this.state.installedApps = [...this.state.installedApps, {
      id: appName,
      installedAt: Date.now(),
    }];
    this.save();
  }

  uninstall(appName: string) {
    this.state.installedApps = this.state.installedApps.filter(a => a.id !== appName);
    if (this.state.activeAppId === appName) {
      this.state.activeAppId = null;
    }
    localStorage.removeItem(`app-${appName}`);
    this.save();
  }

  openApp(appName: string) {
    this.state.activeAppId = appName;
  }

  closeApp() {
    this.state.activeAppId = null;
  }

  loadFromLocalStorage() {
    try {
      const saved = localStorage.getItem("installed-apps");
      if (saved) {
        const parsed = JSON.parse(saved);
        this.state.installedApps = parsed.installedApps || [];
        // Don't restore activeAppId from localStorage
      }
    } catch (e) {
      console.error("Failed to load apps state:", e);
    }
  }

  private save() {
    try {
      localStorage.setItem("installed-apps", JSON.stringify({
        installedApps: this.state.installedApps,
        activeAppId: null,
      }));
    } catch (e) {
      console.error("Failed to save apps state:", e);
    }
  }
}

export const appsStore = new AppsStore();
