<script lang="ts">
    import * as api from '#lib/api/api.ts';
    import type { Entity, Qso } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import QslBadge from '#lib/components/ui/QslBadge.svelte';
    import { hhmmss, isoDate, parks } from '#lib/qso/format.ts';
    import { qslLabel } from '#lib/qso/qsl.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';

    let {
        qso,
        stale = false,
        onclose
    }: { qso: Qso; stale?: boolean; onclose: () => void } = $props();

    const logbook = useLogbook();
    const f = $derived(qso.fields);
    let entity = $state<Entity | null | undefined>(undefined);

    $effect(() => {
        const call = qso.fields.CALL ?? '';
        const controller = new AbortController();
        entity = undefined;
        api.lookup(logbook.logbook.id, call, controller.signal)
            .then((found) => (entity = found.entity))
            .catch(() => (entity = null));
        return () => controller.abort();
    });

    const join = (...values: (string | undefined)[]) =>
        values.filter((v) => v !== undefined && v.trim() !== '').join(' · ') || '—';
    const pair = (a: string | undefined, b: string | undefined) => `${a || '—'} / ${b || '—'}`;
</script>

<aside
    aria-label="QSO details"
    class="absolute inset-y-0 right-0 z-10 flex w-[380px] max-w-full flex-col gap-4 overflow-y-auto border-l border-border bg-surface px-6 pt-4 pb-6 shadow-[-12px_0_32px] shadow-shadow"
>
    <div class="flex items-center justify-between gap-2">
        <span class="eyebrow">QSO</span>
        <div class="flex gap-2">
            <Button size="sm" href={`/logbooks/${logbook.logbook.id}/qsos/${qso.id}`}>Edit</Button>
            <button
                type="button"
                aria-label="Close details"
                class="inline-flex size-8 items-center justify-center rounded-md border border-border text-muted"
                onclick={onclose}
            >
                <Icon name="close" size={14} strokeWidth={2.5} />
            </button>
        </div>
    </div>
    {#if stale}
        <p class="rounded-md bg-warn-bg px-3 py-2 text-xs text-warn">
            {qso.deleted_at
                ? 'This QSO was deleted.'
                : 'This QSO changed and no longer matches the query.'}
        </p>
    {/if}
    <div class="flex flex-col gap-1">
        <span class="font-mono text-[28px] font-semibold tracking-wide">{f.CALL}</span>
        <span class="text-muted"
            >{entity?.name ?? f.COUNTRY ?? (entity === undefined ? '…' : '—')}</span
        >
    </div>
    <dl class="m-0 grid grid-cols-2 gap-x-4 gap-y-3 text-[13px]">
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">Date / UTC</dt>
            <dd class="m-0 font-mono">{isoDate(f.QSO_DATE)} {hhmmss(f.TIME_ON)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">Band / Mode</dt>
            <dd class="m-0 font-mono">
                {f.BAND}
                {f.MODE}
                {#if f.SUBMODE}<span class="text-muted">{f.SUBMODE}</span>{/if}
            </dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">Frequency</dt>
            <dd class={['m-0 font-mono', !f.FREQ && 'text-muted']}>
                {f.FREQ ? `${f.FREQ} MHz` : 'not recorded'}
            </dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">RST sent / rcvd</dt>
            <dd class="m-0 font-mono">{pair(f.RST_SENT, f.RST_RCVD)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">Station</dt>
            <dd class="m-0">{join(f.STATION_CALLSIGN, logbook.stationName(qso.station_id))}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">Instrument</dt>
            <dd class="m-0">{join(logbook.instrumentName(qso.instrument_id), f.MY_RIG)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">My park</dt>
            <dd class="m-0 font-mono text-info">{parks(f).join(', ') || '—'}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">Name / QTH</dt>
            <dd class="m-0">{join(f.NAME, f.QTH)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted">DXCC / CQ / ITU</dt>
            <dd class="m-0 font-mono">{f.DXCC || '—'} / {f.CQZ || '—'} / {f.ITUZ || '—'}</dd>
        </div>
        {#if f.CONTEST_ID || f.STX_STRING || f.SRX_STRING}
            <div class="flex flex-col gap-0.5">
                <dt class="text-xs text-muted">Contest</dt>
                <dd class="m-0 font-mono">{f.CONTEST_ID || '—'}</dd>
            </div>
            <div class="flex flex-col gap-0.5">
                <dt class="text-xs text-muted">Number sent / rcvd</dt>
                <dd class="m-0 font-mono">{pair(f.STX_STRING, f.SRX_STRING)}</dd>
            </div>
        {/if}
    </dl>
    <div class="flex flex-col gap-2 rounded-lg border border-border bg-bg p-3 text-[13px]">
        <span class="eyebrow">QSL</span>
        <div class="flex items-center justify-between">
            <span>Sent</span>
            <QslBadge value={f.QSL_SENT} label={qslLabel(f.QSL_SENT, true)} />
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>Via / date</span>
            <span class="font-mono">{f.QSL_SENT_VIA || '—'} · {isoDate(f.QSLSDATE) || '—'}</span>
        </div>
        <div class="flex items-center justify-between">
            <span>Received</span>
            <QslBadge value={f.QSL_RCVD} label={qslLabel(f.QSL_RCVD, false)} />
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>LoTW sent / rcvd</span>
            <span class="font-mono">{pair(f.LOTW_QSL_SENT, f.LOTW_QSL_RCVD)}</span>
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>eQSL sent / rcvd</span>
            <span class="font-mono">{pair(f.EQSL_QSL_SENT, f.EQSL_QSL_RCVD)}</span>
        </div>
    </div>
    <div class="flex flex-col gap-1.5 text-xs text-muted">
        <span>Comment</span>
        <p class="m-0 text-[13px] text-text">{f.COMMENT || '—'}</p>
    </div>
    <div class="flex-1"></div>
    <span class="font-mono text-[11px] text-muted"
        >{qso.id} · rev {qso.revision} · {qso.updated_by_callsign ?? ''}</span
    >
</aside>
