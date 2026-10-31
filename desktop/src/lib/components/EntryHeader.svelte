<script lang="ts">
    import type { Instrument, Station } from '#client/api/types.ts';
    import { splitFavorites } from '#client/favorite.ts';
    import { stationLabel } from '#client/qso/format.ts';
    import type { EntryState } from '#lib/entry/entry.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';

    let { desk, oninstrument }: { desk: EntryState; oninstrument: () => void } = $props();

    const single = $derived(desk.groups.length === 1);
    const instruments = $derived(splitFavorites(desk.activeInstruments));
</script>

{#snippet stationOptions(stations: Station[])}
    {#each stations as station (station.id)}
        <option value={station.id}>{stationLabel(station)}</option>
    {/each}
{/snippet}

{#snippet instrumentOptions(items: Instrument[])}
    {#each items as instrument (instrument.id)}
        <option value={instrument.id}
            >{instrument.name}{instrument.tx_pwr === null
                ? ''
                : ` · ${instrument.tx_pwr} W`}</option
        >
    {/each}
{/snippet}

<header class="flex h-[32px] flex-none items-center gap-1.5">
    <label for="station" class="text-2xs text-muted">{m.entry_station()}</label>
    {#if desk.groups.length === 0}
        <span class="text-xs text-warn">{m.entry_no_station()}</span>
    {:else}
        <select
            id="station"
            class="field h-8 w-[250px] bg-surface px-1.5 text-xs"
            value={desk.stationId}
            onchange={(e) => (desk.settings.stationId = e.currentTarget.value)}
        >
            {#if single}
                {@const split = splitFavorites(desk.groups[0].stations)}
                {#if split.favorites.length === 0}
                    {@render stationOptions(split.others)}
                {:else}
                    <optgroup label={m.entry_favorites()}
                        >{@render stationOptions(split.favorites)}</optgroup
                    >
                    {#if split.others.length > 0}
                        <optgroup label={m.entry_others()}
                            >{@render stationOptions(split.others)}</optgroup
                        >
                    {/if}
                {/if}
            {:else}
                {#each desk.groups as group (group.logbook.id)}
                    <optgroup label={group.logbook.name}
                        >{@render stationOptions(group.stations)}</optgroup
                    >
                {/each}
            {/if}
        </select>
    {/if}
    <label for="instrument" class="ml-2 text-2xs text-muted">{m.entry_instrument()}</label>
    <select
        id="instrument"
        class="field h-8 w-[200px] bg-surface px-1.5 text-xs"
        value={desk.instrumentId}
        onchange={(e) => {
            desk.settings.instrumentId = e.currentTarget.value;
            oninstrument();
        }}
    >
        <option value="">{m.entry_no_instrument()}</option>
        {#if instruments.favorites.length === 0}
            {@render instrumentOptions(instruments.others)}
        {:else}
            <optgroup label={m.entry_favorites()}
                >{@render instrumentOptions(instruments.favorites)}</optgroup
            >
            {#if instruments.others.length > 0}
                <optgroup label={m.entry_others()}
                    >{@render instrumentOptions(instruments.others)}</optgroup
                >
            {/if}
        {/if}
    </select>
    <div class="flex-1"></div>
    <button
        type="button"
        aria-pressed={desk.settings.contest.enabled}
        class={[
            'h-8 rounded-md border px-2.5 text-xs',
            desk.settings.contest.enabled
                ? 'border-info-strong bg-selected font-semibold text-info'
                : 'border-border bg-surface'
        ]}
        onclick={() => (desk.settings.contest.enabled = !desk.settings.contest.enabled)}
        >{m.entry_contest()}</button
    >
</header>
