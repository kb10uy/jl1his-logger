<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Batch, ExportKind, LengthUnit, MergeReport } from '#lib/api/types.ts';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import BatchTable from '#lib/components/transfer/BatchTable.svelte';
    import RecordList from '#lib/components/transfer/RecordList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import FileDrop from '#lib/components/ui/FileDrop.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Segmented from '#lib/components/ui/Segmented.svelte';
    import Stat from '#lib/components/ui/Stat.svelte';
    import { download } from '#lib/download.ts';
    import { count, stationLabel } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import {
        MAX_IMPORT,
        SENT_FIELDS,
        differencesText,
        fileName,
        formatOf,
        outcomeText,
        recordOnly,
        rejectCount
    } from '#lib/transfer/transfer.ts';

    type SendKind = 'qsl_card' | 'eqsl' | 'lotw';
    type Source = 'lotw' | 'eqsl' | 'card';

    const PRESETS: Record<SendKind, string> = {
        qsl_card: 'sent:R,Q',
        eqsl: '-eqsl-sent:Y',
        lotw: '-lotw-sent:Y'
    };

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const qsoHref = (id: string) => `/logbooks/${logbookId}/qsos/${id}`;

    let tab = $state<'send' | 'receive'>('send');
    let kind = $state<SendKind>('qsl_card');
    let q = $state(PRESETS.qsl_card);
    let exportLength = $state<LengthUnit>('codepoints');
    let creating = $state(false);
    let batches = $state<Batch[]>([]);
    let callsigns = $state<Record<string, string>>({});
    let results = $state<Record<string, string>>({});
    let applying = $state<string | undefined>(undefined);

    let source = $state<Source>('lotw');
    let file = $state<File | undefined>(undefined);
    let stationId = $state('');
    let importLength = $state<LengthUnit>('codepoints');
    let report = $state<MergeReport | undefined>(undefined);
    let reportError = $state<string | undefined>(undefined);
    let previewing = $state(false);
    let merging = $state(false);
    let merged = $state<MergeReport | undefined>(undefined);

    const matches = new Matches(logbookId, () => q);
    const fields = $derived(SENT_FIELDS[kind] ?? ['', '']);
    const format = $derived(file ? formatOf(file.name) : 'adi');
    const lengths: { value: LengthUnit; label: string }[] = [
        { value: 'codepoints', label: 'Codepoints' },
        { value: 'bytes', label: 'Bytes' }
    ];
    const rejects = $derived(report ? rejectCount(report) : 0);

    onMount(() => {
        api.batches(logbookId)
            .then((found) => (batches = found.filter((b) => !recordOnly(b.kind))))
            .catch((e) => toasts.error((e as Error).message));
        api.members(logbookId)
            .then((found) => {
                callsigns = Object.fromEntries(found.map((m) => [m.user_id, m.callsign]));
            })
            .catch(() => undefined);
    });

    function pick(next: SendKind) {
        if (q === PRESETS[kind]) q = PRESETS[next];
        kind = next;
    }

    async function create() {
        creating = true;
        try {
            const result = await api.createExport(logbookId, {
                kind,
                q: q.trim() || undefined,
                length: exportLength
            });
            if (result.batch === null || result.adif === null) {
                toasts.show('No QSO matches, so no batch was created.');
                return;
            }
            download(fileName(logbook.logbook.name, kind, new Date()), result.adif);
            batches = [result.batch, ...batches];
            toasts.show(`Created a batch of ${count(result.batch.qso_count, 'QSO')}`);
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            creating = false;
        }
    }

    async function apply(batch: Batch) {
        applying = batch.id;
        try {
            const outcome = await api.applyBatch(logbookId, batch.id);
            results = { ...results, [batch.id]: outcomeText(outcome) };
            batches = batches.map((b) =>
                b.id === batch.id ? { ...b, applied_at: new Date().toISOString() } : b
            );
            matches.refresh();
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            applying = undefined;
        }
    }

    async function run(apply: boolean) {
        if (file === undefined) return;
        if (file.size > MAX_IMPORT) {
            reportError = 'The file is larger than 64 MiB.';
            return;
        }
        reportError = undefined;
        if (apply) merging = true;
        else previewing = true;
        try {
            const result = await api.importAdif(
                logbookId,
                source,
                {
                    station_id: stationId || undefined,
                    format,
                    length: format === 'adi' ? importLength : undefined,
                    apply
                },
                file
            );
            report = result;
            merged = apply ? result : undefined;
            if (apply) {
                toasts.show(`Merged ${count(result.merged, 'confirmation')}`);
                matches.refresh();
            }
        } catch (e) {
            report = undefined;
            reportError = (e as Error).message;
        } finally {
            previewing = false;
            merging = false;
        }
    }

    $effect(() => {
        void source;
        void stationId;
        void importLength;
        if (file === undefined) return;
        untrack(() => {
            merged = undefined;
            run(false);
        });
    });
</script>

<PageTitle title="QSL Management" />

{#snippet lengthChoice(name: string, value: LengthUnit, set: (v: LengthUnit) => void)}
    <div role="radiogroup" aria-label="Lengths" class="flex gap-4 text-[13px]">
        {#each lengths as l (l.value)}
            <label class="flex cursor-pointer items-center gap-1.5">
                <input
                    type="radio"
                    {name}
                    class="accent-info"
                    checked={value === l.value}
                    onchange={() => set(l.value)}
                />
                {l.label}
            </label>
        {/each}
    </div>
{/snippet}

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header class="flex flex-wrap items-center gap-3">
        <a
            href={`/logbooks/${logbookId}/more`}
            aria-label="Back to More"
            class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text md:hidden"
            ><Icon name="back" size={22} /></a
        >
        <h1 class="text-xl font-semibold">QSL Management</h1>
    </header>
    <div role="tablist" aria-label="Direction" class="grid grid-cols-2 gap-1.5 md:hidden">
        {#each [['send', 'Send'], ['receive', 'Receive']] as [id, label] (id)}
            <button
                type="button"
                role="tab"
                aria-selected={tab === id}
                class={[
                    'h-10 rounded-md border text-[13px]',
                    tab === id
                        ? 'border-accent bg-warn-bg font-semibold text-accent'
                        : 'border-border bg-surface'
                ]}
                onclick={() => (tab = id as 'send' | 'receive')}>{label}</button
            >
        {/each}
    </div>

    <div class="flex flex-wrap items-start gap-4">
        <div
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-4',
                tab !== 'send' && 'max-md:hidden'
            ]}
        >
            <section
                aria-label="Send"
                class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
            >
                <h2 class="text-[15px] font-semibold">Send</h2>
                <Segmented
                    label="Kind"
                    options={[
                        { value: 'qsl_card', label: 'QSL card' },
                        { value: 'eqsl', label: 'eQSL' },
                        { value: 'lotw', label: 'LoTW' }
                    ]}
                    bind:value={() => kind, (next) => pick(next as SendKind)}
                />
                <div class="flex flex-col gap-1">
                    <label for="qsl-query" class="text-xs text-muted">Query</label>
                    <input
                        id="qsl-query"
                        class="control h-10 font-mono"
                        spellcheck="false"
                        autocomplete="off"
                        autocapitalize="off"
                        bind:value={q}
                    />
                </div>
                <div class="flex flex-wrap items-center gap-3">
                    <span class="text-xs text-muted">Lengths</span>
                    {@render lengthChoice('qsl-len', exportLength, (v) => (exportLength = v))}
                </div>
                {#if matches.error}
                    <Notice tone="error">{matches.error}</Notice>
                {/if}
                <div class="flex flex-wrap items-center gap-3">
                    <span class="text-[13px]"
                        ><strong class="font-mono text-base">{matches.total ?? '…'}</strong> QSOs match</span
                    >
                    <div class="flex-1"></div>
                    <Button
                        variant="primary"
                        size="lg"
                        disabled={creating || matches.error !== undefined || matches.total === 0}
                        onclick={create}
                        >{creating ? 'Creating…' : 'Create batch and download ADIF'}</Button
                    >
                </div>
                <span class="text-xs text-muted"
                    >Marking the batch as sent later sets {fields[0]} = Y and {fields[1]} on the QSOs
                    still at their recorded revisions.</span
                >
            </section>

            <section aria-label="Batches" class="flex flex-col gap-2">
                <h2 class="eyebrow">Batches</h2>
                <BatchTable {batches} {callsigns} {results} {applying} onapply={apply} />
            </section>
        </div>

        <section
            aria-label="Receive"
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-3 rounded-lg border border-border bg-surface p-4',
                tab !== 'receive' && 'max-md:hidden'
            ]}
        >
            <h2 class="text-[15px] font-semibold">Receive</h2>
            <Segmented
                label="Source"
                options={[
                    { value: 'lotw', label: 'LoTW' },
                    { value: 'eqsl', label: 'eQSL' },
                    { value: 'card', label: 'Card' }
                ]}
                bind:value={source}
            />
            <FileDrop bind:file />
            <div class="flex flex-wrap items-end gap-3">
                <div class="flex min-w-0 flex-col gap-1">
                    <label for="rcv-station" class="text-xs text-muted">Station</label>
                    <select id="rcv-station" class="control" bind:value={stationId}>
                        <option value="">Any Station</option>
                        {#each logbook.stations as s (s.id)}
                            <option value={s.id}>{stationLabel(s)}</option>
                        {/each}
                    </select>
                </div>
                {#if format === 'adi'}
                    <div class="flex h-10 items-center">
                        {@render lengthChoice('rcv-len', importLength, (v) => (importLength = v))}
                    </div>
                {/if}
                <div class="flex-1"></div>
                <Button disabled={file === undefined || previewing} onclick={() => run(false)}
                    >{previewing ? 'Reading…' : 'Preview again'}</Button
                >
            </div>

            {#if reportError}
                <Notice tone="error">{reportError}</Notice>
            {/if}
            {#if report}
                <span class="text-xs text-muted"
                    >{merged ? 'Merged' : 'Preview · nothing written yet'}</span
                >
                <div class="grid grid-cols-3 gap-2">
                    <Stat label="Read" value={report.read} />
                    <Stat label={merged ? 'Merged' : 'To merge'} value={report.merged} tone="ok" />
                    <Stat label="Already received" value={report.already} />
                    <Stat
                        label="Unmatched"
                        value={report.unmatched.length}
                        tone={report.unmatched.length > 0 ? 'warn' : undefined}
                    />
                    <Stat
                        label="Ambiguous"
                        value={report.ambiguous.length}
                        tone={report.ambiguous.length > 0 ? 'warn' : undefined}
                    />
                    <Stat
                        label="Invalid"
                        value={report.invalid.length}
                        tone={report.invalid.length > 0 ? 'error' : undefined}
                    />
                </div>
                {#if report.not_confirmations > 0}
                    <span class="text-xs text-muted"
                        >{count(report.not_confirmations, 'record')} are not confirmations and are left
                        alone.</span
                    >
                {/if}
                <RecordList
                    title="Ambiguous"
                    note="matches several QSOs"
                    items={report.ambiguous.map((a) => ({
                        record: a.record,
                        text: `→ ${count(a.ids.length, 'QSO')}`,
                        ids: a.ids
                    }))}
                    {qsoHref}
                />
                <RecordList
                    title="Unmatched"
                    note="no QSO found"
                    items={report.unmatched.map((record) => ({ record, text: '' }))}
                />
                <RecordList
                    title="Invalid"
                    note="skipped"
                    items={report.invalid.map((i) => ({ record: i.record, text: i.error }))}
                />
                {#if report.skipped.length > 0}
                    <Notice
                        >{count(report.skipped.length, 'QSO')} changed while merging and were skipped.
                        Preview again to merge them.</Notice
                    >
                {/if}
                {#if differencesText(report.differences)}
                    <details class="rounded-lg border border-border px-3">
                        <summary class="flex h-10 cursor-pointer items-center text-[13px]"
                            >Differences not applied · {differencesText(
                                report.differences
                            )}</summary
                        >
                        <p class="pb-3 text-[13px] text-muted">
                            Fields the confirmations carry that differ from the QSOs. They are
                            counted, never written.
                        </p>
                    </details>
                {/if}
                <div class="flex flex-wrap items-center gap-2">
                    {#if report.rejects}
                        <Button
                            variant="link"
                            onclick={() =>
                                download(
                                    fileName(logbook.logbook.name, 'adif', new Date(), '-rejects'),
                                    report?.rejects ?? ''
                                )}>Download rejects ({count(rejects, 'record')})</Button
                        >
                    {/if}
                    <div class="flex-1"></div>
                    {#if merged}
                        <span
                            class="flex h-10 items-center rounded-md bg-ok-bg px-4 font-bold text-ok"
                            >Merged {count(merged.merged, 'confirmation')}</span
                        >
                    {:else}
                        <Button
                            variant="primary"
                            size="lg"
                            disabled={report.merged === 0 || merging || previewing}
                            onclick={() => run(true)}
                            >{merging
                                ? 'Merging…'
                                : `Merge ${count(report.merged, 'confirmation')}`}</Button
                        >
                    {/if}
                </div>
            {/if}
        </section>
    </div>
</main>
