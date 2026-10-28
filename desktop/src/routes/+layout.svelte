<script lang="ts">
    import '@fontsource-variable/geist';
    import '@fontsource-variable/m-plus-2';
    import '../app.css';
    import { onMount } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { getCurrentWindow } from '@tauri-apps/api/window';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import { m } from '#lib/paraglide/messages.js';
    import { getLocale } from '#lib/paraglide/runtime.js';
    import { incoming } from '#lib/incoming/incoming.svelte.ts';
    import { session } from '#lib/session/session.svelte.ts';
    import { settings } from '#lib/settings/settings.svelte.ts';
    import { openTextMenu } from '#lib/text-menu.ts';

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
        invoke('set_tray_text', { tooltip: title, show: m.tray_show(), quit: m.tray_quit() }).catch(
            () => undefined
        );
    });

    $effect(() => {
        const current = session.current;
        if (current === undefined) return;
        const signingIn = page.url.pathname === '/sign-in';
        if (current.state === 'signedOut' && !signingIn) goto('/sign-in', { replace: true });
        if (current.state === 'signedIn' && signingIn) goto('/', { replace: true });
    });

    $effect(() => {
        if (session.current?.state === 'signedIn' && ready) incoming.start();
    });
</script>

<svelte:document oncontextmenu={openTextMenu} />

<svelte:head>
    <title>{title}</title>
</svelte:head>

{#if session.current && ready}
    {@render children()}
{:else}
    <main class="flex min-h-dvh items-center justify-center text-muted">{m.common_loading()}</main>
{/if}
