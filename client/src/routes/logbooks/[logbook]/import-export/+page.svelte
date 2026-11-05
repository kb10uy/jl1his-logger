<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type {
        Encoding,
        ExportKind,
        ImportSource,
        LengthUnit,
        MergeReport,
        NewReport,
        Unencodable
    } from '#lib/api/types.ts';
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import StationSelect from '#lib/components/StationSelect.svelte';
    import BatchTable from '#lib/components/transfer/BatchTable.svelte';
    import ExportForm from '#lib/components/transfer/ExportForm.svelte';
    import MergeReportPanel from '#lib/components/transfer/MergeReportPanel.svelte';
    import NewReportPanel from '#lib/components/transfer/NewReportPanel.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import FileDrop from '#lib/components/ui/FileDrop.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Segmented from '#lib/components/ui/Segmented.svelte';
    import TabSwitch from '#lib/components/ui/TabSwitch.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { stationLabel } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import { Batches, ImportRun } from '#lib/state/transfer.svelte.ts';
    import {
        ENCODING_NAMES,
        KIND_LABELS,
        LENGTH_LABELS,
        QUERY_PRESETS,
        SENT_FIELDS,
        formatOf,
        readingNote,
        recordOnly
    } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const qsoHref = (id: string) => `/logbooks/${logbookId}/qsos/${id}`;

    let tab = $state<'import' | 'export'>('import');

    let source = $state<ImportSource>('new');
    let file = $state<File | undefined>(undefined);
    let stationId = $state('');
    let instrumentId = $state('');
    let importLength = $state<LengthUnit | 'auto'>('auto');
    let importEncoding = $state<Encoding | 'auto'>('auto');

    let kind = $state<ExportKind>('adif');
    let q = $state('');
    let park = $state('');
    let exportLength = $state<LengthUnit>('codepoints');
    let exportEncoding = $state<Encoding>('utf-8');
    let exportUnencodable = $state<Unencodable>('error');

    const station = $derived(
        logbook.activeStations.some((s) => s.id === stationId)
            ? stationId
            : (logbook.activeStations[0]?.id ?? '')
    );
    const instrument = $derived(logbook.activeInstrumentId(instrumentId));
    const format = $derived(file ? formatOf(file.name) : 'adi');
    const matches = new Matches(logbookId, () => q);
    const batches = new Batches(logbookId, () => logbook.logbook.name);
    const importer = new ImportRun<NewReport | MergeReport>((file, apply) => {
        const options = {
            format,
            length: format === 'adi' && importLength !== 'auto' ? importLength : undefined,
            encoding: format === 'adi' && importEncoding !== 'auto' ? importEncoding : undefined,
            apply
        };
        return source === 'new'
            ? api.importAdif(
                  logbookId,
                  source,
                  { ...options, station_id: station, instrument_id: instrument || undefined },
                  file
              )
            : api.importAdif(
                  logbookId,
                  source,
                  { ...options, station_id: stationId || undefined },
                  file
              );
    });
    const ready = $derived(file !== undefined && (source !== 'new' || station !== ''));
    const fields = $derived(SENT_FIELDS[kind]);

    onMount(() => batches.load());

    const isMerge = (report: NewReport | MergeReport): report is MergeReport => 'merged' in report;

    async function importFile() {
        if (file === undefined || !ready) return;
        const result = await importer.run(file, true);
        if (result === undefined) return;
        toasts.show(
            isMerge(result)
                ? m.transfer_qsl_merged_count({ count: result.merged })
                : m.transfer_imported({ count: result.created })
        );
        matches.refresh();
    }

    function preview() {
        if (file !== undefined && ready) void importer.preview(file);
    }

    $effect(() => {
        void source;
        void station;
        void stationId;
        void instrument;
        void importLength;
        void importEncoding;
        void file;
        untrack(preview);
    });

    function pick(next: ExportKind) {
        if (q === QUERY_PRESETS[kind]) q = QUERY_PRESETS[next];
        kind = next;
    }

    async function runExport() {
        const result = await batches.create({
            kind,
            q: q.trim() || undefined,
            park: kind === 'pota' && park.trim() !== '' ? park.trim().toUpperCase() : undefined,
            length: exportEncoding === 'utf-8' ? exportLength : undefined,
            encoding: exportEncoding,
            unencodable: exportEncoding === 'utf-8' ? undefined : exportUnencodable
        });
        if (result === undefined) return;
        toasts.show(
            result.batch === null
                ? m.transfer_export_nothing()
                : m.transfer_exported({ count: result.records })
        );
    }

    async function apply(batch: Parameters<typeof batches.apply>[0]) {
        if (await batches.apply(batch)) matches.refresh();
    }

    const sources: { value: ImportSource; label: string }[] = [
        { value: 'new', label: m.transfer_source_new() },
        { value: 'lotw', label: 'LoTW' },
        { value: 'eqsl', label: 'eQSL' },
        { value: 'card', label: m.transfer_qsl_source_card() }
    ];
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

<PageTitle title={m.nav_import_export()} />

<PageHeader
    title={m.nav_import_export()}
    subtitle={logbook.logbook.name}
    back={{ href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
    toolbarClass="md:hidden"
>
    {#snippet toolbar()}
        <TabSwitch
            label={m.transfer_direction()}
            class="md:hidden"
            tabs={[
                { id: 'import', label: m.transfer_import() },
                { id: 'export', label: m.transfer_export() }
            ]}
            bind:value={tab}
        />
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-5">
    <div class="flex flex-wrap items-start gap-4">
        <section
            aria-label={m.transfer_import()}
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-3 rounded-lg border border-border bg-surface p-4',
                tab !== 'import' && 'max-md:hidden'
            ]}
        >
            <h2 class="text-base font-semibold">{m.transfer_import()}</h2>
            <Segmented label={m.transfer_qsl_source()} options={sources} bind:value={source} />
            <FileDrop bind:file />
            <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                <label for="imp-station" class="text-2xs text-muted">{m.field_station()}</label>
                {#if source === 'new'}
                    <StationSelect
                        id="imp-station"
                        class="h-9"
                        value={station}
                        stations={logbook.activeStations}
                        onchange={(id) => (stationId = id)}
                    />
                    <label for="imp-instrument" class="text-2xs text-muted"
                        >{m.field_instrument()}</label
                    >
                    <InstrumentSelect
                        id="imp-instrument"
                        class="h-9"
                        value={instrument}
                        instruments={logbook.activeInstruments}
                        onchange={(id) => (instrumentId = id)}
                    />
                {:else}
                    <select id="imp-station" class="control h-9" bind:value={stationId}>
                        <option value="">{m.transfer_qsl_any_station()}</option>
                        {#each logbook.stations as s (s.id)}
                            <option value={s.id}>{stationLabel(s)}</option>
                        {/each}
                    </select>
                {/if}
                {#if format === 'adi'}
                    <label for="imp-encoding" class="text-2xs text-muted"
                        >{m.transfer_encoding()}</label
                    >
                    <select
                        id="imp-encoding"
                        class="control h-9"
                        value={importEncoding}
                        onchange={(e) =>
                            (importEncoding = e.currentTarget.value as Encoding | 'auto')}
                    >
                        <option value="auto">{m.transfer_encoding_auto()}</option>
                        {#each encodings as [value, name] (value)}
                            <option {value}>{name}</option>
                        {/each}
                    </select>
                    <label for="imp-len" class="text-2xs text-muted">{m.transfer_lengths()}</label>
                    <select id="imp-len" class="control h-9" bind:value={importLength}>
                        <option value="auto">{m.transfer_length_auto()}</option>
                        {#each lengths as l (l.value)}
                            <option value={l.value}>{l.label}</option>
                        {/each}
                    </select>
                {/if}
            </div>
            <LogbookLoadError />
            {#if source === 'new' && logbook.noActiveStation}
                <Notice>{m.transfer_station_required()}</Notice>
            {/if}

            {#if importer.error}
                <Notice tone="error">{importer.error}</Notice>
            {/if}
            {#if importer.report}
                {@const report = importer.report}
                {@const applied = importer.applied}
                <div class="flex items-center gap-2">
                    <span class="text-2xs text-muted"
                        >{applied
                            ? isMerge(report)
                                ? m.transfer_qsl_merged()
                                : m.transfer_imported_label()
                            : m.transfer_preview_note()} · {readingNote(report)}</span
                    >
                    <div class="flex-1"></div>
                    <Button size="sm" disabled={importer.previewing} onclick={preview}
                        >{importer.previewing
                            ? m.transfer_reading()
                            : m.transfer_preview_again()}</Button
                    >
                </div>
                {#if isMerge(report)}
                    <MergeReportPanel
                        {report}
                        applied={applied && isMerge(applied) ? applied : undefined}
                        applying={importer.applying}
                        previewing={importer.previewing}
                        logbookName={logbook.logbook.name}
                        {qsoHref}
                        onapply={importFile}
                    />
                {:else}
                    <NewReportPanel
                        {report}
                        applied={applied && !isMerge(applied) ? applied : undefined}
                        applying={importer.applying}
                        previewing={importer.previewing}
                        logbookName={logbook.logbook.name}
                        onapply={importFile}
                    />
                {/if}
            {:else if importer.previewing}
                <span class="text-2xs text-muted">{m.transfer_reading_file()}</span>
            {/if}
        </section>

        <div
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-4',
                tab !== 'export' && 'max-md:hidden'
            ]}
        >
            <section
                aria-label={m.transfer_export()}
                class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
            >
                <h2 class="text-base font-semibold">{m.transfer_export()}</h2>
                <Segmented
                    label={m.transfer_kind()}
                    options={kinds.map((value) => ({ value, label: KIND_LABELS[value]() }))}
                    bind:value={() => kind, pick}
                />
                <ExportForm
                    id="exp-query"
                    bind:q
                    placeholder={m.transfer_query_everything()}
                    {matches}
                    matchText={kind === 'pota'
                        ? m.transfer_qsos_match_pota({ total: SLOT })
                        : m.transfer_qsos_match({ total: SLOT })}
                    busy={batches.creating}
                    label={m.transfer_download_adif()}
                    busyLabel={m.transfer_exporting()}
                    onexport={runExport}
                >
                    {#if kind === 'pota'}
                        <label for="exp-park" class="text-2xs text-muted">{m.transfer_park()}</label
                        >
                        <input
                            id="exp-park"
                            class="control h-9 font-mono uppercase"
                            placeholder={m.transfer_park_every()}
                            spellcheck="false"
                            autocomplete="off"
                            bind:value={park}
                        />
                    {/if}
                    <label for="exp-encoding" class="text-2xs text-muted"
                        >{m.transfer_encoding()}</label
                    >
                    <select id="exp-encoding" class="control h-9" bind:value={exportEncoding}>
                        {#each encodings as [value, name] (value)}
                            <option {value}>{name}</option>
                        {/each}
                    </select>
                    {#if exportEncoding === 'utf-8'}
                        <label for="exp-len" class="text-2xs text-muted"
                            >{m.transfer_lengths()}</label
                        >
                        <select id="exp-len" class="control h-9" bind:value={exportLength}>
                            {#each lengths as l (l.value)}
                                <option value={l.value}>{l.label}</option>
                            {/each}
                        </select>
                    {:else}
                        <label for="exp-unencodable" class="text-2xs text-muted"
                            >{m.transfer_unencodable()}</label
                        >
                        <select
                            id="exp-unencodable"
                            class="control h-9"
                            bind:value={exportUnencodable}
                        >
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

            <section aria-label={m.transfer_recent_exports()} class="flex flex-col gap-2">
                <h2 class="eyebrow">{m.transfer_recent_exports()}</h2>
                <BatchTable
                    batches={batches.items}
                    callsigns={batches.callsigns}
                    results={batches.results}
                    applying={batches.applying}
                    onapply={apply}
                />
            </section>
        </div>
    </div>
</main>
