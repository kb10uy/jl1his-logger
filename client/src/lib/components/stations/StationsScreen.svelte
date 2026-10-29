<script lang="ts">
    import * as api from '#lib/api/api.ts';
    import ChooserScreen from '#lib/components/ChooserScreen.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { parks } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import StationEditor from './StationEditor.svelte';

    let { selected }: { selected: string | undefined } = $props();

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const canEdit = $derived(logbook.logbook.role === 'owner');
</script>

<ChooserScreen
    segment="stations"
    {selected}
    items={logbook.stations}
    canCreate={canEdit}
    words={{
        nav: m.nav_stations(),
        item: m.field_station(),
        title: (name) => m.stations_page_title({ name }),
        new: m.stations_new(),
        back: m.stations_back(),
        list: m.stations_list(),
        counts: (active, archived) => m.stations_counts({ active, archived }),
        filter: m.stations_filter_placeholder(),
        empty: m.stations_no_active(),
        choose: canEdit ? m.stations_choose_or_create() : m.stations_choose(),
        cannotCreate: m.stations_owners_create(),
        notFound: m.stations_not_found()
    }}
    detail={(station) => station.callsign}
    search={(station) => [
        station.name,
        station.callsign,
        station.fields.MY_GRIDSQUARE,
        station.fields.MY_CITY,
        ...parks(station.fields)
    ]}
    favorite={(id, favorite) => api.favoriteStation(logbookId, id, favorite)}
    reload={() => logbook.loadStations()}
>
    {#snippet editor(station, saved)}
        <StationEditor {logbookId} {station} {canEdit} onsaved={saved} />
    {/snippet}
</ChooserScreen>
