const KEY = 'jl1his-logger.sidebar-collapsed';

export function loadCollapsed(): boolean {
    try {
        return localStorage.getItem(KEY) === 'true';
    } catch {
        return false;
    }
}

export function saveCollapsed(collapsed: boolean) {
    try {
        localStorage.setItem(KEY, String(collapsed));
    } catch {
        return;
    }
}
