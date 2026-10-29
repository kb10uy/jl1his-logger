<script lang="ts">
    import { goto } from '$app/navigation';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { lastLogbook, session } from '#lib/state/session.svelte.ts';

    const target = $derived.by(() => {
        const last = lastLogbook();
        return session.logbook(last ?? '') ?? session.logbooks[0];
    });

    $effect(() => {
        if (target) goto(`/logbooks/${target.id}`, { replace: true });
    });
</script>

<PageTitle />

{#if !target}
    <main class="flex min-h-dvh flex-col items-center justify-center gap-2 p-6 text-center">
        <h1 class="text-xl font-semibold">{m.no_logbook_title()}</h1>
        {#if session.me?.role === 'admin'}
            <p class="max-w-sm text-muted">
                {m.no_logbook_admin_body({ callsign: session.me.callsign })}
            </p>
            <a href="/admin/logbooks">{m.no_logbook_admin_link()}</a>
        {:else}
            <p class="max-w-sm text-muted">
                {m.no_logbook_body({ callsign: session.me?.callsign ?? '' })}
            </p>
        {/if}
    </main>
{/if}
