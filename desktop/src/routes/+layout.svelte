<script lang="ts">
    import '@fontsource-variable/inter';
    import '../app.css';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import { m } from '#lib/paraglide/messages.js';
    import { getLocale } from '#lib/paraglide/runtime.js';
    import { session } from '#lib/session/session.svelte.ts';

    let { children } = $props();

    document.documentElement.lang = getLocale();

    onMount(() => {
        session.load();
    });

    $effect(() => {
        const current = session.current;
        if (current === undefined) return;
        const signingIn = page.url.pathname === '/sign-in';
        if (current.state === 'signedOut' && !signingIn) goto('/sign-in', { replace: true });
        if (current.state === 'signedIn' && signingIn) goto('/', { replace: true });
    });
</script>

<svelte:head>
    <title>{m.app_name()}</title>
</svelte:head>

{#if session.current}
    {@render children()}
{:else}
    <main class="flex min-h-dvh items-center justify-center text-muted">{m.common_loading()}</main>
{/if}
