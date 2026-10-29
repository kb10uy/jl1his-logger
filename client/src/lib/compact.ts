const KEY = 'jl1his-logger.compact';

export function applyCompact(compact: boolean) {
    if (compact) document.documentElement.dataset.compact = '';
    else delete document.documentElement.dataset.compact;
}

export function storedCompact(): boolean {
    try {
        return localStorage.getItem(KEY) === '1';
    } catch {
        return false;
    }
}

export function chooseCompact(compact: boolean) {
    applyCompact(compact);
    try {
        if (compact) localStorage.setItem(KEY, '1');
        else localStorage.removeItem(KEY);
    } catch {
        return;
    }
}
