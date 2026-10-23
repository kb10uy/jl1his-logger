<script lang="ts">
    import { MODE_CHOICES, MODE_ENTRIES, modeKey, parseModeKey } from '#lib/qso/modes.ts';

    let {
        mode = $bindable(''),
        submode = $bindable(''),
        id,
        class: className = '',
        onchange
    }: {
        mode?: string;
        submode?: string;
        id?: string;
        class?: string;
        onchange?: (mode: string, submode: string) => void;
    } = $props();

    const key = $derived(modeKey(mode, submode));
    const known = $derived(MODE_CHOICES.some((c) => modeKey(c.mode, c.submode) === key));
</script>

<select
    {id}
    class={['control font-mono', className]}
    value={key}
    onchange={(e) => {
        const picked = parseModeKey(e.currentTarget.value);
        mode = picked.mode;
        submode = picked.submode;
        onchange?.(mode, submode);
    }}
>
    {#if !known}
        <option value={key}
            >{[mode, submode].filter((s) => s).join(' ') || 'Mode'} (as recorded)</option
        >
    {/if}
    {#each MODE_ENTRIES as entry, i (i)}
        {#if entry === 'separator'}
            <hr />
        {:else if 'group' in entry}
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
