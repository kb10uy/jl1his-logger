<script lang="ts">
    import type { NewReport } from '#lib/api/types.ts';
    import ImportActions from '#lib/components/transfer/ImportActions.svelte';
    import RecordList from '#lib/components/transfer/RecordList.svelte';
    import Stat from '#lib/components/ui/Stat.svelte';
    import { m } from '#lib/paraglide/messages.js';

    let {
        report,
        applied,
        applying,
        previewing,
        logbookName,
        onapply
    }: {
        report: NewReport;
        applied: NewReport | undefined;
        applying: boolean;
        previewing: boolean;
        logbookName: string;
        onapply: () => void;
    } = $props();
</script>

<div class="grid grid-cols-3 gap-2">
    <Stat label={m.transfer_stat_read()} value={report.read} />
    <Stat
        label={applied ? m.transfer_stat_created() : m.transfer_stat_new()}
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
        tone={report.outside_band.length + report.modes.length > 0 ? 'warn' : undefined}
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
    {logbookName}
    done={applied ? m.transfer_imported({ count: applied.created }) : undefined}
    label={applying ? m.transfer_importing() : m.transfer_import_count({ count: report.created })}
    disabled={report.created === 0 || applying || previewing}
    {onapply}
/>
