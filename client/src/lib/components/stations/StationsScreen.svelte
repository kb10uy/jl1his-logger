<script lang="ts">
    import { MediaQuery } from 'svelte/reactivity';
    import { afterNavigate, goto } from '$app/navigation';
    import * as api from '#lib/api/api.ts';
    import type { Station } from '#lib/api/types.ts';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import PickList from '#lib/components/PickList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { parks } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
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

    const desktop = new MediaQuery('min-width: 768px');
    const detail = $derived(selected !== undefined && !desktop.current);

    let fromList = $state(false);
    afterNavigate(({ from }) => {
        fromList = from?.url.pathname === root;
    });

    function back(e: MouseEvent) {
        if (!fromList) return;
        e.preventDefault();
        history.back();
    }

    async function favorite(station: Station, favorite: boolean) {
        try {
            await api.favoriteStation(logbookId, station.id, favorite);
            await logbook.loadStations();
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        }
    }

    async function saved(station: Station) {
        await logbook.loadStations();
        if (selected !== station.id) goto(`${root}/${station.id}`);
    }
</script>

<PageTitle
    title={current === undefined ? m.nav_stations() : m.stations_page_title({ name: current.name })}
/>

<PageHeader
    title={detail
        ? selected === 'new'
            ? m.stations_new()
            : (current?.name ?? m.field_station())
        : m.nav_stations()}
    subtitle={detail
        ? `${logbook.logbook.name} · ${m.nav_stations()}`
        : `${logbook.logbook.name} · ${m.stations_counts({
              active: logbook.activeStations.length,
              archived: archived.length
          })}`}
    back={detail
        ? { href: root, onclick: back, label: m.stations_back() }
        : { href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
>
    {#snippet actions()}
        {#if canEdit && (selected === undefined || desktop.current)}
            <Button variant="primary" href={`${root}/new`} class="max-md:w-9 max-md:px-0"
                ><Icon name="plus" /><span class="max-md:sr-only">{m.stations_new()}</span></Button
            >
        {/if}
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <LogbookLoadError />
    <div class="grid items-start gap-4 md:grid-cols-[minmax(240px,320px)_minmax(0,1fr)]">
        <PickList
            label={m.stations_list()}
            items={logbook.stations}
            {selected}
            onfavorite={favorite}
            href={(station) => `${root}/${station.id}`}
            detail={(station) => station.callsign}
            search={(station) => [
                station.name,
                station.callsign,
                station.fields.MY_GRIDSQUARE,
                station.fields.MY_CITY,
                ...parks(station.fields)
            ]}
            placeholder={m.stations_filter_placeholder()}
            empty={m.stations_no_active()}
            class={selected !== undefined ? 'max-md:hidden' : ''}
        />

        <div class="flex min-w-0 flex-col gap-4">
            {#if selected === undefined}
                <p class="hidden rounded-lg border border-border p-6 text-muted md:block">
                    {canEdit ? m.stations_choose_or_create() : m.stations_choose()}
                </p>
            {:else if selected === 'new' && !canEdit}
                <p class="text-muted">{m.stations_owners_create()}</p>
            {:else if selected !== 'new' && current === undefined}
                <p class="text-muted">
                    {logbook.loaded ? m.stations_not_found() : m.common_loading()}
                </p>
            {:else}
                {#key selected}
                    <StationEditor {logbookId} station={current} {canEdit} onsaved={saved} />
                {/key}
            {/if}
        </div>
    </div>
</main>
