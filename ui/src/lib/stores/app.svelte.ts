import type { Route, Toast } from "$lib/types";

let _route = $state<Route>("dashboard");
let _toasts = $state<Toast[]>([]);
let _initialized = $state(false);

export const appStore = {
  get route() { return _route; },
  set route(v: Route) { _route = v; },
  get toasts() { return _toasts; },
  get initialized() { return _initialized; },
  set initialized(v: boolean) { _initialized = v; },

  navigate(route: Route) {
    _route = route;
  },

  toast(message: string, type: Toast["type"] = "success") {
    const id = crypto.randomUUID();
    _toasts = [..._toasts, { id, message, type }];
    setTimeout(() => {
      _toasts = _toasts.filter((t) => t.id !== id);
    }, 3000);
  },

  removeToast(id: string) {
    _toasts = _toasts.filter((t) => t.id !== id);
  },
};
