<script lang="ts">
    import '@fontsource-variable/inter';
    import '../app.css';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import { onUnauthorized } from '#lib/api/client.ts';
    import Toaster from '#lib/components/ui/Toaster.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { getLocale } from '#lib/paraglide/runtime.js';
    import { loadQuery } from '#lib/query/index.ts';
    import { session } from '#lib/state/session.svelte.ts';

    let { children } = $props();

    document.documentElement.lang = getLocale();

    const open = $derived(
        page.url.pathname.startsWith('/login') || page.url.pathname.startsWith('/register')
    );

    onMount(() => {
        onUnauthorized(() => session.expire());
        session.load();
        loadQuery();
    });

    $effect(() => {
        if (session.me === null && !open) {
            const next = page.url.pathname + page.url.search;
            goto(`/login?next=${encodeURIComponent(next)}`, { replace: true });
        }
    });
</script>

{#if open || session.me}
    {@render children()}
{:else if session.error}
    <main class="flex min-h-dvh flex-col items-center justify-center gap-3 p-6 text-center">
        <p class="text-error-text">{session.error}</p>
        <button
            type="button"
            class="h-9 rounded-md border border-border bg-surface-2 px-3.5"
            onclick={() => session.load()}>{m.common_try_again()}</button
        >
    </main>
{:else}
    <main class="flex min-h-dvh items-center justify-center text-muted">{m.common_loading()}</main>
{/if}

<Toaster />
