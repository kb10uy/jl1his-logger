<script lang="ts">
    import { onMount } from 'svelte';
    import type { Batch, Encoding, ExportKind, LengthUnit, Unencodable } from '#lib/api/types.ts';
    import ExportForm from '#lib/components/transfer/ExportForm.svelte';
    import ExportList from '#lib/components/transfer/ExportList.svelte';
    import Segmented from '#lib/components/ui/Segmented.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import { transferState } from '#lib/state/transfer.svelte.ts';
    import {
        ENCODING_NAMES,
        KIND_LABELS,
        LENGTH_LABELS,
        QUERY_PRESETS,
        SENT_FIELDS,
        SPLIT_FIELDS,
        defaultSplit,
        exportRuns,
        missingCount,
        partitionRuns,
        splitChoices,
        splitLabel
    } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const transfer = transferState(logbook);
    const draft = transfer.exportDraft;
    const batches = transfer.batches;
    const matches = new Matches(logbook.logbook.id, () => draft.q);
    const fields = $derived(SENT_FIELDS[draft.kind]);
    const lists = $derived(partitionRuns(exportRuns(batches.items)));

    const actions = {
        onapply: async (batch: Batch) => {
            if (await batches.apply(batch)) matches.refresh();
        },
        ondismiss: (batch: Batch) => batches.dismiss(batch),
        onrestore: (batch: Batch) => batches.restore(batch),
        onapplyall: async (run: { id: string }) => {
            if (await batches.applyExport(run.id)) matches.refresh();
        },
        ondismissall: (run: { id: string }) => batches.dismissExport(run.id)
    };

    onMount(() => batches.load());

    function pick(next: ExportKind) {
        if (draft.q === QUERY_PRESETS[draft.kind]) draft.q = QUERY_PRESETS[next];
        if (draft.split === defaultSplit(draft.kind) || !splitChoices(next).includes(draft.split)) {
            draft.split = defaultSplit(next);
        }
        draft.kind = next;
    }

    async function runExport() {
        const req = {
            kind: draft.kind,
            q: draft.q.trim() || undefined,
            park:
                draft.kind === 'pota' && draft.park.trim() !== ''
                    ? draft.park.trim().toUpperCase()
                    : undefined,
            length: draft.encoding === 'utf-8' ? draft.length : undefined,
            encoding: draft.encoding,
            unencodable: draft.encoding === 'utf-8' ? undefined : draft.unencodable
        };
        if (draft.split !== 'none') {
            const result = await batches.createSplit(req, SPLIT_FIELDS[draft.split]);
            if (result === undefined) return;
            if (result.format === null) {
                toasts.show(m.transfer_export_nothing());
                return;
            }
            toasts.show(
                m.transfer_exported_files({ records: result.records, files: result.files.length })
            );
            const missing = missingCount(result.files);
            if (missing > 0) toasts.show(m.transfer_exported_missing({ count: missing }));
            return;
        }
        const result = await batches.create(req);
        if (result === undefined) return;
        toasts.show(
            result.batch === null
                ? m.transfer_export_nothing()
                : m.transfer_exported({ count: result.records })
        );
    }

    const kinds: ExportKind[] = ['adif', 'pota', 'qsl_card', 'eqsl', 'lotw'];
    const units: LengthUnit[] = ['codepoints', 'bytes', 'utf16'];
    const lengths = units.map((value) => ({ value, label: LENGTH_LABELS[value]() }));
    const encodings = Object.entries(ENCODING_NAMES) as [Encoding, string][];
    const unencodables: { value: Unencodable; label: string }[] = [
        { value: 'error', label: m.transfer_unencodable_error() },
        { value: 'replace', label: m.transfer_unencodable_replace() },
        { value: 'entity', label: m.transfer_unencodable_entity() },
        { value: 'omit', label: m.transfer_unencodable_omit() }
    ];
</script>

<section
    aria-label={m.transfer_export_adif()}
    class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
>
    <Segmented
        label={m.transfer_kind()}
        options={kinds.map((value) => ({ value, label: KIND_LABELS[value]() }))}
        bind:value={() => draft.kind, pick}
    />
    <ExportForm
        id="exp-query"
        bind:q={draft.q}
        placeholder={m.transfer_query_everything()}
        {matches}
        matchText={draft.kind === 'pota'
            ? m.transfer_qsos_match_pota({ total: SLOT })
            : m.transfer_qsos_match({ total: SLOT })}
        busy={batches.creating}
        label={m.transfer_download_adif()}
        busyLabel={m.transfer_exporting()}
        onexport={runExport}
    >
        {#if draft.kind === 'pota'}
            <label for="exp-park" class="text-2xs text-muted">{m.transfer_park()}</label>
            <input
                id="exp-park"
                class="control h-9 font-mono uppercase"
                placeholder={m.transfer_park_every()}
                spellcheck="false"
                autocomplete="off"
                bind:value={draft.park}
            />
        {/if}
        <label for="exp-split" class="text-2xs text-muted">{m.transfer_split()}</label>
        <select id="exp-split" class="control h-9" bind:value={draft.split}>
            {#each splitChoices(draft.kind) as choice (choice)}
                <option value={choice}>{splitLabel(choice)}</option>
            {/each}
        </select>
        <label for="exp-encoding" class="text-2xs text-muted">{m.transfer_encoding()}</label>
        <select id="exp-encoding" class="control h-9" bind:value={draft.encoding}>
            {#each encodings as [value, name] (value)}
                <option {value}>{name}</option>
            {/each}
        </select>
        {#if draft.encoding === 'utf-8'}
            <label for="exp-len" class="text-2xs text-muted">{m.transfer_lengths()}</label>
            <select id="exp-len" class="control h-9" bind:value={draft.length}>
                {#each lengths as l (l.value)}
                    <option value={l.value}>{l.label}</option>
                {/each}
            </select>
        {:else}
            <label for="exp-unencodable" class="text-2xs text-muted"
                >{m.transfer_unencodable()}</label
            >
            <select id="exp-unencodable" class="control h-9" bind:value={draft.unencodable}>
                {#each unencodables as u (u.value)}
                    <option value={u.value}>{u.label}</option>
                {/each}
            </select>
        {/if}
    </ExportForm>
    {#if fields}
        <span class="text-2xs text-muted"
            >{m.transfer_qsl_mark_note({ sent: fields[0], date: fields[1] })}</span
        >
    {/if}
</section>

<ExportList
    title={m.transfer_list_pending()}
    empty={batches.items.length === 0 ? m.transfer_batch_none() : m.transfer_pending_none()}
    runs={lists.pending}
    callsigns={batches.callsigns}
    results={batches.results}
    working={batches.working}
    {...actions}
/>
<ExportList
    title={m.transfer_list_finished()}
    runs={lists.finished}
    callsigns={batches.callsigns}
    results={batches.results}
    working={batches.working}
    {...actions}
/>
