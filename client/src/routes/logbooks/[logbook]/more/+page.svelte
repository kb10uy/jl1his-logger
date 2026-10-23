<script lang="ts">
    import { goto } from '$app/navigation';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Icon, { type IconName } from '#lib/components/ui/Icon.svelte';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { session } from '#lib/state/session.svelte.ts';

    const logbook = useLogbook();
    const root = `/logbooks/${logbook.logbook.id}`;

    const links: { label: string; icon: IconName; href: string; detail?: () => string }[] = [
        {
            label: 'Stations',
            icon: 'stations',
            href: `${root}/stations`,
            detail: () => `${logbook.activeStations.length} active`
        },
        {
            label: 'Instruments',
            icon: 'instruments',
            href: `${root}/instruments`,
            detail: () => `${logbook.activeInstruments.length} active`
        },
        { label: 'QSL', icon: 'qsl', href: `${root}/qsl` },
        { label: 'Import / Export', icon: 'transfer', href: `${root}/import-export` },
        { label: 'Members', icon: 'members', href: `${root}/members` },
        { label: 'Settings', icon: 'settings', href: '/settings' }
    ];

    async function signOut() {
        await session.signOut();
        goto('/login');
    }
</script>

<PageTitle title="More" />

<header class="flex flex-col gap-2.5 border-b border-border bg-surface px-4 pt-4 pb-3">
    <h1 class="text-xl font-semibold">More</h1>
    <label for="more-logbook" class="text-xs text-muted">Logbook</label>
    <select
        id="more-logbook"
        class="control h-11"
        value={logbook.logbook.id}
        onchange={(e) => goto(`/logbooks/${e.currentTarget.value}/more`)}
    >
        {#each session.logbooks as l (l.id)}
            <option value={l.id}>{l.name} · {l.role}</option>
        {/each}
    </select>
</header>

<main class="flex flex-col gap-4 p-4">
    <div class="flex flex-col overflow-hidden rounded-lg border border-border bg-surface">
        {#each links as link (link.href)}
            <a
                href={link.href}
                class="flex min-h-14 items-center gap-3 border-b border-surface-2 px-4 text-text no-underline last:border-b-0"
            >
                <Icon name={link.icon} size={20} class="text-muted" />
                <span class="flex flex-1 flex-col">
                    <span>{link.label}</span>
                    {#if link.detail}
                        <span class="text-xs text-muted">{link.detail()}</span>
                    {/if}
                </span>
                <Icon name="chevron" size={16} class="-rotate-90 text-muted" />
            </a>
        {/each}
    </div>
    <div class="flex flex-col gap-1.5 px-1 text-xs text-muted">
        <span class="flex items-center gap-2">
            <span
                class={[
                    'size-2 rounded-full',
                    logbook.status === 'live' && outbox.items.length === 0 ? 'bg-ok' : 'bg-warn'
                ]}
            ></span>
            {logbook.status === 'live' ? `Synced · seq ${logbook.lastUpdatedSeq}` : 'Offline'}
            {outbox.items.length > 0 ? ` · ${outbox.items.length} unsent` : ''}
        </span>
        <span
            >Signed in as <span class="font-mono text-text">{session.me?.callsign}</span> · {session
                .me?.role}</span
        >
    </div>
    <button
        type="button"
        class="h-11 rounded-lg border border-border bg-surface-2 font-medium"
        onclick={signOut}>Sign out</button
    >
</main>
