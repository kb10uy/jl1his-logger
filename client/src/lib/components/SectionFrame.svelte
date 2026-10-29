<script lang="ts" module>
    export interface Section {
        label: string;
        href: string;
    }

    export interface Group {
        label: string;
        sections: Section[];
    }
</script>

<script lang="ts">
    import type { Snippet } from 'svelte';
    import { page } from '$app/state';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let {
        title,
        sectionTitle,
        back,
        index,
        label,
        groups,
        logbookId,
        children
    }: {
        title: string;
        sectionTitle: (section: string) => string;
        back: string;
        index: string;
        label: string;
        groups: Group[];
        logbookId: string | null;
        children: Snippet;
    } = $props();

    const current = $derived(
        groups
            .flatMap((group) => group.sections.map((section) => ({ group, section })))
            .find((entry) => page.url.pathname === entry.section.href)
    );
</script>

<PageTitle title={current === undefined ? title : sectionTitle(current.section.label)} />

<PageHeader
    title={current?.section.label ?? title}
    subtitle={current ? `${title} · ${current.group.label}` : session.me?.callsign}
    back={logbookId === null
        ? undefined
        : current
          ? { href: index, label: back }
          : { href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
/>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <div class="flex items-start gap-6">
        <nav
            aria-label={label}
            class={[
                'flex shrink-0 flex-col gap-4 max-md:w-full md:w-[220px]',
                current && 'max-md:hidden'
            ]}
        >
            {#each groups as group (group.label)}
                <section aria-label={group.label} class="flex flex-col gap-1">
                    <h2 class="px-3 eyebrow max-md:px-1">{group.label}</h2>
                    <div
                        class="flex flex-col gap-0.5 max-md:overflow-hidden max-md:rounded-lg max-md:border max-md:border-border max-md:bg-surface"
                    >
                        {#each group.sections as section (section.href)}
                            <a
                                href={section.href}
                                aria-current={section === current?.section ? 'page' : undefined}
                                class={[
                                    'flex items-center px-3 no-underline max-md:min-h-12 max-md:border-b max-md:border-surface-2 max-md:last:border-b-0 md:h-9 md:rounded-md',
                                    section === current?.section
                                        ? 'bg-surface-2 font-semibold text-accent'
                                        : 'text-text hover:bg-surface-2/60'
                                ]}>{section.label}</a
                            >
                        {/each}
                    </div>
                </section>
            {/each}
        </nav>
        <div class={['flex max-w-3xl min-w-0 flex-1 flex-col gap-4', !current && 'max-md:hidden']}>
            {@render children()}
        </div>
    </div>
</main>
