<script lang="ts">
    import { untrack, type Snippet } from 'svelte';
    import type { Logbook } from '#lib/api/types.ts';
    import AppShell from '#lib/components/AppShell.svelte';
    import { logbookState, provideLogbook } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { rememberLogbook } from '#lib/state/session.svelte.ts';

    let { logbook, children }: { logbook: Logbook; children: Snippet } = $props();

    const state = logbookState(untrack(() => logbook));
    provideLogbook(state);

    $effect(() => {
        state.logbook = logbook;
    });

    $effect(() => {
        const id = untrack(() => logbook.id);
        state.start();
        rememberLogbook(id);
        outbox.show(id);
        return () => state.stop();
    });
</script>

<AppShell
    logbookId={logbook.id}
    logHref={state.log.href}
    status={state.status}
    lastUpdatedSeq={state.lastUpdatedSeq}
>
    {@render children()}
</AppShell>
