<script lang="ts">
    import type { Snippet } from 'svelte';
    import { page } from '$app/state';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let { logbookId, children }: { logbookId: string | null; children: Snippet } = $props();

    const logbook = $derived(logbookId === null ? undefined : session.logbook(logbookId));

    const sections = $derived(
        [
            {
                label: m.settings_account(),
                scope: m.settings_scope_you(),
                href: '/settings/account'
            },
            { label: m.settings_tokens(), scope: m.settings_scope_you(), href: '/settings/tokens' },
            {
                label: m.settings_voice(),
                scope: m.settings_scope_device(),
                href: '/settings/voice'
            },
            {
                label: m.settings_appearance(),
                scope: m.settings_scope_device(),
                href: '/settings/appearance'
            },
            logbookId === null
                ? undefined
                : {
                      label: m.settings_logbook(),
                      scope: m.settings_scope_logbook(),
                      href: `/logbooks/${logbookId}/settings`
                  },
            logbookId === null
                ? undefined
                : {
                      label: m.settings_members(),
                      scope: m.settings_scope_logbook(),
                      href: `/logbooks/${logbookId}/members`
                  },
            {
                label: m.settings_server(),
                scope: m.settings_scope_instance(),
                href: '/settings/server'
            }
        ].filter((s) => s !== undefined)
    );
    const current = $derived(sections.find((s) => page.url.pathname === s.href));
</script>

<PageTitle
    title={current === undefined
        ? m.settings_title()
        : m.settings_section_title({ section: current.label })}
/>

<PageHeader
    title={current?.label ?? m.settings_title()}
    subtitle={current
        ? `${m.settings_title()} · ${(current.href.startsWith('/logbooks/') && logbook?.name) || current.scope}`
        : session.me?.callsign}
    back={logbookId === null
        ? undefined
        : current
          ? { href: '/settings', label: m.settings_back() }
          : { href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
/>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <div class="flex items-start gap-6">
        <nav
            aria-label={m.settings_sections()}
            class={[
                'flex shrink-0 flex-col gap-0.5 max-md:w-full md:w-[220px]',
                current && 'max-md:hidden',
                !current &&
                    'max-md:overflow-hidden max-md:rounded-lg max-md:border max-md:border-border max-md:bg-surface'
            ]}
        >
            {#each sections as section (section.href)}
                <a
                    href={section.href}
                    aria-current={section === current ? 'page' : undefined}
                    class={[
                        'flex flex-col gap-0.5 px-3 py-2 no-underline max-md:min-h-14 max-md:justify-center max-md:border-b max-md:border-surface-2 max-md:last:border-b-0 md:rounded-md',
                        section === current
                            ? 'bg-surface-2 font-semibold text-accent'
                            : 'text-text hover:bg-surface-2/60'
                    ]}
                >
                    <span>{section.label}</span>
                    <span class="text-xs font-normal text-muted">{section.scope}</span>
                </a>
            {/each}
        </nav>
        <div class={['flex max-w-3xl min-w-0 flex-1 flex-col gap-4', !current && 'max-md:hidden']}>
            {@render children()}
        </div>
    </div>
</main>
