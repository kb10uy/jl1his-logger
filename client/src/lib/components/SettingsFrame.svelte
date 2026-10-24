<script lang="ts">
    import type { Snippet } from 'svelte';
    import { page } from '$app/state';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let { logbookId, children }: { logbookId: string | null; children: Snippet } = $props();

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
                label: m.settings_language(),
                scope: m.settings_scope_device(),
                href: '/settings/language'
            },
            logbookId === null
                ? undefined
                : {
                      label: m.settings_logbook(),
                      scope: m.settings_scope_logbook({
                          name: session.logbook(logbookId)?.name ?? ''
                      }),
                      href: `/logbooks/${logbookId}/settings`
                  },
            logbookId === null
                ? undefined
                : {
                      label: m.settings_members(),
                      scope: m.settings_scope_logbook({
                          name: session.logbook(logbookId)?.name ?? ''
                      }),
                      href: `/logbooks/${logbookId}/members`
                  },
            {
                label: m.settings_reference(),
                scope: m.settings_scope_instance(),
                href: '/settings/reference'
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

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header class="flex items-center gap-2">
        {#if logbookId !== null}
            <a
                href={current ? '/settings' : `/logbooks/${logbookId}/more`}
                aria-label={current ? m.settings_back() : m.settings_back_more()}
                class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text md:hidden"
                ><Icon name="back" size={22} /></a
            >
        {/if}
        <h1 class="text-xl font-semibold">
            <span class={current && 'max-md:hidden'}>{m.settings_title()}</span>
            {#if current}<span class="md:hidden">{current.label}</span>{/if}
        </h1>
    </header>

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
                            ? 'bg-surface-2 font-semibold text-accent shadow-[inset_3px_0_0_var(--accent)]'
                            : 'text-text hover:bg-surface-2/60'
                    ]}
                >
                    <span>{section.label}</span>
                    <span class="text-xs font-normal text-muted">{section.scope}</span>
                </a>
            {/each}
        </nav>
        <div class={['flex min-w-0 flex-1 flex-col gap-4', !current && 'max-md:hidden']}>
            {@render children()}
        </div>
    </div>
</main>
