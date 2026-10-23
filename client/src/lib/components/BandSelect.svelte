<script lang="ts">
    import { BAND_GROUPS, bandNamed } from '#lib/qso/bands.ts';

    let {
        value = $bindable(''),
        id,
        detailed = false,
        class: className = '',
        onchange
    }: {
        value?: string;
        id?: string;
        detailed?: boolean;
        class?: string;
        onchange?: (band: string) => void;
    } = $props();

    const known = $derived(value === '' || bandNamed(value) !== undefined);
</script>

<select
    {id}
    class={['control font-mono', className]}
    value={known ? (bandNamed(value)?.name ?? '') : value}
    onchange={(e) => {
        value = e.currentTarget.value;
        onchange?.(value);
    }}
>
    {#if value === ''}
        <option value="" disabled>Band</option>
    {:else if !known}
        <option {value}>{value} (as recorded)</option>
    {/if}
    {#each BAND_GROUPS as group (group.label)}
        <optgroup label={group.label}>
            {#each group.bands as band (band.name)}
                <option value={band.name}
                    >{detailed ? `${band.name} · ${band.label}` : band.name}</option
                >
            {/each}
        </optgroup>
    {/each}
</select>
