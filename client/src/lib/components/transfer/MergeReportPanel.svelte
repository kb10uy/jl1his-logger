<script lang="ts">
    import type { MergeReport } from '#lib/api/types.ts';
    import ImportActions from '#lib/components/transfer/ImportActions.svelte';
    import RecordList from '#lib/components/transfer/RecordList.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Stat from '#lib/components/ui/Stat.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { differencesText } from '#lib/transfer/transfer.ts';

    let {
        report,
        applied,
        applying,
        previewing,
        logbookName,
        qsoHref,
        onapply
    }: {
        report: MergeReport;
        applied: MergeReport | undefined;
        applying: boolean;
        previewing: boolean;
        logbookName: string;
        qsoHref: (id: string) => string;
        onapply: () => void;
    } = $props();
</script>

<div class="grid grid-cols-3 gap-2">
    <Stat label={m.transfer_stat_read()} value={report.read} />
    <Stat
        label={applied ? m.transfer_qsl_merged() : m.transfer_qsl_to_merge()}
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
        <p class="pb-3 text-sm text-muted">{m.transfer_qsl_differences_help()}</p>
    </details>
{/if}
<ImportActions
    {report}
    {logbookName}
    done={applied ? m.transfer_qsl_merged_count({ count: applied.merged }) : undefined}
    label={applying
        ? m.transfer_qsl_merging()
        : m.transfer_qsl_merge_count({ count: report.merged })}
    disabled={report.merged === 0 || applying || previewing}
    {onapply}
/>
