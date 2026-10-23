import * as api from '#lib/api/api.ts';

export class Matches {
    total = $state<number | undefined>(undefined);
    error = $state<string | undefined>(undefined);
    private version = $state(0);

    constructor(logbookId: string, query: () => string) {
        $effect(() => {
            void this.version;
            const q = query().trim();
            this.total = undefined;
            this.error = undefined;
            const controller = new AbortController();
            const timer = setTimeout(() => {
                api.searchQsos(logbookId, { q: q || undefined, limit: 1 }, controller.signal)
                    .then((page) => (this.total = page.total))
                    .catch((e) => {
                        if (!(e instanceof DOMException)) this.error = (e as Error).message;
                    });
            }, 300);
            return () => {
                clearTimeout(timer);
                controller.abort();
            };
        });
    }

    refresh() {
        this.version += 1;
    }
}
