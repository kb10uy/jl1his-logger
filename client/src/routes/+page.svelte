<script lang="ts">
    import { goto } from '$app/navigation';
    import { lastLogbook, session } from '#lib/state/session.svelte.ts';

    const target = $derived.by(() => {
        const last = lastLogbook();
        return session.logbook(last ?? '') ?? session.logbooks[0];
    });

    $effect(() => {
        if (target) goto(`/logbooks/${target.id}/log`, { replace: true });
    });
</script>

{#if !target}
    <main class="flex min-h-dvh flex-col items-center justify-center gap-2 p-6 text-center">
        <h1 class="text-xl font-semibold">No Logbook yet</h1>
        <p class="max-w-sm text-muted">
            You are signed in as {session.me?.callsign}, but no Logbook has you as a member. An
            owner of a Logbook can add you.
        </p>
    </main>
{/if}
