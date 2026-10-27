<script lang="ts">
    import '@fontsource-variable/inter';
    import '../app.css';
    import { onMount } from 'svelte';
    import { getCurrentWindow } from '@tauri-apps/api/window';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import { m } from '#lib/paraglide/messages.js';
    import { getLocale } from '#lib/paraglide/runtime.js';
    import { session } from '#lib/session/session.svelte.ts';
    import { settings } from '#lib/settings/settings.svelte.ts';

    let { children } = $props();
    let ready = $state(false);

    document.documentElement.lang = getLocale();

    onMount(() => {
        void settings.load().then((go) => (ready = go));
        session.load();
    });

    const title = $derived(
        session.current?.state === 'signedIn' && session.current.callsign
            ? m.window_title({ callsign: session.current.callsign })
            : m.app_name()
    );

    $effect(() => {
        getCurrentWindow()
            .setTitle(title)
            .catch(() => undefined);
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
    <title>{title}</title>
</svelte:head>

{#if session.current && ready}
    {@render children()}
{:else}
    <main class="flex min-h-dvh items-center justify-center text-muted">{m.common_loading()}</main>
{/if}
