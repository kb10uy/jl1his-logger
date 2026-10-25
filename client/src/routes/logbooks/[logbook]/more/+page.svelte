<script lang="ts">
    import { goto } from '$app/navigation';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Icon, { type IconName } from '#lib/components/ui/Icon.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { roleLabel, SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { session } from '#lib/state/session.svelte.ts';

    const logbook = useLogbook();
    const root = `/logbooks/${logbook.logbook.id}`;

    const links: { label: string; icon: IconName; href: string; detail?: () => string }[] = [
        {
            label: m.nav_stations(),
            icon: 'stations',
            href: `${root}/stations`,
            detail: () => m.more_active({ count: logbook.activeStations.length })
        },
        {
            label: m.nav_instruments(),
            icon: 'instruments',
            href: `${root}/instruments`,
            detail: () => m.more_active({ count: logbook.activeInstruments.length })
        },
        { label: m.nav_qsl_management(), icon: 'qsl', href: `${root}/qsl-management` },
        { label: m.nav_import_export(), icon: 'transfer', href: `${root}/import-export` },
        { label: m.nav_members(), icon: 'members', href: `${root}/members` },
        { label: m.nav_settings(), icon: 'settings', href: '/settings' }
    ];

    async function signOut() {
        await session.signOut();
        goto('/login');
    }
</script>

<PageTitle title={m.nav_more()} />

<PageHeader title={m.nav_more()} subtitle={logbook.logbook.name}>
    {#snippet toolbar()}
        <div class="flex flex-col gap-1">
            <label for="more-logbook" class="text-xs text-muted">{m.more_logbook()}</label>
            <select
                id="more-logbook"
                class="control h-11 bg-bg"
                value={logbook.logbook.id}
                onchange={(e) => goto(`/logbooks/${e.currentTarget.value}/more`)}
            >
                {#each session.logbooks as l (l.id)}
                    <option value={l.id}>{l.name} · {roleLabel(l.role)}</option>
                {/each}
            </select>
        </div>
    {/snippet}
</PageHeader>

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
            {logbook.status === 'live'
                ? m.shell_synced({ seq: logbook.lastUpdatedSeq })
                : m.shell_offline()}
            {outbox.items.length > 0 ? ` · ${m.common_unsent({ count: outbox.items.length })}` : ''}
        </span>
        <span
            ><Slotted
                text={m.more_signed_in_as({
                    callsign: SLOT,
                    role: roleLabel(session.me?.role ?? '')
                })}><span class="font-mono text-text">{session.me?.callsign}</span></Slotted
            ></span
        >
    </div>
    <button
        type="button"
        class="h-11 rounded-lg border border-border bg-surface-2 font-medium"
        onclick={signOut}>{m.shell_sign_out()}</button
    >
</main>
