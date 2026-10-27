<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';
    import { dxccEntity, searchDxcc } from '#lib/station/dxcc.ts';

    let { id, value = $bindable() }: { id: string; value: string } = $props();

    let query = $state<string | undefined>(undefined);
    let active = $state(0);

    const matches = $derived(query === undefined ? [] : searchDxcc(query));
    const open = $derived(matches.length > 0);
    const entity = $derived(dxccEntity(value ?? ''));

    function input(text: string) {
        if (/^\s*\d*\s*$/.test(text)) {
            value = text;
            query = undefined;
        } else {
            query = text;
            active = 0;
        }
    }

    function choose(code: number) {
        value = String(code);
        query = undefined;
    }

    function keydown(e: KeyboardEvent) {
        if (e.key === 'Escape' && query !== undefined) {
            e.preventDefault();
            query = undefined;
        }
        if (!open) return;
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            const step = e.key === 'ArrowDown' ? 1 : -1;
            active = (active + step + matches.length) % matches.length;
        } else if (e.key === 'Enter') {
            e.preventDefault();
            const chosen = matches[active];
            if (chosen) choose(chosen.code);
        }
    }
</script>

<div class="flex min-w-0 flex-col gap-1">
    <div class="relative">
        <input
            {id}
            role="combobox"
            aria-expanded={open}
            aria-controls={`${id}-list`}
            aria-autocomplete="list"
            aria-activedescendant={open ? `${id}-option-${active}` : undefined}
            placeholder={m.stations_dxcc_placeholder()}
            class="control h-10 w-full font-mono"
            autocomplete="off"
            spellcheck="false"
            value={query ?? value}
            oninput={(e) => input(e.currentTarget.value)}
            onkeydown={keydown}
            onblur={() => (query = undefined)}
        />
        {#if open}
            <ul
                id={`${id}-list`}
                role="listbox"
                class="absolute inset-x-0 top-full z-40 mt-1 flex max-h-72 flex-col overflow-y-auto rounded-lg border border-border bg-surface p-1 shadow-lg shadow-shadow"
            >
                {#each matches as match, i (match.code)}
                    <li
                        id={`${id}-option-${i}`}
                        role="option"
                        aria-selected={i === active}
                        class={[
                            'flex cursor-pointer items-baseline gap-3 rounded-md px-2.5 py-1.5 text-[13px]',
                            i === active && 'bg-surface-2'
                        ]}
                        onpointerdown={(e) => {
                            e.preventDefault();
                            choose(match.code);
                        }}
                        onpointermove={() => (active = i)}
                    >
                        <span class="w-9 shrink-0 text-right font-mono text-muted tabular"
                            >{match.code}</span
                        >
                        <span class="min-w-0 truncate">{match.name}</span>
                    </li>
                {/each}
            </ul>
        {/if}
    </div>
    <span class="text-xs text-muted">
        {#if entity}
            <span class="text-text">{entity.name}</span>{entity.deleted
                ? ` (${m.stations_dxcc_deleted()})`
                : ''} ·
        {:else if (value ?? '').trim() !== ''}
            <span class="text-warn">{m.stations_dxcc_unknown()}</span> ·
        {/if}
        {m.stations_state_follows()}
    </span>
</div>
