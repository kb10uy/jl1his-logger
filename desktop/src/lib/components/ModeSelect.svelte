<script lang="ts">
    import { MODE_CHOICES, MODE_ENTRIES, modeKey, parseModeKey } from '#client/qso/modes.ts';

    let {
        id,
        mode,
        submode,
        class: className = '',
        onchange
    }: {
        id: string;
        mode: string;
        submode: string;
        class?: string;
        onchange: (mode: string, submode: string) => void;
    } = $props();

    const key = $derived(modeKey(mode, submode));
    const known = $derived(MODE_CHOICES.some((c) => modeKey(c.mode, c.submode) === key));
</script>

<select
    {id}
    class={['field font-mono', className]}
    value={key}
    onchange={(e) => {
        const picked = parseModeKey(e.currentTarget.value);
        onchange(picked.mode, picked.submode);
    }}
>
    {#if !known}
        <option value={key}>{[mode, submode].filter((s) => s).join(' ')}</option>
    {/if}
    {#each MODE_ENTRIES as entry, i (i)}
        {#if 'group' in entry}
            <optgroup label={entry.group}>
                {#each entry.choices as choice (choice.submode)}
                    <option value={modeKey(choice.mode, choice.submode)}>{choice.label}</option>
                {/each}
            </optgroup>
        {:else}
            <option value={modeKey(entry.mode, entry.submode)}>{entry.label}</option>
        {/if}
    {/each}
</select>
