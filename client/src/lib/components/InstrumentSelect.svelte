<script lang="ts">
    import type { Instrument } from '#lib/api/types.ts';
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
</script>

<select
    {id}
    class={['control', className]}
    {value}
    onchange={(e) => onchange(e.currentTarget.value)}
>
    <option value="">{m.instrument_none()}</option>
    {#each instruments as instrument (instrument.id)}
        <option value={instrument.id}
            >{instrument.name}{instrument.tx_pwr === null
                ? ''
                : ` · ${instrument.tx_pwr} W`}</option
        >
    {/each}
</select>
