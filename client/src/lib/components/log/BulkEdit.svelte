<script lang="ts">
    import type { BulkChange, Instrument, Qso, Station } from '#lib/api/types.ts';
    import Icon from '#lib/components/ui/Icon.svelte';
    import {
        bulkChange,
        fieldProblems,
        isEmpty,
        KEEP,
        NO_INSTRUMENT,
        type FieldEdit
    } from '#lib/log/bulk.ts';
    import { distribution, tally } from '#lib/log/rows.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { ADIF_QSO_FIELDS } from '#lib/qso/adif-fields.ts';
    import { adifDate } from '#lib/qso/format.ts';
    import {
        QSL_RCVD_VALUES,
        QSL_SENT_VALUES,
        QSL_VIA_VALUES,
        type QslChoice
    } from '#lib/qso/qsl.ts';

    let {
        count,
        known,
        today,
        busy,
        stations,
        instruments,
        onapply,
        onclose
    }: {
        count: number;
        known: Qso[];
        today: string;
        busy: boolean;
        stations: Station[];
        instruments: Instrument[];
        onapply: (change: BulkChange) => void;
        onclose: () => void;
    } = $props();

    type Choice = { name: string; label: string; options: QslChoice[]; date?: boolean };
    const groups: { legend: string; choices: Choice[] }[] = [
        {
            legend: m.field_qsl_sent(),
            choices: [
                { name: 'QSL_SENT', label: m.log_sent(), options: QSL_SENT_VALUES },
                { name: 'QSL_SENT_VIA', label: m.log_via(), options: QSL_VIA_VALUES },
                { name: 'QSLSDATE', label: m.log_sent_date(), options: [], date: true }
            ]
        },
        {
            legend: m.log_qsl_received(),
            choices: [
                { name: 'QSL_RCVD', label: m.log_received(), options: QSL_RCVD_VALUES },
                { name: 'QSL_RCVD_VIA', label: m.log_via(), options: QSL_VIA_VALUES },
                { name: 'QSLRDATE', label: m.log_received_date(), options: [], date: true }
            ]
        }
    ];

    let values = $state<Record<string, string>>({});
    let edits = $state<FieldEdit[]>([]);
    let station = $state(KEEP);
    let instrument = $state(KEEP);

    const stationName = (id: string | null | undefined) =>
        stations.find((s) => s.id === id)?.name ?? id ?? undefined;
    const instrumentName = (id: string | null | undefined) =>
        id === null ? m.common_none() : (instruments.find((i) => i.id === id)?.name ?? id);

    const fixed = $derived(
        Object.fromEntries(
            Object.entries(values)
                .filter(([, v]) => v !== '')
                .map(([name, v]) => [
                    name,
                    v === 'today' ? adifDate(today) : v === 'clear' ? '' : v
                ])
        )
    );
    const problems = $derived(fieldProblems(fixed, edits));
    const change = $derived(bulkChange(fixed, edits, station, instrument));
    const summary = $derived([
        ...(change.station_id === undefined
            ? []
            : [[m.field_station(), stationName(change.station_id) ?? ''] as const]),
        ...(change.instrument_id === undefined
            ? []
            : [[m.field_instrument(), instrumentName(change.instrument_id) ?? ''] as const]),
        ...Object.entries(change.fields ?? {})
    ]);
    const blocked = $derived(problems.some((p) => p !== undefined));

    function reset() {
        values = {};
        edits = [];
        station = KEEP;
        instrument = KEEP;
    }
</script>

{#snippet now(text: string)}
    <span class="font-mono text-xs break-words text-muted">{m.log_bulk_now({ values: text })}</span>
{/snippet}

<aside
    aria-label={m.log_bulk_edit()}
    class="absolute inset-y-0 right-0 z-10 flex w-[380px] max-w-full flex-col gap-3.5 overflow-y-auto border-l border-border bg-surface px-6 pt-4 pb-6 shadow-[-12px_0_32px] shadow-shadow"
>
    <div class="flex items-start justify-between gap-2">
        <div class="flex flex-col gap-0.5">
            <span class="eyebrow">{m.log_bulk_edit()}</span>
            <span class="text-xl font-semibold">{m.log_bulk_title({ count })}</span>
        </div>
        <button
            type="button"
            aria-label={m.log_bulk_close()}
            class="inline-flex size-8 items-center justify-center rounded-md border border-border text-muted"
            onclick={onclose}
        >
            <Icon name="close" size={14} strokeWidth={2.5} />
        </button>
    </div>

    <fieldset class="m-0 flex flex-col gap-2.5 rounded-lg border border-border p-3">
        <legend class="px-1.5 eyebrow">{m.log_bulk_place()}</legend>
        <div class="flex flex-col gap-1">
            <label for="bulk-station" class="text-xs text-muted">{m.field_station()}</label>
            <select id="bulk-station" class="control" bind:value={station}>
                <option value={KEEP}>{m.log_bulk_keep()}</option>
                {#each stations.filter((s) => s.archived_at === null) as s (s.id)}
                    <option value={s.id}>{s.name} · {s.callsign}</option>
                {/each}
            </select>
            {@render now(tally(known.map((q) => stationName(q.station_id))))}
        </div>
        <div class="flex flex-col gap-1">
            <label for="bulk-instrument" class="text-xs text-muted">{m.field_instrument()}</label>
            <select id="bulk-instrument" class="control" bind:value={instrument}>
                <option value={KEEP}>{m.log_bulk_keep()}</option>
                <option value={NO_INSTRUMENT}>{m.common_none()}</option>
                {#each instruments.filter((i) => i.archived_at === null) as i (i.id)}
                    <option value={i.id}>{i.name}</option>
                {/each}
            </select>
            {@render now(tally(known.map((q) => instrumentName(q.instrument_id))))}
        </div>
        <span class="text-[11px] text-muted">{m.log_bulk_place_help()}</span>
    </fieldset>

    {#each groups as group (group.legend)}
        <fieldset class="m-0 flex flex-col gap-2.5 rounded-lg border border-border p-3">
            <legend class="px-1.5 eyebrow">{group.legend}</legend>
            {#each group.choices as choice (choice.name)}
                <div class="flex flex-col gap-1">
                    <label for={`bulk-${choice.name}`} class="flex flex-col text-xs text-muted">
                        <span>{choice.label}</span>
                        <span class="font-mono text-[10px] opacity-75">{choice.name}</span>
                    </label>
                    <select
                        id={`bulk-${choice.name}`}
                        class="control"
                        value={values[choice.name] ?? ''}
                        onchange={(e) => (values[choice.name] = e.currentTarget.value)}
                    >
                        <option value="">{m.log_bulk_keep()}</option>
                        {#if choice.date}
                            <option value="today">{m.log_bulk_today({ date: today })}</option>
                            <option value="clear">{m.common_clear()}</option>
                        {:else}
                            {#each choice.options as [code, label] (code)}
                                <option value={code}>{code} · {label()}</option>
                            {/each}
                            <option value="clear">{m.common_clear()}</option>
                        {/if}
                    </select>
                    {@render now(distribution(known, choice.name))}
                </div>
            {/each}
        </fieldset>
    {/each}

    <fieldset class="m-0 flex flex-col gap-2.5 rounded-lg border border-border p-3">
        <legend class="px-1.5 eyebrow">{m.log_bulk_other_fields()}</legend>
        {#each edits as edit, i (i)}
            {@const name = edit.name.trim().toUpperCase()}
            <div class="flex flex-col gap-1">
                <div class="flex items-center gap-2">
                    <input
                        aria-label={m.log_bulk_field_name()}
                        aria-invalid={problems[i] !== undefined}
                        list="bulk-adif-fields"
                        placeholder="FIELD_NAME"
                        autocomplete="off"
                        autocapitalize="characters"
                        spellcheck="false"
                        class="control h-9 min-w-0 flex-1 font-mono text-xs uppercase"
                        bind:value={edit.name}
                    />
                    <button
                        type="button"
                        aria-label={m.log_bulk_remove_field()}
                        class="inline-flex size-9 shrink-0 items-center justify-center rounded-md text-muted hover:text-text"
                        onclick={() => (edits = edits.filter((_, j) => j !== i))}
                        ><Icon name="close" size={16} /></button
                    >
                </div>
                <div class="flex items-center gap-2">
                    <input
                        aria-label={m.log_bulk_field_value({ name: name || 'FIELD_NAME' })}
                        disabled={edit.clear}
                        class="control h-9 min-w-0 flex-1 disabled:opacity-50"
                        bind:value={edit.value}
                    />
                    <label class="flex shrink-0 items-center gap-1.5 text-xs">
                        <input type="checkbox" class="accent-info" bind:checked={edit.clear} />
                        {m.log_bulk_clear_field()}
                    </label>
                </div>
                {#if problems[i] === 'name'}
                    <span role="alert" class="text-xs text-error-text">{m.log_bulk_bad_name()}</span
                    >
                {:else if problems[i] === 'repeated'}
                    <span role="alert" class="text-xs text-error-text">{m.log_bulk_repeated()}</span
                    >
                {:else if name !== ''}
                    {@render now(distribution(known, name))}
                {/if}
            </div>
        {/each}
        <button
            type="button"
            class="inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-dashed border-border text-[13px] text-muted hover:text-text"
            onclick={() => edits.push({ name: '', value: '', clear: false })}
        >
            <Icon name="plus" size={14} />
            {m.log_bulk_add_field()}
        </button>
        <datalist id="bulk-adif-fields">
            {#each ADIF_QSO_FIELDS as field (field)}
                <option value={field}></option>
            {/each}
        </datalist>
    </fieldset>

    <div class="flex flex-col gap-1.5 rounded-lg border border-border bg-bg p-3">
        <span class="eyebrow">{m.log_bulk_changes()}</span>
        {#each summary as [name, value] (name)}
            <span class="font-mono text-xs break-words"
                >{name} → {value === '' ? m.log_bulk_removed() : value}</span
            >
        {:else}
            <span class="text-xs text-muted">{m.log_bulk_nothing()}</span>
        {/each}
    </div>

    <div class="flex gap-2">
        <button type="button" class="h-10 rounded-md border border-border px-3.5" onclick={reset}
            >{m.log_reset()}</button
        >
        <button
            type="button"
            disabled={isEmpty(change) || blocked || busy}
            class="h-10 flex-1 rounded-md font-bold enabled:bg-accent enabled:text-accent-text disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-muted"
            onclick={() => {
                onapply(change);
                reset();
            }}>{m.log_bulk_apply({ count })}</button
        >
    </div>
    <span class="text-[11px] text-muted">
        {m.log_bulk_help()}
    </span>
</aside>
