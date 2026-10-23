export interface Toast {
    id: number;
    text: string;
    tone: 'normal' | 'error';
    action?: { label: string; run: () => void };
}

class Toasts {
    items = $state<Toast[]>([]);
    lifted = $state(false);
    private next = 1;

    show(text: string, options: Omit<Partial<Toast>, 'id' | 'text'> = {}, timeout?: number) {
        const toast: Toast = { id: this.next++, text, tone: 'normal', ...options };
        timeout ??= toast.action === undefined ? 6000 : 12000;
        this.items = [...this.items, toast];
        setTimeout(() => this.dismiss(toast.id), timeout);
    }

    error(text: string) {
        this.show(text, { tone: 'error' }, 8000);
    }

    dismiss(id: number) {
        this.items = this.items.filter((t) => t.id !== id);
    }
}

export const toasts = new Toasts();
