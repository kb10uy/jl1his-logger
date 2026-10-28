<script lang="ts">
    import { Popover } from 'bits-ui';
    import Icon from '#lib/components/ui/Icon.svelte';
    import {
        BUILT_IN_COLUMNS,
        DEFAULT_COLUMNS,
        adifColumnId,
        adifColumnProblem,
        column,
        moveColumn
    } from '#lib/log/columns.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { ADIF_QSO_FIELDS } from '#lib/qso/adif-fields.ts';

    let {
        columns,
        onchange
    }: { columns: readonly string[]; onchange: (columns: string[]) => void } = $props();

    let field = $state('');
    let problem = $state<string | undefined>(undefined);
    let dragging = $state<number | undefined>(undefined);
    let over = $state<number | undefined>(undefined);

    const hidden = $derived(BUILT_IN_COLUMNS.filter((c) => !columns.includes(c.id)));
    const isDefault = $derived(
        columns.length === DEFAULT_COLUMNS.length &&
            columns.every((id, i) => id === DEFAULT_COLUMNS[i])
    );

    const move = (from: number, to: number) => onchange(moveColumn(columns, from, to));
    const hide = (i: number) => onchange(columns.filter((_, j) => j !== i));
    const show = (id: string) => onchange([...columns, id]);

    function addField() {
        const name = field.trim().toUpperCase();
        problem = adifColumnProblem(name);
        if (problem !== undefined) return;
        const id = adifColumnId(name);
        if (!columns.includes(id)) show(id);
        field = '';
    }

    function drop(to: number) {
        if (dragging !== undefined) move(dragging, to);
        dragging = undefined;
        over = undefined;
    }

    const iconButton =
        'inline-flex size-7 items-center justify-center rounded bg-none text-muted hover:bg-surface-2 hover:text-text disabled:pointer-events-none disabled:opacity-30';
</script>

<Popover.Root>
    <Popover.Trigger
        class="inline-flex h-9 shrink-0 items-center justify-center gap-2 rounded-md border border-border bg-surface-2 px-3.5 font-medium whitespace-nowrap text-text"
    >
        <Icon name="columns" size={16} />{m.log_columns()}
    </Popover.Trigger>
    <Popover.Portal>
        <Popover.Content
            align="end"
            sideOffset={6}
            class="z-40 flex max-h-[min(640px,calc(100dvh-96px))] w-[300px] flex-col gap-3 overflow-y-auto rounded-lg border border-border bg-surface p-3 text-[13px] shadow-lg shadow-shadow"
        >
            <div class="flex items-center justify-between">
                <h2 class="text-sm font-semibold">{m.log_columns()}</h2>
                <button
                    type="button"
                    class="h-7 px-1 text-info disabled:text-muted"
                    disabled={isDefault}
                    onclick={() => onchange([...DEFAULT_COLUMNS])}>{m.log_reset()}</button
                >
            </div>

            <ol aria-label={m.log_columns_shown()} class="flex flex-col">
                {#each columns as id, i (id)}
                    <li
                        draggable="true"
                        ondragstart={(e) => {
                            dragging = i;
                            e.dataTransfer?.setData('text/plain', id);
                            if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
                        }}
                        ondragover={(e) => {
                            if (dragging === undefined) return;
                            e.preventDefault();
                            over = i;
                        }}
                        ondrop={(e) => {
                            e.preventDefault();
                            drop(i);
                        }}
                        ondragend={() => {
                            dragging = undefined;
                            over = undefined;
                        }}
                        class={[
                            'flex h-9 items-center gap-1 rounded-md pr-1 pl-1.5',
                            dragging === i ? 'opacity-40' : 'hover:bg-surface-2/60',
                            over === i && dragging !== i && 'bg-selected'
                        ]}
                    >
                        <Icon
                            name="grip"
                            size={16}
                            strokeWidth={3}
                            class="cursor-grab text-muted"
                        />
                        <span
                            class={[
                                'min-w-0 flex-1 truncate',
                                id.startsWith('adif.') && 'font-mono text-xs'
                            ]}>{column(id)?.label ?? id}</span
                        >
                        <button
                            type="button"
                            aria-label={m.log_column_move_left({ name: column(id)?.label ?? id })}
                            class={iconButton}
                            disabled={i === 0}
                            onclick={() => move(i, i - 1)}
                            ><Icon name="chevron" size={14} class="rotate-180" /></button
                        >
                        <button
                            type="button"
                            aria-label={m.log_column_move_right({ name: column(id)?.label ?? id })}
                            class={iconButton}
                            disabled={i === columns.length - 1}
                            onclick={() => move(i, i + 1)}><Icon name="chevron" size={14} /></button
                        >
                        <button
                            type="button"
                            aria-label={m.log_column_hide({ name: column(id)?.label ?? id })}
                            class={iconButton}
                            disabled={columns.length === 1}
                            onclick={() => hide(i)}><Icon name="close" size={14} /></button
                        >
                    </li>
                {/each}
            </ol>

            {#if hidden.length > 0}
                <div class="flex flex-wrap gap-1.5 border-t border-border pt-3">
                    {#each hidden as c (c.id)}
                        <button
                            type="button"
                            class="h-7 rounded-md border border-dashed border-border px-2 text-muted hover:text-text"
                            onclick={() => show(c.id)}>+ {c.label}</button
                        >
                    {/each}
                </div>
            {/if}

            <form
                class="flex flex-col gap-1.5 border-t border-border pt-3"
                onsubmit={(e) => {
                    e.preventDefault();
                    addField();
                }}
            >
                <label for="column-field" class="text-xs text-muted">{m.log_adif_field()}</label>
                <div class="flex gap-2">
                    <input
                        id="column-field"
                        list="column-fields"
                        placeholder="TX_PWR"
                        autocomplete="off"
                        autocapitalize="characters"
                        spellcheck="false"
                        aria-invalid={problem !== undefined}
                        class="control h-9 min-w-0 flex-1 rounded-md px-2 font-mono text-xs uppercase"
                        bind:value={field}
                        oninput={() => (problem = undefined)}
                    />
                    <button
                        type="submit"
                        class="h-9 rounded-md border border-border bg-surface-2 px-3 font-medium"
                        >{m.common_add()}</button
                    >
                </div>
                {#if problem}
                    <p role="alert" class="text-xs text-error-text">{problem}</p>
                {/if}
                <datalist id="column-fields">
                    {#each ADIF_QSO_FIELDS as name (name)}
                        <option value={name}></option>
                    {/each}
                </datalist>
            </form>
        </Popover.Content>
    </Popover.Portal>
</Popover.Root>
