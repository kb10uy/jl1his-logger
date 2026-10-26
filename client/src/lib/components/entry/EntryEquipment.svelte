<script lang="ts">
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { stationLabel } from '#lib/qso/format.ts';
    import type { EntrySession } from '#lib/state/entry.svelte.ts';

    let { session, oninstrument }: { session: EntrySession; oninstrument?: () => void } = $props();
</script>

<div class="flex items-center gap-2 md:gap-3">
    <label for="station" class="shrink-0 text-xs text-muted max-md:sr-only"
        >{m.field_station()}</label
    >
    <select
        id="station"
        class="control max-w-full min-w-0 bg-bg max-md:flex-[3_1_0]"
        value={session.stationId}
        onchange={(e) => (session.settings.stationId = e.currentTarget.value)}
    >
        {#each session.logbook.activeStations as station (station.id)}
            <option value={station.id}>{stationLabel(station)}</option>
        {/each}
    </select>
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
