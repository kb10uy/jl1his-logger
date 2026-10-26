<script lang="ts">
    import type { Station } from '#lib/api/types.ts';
    import { splitFavorites } from '#lib/favorite.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { stationLabel } from '#lib/qso/format.ts';

    let {
        id,
        value,
        stations,
        class: className = '',
        onchange
    }: {
        id: string;
        value: string;
        stations: Station[];
        class?: string;
        onchange: (id: string) => void;
    } = $props();

    const groups = $derived(splitFavorites(stations));
</script>

{#snippet options(items: Station[])}
    {#each items as station (station.id)}
        <option value={station.id}>{stationLabel(station)}</option>
    {/each}
{/snippet}

<select
    {id}
    class={['control', className]}
    {value}
    onchange={(e) => onchange(e.currentTarget.value)}
>
    {#if groups.favorites.length === 0}
        {@render options(stations)}
    {:else}
        <optgroup label={m.favorite_group()}>{@render options(groups.favorites)}</optgroup>
        {#if groups.others.length > 0}
            <optgroup label={m.favorite_others()}>{@render options(groups.others)}</optgroup>
        {/if}
    {/if}
</select>
