<script lang="ts">
    import { BAND_GROUPS, bandNamed } from '#client/qso/bands.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        id,
        value,
        class: className = '',
        onchange
    }: {
        id: string;
        value: string;
        class?: string;
        onchange: (band: string) => void;
    } = $props();

    const named = $derived(bandNamed(value)?.name);
    const groupLabel = (label: string) => (label === 'SHF and up' ? m.entry_band_shf() : label);
</script>

<select
    {id}
    class={['field font-mono', className]}
    value={named ?? value}
    onchange={(e) => onchange(e.currentTarget.value)}
>
    {#if named === undefined}
        <option {value}>{value}</option>
    {/if}
    {#each BAND_GROUPS as group (group.label)}
        <optgroup label={groupLabel(group.label)}>
            {#each group.bands as band (band.name)}
                <option value={band.name}>{band.name}</option>
            {/each}
        </optgroup>
    {/each}
</select>
