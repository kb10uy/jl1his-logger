<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Encoding, LengthUnit, Unencodable } from '#lib/api/types.ts';
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import StationSelect from '#lib/components/StationSelect.svelte';
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
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import { Batches, ImportRun } from '#lib/state/transfer.svelte.ts';
    import {
        ENCODING_NAMES,
        LENGTH_LABELS,
        formatOf,
        readingNote
    } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;

    let tab = $state<'import' | 'export'>('import');

    let file = $state<File | undefined>(undefined);
    let stationId = $state('');
    let instrumentId = $state('');
    let importLength = $state<LengthUnit | 'auto'>('auto');
    let importEncoding = $state<Encoding | 'auto'>('auto');

    let kind = $state<'adif' | 'pota'>('adif');
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
    const batches = new Batches(
        logbookId,
        () => logbook.logbook.name,
        (b) => b.kind === 'adif' || b.kind === 'pota'
    );
    const importer = new ImportRun((file, apply) =>
        api.importAdif(
            logbookId,
            'new',
            {
                station_id: station,
                instrument_id: instrument || undefined,
                format,
                length: format === 'adi' && importLength !== 'auto' ? importLength : undefined,
                encoding:
                    format === 'adi' && importEncoding !== 'auto' ? importEncoding : undefined,
                apply
            },
            file
        )
    );

    onMount(() => batches.load());

    async function importFile() {
        if (file === undefined || station === '') return;
        const result = await importer.run(file, true);
        if (result === undefined) return;
        toasts.show(m.transfer_imported({ count: result.created }));
        matches.refresh();
    }

    function preview() {
        if (file !== undefined && station !== '') void importer.preview(file);
    }

    $effect(() => {
        void station;
        void instrument;
        void importLength;
        void importEncoding;
        void file;
        untrack(preview);
    });

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
            <h2 class="text-base font-semibold">{m.transfer_import_new_title()}</h2>
            <FileDrop bind:file />
            <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                <label for="imp-station" class="text-2xs text-muted">{m.field_station()}</label>
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
            {#if logbook.noActiveStation}
                <Notice>{m.transfer_station_required()}</Notice>
            {/if}

            {#if importer.error}
                <Notice tone="error">{importer.error}</Notice>
            {/if}
            {#if importer.report}
                {@const report = importer.report}
                {@const imported = importer.applied}
                <div class="flex items-center gap-2">
                    <span class="text-2xs text-muted"
                        >{imported ? m.transfer_imported_label() : m.transfer_preview_note()} · {readingNote(
                            report
                        )}</span
                    >
                    <div class="flex-1"></div>
                    <Button size="sm" disabled={importer.previewing} onclick={preview}
                        >{importer.previewing
                            ? m.transfer_reading()
                            : m.transfer_preview_again()}</Button
                    >
                </div>
                <div class="grid grid-cols-3 gap-2">
                    <Stat label={m.transfer_stat_read()} value={report.read} />
                    <Stat
                        label={imported ? m.transfer_stat_created() : m.transfer_stat_new()}
                        value={report.created}
                        tone="ok"
                    />
                    <Stat label={m.transfer_stat_already_imported()} value={report.existing} />
                    <Stat
                        label={m.transfer_possible_duplicates()}
                        value={report.duplicates.length}
                        tone={report.duplicates.length > 0 ? 'warn' : undefined}
                    />
                    <Stat
                        label={m.transfer_stat_outside_band_modes()}
                        value={`${report.outside_band.length} · ${report.modes.length}`}
                        tone={report.outside_band.length + report.modes.length > 0
                            ? 'warn'
                            : undefined}
                    />
                    <Stat
                        label={m.transfer_invalid()}
                        value={report.invalid.length}
                        tone={report.invalid.length > 0 ? 'error' : undefined}
                    />
                </div>
                <RecordList
                    title={m.transfer_possible_duplicates()}
                    note={m.transfer_note_imported_anyway()}
                    open
                    items={report.duplicates.map((d) => ({
                        record: d.record,
                        text: m.transfer_duplicate_item({ count: d.ids.length })
                    }))}
                />
                <RecordList
                    title={m.transfer_outside_band()}
                    note={m.transfer_note_kept()}
                    items={report.outside_band.map((o) => ({
                        record: o.record,
                        text: m.transfer_outside_band_item({ freq: o.freq, band: o.band })
                    }))}
                />
                <RecordList
                    title={m.transfer_modes()}
                    note={m.transfer_note_kept()}
                    items={report.modes.map((w) => ({
                        record: w.record,
                        text:
                            w.kind === 'unknown_mode'
                                ? m.transfer_unknown_mode({ mode: w.mode })
                                : w.kind === 'unknown_submode'
                                  ? m.transfer_unknown_submode({ submode: w.submode })
                                  : w.kind === 'submode_of_another_mode'
                                    ? m.transfer_submode_of_another_mode({
                                          submode: w.submode,
                                          expected: w.expected
                                      })
                                    : w.kind
                    }))}
                />
                <RecordList
                    title={m.transfer_invalid()}
                    note={m.transfer_note_skipped()}
                    items={report.invalid.map((i) => ({ record: i.record, text: i.error }))}
                />
                <ImportActions
                    {report}
                    logbookName={logbook.logbook.name}
                    done={imported ? m.transfer_imported({ count: imported.created }) : undefined}
                    label={importer.applying
                        ? m.transfer_importing()
                        : m.transfer_import_count({ count: report.created })}
                    disabled={report.created === 0 || importer.applying || importer.previewing}
                    onapply={importFile}
                />
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
                    options={[
                        { value: 'adif', label: 'ADIF' },
                        { value: 'pota', label: 'POTA' }
                    ]}
                    bind:value={kind}
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
            </section>

            <section aria-label={m.transfer_recent_exports()} class="flex flex-col gap-2">
                <h2 class="eyebrow">{m.transfer_recent_exports()}</h2>
                <BatchTable batches={batches.items} callsigns={batches.callsigns} />
            </section>
        </div>
    </div>
</main>
