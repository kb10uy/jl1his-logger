<script lang="ts">
    import { untrack } from 'svelte';
    import type {
        Encoding,
        ImportSource,
        LengthUnit,
        MergeReport,
        NewReport
    } from '#lib/api/types.ts';
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import StationSelect from '#lib/components/StationSelect.svelte';
    import MergeReportPanel from '#lib/components/transfer/MergeReportPanel.svelte';
    import NewReportPanel from '#lib/components/transfer/NewReportPanel.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import FileDrop from '#lib/components/ui/FileDrop.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Segmented from '#lib/components/ui/Segmented.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { stationLabel } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { transferState } from '#lib/state/transfer.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import {
        ENCODING_NAMES,
        LENGTH_LABELS,
        formatOf,
        readingNote
    } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const transfer = transferState(logbook);
    const draft = transfer.importDraft;
    const importer = transfer.importer;
    const qsoHref = (id: string) => `/logbooks/${logbookId}/qsos/${id}`;

    const station = $derived(transfer.station(logbook));
    const instrument = $derived(logbook.activeInstrumentId(draft.instrumentId));
    const format = $derived(draft.file ? formatOf(draft.file.name) : 'adi');
    const ready = $derived(draft.file !== undefined && (draft.source !== 'new' || station !== ''));

    const isMerge = (report: NewReport | MergeReport): report is MergeReport => 'merged' in report;

    async function importFile() {
        if (draft.file === undefined || !ready) return;
        const result = await importer.run(draft.file, true);
        if (result === undefined) return;
        toasts.show(
            isMerge(result)
                ? m.transfer_qsl_merged_count({ count: result.merged })
                : m.transfer_imported({ count: result.created })
        );
    }

    function preview() {
        if (draft.file !== undefined && ready) void importer.preview(draft.file);
    }

    $effect(() => {
        void draft.source;
        void station;
        void draft.stationId;
        void instrument;
        void draft.length;
        void draft.encoding;
        void draft.file;
        untrack(preview);
    });

    const sources: { value: ImportSource; label: string }[] = [
        { value: 'new', label: m.transfer_source_new() },
        { value: 'lotw', label: 'LoTW' },
        { value: 'eqsl', label: 'eQSL' },
        { value: 'card', label: m.transfer_qsl_source_card() }
    ];
    const units: LengthUnit[] = ['codepoints', 'bytes', 'utf16'];
    const lengths = units.map((value) => ({ value, label: LENGTH_LABELS[value]() }));
    const encodings = Object.entries(ENCODING_NAMES) as [Encoding, string][];
</script>

<section
    aria-label={m.transfer_import_adif()}
    class="flex min-w-0 flex-col gap-3 rounded-lg border border-border bg-surface p-4"
>
    <Segmented label={m.transfer_qsl_source()} options={sources} bind:value={draft.source} />
    <FileDrop bind:file={draft.file} />
    <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
        <label for="imp-station" class="text-2xs text-muted">{m.field_station()}</label>
        {#if draft.source === 'new'}
            <StationSelect
                id="imp-station"
                class="h-9"
                value={station}
                stations={logbook.activeStations}
                onchange={(id) => (draft.stationId = id)}
            />
            <label for="imp-instrument" class="text-2xs text-muted">{m.field_instrument()}</label>
            <InstrumentSelect
                id="imp-instrument"
                class="h-9"
                value={instrument}
                instruments={logbook.activeInstruments}
                onchange={(id) => (draft.instrumentId = id)}
            />
        {:else}
            <select id="imp-station" class="control h-9" bind:value={draft.stationId}>
                <option value="">{m.transfer_qsl_any_station()}</option>
                {#each logbook.stations as s (s.id)}
                    <option value={s.id}>{stationLabel(s)}</option>
                {/each}
            </select>
        {/if}
        {#if format === 'adi'}
            <label for="imp-encoding" class="text-2xs text-muted">{m.transfer_encoding()}</label>
            <select
                id="imp-encoding"
                class="control h-9"
                value={draft.encoding}
                onchange={(e) => (draft.encoding = e.currentTarget.value as Encoding | 'auto')}
            >
                <option value="auto">{m.transfer_encoding_auto()}</option>
                {#each encodings as [value, name] (value)}
                    <option {value}>{name}</option>
                {/each}
            </select>
            <label for="imp-len" class="text-2xs text-muted">{m.transfer_lengths()}</label>
            <select id="imp-len" class="control h-9" bind:value={draft.length}>
                <option value="auto">{m.transfer_length_auto()}</option>
                {#each lengths as l (l.value)}
                    <option value={l.value}>{l.label}</option>
                {/each}
            </select>
        {/if}
    </div>
    <LogbookLoadError />
    {#if draft.source === 'new' && logbook.noActiveStation}
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
                >{importer.previewing ? m.transfer_reading() : m.transfer_preview_again()}</Button
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
