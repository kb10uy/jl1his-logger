<script lang="ts">
    import type { Snippet } from 'svelte';
    import { DropdownMenu } from 'bits-ui';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import Icon, { type IconName } from '#lib/components/ui/Icon.svelte';
    import type { StreamStatus } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { session } from '#lib/state/session.svelte.ts';

    let {
        logbookId,
        status,
        lastUpdatedSeq,
        children
    }: {
        logbookId: string | null;
        status?: StreamStatus;
        lastUpdatedSeq?: number;
        children: Snippet;
    } = $props();

    const logbook = $derived(logbookId === null ? undefined : session.logbook(logbookId));
    const root = $derived(logbookId === null ? null : `/logbooks/${logbookId}`);

    interface Item {
        label: string;
        short: string;
        icon: IconName;
        href: string;
        mobile: boolean;
    }

    const items = $derived<Item[]>(
        root === null
            ? []
            : [
                  { label: 'Log', short: 'Log', icon: 'log', href: `${root}/log`, mobile: true },
                  {
                      label: 'Live entry',
                      short: 'Entry',
                      icon: 'entry',
                      href: `${root}/entry`,
                      mobile: true
                  },
                  {
                      label: 'Voice entry',
                      short: 'Voice',
                      icon: 'voice',
                      href: `${root}/voice`,
                      mobile: true
                  },
                  {
                      label: 'Stations',
                      short: 'Stations',
                      icon: 'stations',
                      href: `${root}/stations`,
                      mobile: false
                  },
                  { label: 'QSL', short: 'QSL', icon: 'qsl', href: `${root}/qsl`, mobile: false },
                  {
                      label: 'Import / Export',
                      short: 'Import',
                      icon: 'transfer',
                      href: `${root}/import-export`,
                      mobile: false
                  }
              ]
    );

    const current = (href: string) =>
        page.url.pathname === href || page.url.pathname.startsWith(`${href}/`);
    const onSettings = $derived(
        page.url.pathname.startsWith('/settings') || page.url.pathname === `${root}/members`
    );
    const onMore = $derived(
        root !== null &&
            !items.some((i) => i.mobile && current(i.href)) &&
            !page.url.pathname.startsWith(`${root}/qsos/`)
    );

    function switchTo(id: string) {
        if (logbookId === null) {
            goto(`/logbooks/${id}/log`);
            return;
        }
        const rest = page.url.pathname.slice(`/logbooks/${logbookId}`.length);
        const screen = rest.split('/').filter((s) => s !== '')[0] ?? 'log';
        goto(`/logbooks/${id}/${screen === 'qsos' ? 'log' : screen}`);
    }

    const statusText = $derived.by(() => {
        const unsent = outbox.items.length;
        if (status === 'offline' || status === 'connecting') {
            const label = status === 'offline' ? 'Offline' : 'Connecting';
            return unsent > 0 ? `${label} · ${unsent} unsent` : label;
        }
        if (unsent > 0) return `${unsent} unsent`;
        return `Synced · seq ${lastUpdatedSeq ?? 0}`;
    });
    const statusColor = $derived(
        status === 'live' && outbox.items.length === 0 ? 'bg-ok' : 'bg-warn'
    );

    async function signOut() {
        await session.signOut();
        goto('/login');
    }
</script>

<div class="flex min-h-dvh bg-bg text-text">
    <nav
        aria-label="Main"
        class="sticky top-0 hidden h-dvh w-[232px] shrink-0 flex-col gap-1 overflow-y-auto border-r border-border bg-surface px-3 py-4 md:flex"
    >
        <div class="px-3 pt-1 pb-3.5">
            <span class="text-[15px] font-bold">JL1HIS Logger</span>
        </div>
        {#each items as item (item.href)}
            <a
                href={item.href}
                aria-current={current(item.href) ? 'page' : undefined}
                class={[
                    'flex h-10 items-center gap-2.5 rounded-md px-3 no-underline',
                    current(item.href)
                        ? 'bg-surface-2 font-semibold text-accent'
                        : 'text-text hover:bg-surface-2/60'
                ]}
            >
                <Icon name={item.icon} />{item.label}
            </a>
        {/each}
        <a
            href="/settings/account"
            aria-current={onSettings ? 'page' : undefined}
            class={[
                'flex h-10 items-center gap-2.5 rounded-md px-3 no-underline',
                onSettings
                    ? 'bg-surface-2 font-semibold text-accent'
                    : 'text-text hover:bg-surface-2/60'
            ]}
        >
            <Icon name="settings" />Settings
        </a>
        <div class="flex-1"></div>
        <div class="flex flex-col gap-1.5 border-t border-border p-3 text-xs text-muted">
            <DropdownMenu.Root>
                <DropdownMenu.Trigger
                    class="mb-1 flex h-8 items-center justify-between rounded-md border border-border bg-bg px-2.5 text-[13px] text-text"
                >
                    <span class="truncate font-mono font-semibold"
                        >{logbook?.name ?? 'Logbook'}</span
                    >
                    <span class="flex items-center gap-1 text-xs text-muted"
                        >Logbook <Icon name="chevron" size={12} /></span
                    >
                </DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                    <DropdownMenu.Content
                        side="top"
                        align="start"
                        sideOffset={4}
                        class="z-40 flex min-w-[200px] flex-col rounded-lg border border-border bg-surface p-1 shadow-lg shadow-shadow"
                    >
                        {#each session.logbooks as l (l.id)}
                            <DropdownMenu.Item
                                onSelect={() => switchTo(l.id)}
                                class="flex h-8 cursor-pointer items-center justify-between gap-3 rounded-md px-2.5 text-[13px] data-highlighted:bg-surface-2"
                            >
                                <span class="font-mono font-semibold">{l.name}</span>
                                <span class="text-xs text-muted">{l.role}</span>
                            </DropdownMenu.Item>
                        {/each}
                    </DropdownMenu.Content>
                </DropdownMenu.Portal>
            </DropdownMenu.Root>
            {#if status !== undefined}
                <span class="flex items-center gap-2"
                    ><span class={['size-2 rounded-full', statusColor]}></span>{statusText}</span
                >
            {/if}
            <span class="flex items-center justify-between gap-2">
                <span>{session.me?.callsign}{logbook ? ` · ${logbook.role}` : ''}</span>
                <button
                    type="button"
                    aria-label="Sign out"
                    title="Sign out"
                    class="inline-flex size-7 items-center justify-center rounded-md text-muted hover:text-text"
                    onclick={signOut}><Icon name="signout" size={16} /></button
                >
            </span>
        </div>
    </nav>

    <div class="flex min-w-0 flex-1 flex-col pb-[84px] md:pb-0">
        {@render children()}
    </div>

    {#if root !== null}
        <nav
            aria-label="Main"
            class="fixed inset-x-0 bottom-0 z-30 grid h-[84px] grid-cols-4 border-t border-border bg-surface px-2 pt-1.5 pb-6 md:hidden"
        >
            {#each items.filter((i) => i.mobile) as item (item.href)}
                <a
                    href={item.href}
                    aria-current={current(item.href) ? 'page' : undefined}
                    class={[
                        'flex flex-col items-center justify-center gap-1 rounded-lg text-[11px] no-underline',
                        current(item.href) ? 'font-semibold text-accent' : 'text-muted'
                    ]}
                >
                    <Icon name={item.icon} size={22} />{item.short}
                </a>
            {/each}
            <a
                href={`${root}/more`}
                aria-current={onMore ? 'page' : undefined}
                class={[
                    'flex flex-col items-center justify-center gap-1 rounded-lg text-[11px] no-underline',
                    onMore ? 'font-semibold text-accent' : 'text-muted'
                ]}
            >
                <Icon name="more" size={22} />More
            </a>
        </nav>
    {/if}
</div>
