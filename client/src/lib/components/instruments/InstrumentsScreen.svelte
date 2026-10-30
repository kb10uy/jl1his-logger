<script lang="ts">
    import * as api from '#lib/api/api.ts';
    import ChooserScreen from '#lib/components/ChooserScreen.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import InstrumentEditor from './InstrumentEditor.svelte';

    let { selected }: { selected: string | undefined } = $props();

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const canRewrite = $derived(logbook.logbook.role === 'owner');
</script>

<ChooserScreen
    segment="instruments"
    {selected}
    items={logbook.instruments}
    canCreate
    words={{
        nav: m.nav_instruments(),
        item: m.field_instrument(),
        title: (name) => m.stations_instrument_page_title({ name }),
        new: m.stations_instrument_new(),
        back: m.stations_instrument_back(),
        list: m.stations_instrument_list(),
        counts: (active, archived) => m.stations_counts({ active, archived }),
        filter: m.stations_instrument_filter_placeholder(),
        empty: m.stations_instrument_no_active(),
        choose: m.stations_instrument_choose(),
        cannotCreate: '',
        notFound: m.stations_instrument_not_found()
    }}
    detail={(instrument) => (instrument.tx_pwr === null ? '' : `${instrument.tx_pwr} W`)}
    search={(instrument) => [
        instrument.name,
        instrument.my_rig,
        instrument.my_antenna,
        instrument.tx_pwr
    ]}
    favorite={(id, favorite) => api.favoriteInstrument(logbookId, id, favorite)}
    reload={() => logbook.loadInstruments()}
>
    {#snippet editor(instrument, saved)}
        <InstrumentEditor {logbookId} {instrument} {canRewrite} onsaved={saved} />
    {/snippet}
</ChooserScreen>
