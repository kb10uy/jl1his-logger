<script lang="ts">
    import type { Fields, Qso } from '#lib/api/types.ts';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { distribution } from '#lib/log/rows.ts';
    import { QSL_RCVD_VALUES, QSL_SENT_VALUES, QSL_VIA_VALUES } from '#lib/qso/qsl.ts';

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

    type Choice = { name: string; label: string; options: [string, string][]; date?: boolean };
    const groups: { legend: string; choices: Choice[] }[] = [
        {
            legend: 'QSL sent',
            choices: [
                { name: 'QSL_SENT', label: 'Sent', options: QSL_SENT_VALUES },
                { name: 'QSL_SENT_VIA', label: 'Via', options: QSL_VIA_VALUES },
                { name: 'QSLSDATE', label: 'Sent date', options: [], date: true }
            ]
        },
        {
            legend: 'QSL received',
            choices: [
                { name: 'QSL_RCVD', label: 'Received', options: QSL_RCVD_VALUES },
                { name: 'QSL_RCVD_VIA', label: 'Via', options: QSL_VIA_VALUES },
                { name: 'QSLRDATE', label: 'Received date', options: [], date: true }
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
    const n = $derived(`${count} QSO${count === 1 ? '' : 's'}`);
</script>

<aside
    aria-label="Bulk edit"
    class="absolute inset-y-0 right-0 z-10 flex w-[380px] max-w-full flex-col gap-3.5 overflow-y-auto border-l border-border bg-surface px-6 pt-4 pb-6 shadow-[-12px_0_32px] shadow-shadow"
>
    <div class="flex items-start justify-between gap-2">
        <div class="flex flex-col gap-0.5">
            <span class="eyebrow">Bulk edit</span>
            <span class="text-xl font-semibold"
                >Edit {count} checked QSO{count === 1 ? '' : 's'}</span
            >
        </div>
        <button
            type="button"
            aria-label="Close bulk edit"
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
                        <option value="">Keep</option>
                        {#if choice.date}
                            <option value="today">Today · {today}</option>
                            <option value="clear">Clear</option>
                        {:else}
                            {#each choice.options as [code, label] (code)}
                                <option value={code}>{code} · {label}</option>
                            {/each}
                            <option value="clear">Clear</option>
                        {/if}
                    </select>
                    <span class="font-mono text-xs text-muted"
                        >now {distribution(known, choice.name)}</span
                    >
                </div>
            {/each}
        </fieldset>
    {/each}

    <div class="flex flex-col gap-1.5 rounded-lg border border-border bg-bg p-3">
        <span class="eyebrow">Changes</span>
        {#each changes as [name, value] (name)}
            <span class="font-mono text-xs">{name} → {value === '' ? '(removed)' : value}</span>
        {:else}
            <span class="text-xs text-muted">Nothing yet.</span>
        {/each}
    </div>

    <div class="flex gap-2">
        <button
            type="button"
            class="h-10 rounded-md border border-border px-3.5"
            onclick={() => (values = {})}>Reset</button
        >
        <button
            type="button"
            disabled={changes.length === 0 || busy}
            class="h-10 flex-1 rounded-md font-bold enabled:bg-accent enabled:text-accent-text disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-muted"
            onclick={() => {
                onapply(fields);
                values = {};
            }}>Apply to {n}</button
        >
    </div>
    <span class="text-[11px] text-muted">
        Each row is sent with its revision. A row someone changed meanwhile is skipped and listed
        afterwards.
    </span>
</aside>
