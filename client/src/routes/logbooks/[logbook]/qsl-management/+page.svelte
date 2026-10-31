<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { LengthUnit } from '#lib/api/types.ts';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import BatchTable from '#lib/components/transfer/BatchTable.svelte';
    import ExportForm from '#lib/components/transfer/ExportForm.svelte';
    import ImportActions from '#lib/components/transfer/ImportActions.svelte';
    import RecordList from '#lib/components/transfer/RecordList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import FileDrop from '#lib/components/ui/FileDrop.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Segmented from '#lib/components/ui/Segmented.svelte';
    import Stat from '#lib/components/ui/Stat.svelte';
    import TabSwitch from '#lib/components/ui/TabSwitch.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { stationLabel } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import { Batches, ImportRun } from '#lib/state/transfer.svelte.ts';
    import { SENT_FIELDS, differencesText, formatOf, recordOnly } from '#lib/transfer/transfer.ts';

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

    let source = $state<Source>('lotw');
    let file = $state<File | undefined>(undefined);
    let stationId = $state('');
    let importLength = $state<LengthUnit>('codepoints');

    const format = $derived(file ? formatOf(file.name) : 'adi');
    const matches = new Matches(logbookId, () => q);
    const batches = new Batches(
        logbookId,
        () => logbook.logbook.name,
        (b) => !recordOnly(b.kind)
    );
    const merge = new ImportRun((file, apply) =>
        api.importAdif(
            logbookId,
            source,
            {
                station_id: stationId || undefined,
                format,
                length: format === 'adi' ? importLength : undefined,
                apply
            },
            file
        )
    );
    const fields = $derived(SENT_FIELDS[kind] ?? ['', '']);
    const lengths: { value: LengthUnit; label: string }[] = [
        { value: 'codepoints', label: m.transfer_length_codepoints() },
        { value: 'bytes', label: m.transfer_length_bytes() }
    ];

    onMount(() => batches.load());

    function pick(next: SendKind) {
        if (q === PRESETS[kind]) q = PRESETS[next];
        kind = next;
    }

    async function create() {
        const result = await batches.create({
            kind,
            q: q.trim() || undefined,
            length: exportLength
        });
        if (result === undefined) return;
        toasts.show(
            result.batch === null
                ? m.transfer_qsl_no_batch()
                : m.transfer_qsl_batch_created({ count: result.batch.qso_count })
        );
    }

    async function apply(batch: Parameters<typeof batches.apply>[0]) {
        if (await batches.apply(batch)) matches.refresh();
    }

    async function mergeFile() {
        if (file === undefined) return;
        const result = await merge.run(file, true);
        if (result === undefined) return;
        toasts.show(m.transfer_qsl_merged_count({ count: result.merged }));
        matches.refresh();
    }

    $effect(() => {
        void source;
        void stationId;
        void importLength;
        const chosen = file;
        if (chosen === undefined) return;
        untrack(() => merge.preview(chosen));
    });
</script>

<PageTitle title={m.nav_qsl_management()} />

<PageHeader
    title={m.nav_qsl_management()}
    subtitle={logbook.logbook.name}
    back={{ href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
    toolbarClass="md:hidden"
>
    {#snippet toolbar()}
        <TabSwitch
            label={m.transfer_direction()}
            class="md:hidden"
            tabs={[
                { id: 'send', label: m.transfer_qsl_send() },
                { id: 'receive', label: m.transfer_qsl_receive() }
            ]}
            bind:value={tab}
        />
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <div class="flex flex-wrap items-start gap-4">
        <div
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-4',
                tab !== 'send' && 'max-md:hidden'
            ]}
        >
            <section
                aria-label={m.transfer_qsl_send()}
                class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
            >
                <h2 class="text-base font-semibold">{m.transfer_qsl_send()}</h2>
                <Segmented
                    label={m.transfer_kind()}
                    options={[
                        { value: 'qsl_card', label: m.transfer_kind_qsl_card() },
                        { value: 'eqsl', label: 'eQSL' },
                        { value: 'lotw', label: 'LoTW' }
                    ]}
                    bind:value={() => kind, (next) => pick(next as SendKind)}
                />
                <ExportForm
                    id="qsl-query"
                    bind:q
                    {matches}
                    matchText={m.transfer_qsos_match({ total: SLOT })}
                    busy={batches.creating}
                    label={m.transfer_qsl_create_batch()}
                    busyLabel={m.transfer_qsl_creating()}
                    onexport={create}
                >
                    <label for="qsl-len" class="text-2xs text-muted">{m.transfer_lengths()}</label>
                    <select id="qsl-len" class="control h-10" bind:value={exportLength}>
                        {#each lengths as l (l.value)}
                            <option value={l.value}>{l.label}</option>
                        {/each}
                    </select>
                </ExportForm>
                <span class="text-2xs text-muted"
                    >{m.transfer_qsl_mark_note({ sent: fields[0], date: fields[1] })}</span
                >
            </section>

            <section aria-label={m.transfer_qsl_batches()} class="flex flex-col gap-2">
                <h2 class="eyebrow">{m.transfer_qsl_batches()}</h2>
                <BatchTable
                    batches={batches.items}
                    callsigns={batches.callsigns}
                    results={batches.results}
                    applying={batches.applying}
                    onapply={apply}
                />
            </section>
        </div>

        <section
            aria-label={m.transfer_qsl_receive()}
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-3 rounded-lg border border-border bg-surface p-4',
                tab !== 'receive' && 'max-md:hidden'
            ]}
        >
            <h2 class="text-base font-semibold">{m.transfer_qsl_receive()}</h2>
            <Segmented
                label={m.transfer_qsl_source()}
                options={[
                    { value: 'lotw', label: 'LoTW' },
                    { value: 'eqsl', label: 'eQSL' },
                    { value: 'card', label: m.transfer_qsl_source_card() }
                ]}
                bind:value={source}
            />
            <FileDrop bind:file />
            <div class="flex flex-wrap items-end gap-3">
                <div class="flex min-w-0 flex-col gap-1">
                    <label for="rcv-station" class="text-2xs text-muted">{m.field_station()}</label>
                    <select id="rcv-station" class="control" bind:value={stationId}>
                        <option value="">{m.transfer_qsl_any_station()}</option>
                        {#each logbook.stations as s (s.id)}
                            <option value={s.id}>{stationLabel(s)}</option>
                        {/each}
                    </select>
                </div>
                {#if format === 'adi'}
                    <div class="flex min-w-0 flex-col gap-1">
                        <label for="rcv-len" class="text-2xs text-muted"
                            >{m.transfer_lengths()}</label
                        >
                        <select id="rcv-len" class="control" bind:value={importLength}>
                            {#each lengths as l (l.value)}
                                <option value={l.value}>{l.label}</option>
                            {/each}
                        </select>
                    </div>
                {/if}
                <div class="flex-1"></div>
                <Button
                    disabled={file === undefined || merge.previewing}
                    onclick={() => file && merge.preview(file)}
                    >{merge.previewing ? m.transfer_reading() : m.transfer_preview_again()}</Button
                >
            </div>

            {#if merge.error}
                <Notice tone="error">{merge.error}</Notice>
            {/if}
            {#if merge.report}
                {@const report = merge.report}
                {@const merged = merge.applied}
                <span class="text-2xs text-muted"
                    >{merged ? m.transfer_qsl_merged() : m.transfer_preview_note()}</span
                >
                <div class="grid grid-cols-3 gap-2">
                    <Stat label={m.transfer_stat_read()} value={report.read} />
                    <Stat
                        label={merged ? m.transfer_qsl_merged() : m.transfer_qsl_to_merge()}
                        value={report.merged}
                        tone="ok"
                    />
                    <Stat label={m.transfer_qsl_already_received()} value={report.already} />
                    <Stat
                        label={m.transfer_qsl_unmatched()}
                        value={report.unmatched.length}
                        tone={report.unmatched.length > 0 ? 'warn' : undefined}
                    />
                    <Stat
                        label={m.transfer_qsl_ambiguous()}
                        value={report.ambiguous.length}
                        tone={report.ambiguous.length > 0 ? 'warn' : undefined}
                    />
                    <Stat
                        label={m.transfer_invalid()}
                        value={report.invalid.length}
                        tone={report.invalid.length > 0 ? 'error' : undefined}
                    />
                </div>
                {#if report.not_confirmations > 0}
                    <span class="text-2xs text-muted"
                        >{m.transfer_qsl_not_confirmations({
                            records: m.common_record_count({ count: report.not_confirmations })
                        })}</span
                    >
                {/if}
                <RecordList
                    title={m.transfer_qsl_ambiguous()}
                    note={m.transfer_qsl_note_several()}
                    items={report.ambiguous.map((a) => ({
                        record: a.record,
                        text: `→ ${m.common_qso_count({ count: a.ids.length })}`,
                        ids: a.ids
                    }))}
                    {qsoHref}
                />
                <RecordList
                    title={m.transfer_qsl_unmatched()}
                    note={m.transfer_qsl_note_no_qso()}
                    items={report.unmatched.map((record) => ({ record, text: '' }))}
                />
                <RecordList
                    title={m.transfer_invalid()}
                    note={m.transfer_note_skipped()}
                    items={report.invalid.map((i) => ({ record: i.record, text: i.error }))}
                />
                {#if report.skipped.length > 0}
                    <Notice
                        >{m.transfer_qsl_skipped_notice({
                            qsos: m.common_qso_count({ count: report.skipped.length })
                        })}</Notice
                    >
                {/if}
                {#if differencesText(report.differences)}
                    <details class="rounded-lg border border-border px-3">
                        <summary class="flex h-10 cursor-pointer items-center text-sm"
                            >{m.transfer_qsl_differences({
                                differences: differencesText(report.differences)
                            })}</summary
                        >
                        <p class="pb-3 text-sm text-muted">
                            {m.transfer_qsl_differences_help()}
                        </p>
                    </details>
                {/if}
                <ImportActions
                    {report}
                    logbookName={logbook.logbook.name}
                    done={merged
                        ? m.transfer_qsl_merged_count({ count: merged.merged })
                        : undefined}
                    label={merge.applying
                        ? m.transfer_qsl_merging()
                        : m.transfer_qsl_merge_count({ count: report.merged })}
                    disabled={report.merged === 0 || merge.applying || merge.previewing}
                    onapply={mergeFile}
                />
            {/if}
        </section>
    </div>
</main>
