<script lang="ts">
    import type { Area } from '#client/api/types.ts';
    import { extraFieldNames, type Entry } from '#client/entry/record.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        entry = $bindable(),
        area,
        power
    }: { entry: Entry; area: Area; power: string } = $props();
</script>

<div
    id="extra"
    role="group"
    aria-label={m.entry_extra_fields()}
    class="flex flex-wrap items-end gap-2 border-t border-border pt-2"
>
    <div class="flex flex-col gap-1">
        <label for="tx-pwr" class="text-2xs text-muted"
            >{m.entry_tx_power()} <span class="font-mono text-2xs">W</span></label
        >
        <input
            id="tx-pwr"
            inputmode="decimal"
            placeholder={power}
            class="field w-[64px] text-right font-mono"
            bind:value={entry.txPwr}
        />
    </div>
    <div class="flex flex-[1_1_160px] flex-col gap-1">
        <label for="notes" class="text-2xs text-muted">{m.entry_notes()}</label>
        <input id="notes" class="field" bind:value={entry.notes} />
    </div>
    {#each entry.extra as extra, i (i)}
        <div class="flex flex-col gap-1">
            <span class="text-2xs text-muted">{m.entry_adif_field()}</span>
            <div class="flex items-center">
                <input
                    aria-label={m.entry_adif_field_name()}
                    list="adif-fields"
                    placeholder="FIELD"
                    autocomplete="off"
                    spellcheck="false"
                    class="field w-[104px] rounded-r-none border-r-0 bg-surface-2 font-mono text-2xs uppercase"
                    bind:value={extra.name}
                />
                <input
                    aria-label={m.entry_value()}
                    class="field w-[112px] rounded-l-none font-mono"
                    bind:value={extra.value}
                />
                <button
                    type="button"
                    aria-label={m.entry_remove_field()}
                    class="flex h-[32px] w-[28px] items-center justify-center text-muted hover:text-text"
                    onclick={() => entry.extra.splice(i, 1)}
                    ><svg
                        aria-hidden="true"
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg
                    ></button
                >
            </div>
        </div>
    {/each}
    <button
        type="button"
        class="h-[32px] rounded-md border border-dashed border-border px-2.5 text-xs text-muted"
        onclick={() => entry.extra.push({ name: '', value: '' })}>{m.entry_add_adif_field()}</button
    >
    <datalist id="adif-fields">
        {#each extraFieldNames(area) as name (name)}
            <option value={name}></option>
        {/each}
    </datalist>
</div>
