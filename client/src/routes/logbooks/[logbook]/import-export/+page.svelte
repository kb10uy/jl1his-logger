<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Batch, LengthUnit, NewReport } from '#lib/api/types.ts';
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
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import { count, stationLabel } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import { MAX_IMPORT, fileName, formatOf } from '#lib/transfer/transfer.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;

    let tab = $state<'import' | 'export'>('import');

    let file = $state<File | undefined>(undefined);
    let stationId = $state('');
    let instrumentId = $state('');
    let importLength = $state<LengthUnit>('codepoints');
    let report = $state<NewReport | undefined>(undefined);
    let reportError = $state<string | undefined>(undefined);
    let previewing = $state(false);
    let importing = $state(false);
    let imported = $state<NewReport | undefined>(undefined);

    let kind = $state<'adif' | 'pota'>('adif');
    let q = $state('');
    let park = $state('');
    let exportLength = $state<LengthUnit>('codepoints');
    let exporting = $state(false);
    let batches = $state<Batch[]>([]);
    let callsigns = $state<Record<string, string>>({});

    const station = $derived(
        logbook.activeStations.some((s) => s.id === stationId)
            ? stationId
            : (logbook.activeStations[0]?.id ?? '')
    );
    const instrument = $derived(logbook.activeInstrumentId(instrumentId));
    const format = $derived(file ? formatOf(file.name) : 'adi');
    const matches = new Matches(logbookId, () => q);
    const exports = $derived(batches.filter((b) => b.kind === 'adif' || b.kind === 'pota'));

    onMount(() => {
        api.batches(logbookId)
            .then((found) => (batches = found))
            .catch(() => undefined);
        api.members(logbookId)
            .then((found) => {
                callsigns = Object.fromEntries(found.map((m) => [m.user_id, m.callsign]));
            })
            .catch(() => undefined);
    });

    async function run(apply: boolean) {
        if (file === undefined || station === '') return;
        if (file.size > MAX_IMPORT) {
            reportError = 'The file is larger than 64 MiB.';
            return;
        }
        reportError = undefined;
        if (apply) importing = true;
        else previewing = true;
        try {
            const result = await api.importAdif(
                logbookId,
                'new',
                {
                    station_id: station,
                    instrument_id: instrument || undefined,
                    format,
                    length: format === 'adi' ? importLength : undefined,
                    apply
                },
                file
            );
            report = result;
            imported = apply ? result : undefined;
            if (apply) {
                toasts.show(`Imported ${count(result.created, 'QSO')}`);
                matches.refresh();
            }
        } catch (e) {
            report = undefined;
            reportError = (e as Error).message;
        } finally {
            previewing = false;
            importing = false;
        }
    }

    $effect(() => {
        void station;
        void instrument;
        void importLength;
        if (file === undefined) return;
        untrack(() => {
            imported = undefined;
            run(false);
        });
    });

    async function runExport() {
        exporting = true;
        try {
            const result = await api.createExport(logbookId, {
                kind,
                q: q.trim() || undefined,
                park: kind === 'pota' && park.trim() !== '' ? park.trim().toUpperCase() : undefined,
                length: exportLength
            });
            if (result.batch === null || result.adif === null) {
                toasts.show('No QSO matches, so nothing was exported.');
                return;
            }
            download(fileName(logbook.logbook.name, kind, new Date()), result.adif);
            batches = [result.batch, ...batches];
            toasts.show(`Exported ${count(result.records, 'record')}`);
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            exporting = false;
        }
    }

    const lengths: { value: LengthUnit; label: string }[] = [
        { value: 'codepoints', label: 'Codepoints' },
        { value: 'bytes', label: 'Bytes' }
    ];
</script>

<PageTitle title="Import / Export" />

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
        <h1 class="text-xl font-semibold">Import / Export</h1>
    </header>
    <div role="tablist" aria-label="Direction" class="grid grid-cols-2 gap-1.5 md:hidden">
        {#each [['import', 'Import'], ['export', 'Export']] as [id, label] (id)}
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
                onclick={() => (tab = id as 'import' | 'export')}>{label}</button
            >
        {/each}
    </div>

    <div class="flex flex-wrap items-start gap-4">
        <section
            aria-label="Import"
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-3 rounded-lg border border-border bg-surface p-4',
                tab !== 'import' && 'max-md:hidden'
            ]}
        >
            <h2 class="text-[15px] font-semibold">Import new QSOs</h2>
            <FileDrop bind:file />
            <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                <label for="imp-station" class="text-xs text-muted">Station</label>
                <select
                    id="imp-station"
                    class="control h-10"
                    bind:value={() => station, (v) => (stationId = v)}
                >
                    {#each logbook.activeStations as s (s.id)}
                        <option value={s.id}>{stationLabel(s)}</option>
                    {/each}
                </select>
                <label for="imp-instrument" class="text-xs text-muted">Instrument</label>
                <InstrumentSelect
                    id="imp-instrument"
                    class="h-10"
                    value={instrument}
                    instruments={logbook.activeInstruments}
                    onchange={(id) => (instrumentId = id)}
                />
                {#if format === 'adi'}
                    <span class="text-xs text-muted">Lengths</span>
                    {@render lengthChoice('imp-len', importLength, (v) => (importLength = v))}
                {/if}
            </div>
            {#if logbook.activeStations.length === 0}
                <Notice>Create a Station first; new QSOs are recorded at one.</Notice>
            {/if}

            {#if reportError}
                <Notice tone="error">{reportError}</Notice>
            {/if}
            {#if report}
                <div class="flex items-center gap-2">
                    <span class="text-xs text-muted"
                        >{imported ? 'Imported' : 'Preview · nothing written yet'}</span
                    >
                    <div class="flex-1"></div>
                    <Button size="sm" disabled={previewing} onclick={() => run(false)}
                        >{previewing ? 'Reading…' : 'Preview again'}</Button
                    >
                </div>
                <div class="grid grid-cols-3 gap-2">
                    <Stat label="Read" value={report.read} />
                    <Stat label={imported ? 'Created' : 'New'} value={report.created} tone="ok" />
                    <Stat label="Already imported" value={report.existing} />
                    <Stat
                        label="Possible duplicates"
                        value={report.duplicates.length}
                        tone={report.duplicates.length > 0 ? 'warn' : undefined}
                    />
                    <Stat
                        label="Outside band · modes"
                        value={`${report.outside_band.length} · ${report.modes.length}`}
                        tone={report.outside_band.length + report.modes.length > 0
                            ? 'warn'
                            : undefined}
                    />
                    <Stat
                        label="Invalid"
                        value={report.invalid.length}
                        tone={report.invalid.length > 0 ? 'error' : undefined}
                    />
                </div>
                <RecordList
                    title="Possible duplicates"
                    note="imported anyway"
                    open
                    items={report.duplicates.map((d) => ({
                        record: d.record,
                        text: `≈ ${count(d.ids.length, 'other QSO')}`
                    }))}
                />
                <RecordList
                    title="Outside band"
                    note="kept as they are"
                    items={report.outside_band.map((o) => ({
                        record: o.record,
                        text: `${o.freq} MHz is not in ${o.band}`
                    }))}
                />
                <RecordList
                    title="Modes"
                    note="kept as they are"
                    items={report.modes.map((m) => ({
                        record: m.record,
                        text:
                            m.kind === 'unknown_mode'
                                ? `MODE ${m.mode} is unknown`
                                : m.kind === 'unknown_submode'
                                  ? `SUBMODE ${m.submode} is unknown`
                                  : m.kind === 'submode_of_another_mode'
                                    ? `SUBMODE ${m.submode} belongs to ${m.expected}`
                                    : m.kind
                    }))}
                />
                <RecordList
                    title="Invalid"
                    note="skipped"
                    items={report.invalid.map((i) => ({ record: i.record, text: i.error }))}
                />
                <div class="flex flex-wrap items-center gap-2">
                    {#if report.rejects}
                        <Button
                            variant="link"
                            onclick={() =>
                                download(
                                    fileName(logbook.logbook.name, 'adif', new Date(), '-rejects'),
                                    report?.rejects ?? ''
                                )}
                            >Download rejects ({count(report.invalid.length, 'record')})</Button
                        >
                    {/if}
                    <div class="flex-1"></div>
                    {#if imported}
                        <span
                            class="flex h-10 items-center rounded-md bg-ok-bg px-4 font-bold text-ok"
                            >Imported {count(imported.created, 'QSO')}</span
                        >
                    {:else}
                        <Button
                            variant="primary"
                            size="lg"
                            disabled={report.created === 0 || importing || previewing}
                            onclick={() => run(true)}
                            >{importing
                                ? 'Importing…'
                                : `Import ${count(report.created, 'QSO')}`}</Button
                        >
                    {/if}
                </div>
            {:else if previewing}
                <span class="text-xs text-muted">Reading the file…</span>
            {/if}
        </section>

        <div
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-4',
                tab !== 'export' && 'max-md:hidden'
            ]}
        >
            <section
                aria-label="Export"
                class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
            >
                <h2 class="text-[15px] font-semibold">Export</h2>
                <Segmented
                    label="Kind"
                    options={[
                        { value: 'adif', label: 'ADIF' },
                        { value: 'pota', label: 'POTA' }
                    ]}
                    bind:value={kind}
                />
                <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                    <label for="exp-query" class="text-xs text-muted">Query</label>
                    <input
                        id="exp-query"
                        class="control h-10 font-mono"
                        placeholder="everything"
                        spellcheck="false"
                        autocomplete="off"
                        autocapitalize="off"
                        bind:value={q}
                    />
                    {#if kind === 'pota'}
                        <label for="exp-park" class="text-xs text-muted">Park</label>
                        <input
                            id="exp-park"
                            class="control h-10 font-mono uppercase"
                            placeholder="every park"
                            spellcheck="false"
                            autocomplete="off"
                            bind:value={park}
                        />
                    {/if}
                    <span class="text-xs text-muted">Lengths</span>
                    {@render lengthChoice('exp-len', exportLength, (v) => (exportLength = v))}
                </div>
                {#if matches.error}
                    <Notice tone="error">{matches.error}</Notice>
                {/if}
                <div class="flex flex-wrap items-center gap-3">
                    <span class="text-[13px]"
                        ><strong class="font-mono text-base">{matches.total ?? '…'}</strong>
                        {kind === 'pota' ? 'QSOs match, one record per park' : 'QSOs match'}</span
                    >
                    <div class="flex-1"></div>
                    <Button
                        variant="primary"
                        size="lg"
                        disabled={exporting || matches.error !== undefined || matches.total === 0}
                        onclick={runExport}>{exporting ? 'Exporting…' : 'Download ADIF'}</Button
                    >
                </div>
            </section>

            <section aria-label="Recent exports" class="flex flex-col gap-2">
                <h2 class="eyebrow">Recent exports</h2>
                <BatchTable batches={exports} {callsigns} />
            </section>
        </div>
    </div>
</main>
