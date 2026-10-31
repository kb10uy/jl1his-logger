<script lang="ts">
    import * as api from '#lib/api/api.ts';
    import type { Entity, Qso } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import QslBadge from '#lib/components/ui/QslBadge.svelte';
    import { m } from '#lib/paraglide/messages.js';
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
        api.lookup(logbook.logbook.id, call, undefined, controller.signal)
            .then((found) => (entity = found.entity))
            .catch(() => (entity = null));
        return () => controller.abort();
    });

    const join = (...values: (string | undefined)[]) =>
        values.filter((v) => v !== undefined && v.trim() !== '').join(' · ') || '—';
    const pair = (a: string | undefined, b: string | undefined) => `${a || '—'} / ${b || '—'}`;

    const SHOWN = new Set([
        'CALL',
        'COUNTRY',
        'QSO_DATE',
        'TIME_ON',
        'BAND',
        'MODE',
        'SUBMODE',
        'FREQ',
        'RST_SENT',
        'RST_RCVD',
        'STATION_CALLSIGN',
        'MY_RIG',
        'MY_ANTENNA',
        'TX_PWR',
        'MY_POTA_REF',
        'NAME',
        'QTH',
        'STATE',
        'GRIDSQUARE',
        'DXCC',
        'CQZ',
        'ITUZ',
        'CONTEST_ID',
        'STX_STRING',
        'SRX_STRING',
        'QSL_SENT',
        'QSL_SENT_VIA',
        'QSLSDATE',
        'QSL_RCVD',
        'QSL_RCVD_VIA',
        'QSLRDATE',
        'LOTW_QSL_SENT',
        'LOTW_QSL_RCVD',
        'EQSL_QSL_SENT',
        'EQSL_QSL_RCVD',
        'COMMENT',
        'NOTES'
    ]);
    const area = $derived(logbook.area);
    const others = $derived(
        Object.entries(f).filter(([name]) => !SHOWN.has(name) && name !== area?.name)
    );
</script>

<aside
    aria-label={m.log_detail_label()}
    class="absolute inset-y-0 right-0 z-10 flex w-[380px] max-w-full flex-col gap-3 overflow-y-auto border-l border-border bg-surface px-4 pt-3 pb-4 shadow-[-12px_0_32px] shadow-shadow"
>
    <div class="flex items-center justify-between gap-2">
        <span class="eyebrow">QSO</span>
        <div class="flex gap-2">
            <Button size="sm" href={`/logbooks/${logbook.logbook.id}/qsos/${qso.id}`}
                >{m.common_edit()}</Button
            >
            <button
                type="button"
                aria-label={m.log_detail_close()}
                class="inline-flex size-8 items-center justify-center rounded-md border border-border text-muted"
                onclick={onclose}
            >
                <Icon name="close" size={14} strokeWidth={2.5} />
            </button>
        </div>
    </div>
    {#if stale}
        <p class="rounded-md bg-warn-bg px-2.5 py-1.5 text-2xs text-warn">
            {qso.deleted_at ? m.log_detail_deleted() : m.log_detail_stale()}
        </p>
    {/if}
    <div class="flex flex-col gap-1">
        <span class="font-mono text-2xl font-semibold tracking-wide">{f.CALL}</span>
        <span class="text-muted"
            >{entity?.name ?? f.COUNTRY ?? (entity === undefined ? '…' : '—')}</span
        >
    </div>
    <dl class="m-0 grid grid-cols-2 gap-x-3 gap-y-2 text-sm wrap-anywhere">
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_date_utc()}</dt>
            <dd class="m-0 font-mono">{isoDate(f.QSO_DATE)} {hhmmss(f.TIME_ON)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_band_mode()}</dt>
            <dd class="m-0 font-mono">
                {f.BAND}
                {f.MODE}
                {#if f.SUBMODE}<span class="text-muted">{f.SUBMODE}</span>{/if}
            </dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_frequency()}</dt>
            <dd class={['m-0 font-mono', !f.FREQ && 'text-muted']}>
                {f.FREQ ? `${f.FREQ} MHz` : m.log_detail_not_recorded()}
            </dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.log_detail_rst()}</dt>
            <dd class="m-0 font-mono">{pair(f.RST_SENT, f.RST_RCVD)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_station()}</dt>
            <dd class="m-0">{join(f.STATION_CALLSIGN, logbook.stationName(qso.station_id))}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_instrument()}</dt>
            <dd class="m-0">
                {join(logbook.instrumentName(qso.instrument_id), f.MY_RIG, f.MY_ANTENNA)}
            </dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_tx_power()}</dt>
            <dd class={['m-0 font-mono', !f.TX_PWR && 'text-muted']}>
                {f.TX_PWR ? `${f.TX_PWR} W` : '—'}
            </dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.field_park()}</dt>
            <dd class="m-0 font-mono text-info">{parks(f).join(', ') || '—'}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.log_detail_name_qth()}</dt>
            <dd class="m-0">{join(f.NAME, f.QTH)}</dd>
        </div>
        {#if area}
            <div class="flex flex-col gap-0.5">
                <dt class="text-2xs text-muted">{area.label}</dt>
                <dd class="m-0 font-mono">{f[area.name] || '—'}</dd>
            </div>
        {/if}
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">{m.log_detail_state_grid()}</dt>
            <dd class="m-0 font-mono">{join(f.STATE, f.GRIDSQUARE)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
            <dt class="text-2xs text-muted">DXCC / CQ / ITU</dt>
            <dd class="m-0 font-mono">{f.DXCC || '—'} / {f.CQZ || '—'} / {f.ITUZ || '—'}</dd>
        </div>
        {#if f.CONTEST_ID || f.STX_STRING || f.SRX_STRING}
            <div class="flex flex-col gap-0.5">
                <dt class="text-2xs text-muted">{m.field_contest()}</dt>
                <dd class="m-0 font-mono">{f.CONTEST_ID || '—'}</dd>
            </div>
            <div class="flex flex-col gap-0.5">
                <dt class="text-2xs text-muted">{m.log_detail_number()}</dt>
                <dd class="m-0 font-mono">{pair(f.STX_STRING, f.SRX_STRING)}</dd>
            </div>
        {/if}
    </dl>
    <div class="flex flex-col gap-1.5 rounded-lg border border-border bg-bg p-2.5 text-sm">
        <span class="eyebrow">QSL</span>
        <div class="flex items-center justify-between">
            <span>{m.log_sent()}</span>
            <QslBadge value={f.QSL_SENT} label={qslLabel(f.QSL_SENT, true)} />
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>{m.log_via_date()}</span>
            <span class="font-mono">{f.QSL_SENT_VIA || '—'} · {isoDate(f.QSLSDATE) || '—'}</span>
        </div>
        <div class="flex items-center justify-between">
            <span>{m.log_received()}</span>
            <QslBadge value={f.QSL_RCVD} label={qslLabel(f.QSL_RCVD, false)} />
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>{m.log_via_date()}</span>
            <span class="font-mono">{f.QSL_RCVD_VIA || '—'} · {isoDate(f.QSLRDATE) || '—'}</span>
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>{m.log_detail_lotw()}</span>
            <span class="font-mono">{pair(f.LOTW_QSL_SENT, f.LOTW_QSL_RCVD)}</span>
        </div>
        <div class="flex items-center justify-between text-muted">
            <span>{m.log_detail_eqsl()}</span>
            <span class="font-mono">{pair(f.EQSL_QSL_SENT, f.EQSL_QSL_RCVD)}</span>
        </div>
    </div>
    <div class="flex flex-col gap-1.5 text-2xs text-muted">
        <span>{m.field_comment()}</span>
        <p class="m-0 text-sm text-text">{f.COMMENT || '—'}</p>
    </div>
    {#if f.NOTES}
        <div class="flex flex-col gap-1.5 text-2xs text-muted">
            <span>{m.field_notes()}</span>
            <p class="m-0 text-sm whitespace-pre-line text-text">{f.NOTES}</p>
        </div>
    {/if}
    {#if others.length > 0}
        <details class="rounded-lg border border-border bg-bg p-2.5 text-sm">
            <summary class="cursor-pointer"
                >{m.log_detail_other_fields({ count: others.length })}</summary
            >
            <div class="mt-2 flex flex-col gap-1">
                {#each others as [name, value] (name)}
                    <span class="break-all whitespace-pre-line"
                        ><span class="font-mono text-2xs text-muted">{name}</span>
                        {value}</span
                    >
                {/each}
            </div>
        </details>
    {/if}
    <div class="flex-1"></div>
    <span class="font-mono text-2xs text-muted"
        >{qso.id} · {m.log_detail_rev({ revision: qso.revision })} · {qso.updated_by_callsign ??
            ''}</span
    >
</aside>
