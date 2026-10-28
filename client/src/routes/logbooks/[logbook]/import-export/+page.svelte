<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Batch, Encoding, LengthUnit, NewReport, Unencodable } from '#lib/api/types.ts';
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
    import { download, fromBase64 } from '#lib/download.ts';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import StationSelect from '#lib/components/StationSelect.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { Matches } from '#lib/state/matches.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import {
        ENCODING_NAMES,
        LENGTH_LABELS,
        MAX_IMPORT,
        fileName,
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
    let report = $state<NewReport | undefined>(undefined);
    let reportError = $state<string | undefined>(undefined);
    let previewing = $state(false);
    let importing = $state(false);
    let imported = $state<NewReport | undefined>(undefined);
    let attempt = 0;

    let kind = $state<'adif' | 'pota'>('adif');
    let q = $state('');
    let park = $state('');
    let exportLength = $state<LengthUnit>('codepoints');
    let exportEncoding = $state<Encoding>('utf-8');
    let exportUnencodable = $state<Unencodable>('error');
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
        const mine = ++attempt;
        previewing = false;
        if (file.size > MAX_IMPORT) {
            report = undefined;
            reportError = m.transfer_file_too_large();
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
                    length: format === 'adi' && importLength !== 'auto' ? importLength : undefined,
                    encoding:
                        format === 'adi' && importEncoding !== 'auto' ? importEncoding : undefined,
                    apply
                },
                file
            );
            if (apply) {
                toasts.show(m.transfer_imported({ count: result.created }));
                matches.refresh();
            }
            if (mine !== attempt) return;
            report = result;
            imported = apply ? result : undefined;
        } catch (e) {
            if (mine === attempt) {
                report = undefined;
                reportError = (e as Error).message;
            } else if (apply) {
                toasts.error((e as Error).message);
            }
        } finally {
            if (apply) importing = false;
            if (mine === attempt) previewing = false;
        }
    }

    $effect(() => {
        void station;
        void instrument;
        void importLength;
        void importEncoding;
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
                length: exportEncoding === 'utf-8' ? exportLength : undefined,
                encoding: exportEncoding,
                unencodable: exportEncoding === 'utf-8' ? undefined : exportUnencodable
            });
            if (result.batch === null || result.adif === null) {
                toasts.show(m.transfer_export_nothing());
                return;
            }
            download(
                fileName(logbook.logbook.name, kind, new Date()),
                result.file === null ? result.adif : fromBase64(result.file)
            );
            batches = [result.batch, ...batches];
            toasts.show(m.transfer_exported({ count: result.records }));
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            exporting = false;
        }
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
        <div
            role="tablist"
            aria-label={m.transfer_direction()}
            class="grid grid-cols-2 gap-1.5 md:hidden"
        >
            {#each [['import', m.transfer_import()], ['export', m.transfer_export()]] as [id, label] (id)}
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
                    onclick={() => (tab = id as 'import' | 'export')}>{label}</button
                >
            {/each}
        </div>
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <div class="flex flex-wrap items-start gap-4">
        <section
            aria-label={m.transfer_import()}
            class={[
                'flex min-w-0 flex-[1_1_440px] flex-col gap-3 rounded-lg border border-border bg-surface p-4',
                tab !== 'import' && 'max-md:hidden'
            ]}
        >
            <h2 class="text-[15px] font-semibold">{m.transfer_import_new_title()}</h2>
            <FileDrop bind:file />
            <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                <label for="imp-station" class="text-xs text-muted">{m.field_station()}</label>
                <StationSelect
                    id="imp-station"
                    class="h-10"
                    value={station}
                    stations={logbook.activeStations}
                    onchange={(id) => (stationId = id)}
                />
                <label for="imp-instrument" class="text-xs text-muted">{m.field_instrument()}</label
                >
                <InstrumentSelect
                    id="imp-instrument"
                    class="h-10"
                    value={instrument}
                    instruments={logbook.activeInstruments}
                    onchange={(id) => (instrumentId = id)}
                />
                {#if format === 'adi'}
                    <label for="imp-encoding" class="text-xs text-muted"
                        >{m.transfer_encoding()}</label
                    >
                    <select
                        id="imp-encoding"
                        class="control h-10"
                        value={importEncoding}
                        onchange={(e) =>
                            (importEncoding = e.currentTarget.value as Encoding | 'auto')}
                    >
                        <option value="auto">{m.transfer_encoding_auto()}</option>
                        {#each encodings as [value, name] (value)}
                            <option {value}>{name}</option>
                        {/each}
                    </select>
                    <label for="imp-len" class="text-xs text-muted">{m.transfer_lengths()}</label>
                    <select id="imp-len" class="control h-10" bind:value={importLength}>
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

            {#if reportError}
                <Notice tone="error">{reportError}</Notice>
            {/if}
            {#if report}
                <div class="flex items-center gap-2">
                    <span class="text-xs text-muted"
                        >{imported ? m.transfer_imported_label() : m.transfer_preview_note()} · {readingNote(
                            report
                        )}</span
                    >
                    <div class="flex-1"></div>
                    <Button size="sm" disabled={previewing} onclick={() => run(false)}
                        >{previewing ? m.transfer_reading() : m.transfer_preview_again()}</Button
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
                <div class="flex flex-wrap items-center gap-2">
                    {#if report.rejects}
                        <Button
                            variant="link"
                            onclick={() =>
                                download(
                                    fileName(logbook.logbook.name, 'adif', new Date(), '-rejects'),
                                    report?.rejects ?? ''
                                )}
                            >{m.transfer_download_rejects({ count: report.invalid.length })}</Button
                        >
                    {/if}
                    <div class="flex-1"></div>
                    {#if imported}
                        <span
                            class="flex h-10 items-center rounded-md bg-ok-bg px-4 font-bold text-ok"
                            >{m.transfer_imported({ count: imported.created })}</span
                        >
                    {:else}
                        <Button
                            variant="primary"
                            size="lg"
                            disabled={report.created === 0 || importing || previewing}
                            onclick={() => run(true)}
                            >{importing
                                ? m.transfer_importing()
                                : m.transfer_import_count({ count: report.created })}</Button
                        >
                    {/if}
                </div>
            {:else if previewing}
                <span class="text-xs text-muted">{m.transfer_reading_file()}</span>
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
                <h2 class="text-[15px] font-semibold">{m.transfer_export()}</h2>
                <Segmented
                    label={m.transfer_kind()}
                    options={[
                        { value: 'adif', label: 'ADIF' },
                        { value: 'pota', label: 'POTA' }
                    ]}
                    bind:value={kind}
                />
                <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                    <label for="exp-query" class="text-xs text-muted">{m.transfer_query()}</label>
                    <input
                        id="exp-query"
                        class="control h-10 font-mono"
                        placeholder={m.transfer_query_everything()}
                        spellcheck="false"
                        autocomplete="off"
                        autocapitalize="off"
                        bind:value={q}
                    />
                    {#if kind === 'pota'}
                        <label for="exp-park" class="text-xs text-muted">{m.transfer_park()}</label>
                        <input
                            id="exp-park"
                            class="control h-10 font-mono uppercase"
                            placeholder={m.transfer_park_every()}
                            spellcheck="false"
                            autocomplete="off"
                            bind:value={park}
                        />
                    {/if}
                    <label for="exp-encoding" class="text-xs text-muted"
                        >{m.transfer_encoding()}</label
                    >
                    <select id="exp-encoding" class="control h-10" bind:value={exportEncoding}>
                        {#each encodings as [value, name] (value)}
                            <option {value}>{name}</option>
                        {/each}
                    </select>
                    {#if exportEncoding === 'utf-8'}
                        <label for="exp-len" class="text-xs text-muted"
                            >{m.transfer_lengths()}</label
                        >
                        <select id="exp-len" class="control h-10" bind:value={exportLength}>
                            {#each lengths as l (l.value)}
                                <option value={l.value}>{l.label}</option>
                            {/each}
                        </select>
                    {:else}
                        <label for="exp-unencodable" class="text-xs text-muted"
                            >{m.transfer_unencodable()}</label
                        >
                        <select
                            id="exp-unencodable"
                            class="control h-10"
                            bind:value={exportUnencodable}
                        >
                            {#each unencodables as u (u.value)}
                                <option value={u.value}>{u.label}</option>
                            {/each}
                        </select>
                    {/if}
                </div>
                {#if matches.error}
                    <Notice tone="error">{matches.error}</Notice>
                {/if}
                <div class="flex flex-wrap items-center gap-3">
                    <span class="text-[13px]"
                        ><Slotted
                            text={kind === 'pota'
                                ? m.transfer_qsos_match_pota({ total: SLOT })
                                : m.transfer_qsos_match({ total: SLOT })}
                            ><strong class="font-mono text-base">{matches.total ?? '…'}</strong
                            ></Slotted
                        ></span
                    >
                    <div class="flex-1"></div>
                    <Button
                        variant="primary"
                        size="lg"
                        disabled={exporting || matches.error !== undefined || matches.total === 0}
                        onclick={runExport}
                        >{exporting ? m.transfer_exporting() : m.transfer_download_adif()}</Button
                    >
                </div>
            </section>

            <section aria-label={m.transfer_recent_exports()} class="flex flex-col gap-2">
                <h2 class="eyebrow">{m.transfer_recent_exports()}</h2>
                <BatchTable batches={exports} {callsigns} />
            </section>
        </div>
    </div>
</main>
