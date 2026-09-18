export const DEFAULT_SYSTEM = 'wparse';
export const SHARED_SYSTEM = DEFAULT_SYSTEM;
export const SYSTEM_STORAGE_KEY = 'warpstation_current_system';
export const AVAILABLE_SYSTEMS = Object.freeze(['wparse', 'wfusion']);

let currentSystem = DEFAULT_SYSTEM;
const systemListeners = new Set();

function isBrowser() {
  return typeof window !== 'undefined';
}

export function normalizeSystem(system) {
  const normalized =
    typeof system === 'string' && system.trim() ? system.trim().toLowerCase() : DEFAULT_SYSTEM;
  return AVAILABLE_SYSTEMS.includes(normalized) ? normalized : DEFAULT_SYSTEM;
}

function notifySystemListeners() {
  systemListeners.forEach((listener) => listener(currentSystem));
}

export function getStoredSystem() {
  if (!isBrowser()) {
    return currentSystem;
  }

  try {
    const stored = window.localStorage.getItem(SYSTEM_STORAGE_KEY);
    return normalizeSystem(stored);
  } catch {
    return currentSystem;
  }
}

export function getCurrentSystem() {
  return currentSystem;
}

export function setCurrentSystem(system) {
  const nextSystem = normalizeSystem(system);
  if (nextSystem === currentSystem) {
    return currentSystem;
  }

  currentSystem = nextSystem;

  if (isBrowser()) {
    try {
      window.localStorage.setItem(SYSTEM_STORAGE_KEY, nextSystem);
    } catch {
      // 忽略 localStorage 写入失败
    }
  }

  notifySystemListeners();
  return currentSystem;
}

export function subscribeCurrentSystem(listener) {
  systemListeners.add(listener);
  return () => {
    systemListeners.delete(listener);
  };
}

export function ensureSystemInitialized(system) {
  currentSystem = normalizeSystem(system ?? getStoredSystem());

  if (isBrowser()) {
    try {
      window.localStorage.setItem(SYSTEM_STORAGE_KEY, currentSystem);
    } catch {
      // 忽略 localStorage 写入失败
    }
  }

  return currentSystem;
}

export function getSharedSystem() {
  return SHARED_SYSTEM;
}

export function buildSystemSearch(search = '', system) {
  const params = new URLSearchParams(search || '');
  params.set('system', normalizeSystem(system));
  return params.toString();
}

export function withSystemPath(path, system) {
  const normalizedSystem = normalizeSystem(system);
  const [pathname, rawSearch = ''] = String(path || '').split('?');
  const nextSearch = buildSystemSearch(rawSearch, normalizedSystem);
  return nextSearch ? `${pathname}?${nextSearch}` : pathname;
}

export function resolveSystem(system) {
  return typeof system === 'string' && system.trim()
    ? normalizeSystem(system)
    : currentSystem;
}

ensureSystemInitialized();
