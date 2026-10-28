<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Batch, ExportKind, LengthUnit, MergeReport } from '#lib/api/types.ts';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import BatchTable from '#lib/components/transfer/BatchTable.svelte';
    import RecordList from '#lib/components/transfer/RecordList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import FileDrop from '#lib/components/ui/FileDrop.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Segmented from '#lib/components/ui/Segmented.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import Stat from '#lib/components/ui/Stat.svelte';
    import { download } from '#lib/download.ts';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { stationLabel } from '#lib/qso/format.ts';
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
        { value: 'codepoints', label: m.transfer_length_codepoints() },
        { value: 'bytes', label: m.transfer_length_bytes() }
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
                toasts.show(m.transfer_qsl_no_batch());
                return;
            }
            download(fileName(logbook.logbook.name, kind, new Date()), result.adif);
            batches = [result.batch, ...batches];
            toasts.show(m.transfer_qsl_batch_created({ count: result.batch.qso_count }));
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
            reportError = m.transfer_file_too_large();
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
                toasts.show(m.transfer_qsl_merged_count({ count: result.merged }));
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

<PageTitle title={m.nav_qsl_management()} />

<PageHeader
    title={m.nav_qsl_management()}
    subtitle={logbook.logbook.name}
    back={{ href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
    toolbarClass="md:hidden"
>
    {#snippet toolbar()}
        <div
            role="tablist"
            aria-label={m.transfer_direction()}
            class="grid grid-cols-2 gap-1.5 md:hidden"
        >
            {#each [['send', m.transfer_qsl_send()], ['receive', m.transfer_qsl_receive()]] as [id, label] (id)}
                <button
                    type="button"
                    role="tab"
                    aria-selected={tab === id}
                    class={[
                        'h-10 rounded-md border text-[13px]',
                        tab === id
                            ? 'border-accent bg-selected font-semibold text-accent'
                            : 'border-border bg-bg'
                    ]}
                    onclick={() => (tab = id as 'send' | 'receive')}>{label}</button
                >
            {/each}
        </div>
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
                <h2 class="text-[15px] font-semibold">{m.transfer_qsl_send()}</h2>
                <Segmented
                    label={m.transfer_kind()}
                    options={[
                        { value: 'qsl_card', label: m.transfer_kind_qsl_card() },
                        { value: 'eqsl', label: 'eQSL' },
                        { value: 'lotw', label: 'LoTW' }
                    ]}
                    bind:value={() => kind, (next) => pick(next as SendKind)}
                />
                <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                    <label for="qsl-query" class="text-xs text-muted">{m.transfer_query()}</label>
                    <input
                        id="qsl-query"
                        class="control h-10 font-mono"
                        spellcheck="false"
                        autocomplete="off"
                        autocapitalize="off"
                        bind:value={q}
                    />
                    <label for="qsl-len" class="text-xs text-muted">{m.transfer_lengths()}</label>
                    <select id="qsl-len" class="control h-10" bind:value={exportLength}>
                        {#each lengths as l (l.value)}
                            <option value={l.value}>{l.label}</option>
                        {/each}
                    </select>
                </div>
                {#if matches.error}
                    <Notice tone="error">{matches.error}</Notice>
                {/if}
                <div class="flex flex-wrap items-center gap-3">
                    <span class="text-[13px]"
                        ><Slotted text={m.transfer_qsos_match({ total: SLOT })}
                            ><strong class="font-mono text-base">{matches.total ?? '…'}</strong
                            ></Slotted
                        ></span
                    >
                    <div class="flex-1"></div>
                    <Button
                        variant="primary"
                        size="lg"
                        disabled={creating || matches.error !== undefined || matches.total === 0}
                        onclick={create}
                        >{creating
                            ? m.transfer_qsl_creating()
                            : m.transfer_qsl_create_batch()}</Button
                    >
                </div>
                <span class="text-xs text-muted"
                    >{m.transfer_qsl_mark_note({ sent: fields[0], date: fields[1] })}</span
                >
            </section>

            <section aria-label={m.transfer_qsl_batches()} class="flex flex-col gap-2">
                <h2 class="eyebrow">{m.transfer_qsl_batches()}</h2>
                <BatchTable {batches} {callsigns} {results} {applying} onapply={apply} />
            </section>
        </div>

        <section
            aria-label={m.transfer_qsl_receive()}
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-3 rounded-lg border border-border bg-surface p-4',
                tab !== 'receive' && 'max-md:hidden'
            ]}
        >
            <h2 class="text-[15px] font-semibold">{m.transfer_qsl_receive()}</h2>
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
                    <label for="rcv-station" class="text-xs text-muted">{m.field_station()}</label>
                    <select id="rcv-station" class="control" bind:value={stationId}>
                        <option value="">{m.transfer_qsl_any_station()}</option>
                        {#each logbook.stations as s (s.id)}
                            <option value={s.id}>{stationLabel(s)}</option>
                        {/each}
                    </select>
                </div>
                {#if format === 'adi'}
                    <div class="flex min-w-0 flex-col gap-1">
                        <label for="rcv-len" class="text-xs text-muted"
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
                <Button disabled={file === undefined || previewing} onclick={() => run(false)}
                    >{previewing ? m.transfer_reading() : m.transfer_preview_again()}</Button
                >
            </div>

            {#if reportError}
                <Notice tone="error">{reportError}</Notice>
            {/if}
            {#if report}
                <span class="text-xs text-muted"
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
                    <span class="text-xs text-muted"
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
                        <summary class="flex h-10 cursor-pointer items-center text-[13px]"
                            >{m.transfer_qsl_differences({
                                differences: differencesText(report.differences)
                            })}</summary
                        >
                        <p class="pb-3 text-[13px] text-muted">
                            {m.transfer_qsl_differences_help()}
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
                                )}>{m.transfer_download_rejects({ count: rejects })}</Button
                        >
                    {/if}
                    <div class="flex-1"></div>
                    {#if merged}
                        <span
                            class="flex h-10 items-center rounded-md bg-ok-bg px-4 font-bold text-ok"
                            >{m.transfer_qsl_merged_count({ count: merged.merged })}</span
                        >
                    {:else}
                        <Button
                            variant="primary"
                            size="lg"
                            disabled={report.merged === 0 || merging || previewing}
                            onclick={() => run(true)}
                            >{merging
                                ? m.transfer_qsl_merging()
                                : m.transfer_qsl_merge_count({ count: report.merged })}</Button
                        >
                    {/if}
                </div>
            {/if}
        </section>
    </div>
</main>
