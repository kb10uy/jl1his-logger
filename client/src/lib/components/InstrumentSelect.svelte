<script lang="ts">
    import type { Instrument } from '#lib/api/types.ts';
    import { splitFavorites } from '#lib/favorite.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        id,
        value,
        instruments,
        class: className = '',
        onchange
    }: {
        id: string;
        value: string;
        instruments: Instrument[];
        class?: string;
        onchange: (id: string) => void;
    } = $props();

    const groups = $derived(splitFavorites(instruments));
</script>

{#snippet options(items: Instrument[])}
    {#each items as instrument (instrument.id)}
        <option value={instrument.id}
            >{instrument.name}{instrument.tx_pwr === null
                ? ''
                : ` · ${instrument.tx_pwr} W`}</option
        >
    {/each}
{/snippet}

<select
    {id}
    class={['control', className]}
    {value}
    onchange={(e) => onchange(e.currentTarget.value)}
>
    <option value="">{m.instrument_none()}</option>
    {#if groups.favorites.length === 0}
        {@render options(instruments)}
    {:else}
        <optgroup label={m.favorite_group()}>{@render options(groups.favorites)}</optgroup>
        {#if groups.others.length > 0}
            <optgroup label={m.favorite_others()}>{@render options(groups.others)}</optgroup>
        {/if}
    {/if}
</select>
