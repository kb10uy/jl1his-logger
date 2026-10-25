<script lang="ts" generics="T extends { id: string; name: string; archived_at: string | null }">
    import { goto } from '$app/navigation';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { matchesFilter } from '#lib/filter.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        label,
        items,
        selected,
        href,
        detail,
        search,
        placeholder,
        empty,
        class: className = ''
    }: {
        label: string;
        items: T[];
        selected: string | undefined;
        href: (item: T) => string;
        detail: (item: T) => string;
        search: (item: T) => (string | null)[];
        placeholder: string;
        empty: string;
        class?: string;
    } = $props();

    let query = $state('');
    let archivedOpen = $state(false);

    const filtering = $derived(query.trim() !== '');
    const matching = $derived(items.filter((item) => matchesFilter(search(item), query)));
    const active = $derived(matching.filter((item) => item.archived_at === null));
    const archived = $derived(matching.filter((item) => item.archived_at !== null));
    const total = $derived(items.filter((item) => item.archived_at === null).length);

    $effect(() => {
        if (filtering || items.some((i) => i.id === selected && i.archived_at !== null)) {
            archivedOpen = true;
        }
    });

    function onkeydown(e: KeyboardEvent) {
        if (e.key === 'Enter' && active.length === 1) goto(href(active[0]));
    }
</script>

{#snippet row(item: T)}
    <a
        href={href(item)}
        aria-current={item.id === selected ? 'page' : undefined}
        class={[
            'flex h-14 items-center gap-2 border-b border-surface-2 px-3 text-[15px] no-underline md:h-9 md:text-[13px]',
            item.id === selected
                ? 'bg-surface-2 font-semibold text-accent shadow-[inset_3px_0_0_var(--accent)]'
                : item.archived_at === null
                  ? 'text-text hover:bg-surface-2/60'
                  : 'text-muted hover:bg-surface-2/60'
        ]}
    >
        <span class="min-w-0 flex-1 truncate">{item.name}</span>
        <span class="shrink-0 font-mono text-[13px] font-normal text-muted md:text-xs"
            >{detail(item)}</span
        >
    </a>
{/snippet}

<section
    aria-label={label}
    class={[
        'flex min-w-0 flex-col overflow-hidden rounded-lg border border-border bg-surface md:sticky md:top-4 md:max-h-[calc(100dvh-6rem)]',
        className
    ]}
>
    <div class="flex flex-col gap-1.5 border-b border-border p-2">
        <div
            class="flex h-11 items-center gap-2 rounded-lg border border-border bg-bg px-3 md:h-9 md:rounded-md md:px-2.5"
        >
            <Icon name="search" size={16} class="text-muted" />
            <input
                type="search"
                aria-label={m.stations_filter_label()}
                bind:value={query}
                {onkeydown}
                {placeholder}
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                class="min-w-0 flex-1 border-0 bg-transparent text-base outline-none md:text-[13px]"
            />
        </div>
        {#if filtering}
            <span class="px-0.5 text-xs text-muted" aria-live="polite"
                >{m.stations_filter_count({ shown: active.length, total })}</span
            >
        {/if}
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        {#each active as item (item.id)}
            {@render row(item)}
        {:else}
            <p class="px-3 py-4 text-[13px] text-muted">
                {filtering ? m.stations_filter_none({ query: query.trim() }) : empty}
            </p>
        {/each}
        {#if archived.length > 0}
            <details bind:open={archivedOpen}>
                <summary
                    class="flex h-14 cursor-pointer items-center px-3 text-xs text-muted md:h-9"
                    >{m.stations_archived_count({ count: archived.length })}</summary
                >
                {#each archived as item (item.id)}
                    {@render row(item)}
                {/each}
            </details>
        {/if}
    </div>
</section>
