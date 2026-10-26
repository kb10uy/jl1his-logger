<script lang="ts">
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import StationSelect from '#lib/components/StationSelect.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import type { EntrySession } from '#lib/state/entry.svelte.ts';

    let { session, oninstrument }: { session: EntrySession; oninstrument?: () => void } = $props();
</script>

<div class="flex items-center gap-2 md:gap-3">
    <label for="station" class="shrink-0 text-xs text-muted max-md:sr-only"
        >{m.field_station()}</label
    >
    <StationSelect
        id="station"
        class="max-w-full min-w-0 bg-bg max-md:flex-[3_1_0]"
        value={session.stationId}
        stations={session.logbook.activeStations}
        onchange={(id) => (session.settings.stationId = id)}
    />
    <label for="instrument" class="shrink-0 text-xs text-muted max-md:sr-only"
        >{m.field_instrument()}</label
    >
    <InstrumentSelect
        id="instrument"
        class="max-w-full min-w-0 bg-bg max-md:flex-[2_1_0]"
        value={session.instrumentId}
        instruments={session.logbook.activeInstruments}
        onchange={(id) => {
            session.settings.instrumentId = id;
            oninstrument?.();
        }}
    />
</div>
