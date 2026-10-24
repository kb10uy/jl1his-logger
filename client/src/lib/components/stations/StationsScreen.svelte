<script lang="ts">
    import { goto } from '$app/navigation';
    import type { Station } from '#lib/api/types.ts';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { parks } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import StationEditor from './StationEditor.svelte';

    let { selected }: { selected: string | undefined } = $props();

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const root = `/logbooks/${logbookId}/stations`;
    const canEdit = $derived(logbook.logbook.role === 'owner');
    const archived = $derived(logbook.stations.filter((s) => s.archived_at !== null));
    const current = $derived(
        selected === undefined || selected === 'new' ? undefined : logbook.station(selected)
    );

    async function saved(station: Station) {
        await logbook.loadStations();
        if (selected !== station.id) goto(`${root}/${station.id}`);
    }
</script>

<PageTitle
    title={current === undefined ? m.nav_stations() : m.stations_page_title({ name: current.name })}
/>

{#snippet card(station: Station)}
    <a
        href={`${root}/${station.id}`}
        aria-current={station.id === selected ? 'page' : undefined}
        class={[
            'flex flex-col gap-1.5 rounded-lg border px-3.5 py-3 text-text no-underline',
            station.id === selected
                ? 'border-accent bg-warn-surface shadow-[inset_3px_0_0_var(--accent)]'
                : 'border-border bg-surface'
        ]}
    >
        <span class="flex items-baseline justify-between gap-2">
            <span class="truncate font-semibold">{station.name}</span>
            {#if station.wavelog_id !== null}
                <span class="font-mono text-xs text-muted">#{station.wavelog_id}</span>
            {/if}
        </span>
        <span class="font-mono text-[13px]">{station.callsign}</span>
        <span class="flex flex-wrap gap-1.5 text-xs text-muted">
            {#if station.fields.MY_GRIDSQUARE}
                <span class="font-mono">{station.fields.MY_GRIDSQUARE}</span>
            {/if}
            {#each parks(station.fields) as park (park)}
                <span class="rounded bg-surface-2 px-1.5 font-mono text-text">{park}</span>
            {/each}
        </span>
    </a>
{/snippet}

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header
        class={['flex flex-wrap items-center gap-3', selected !== undefined && 'max-md:hidden']}
    >
        <a
            href={`/logbooks/${logbookId}/more`}
            aria-label={m.settings_back_more()}
            class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text md:hidden"
            ><Icon name="back" size={22} /></a
        >
        <h1 class="text-xl font-semibold">{m.nav_stations()}</h1>
        <span class="text-[13px] text-muted"
            >{m.stations_counts({
                active: logbook.activeStations.length,
                archived: archived.length
            })}</span
        >
        <div class="flex-1"></div>
        {#if canEdit}
            <Button variant="primary" href={`${root}/new`}>{m.stations_new()}</Button>
        {/if}
    </header>

    {#if selected !== undefined}
        <header class="flex items-center gap-2 md:hidden">
            <a
                href={root}
                aria-label={m.stations_back()}
                class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text"
                ><Icon name="back" size={22} /></a
            >
            <h1 class="truncate text-lg font-semibold">
                {selected === 'new' ? m.stations_new() : (current?.name ?? m.field_station())}
            </h1>
        </header>
    {/if}

    <div class="flex flex-wrap items-start gap-4">
        <section
            aria-label={m.stations_list()}
            class={[
                'flex min-w-0 flex-[1_1_280px] flex-col gap-2 md:max-w-[340px]',
                selected !== undefined && 'max-md:hidden'
            ]}
        >
            {#each logbook.activeStations as station (station.id)}
                {@render card(station)}
            {:else}
                <p class="text-[13px] text-muted">{m.stations_no_active()}</p>
            {/each}
            {#if archived.length > 0}
                <h2 class="mt-3 eyebrow">{m.stations_archived()}</h2>
                {#each archived as station (station.id)}
                    <a
                        href={`${root}/${station.id}`}
                        aria-current={station.id === selected ? 'page' : undefined}
                        class={[
                            'flex items-center gap-2 rounded-lg border px-3.5 py-2.5 text-[13px] text-muted no-underline',
                            station.id === selected ? 'border-accent' : 'border-border'
                        ]}
                    >
                        <span class="truncate text-text">{station.name}</span> ·
                        <span class="font-mono">{station.callsign}</span>
                        {#if station.wavelog_id !== null}
                            <span class="font-mono">· #{station.wavelog_id}</span>
                        {/if}
                    </a>
                {/each}
            {/if}
        </section>

        <div class="flex min-w-0 flex-[999_1_480px] flex-col gap-4">
            {#if selected === undefined}
                <p class="hidden rounded-lg border border-border p-6 text-muted md:block">
                    {canEdit ? m.stations_choose_or_create() : m.stations_choose()}
                </p>
            {:else if selected === 'new' && !canEdit}
                <p class="text-muted">{m.stations_owners_create()}</p>
            {:else if selected !== 'new' && current === undefined}
                <p class="text-muted">
                    {logbook.stations.length === 0 ? m.common_loading() : m.stations_not_found()}
                </p>
            {:else}
                {#key selected}
                    <StationEditor {logbookId} station={current} {canEdit} onsaved={saved} />
                {/key}
            {/if}
        </div>
    </div>
</main>
