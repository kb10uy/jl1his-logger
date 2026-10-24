<script lang="ts">
    import type { Fields, Qso } from '#lib/api/types.ts';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { distribution } from '#lib/log/rows.ts';
    import { m } from '#lib/paraglide/messages.js';
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
        onapply,
        onclose
    }: {
        count: number;
        known: Qso[];
        today: string;
        busy: boolean;
        onapply: (fields: Fields) => void;
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

    const changes = $derived(
        Object.entries(values)
            .filter(([, v]) => v !== '')
            .map(([name, v]) => [name, v === 'today' ? today : v === 'clear' ? '' : v] as const)
    );
    const fields = $derived(Object.fromEntries(changes));
</script>

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
                    <span class="font-mono text-xs text-muted"
                        >{m.log_bulk_now({ values: distribution(known, choice.name) })}</span
                    >
                </div>
            {/each}
        </fieldset>
    {/each}

    <div class="flex flex-col gap-1.5 rounded-lg border border-border bg-bg p-3">
        <span class="eyebrow">{m.log_bulk_changes()}</span>
        {#each changes as [name, value] (name)}
            <span class="font-mono text-xs"
                >{name} → {value === '' ? m.log_bulk_removed() : value}</span
            >
        {:else}
            <span class="text-xs text-muted">{m.log_bulk_nothing()}</span>
        {/each}
    </div>

    <div class="flex gap-2">
        <button
            type="button"
            class="h-10 rounded-md border border-border px-3.5"
            onclick={() => (values = {})}>{m.log_reset()}</button
        >
        <button
            type="button"
            disabled={changes.length === 0 || busy}
            class="h-10 flex-1 rounded-md font-bold enabled:bg-accent enabled:text-accent-text disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-muted"
            onclick={() => {
                onapply(fields);
                values = {};
            }}>{m.log_bulk_apply({ count })}</button
        >
    </div>
    <span class="text-[11px] text-muted">
        {m.log_bulk_help()}
    </span>
</aside>
