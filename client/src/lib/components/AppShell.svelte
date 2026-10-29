<script lang="ts">
    import type { Snippet } from 'svelte';
    import { DropdownMenu } from 'bits-ui';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import Icon, { type IconName } from '#lib/components/ui/Icon.svelte';
    import { roleLabel } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import type { StreamStatus } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { session } from '#lib/state/session.svelte.ts';
    import { loadCollapsed, saveCollapsed } from '#lib/shell/settings.ts';

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

    const groups = $derived<Item[][]>(
        root === null
            ? []
            : [
                  [
                      {
                          label: m.nav_logbook(),
                          short: m.nav_logbook_short(),
                          icon: 'log',
                          href: root,
                          mobile: true
                      },
                      {
                          label: m.nav_live_entry(),
                          short: m.nav_live_entry_short(),
                          icon: 'entry',
                          href: `${root}/entry`,
                          mobile: true
                      },
                      {
                          label: m.nav_voice_entry(),
                          short: m.nav_voice_entry_short(),
                          icon: 'voice',
                          href: `${root}/voice`,
                          mobile: true
                      }
                  ],
                  [
                      {
                          label: m.nav_stations(),
                          short: m.nav_stations(),
                          icon: 'stations',
                          href: `${root}/stations`,
                          mobile: false
                      },
                      {
                          label: m.nav_instruments(),
                          short: m.nav_instruments(),
                          icon: 'instruments',
                          href: `${root}/instruments`,
                          mobile: false
                      },
                      {
                          label: m.nav_qsl_management(),
                          short: m.nav_qsl_management_short(),
                          icon: 'qsl',
                          href: `${root}/qsl-management`,
                          mobile: false
                      },
                      {
                          label: m.nav_import_export(),
                          short: m.nav_import_export_short(),
                          icon: 'transfer',
                          href: `${root}/import-export`,
                          mobile: false
                      }
                  ]
              ]
    );
    const items = $derived(groups.flat());

    const current = (href: string) =>
        page.url.pathname === href || (href !== root && page.url.pathname.startsWith(`${href}/`));
    const onSettings = $derived(
        page.url.pathname.startsWith('/settings') ||
            page.url.pathname === `${root}/settings` ||
            page.url.pathname === `${root}/members`
    );
    const onAdmin = $derived(page.url.pathname.startsWith('/admin'));
    const onMore = $derived(
        root !== null &&
            !items.some((i) => i.mobile && current(i.href)) &&
            !page.url.pathname.startsWith(`${root}/qsos/`)
    );

    function switchTo(id: string) {
        if (logbookId === null) {
            goto(`/logbooks/${id}`);
            return;
        }
        const rest = page.url.pathname.slice(`/logbooks/${logbookId}`.length);
        const screen = rest.split('/').filter((s) => s !== '')[0];
        goto(
            screen === undefined || screen === 'qsos'
                ? `/logbooks/${id}`
                : `/logbooks/${id}/${screen}`
        );
    }

    const statusText = $derived.by(() => {
        const unsent = outbox.items.length;
        if (status === 'offline' || status === 'connecting') {
            const label = status === 'offline' ? m.shell_offline() : m.shell_connecting();
            return unsent > 0 ? `${label} · ${m.common_unsent({ count: unsent })}` : label;
        }
        if (unsent > 0) return m.common_unsent({ count: unsent });
        return m.shell_synced({ seq: lastUpdatedSeq ?? 0 });
    });
    const statusColor = $derived(
        status === 'live' && outbox.items.length === 0 ? 'bg-ok' : 'bg-warn'
    );

    let collapsed = $state(loadCollapsed());

    function toggle() {
        collapsed = !collapsed;
        saveCollapsed(collapsed);
    }

    const who = $derived(
        `${session.me?.callsign ?? ''}${logbook ? ` · ${roleLabel(logbook.role)}` : ''}`
    );
    const linkClass = (active: boolean) => [
        'flex h-10 items-center gap-2.5 rounded-md no-underline',
        collapsed ? 'justify-center' : 'px-3',
        active ? 'bg-surface-2 font-semibold text-accent' : 'text-text hover:bg-surface-2/60'
    ];

    async function signOut() {
        await session.signOut();
        goto('/login');
    }
</script>

<div class="flex min-h-dvh bg-bg text-text">
    <nav
        aria-label={m.nav_main()}
        class={[
            'sticky top-0 hidden h-dvh shrink-0 flex-col gap-1 overflow-y-auto border-r border-border bg-surface py-4 md:flex',
            collapsed ? 'w-[60px] px-2' : 'w-[232px] px-3'
        ]}
    >
        <div class={['flex items-center gap-2 pt-1 pb-3.5', collapsed ? 'flex-col' : 'pr-0 pl-3']}>
            <img src="/logo.svg" alt="" class="size-6" />
            {#if !collapsed}
                <span class="flex-1 font-logo text-[15px] font-bold">JL1HIS Logger</span>
            {/if}
            <button
                type="button"
                aria-label={collapsed ? m.shell_expand_sidebar() : m.shell_collapse_sidebar()}
                title={collapsed ? m.shell_expand_sidebar() : m.shell_collapse_sidebar()}
                class="inline-flex size-8 items-center justify-center rounded-md bg-none text-muted hover:bg-surface-2/60 hover:text-text"
                onclick={toggle}><Icon name={collapsed ? 'expand' : 'collapse'} /></button
            >
        </div>
        {#each groups as group, i (i)}
            {#each group as item (item.href)}
                <a
                    href={item.href}
                    aria-current={current(item.href) ? 'page' : undefined}
                    aria-label={collapsed ? item.label : undefined}
                    title={collapsed ? item.label : undefined}
                    class={linkClass(current(item.href))}
                >
                    <Icon name={item.icon} />{#if !collapsed}{item.label}{/if}
                </a>
            {/each}
            {@render separator()}
        {/each}
        <a
            href="/settings/account"
            aria-current={onSettings ? 'page' : undefined}
            aria-label={collapsed ? m.nav_settings() : undefined}
            title={collapsed ? m.nav_settings() : undefined}
            class={linkClass(onSettings)}
        >
            <Icon name="settings" />{#if !collapsed}{m.nav_settings()}{/if}
        </a>
        {#if session.me?.role === 'admin'}
            <a
                href="/admin/logbooks"
                aria-current={onAdmin ? 'page' : undefined}
                aria-label={collapsed ? m.nav_admin() : undefined}
                title={collapsed ? m.nav_admin() : undefined}
                class={linkClass(onAdmin)}
            >
                <Icon name="admin" />{#if !collapsed}{m.nav_admin()}{/if}
            </a>
        {/if}
        <div class="flex-1"></div>
        {#if collapsed}
            <div class="flex flex-col items-center gap-3 border-t border-border pt-3 text-muted">
                {@render logbookMenu()}
                {#if status !== undefined}
                    <span
                        role="img"
                        aria-label={statusText}
                        title={statusText}
                        class={['size-2 rounded-full', statusColor]}
                    ></span>
                {/if}
                <span
                    title={who}
                    class="font-mono text-xs font-semibold text-text [writing-mode:vertical-rl]"
                    >{session.me?.callsign}</span
                >
                <button
                    type="button"
                    aria-label={m.shell_sign_out()}
                    title={m.shell_sign_out()}
                    class="inline-flex size-8 items-center justify-center rounded-md text-muted hover:text-text"
                    onclick={signOut}><Icon name="signout" size={16} /></button
                >
            </div>
        {:else}
            <div class="flex flex-col gap-1.5 border-t border-border p-3 text-xs text-muted">
                {@render logbookMenu()}
                {#if status !== undefined}
                    <span class="flex items-center gap-2"
                        ><span class={['size-2 rounded-full', statusColor]}
                        ></span>{statusText}</span
                    >
                {/if}
                <span class="flex items-center justify-between gap-2">
                    <span>{who}</span>
                    <button
                        type="button"
                        aria-label={m.shell_sign_out()}
                        title={m.shell_sign_out()}
                        class="inline-flex size-7 items-center justify-center rounded-md text-muted hover:text-text"
                        onclick={signOut}><Icon name="signout" size={16} /></button
                    >
                </span>
            </div>
        {/if}
    </nav>

    {#snippet separator()}
        <hr class={['my-1.5 border-border', collapsed ? 'mx-2' : 'mx-3']} />
    {/snippet}

    {#snippet logbookMenu()}
        <DropdownMenu.Root>
            {#if collapsed}
                <DropdownMenu.Trigger
                    aria-label={m.shell_logbook_menu({
                        name: logbook?.name ?? m.shell_logbook_none()
                    })}
                    title={logbook?.name ?? m.nav_logbook()}
                    class="inline-flex size-9 items-center justify-center rounded-md border border-border bg-bg text-text"
                    ><Icon name="logbook" /></DropdownMenu.Trigger
                >
            {:else}
                <DropdownMenu.Trigger
                    class="mb-1 flex h-8 items-center justify-between gap-2 rounded-md border border-border bg-bg px-2.5 text-[13px] text-text"
                >
                    <span class="min-w-0 truncate font-semibold"
                        >{logbook?.name ?? m.nav_logbook()}</span
                    >
                    <span
                        class="flex shrink-0 items-center gap-1 text-xs whitespace-nowrap text-muted"
                        >{m.nav_logbook()} <Icon name="chevron" size={12} /></span
                    >
                </DropdownMenu.Trigger>
            {/if}
            <DropdownMenu.Portal>
                <DropdownMenu.Content
                    side={collapsed ? 'right' : 'top'}
                    align={collapsed ? 'end' : 'start'}
                    sideOffset={collapsed ? 16 : 4}
                    class="z-40 flex min-w-[200px] flex-col rounded-lg border border-border bg-surface p-1 shadow-lg shadow-shadow"
                >
                    {#each session.logbooks as l (l.id)}
                        <DropdownMenu.Item
                            onSelect={() => switchTo(l.id)}
                            class="flex h-8 cursor-pointer items-center justify-between gap-3 rounded-md px-2.5 text-[13px] data-highlighted:bg-surface-2"
                        >
                            <span class="font-semibold">{l.name}</span>
                            <span class="text-xs text-muted">{roleLabel(l.role)}</span>
                        </DropdownMenu.Item>
                    {/each}
                </DropdownMenu.Content>
            </DropdownMenu.Portal>
        </DropdownMenu.Root>
    {/snippet}

    <div class="flex min-w-0 flex-1 flex-col pb-[84px] md:pb-0">
        {@render children()}
    </div>

    {#if root !== null}
        <nav
            aria-label={m.nav_main()}
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
                <Icon name="more" size={22} />{m.nav_more()}
            </a>
        </nav>
    {/if}
</div>
