<script lang="ts" module>
    export interface ChooserWords {
        nav: string;
        item: string;
        title: (name: string) => string;
        new: string;
        back: string;
        list: string;
        counts: (active: number, archived: number) => string;
        filter: string;
        empty: string;
        choose: string;
        cannotCreate: string;
        notFound: string;
    }
</script>

<script
    lang="ts"
    generics="T extends { id: string; name: string; archived_at: string | null; favorite: boolean }"
>
    import type { Snippet } from 'svelte';
    import { MediaQuery } from 'svelte/reactivity';
    import { afterNavigate, goto } from '$app/navigation';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import PickList from '#lib/components/PickList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let {
        segment,
        selected,
        items,
        canCreate,
        words,
        detail,
        search,
        favorite,
        reload,
        editor
    }: {
        segment: string;
        selected: string | undefined;
        items: T[];
        canCreate: boolean;
        words: ChooserWords;
        detail: (item: T) => string;
        search: (item: T) => (string | null | undefined)[];
        favorite: (id: string, favorite: boolean) => Promise<unknown>;
        reload: () => Promise<void>;
        editor: Snippet<[T | undefined, (item: T) => void]>;
    } = $props();

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const root = $derived(`/logbooks/${logbookId}/${segment}`);
    const active = $derived(items.filter((i) => i.archived_at === null).length);
    const current = $derived(
        selected === undefined || selected === 'new'
            ? undefined
            : items.find((i) => i.id === selected)
    );

    const desktop = new MediaQuery('min-width: 768px');
    const detailed = $derived(selected !== undefined && !desktop.current);

    let fromList = $state(false);
    afterNavigate(({ from }) => {
        fromList = from?.url.pathname === root;
    });

    function back(e: MouseEvent) {
        if (!fromList) return;
        e.preventDefault();
        history.back();
    }

    async function mark(item: T, value: boolean) {
        try {
            await favorite(item.id, value);
            await reload();
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        }
    }

    async function saved(item: T) {
        await reload();
        if (selected !== item.id) goto(`${root}/${item.id}`);
    }
</script>

<PageTitle title={current === undefined ? words.nav : words.title(current.name)} />

<PageHeader
    title={detailed ? (selected === 'new' ? words.new : (current?.name ?? words.item)) : words.nav}
    subtitle={detailed
        ? `${logbook.logbook.name} · ${words.nav}`
        : `${logbook.logbook.name} · ${words.counts(active, items.length - active)}`}
    back={detailed
        ? { href: root, onclick: back, label: words.back }
        : { href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
>
    {#snippet actions()}
        {#if canCreate && (selected === undefined || desktop.current)}
            <Button variant="primary" href={`${root}/new`} class="max-md:w-9 max-md:px-0"
                ><Icon name="plus" /><span class="max-md:sr-only">{words.new}</span></Button
            >
        {/if}
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-5">
    <LogbookLoadError />
    <div class="grid items-start gap-4 md:grid-cols-[minmax(240px,320px)_minmax(0,1fr)]">
        <PickList
            label={words.list}
            {items}
            {selected}
            onfavorite={mark}
            href={(item) => `${root}/${item.id}`}
            {detail}
            search={(item) => search(item).map((v) => v ?? null)}
            placeholder={words.filter}
            empty={words.empty}
            class={selected !== undefined ? 'max-md:hidden' : ''}
        />

        <div class="flex min-w-0 flex-col gap-4">
            {#if selected === undefined}
                <p class="hidden rounded-lg border border-border p-6 text-muted md:block">
                    {words.choose}
                </p>
            {:else if selected === 'new' && !canCreate}
                <p class="text-muted">{words.cannotCreate}</p>
            {:else if selected !== 'new' && current === undefined}
                <p class="text-muted">
                    {logbook.loaded ? words.notFound : m.common_loading()}
                </p>
            {:else}
                {#key selected}
                    {@render editor(current, saved)}
                {/key}
            {/if}
        </div>
    </div>
</main>
