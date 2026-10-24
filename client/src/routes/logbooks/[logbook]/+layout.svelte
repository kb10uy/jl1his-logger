<script lang="ts">
    import { page } from '$app/state';
    import AppShell from '#lib/components/AppShell.svelte';
    import LogbookScope from '#lib/components/LogbookScope.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let { children } = $props();

    const logbook = $derived(session.logbook(page.params.logbook ?? ''));
</script>

{#if logbook}
    {#key logbook.id}
        <LogbookScope {logbook}>
            {@render children()}
        </LogbookScope>
    {/key}
{:else}
    <AppShell logbookId={null}>
        <main class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
            <h1 class="text-xl font-semibold">{m.logbook_not_found_title()}</h1>
            <p class="text-muted">{m.logbook_not_found_body()}</p>
            <a href="/">{m.logbook_not_found_link()}</a>
        </main>
    </AppShell>
{/if}
