<script lang="ts">
    import { page } from '$app/state';
    import AppShell from '#lib/components/AppShell.svelte';
    import LogbookScope from '#lib/components/LogbookScope.svelte';
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
            <h1 class="text-xl font-semibold">Logbook not found</h1>
            <p class="text-muted">It does not exist, or you are not a member of it.</p>
            <a href="/">Open your last Logbook</a>
        </main>
    </AppShell>
{/if}
