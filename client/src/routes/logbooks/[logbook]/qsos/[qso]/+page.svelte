<script lang="ts">
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import { ApiError } from '#lib/api/client.ts';
    import type { Entity, Qso, Warning } from '#lib/api/types.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import WarningList from '#lib/components/WarningList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Field from '#lib/components/ui/Field.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import {
        draftOf,
        extraFieldNames,
        fieldsOf,
        isStationField,
        problem as problemOf,
        type Draft
    } from '#lib/qso/edit.ts';
    import { QSL_RCVD_VALUES, QSL_SENT_VALUES, QSL_VIA_VALUES } from '#lib/qso/qsl.ts';
    import { withBand, withFreq, withMode } from '#lib/qso/tuning.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const qsoId = $derived(page.params.qso ?? '');
    const logHref = `/logbooks/${logbookId}`;

    let qso = $state<Qso | undefined>(undefined);
    let draft = $state<Draft | undefined>(undefined);
    let instrumentId = $state('');
    let loadError = $state<string | undefined>(undefined);
    let saveError = $state<string | undefined>(undefined);
    let conflict = $state<Qso | undefined>(undefined);
    let warnings = $state<Warning[]>([]);
    let saving = $state(false);
    let confirmDelete = $state(false);
    let entity = $state<Entity | null | undefined>(undefined);

    const updated = $derived(qso?.updated_at.replace('T', ' ').slice(0, 16) ?? '');

    $effect(() => {
        const id = qsoId;
        api.getQso(logbookId, id)
            .then((found) => reset(found))
            .catch((e) => (loadError = e instanceof Error ? e.message : String(e)));
    });

    $effect(() => {
        const call = draft?.call.trim() ?? '';
        if (call === '') {
            entity = undefined;
            return;
        }
        const controller = new AbortController();
        const timer = setTimeout(() => {
            api.lookup(logbookId, call, controller.signal)
                .then((found) => (entity = found.entity))
                .catch(() => undefined);
        }, 300);
        return () => {
            clearTimeout(timer);
            controller.abort();
        };
    });

    function reset(found: Qso) {
        qso = found;
        draft = draftOf(found.fields, logbook.area);
        instrumentId = found.instrument_id ?? '';
        conflict = undefined;
        saveError = undefined;
    }

    function back() {
        if (history.length > 1) history.back();
        else goto(logHref);
    }

    const problem = $derived(draft === undefined ? undefined : problemOf(draft, logbook.area));
    const instruments = $derived(
        logbook.instruments.filter((i) => i.archived_at === null || i.id === qso?.instrument_id)
    );
    const station = $derived(
        qso === undefined ? [] : Object.entries(qso.fields).filter(([name]) => isStationField(name))
    );

    async function save() {
        if (qso === undefined || draft === undefined || problem !== undefined) return;
        saving = true;
        saveError = undefined;
        try {
            const saved = await api.updateQso(
                logbookId,
                qso.id,
                qso.revision,
                qso.station_id,
                instrumentId || null,
                fieldsOf(draft, qso.fields, logbook.area)
            );
            reset(saved.qso);
            warnings = saved.warnings;
            toasts.show(
                m.entry_editor_saved({ call: saved.qso.fields.CALL, revision: saved.qso.revision })
            );
            if (saved.warnings.length === 0) back();
        } catch (e) {
            if (e instanceof ApiError && (e.status === 412 || e.status === 409) && e.body.current) {
                conflict = e.body.current as Qso;
            } else {
                saveError = e instanceof Error ? e.message : String(e);
            }
        } finally {
            saving = false;
        }
    }

    async function remove() {
        if (qso === undefined) return;
        try {
            await api.deleteQso(logbookId, qso.id, qso.revision);
            toasts.show(m.entry_editor_deleted({ call: qso.fields.CALL }));
            goto(logHref, { replace: true });
        } catch (e) {
            if (e instanceof ApiError && e.body.current) conflict = e.body.current as Qso;
            else saveError = e instanceof Error ? e.message : String(e);
        }
    }

    function onFreq(value: string) {
        if (draft !== undefined) draft = withFreq(draft, value);
    }

    function onBand(band: string) {
        if (draft !== undefined) draft = withBand(draft, band);
    }

    function onMode(mode: string, submode: string) {
        if (draft !== undefined) draft = withMode(draft, mode, submode);
    }
</script>

<PageTitle title={m.entry_editor_title()} />

<PageHeader
    title={m.entry_editor_title()}
    subtitle={qso
        ? qso.updated_by_callsign
            ? m.entry_editor_revision_by({
                  revision: qso.revision,
                  updated,
                  callsign: qso.updated_by_callsign
              })
            : m.entry_editor_revision({ revision: qso.revision, updated })
        : logbook.logbook.name}
    back={{ label: m.common_back(), onclick: back, always: true }}
>
    {#snippet actions()}
        <Button
            variant="primary"
            disabled={saving || problem !== undefined || qso?.deleted_at !== null}
            onclick={save}>{saving ? m.common_saving() : m.common_save()}</Button
        >
    {/snippet}
</PageHeader>

<main class="mx-auto flex w-full max-w-2xl flex-col gap-4 px-4 py-4 md:px-6">
    {#if loadError}
        <Notice tone="error">{loadError}</Notice>
    {:else if qso && draft}
        {#if qso.deleted_at}
            <Notice tone="error">{m.entry_editor_deleted_at({ at: qso.deleted_at })}</Notice>
        {/if}
        {#if conflict}
            <Notice tone="error">
                {m.entry_editor_conflict({ revision: conflict.revision })}
                <button
                    type="button"
                    class="ml-2 font-semibold underline"
                    onclick={() => reset(conflict!)}>{m.entry_editor_load_theirs()}</button
                >
            </Notice>
        {/if}
        {#if saveError}
            <Notice tone="error">{saveError}</Notice>
        {/if}
        <WarningList {warnings} />

        <form
            class="flex flex-col gap-3.5"
            onsubmit={(e) => {
                e.preventDefault();
                save();
            }}
        >
            <Field label={m.field_call()} adif="CALL" for="e-call">
                <input
                    id="e-call"
                    class="control h-12 font-mono text-[22px] font-semibold tracking-wide uppercase"
                    autocomplete="off"
                    spellcheck="false"
                    bind:value={draft.call}
                />
                {#if entity}
                    <span class="text-xs text-muted"
                        >{entity.name} · DXCC {entity.dxcc ?? '—'} · CQ {entity.cq_zone} · ITU {entity.itu_zone}</span
                    >
                {/if}
            </Field>
            <div class="grid grid-cols-2 gap-3">
                <Field label={m.entry_editor_date_utc()} adif="QSO_DATE" for="e-date">
                    <input
                        id="e-date"
                        type="date"
                        class="control h-11 font-mono"
                        bind:value={draft.date}
                    />
                </Field>
                <Field label={m.entry_editor_time_utc()} adif="TIME_ON" for="e-time">
                    <input
                        id="e-time"
                        type="time"
                        step="1"
                        class="control h-11 font-mono"
                        bind:value={draft.time}
                    />
                </Field>
            </div>
            <div class="grid grid-cols-2 gap-3">
                <Field label={m.field_freq_mhz()} adif="FREQ" for="e-freq">
                    <input
                        id="e-freq"
                        inputmode="decimal"
                        placeholder={m.entry_optional()}
                        class="control h-11 font-mono"
                        value={draft.freq}
                        oninput={(e) => onFreq(e.currentTarget.value)}
                    />
                </Field>
                <Field label={m.field_band()} adif="BAND" for="e-band">
                    <BandSelect id="e-band" class="h-11" value={draft.band} onchange={onBand} />
                </Field>
            </div>
            <Field label={m.entry_editor_mode_submode()} adif="MODE · SUBMODE" for="e-mode">
                <ModeSelect
                    id="e-mode"
                    class="h-11"
                    mode={draft.mode}
                    submode={draft.submode}
                    onchange={onMode}
                />
            </Field>
            <div class="grid grid-cols-2 gap-3">
                <Field label={m.field_rst_sent()} adif="RST_SENT" for="e-rsts">
                    <input id="e-rsts" class="control h-11 font-mono" bind:value={draft.rstSent} />
                </Field>
                <Field label={m.field_rst_rcvd()} adif="RST_RCVD" for="e-rstr">
                    <input id="e-rstr" class="control h-11 font-mono" bind:value={draft.rstRcvd} />
                </Field>
            </div>
            <div class="grid grid-cols-2 gap-3">
                <Field label={m.field_name()} adif="NAME" for="e-name">
                    <input id="e-name" class="control h-11 text-base" bind:value={draft.name} />
                </Field>
                <Field label={m.field_qth()} adif="QTH" for="e-qth">
                    <input id="e-qth" class="control h-11 text-base" bind:value={draft.qth} />
                </Field>
            </div>
            <div class="grid grid-cols-2 gap-3">
                {#if logbook.area}
                    <Field label={logbook.area.label} adif={logbook.area.name} for="e-area">
                        <input
                            id="e-area"
                            autocomplete="off"
                            spellcheck="false"
                            class="control h-11 font-mono"
                            bind:value={draft.area}
                        />
                    </Field>
                {/if}
                <Field label={m.entry_tx_power_w()} adif="TX_PWR" for="e-txpwr">
                    <input
                        id="e-txpwr"
                        inputmode="decimal"
                        class="control h-11 font-mono"
                        bind:value={draft.txPwr}
                    />
                </Field>
            </div>
            <Field label={m.field_comment()} adif="COMMENT" for="e-comment">
                <textarea
                    id="e-comment"
                    rows="2"
                    class="control h-auto py-2 text-base"
                    bind:value={draft.comment}></textarea>
            </Field>
            <Field label={m.field_notes()} adif="NOTES" for="e-notes">
                <textarea
                    id="e-notes"
                    rows="2"
                    class="control h-auto py-2 text-base"
                    bind:value={draft.notes}></textarea>
            </Field>

            <fieldset class="m-0 flex flex-col gap-3 rounded-lg border border-border p-3">
                <legend class="px-1.5 eyebrow">QSL</legend>
                <div class="grid grid-cols-2 gap-3">
                    <Field label={m.entry_editor_qsl_sent()} adif="QSL_SENT" for="e-qsls">
                        <select id="e-qsls" class="control h-11" bind:value={draft.qslSent}>
                            <option value="">—</option>
                            {#each QSL_SENT_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label()}</option>
                            {/each}
                        </select>
                    </Field>
                    <Field label={m.entry_editor_qsl_rcvd()} adif="QSL_RCVD" for="e-qslr">
                        <select id="e-qslr" class="control h-11" bind:value={draft.qslRcvd}>
                            <option value="">—</option>
                            {#each QSL_RCVD_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label()}</option>
                            {/each}
                        </select>
                    </Field>
                </div>
                <div class="grid grid-cols-2 gap-3">
                    <Field label={m.entry_editor_qsl_sent_via()} adif="QSL_SENT_VIA" for="e-via">
                        <select id="e-via" class="control h-11" bind:value={draft.qslSentVia}>
                            <option value="">—</option>
                            {#each QSL_VIA_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label()}</option>
                            {/each}
                        </select>
                    </Field>
                    <Field label={m.entry_editor_qsl_sent_date()} adif="QSLSDATE" for="e-qslsdate">
                        <input
                            id="e-qslsdate"
                            type="date"
                            class="control h-11 font-mono"
                            bind:value={draft.qslSentDate}
                        />
                    </Field>
                </div>
                <div class="grid grid-cols-2 gap-3">
                    <Field label={m.entry_editor_qsl_rcvd_via()} adif="QSL_RCVD_VIA" for="e-rvia">
                        <select id="e-rvia" class="control h-11" bind:value={draft.qslRcvdVia}>
                            <option value="">—</option>
                            {#each QSL_VIA_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label()}</option>
                            {/each}
                        </select>
                    </Field>
                    <Field label={m.entry_editor_qsl_rcvd_date()} adif="QSLRDATE" for="e-qslrdate">
                        <input
                            id="e-qslrdate"
                            type="date"
                            class="control h-11 font-mono"
                            bind:value={draft.qslRcvdDate}
                        />
                    </Field>
                </div>
                <div class="flex justify-between text-[13px] text-muted">
                    <span>{m.entry_editor_lotw()}</span>
                    <span class="font-mono"
                        >{qso.fields.LOTW_QSL_SENT || '—'} / {qso.fields.LOTW_QSL_RCVD || '—'}</span
                    >
                </div>
                <div class="flex justify-between text-[13px] text-muted">
                    <span>{m.entry_editor_eqsl()}</span>
                    <span class="font-mono"
                        >{qso.fields.EQSL_QSL_SENT || '—'} / {qso.fields.EQSL_QSL_RCVD || '—'}</span
                    >
                </div>
            </fieldset>

            <div class="flex flex-col gap-2 rounded-lg border border-border bg-surface p-3">
                <div class="flex justify-between">
                    <span class="eyebrow">{m.entry_editor_station_at_time()}</span>
                    <span class="text-xs text-muted">{m.entry_editor_read_only()}</span>
                </div>
                <div class="flex flex-wrap gap-x-4 gap-y-1 text-[13px]">
                    {#each station as [name, value] (name)}
                        <span
                            ><span class="font-mono text-[11px] text-muted">{name}</span>
                            <span class="font-mono">{value}</span></span
                        >
                    {/each}
                </div>
                <span class="text-xs text-muted">{m.entry_editor_baked()}</span>
            </div>

            <Field label={m.field_instrument()} for="e-instrument">
                <InstrumentSelect
                    id="e-instrument"
                    class="h-11"
                    value={instrumentId}
                    {instruments}
                    onchange={(id) => (instrumentId = id)}
                />
            </Field>

            <fieldset class="m-0 flex flex-col gap-2.5 rounded-lg border border-border p-3">
                <legend class="px-1.5 eyebrow">{m.entry_editor_other_fields()}</legend>
                {#each draft.extra as extra, i (i)}
                    <div
                        class="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)_auto] items-center gap-2"
                    >
                        <label for={`e-extra-name-${i}`} class="sr-only"
                            >{m.entry_adif_field()}</label
                        >
                        <input
                            id={`e-extra-name-${i}`}
                            list="e-adif-fields"
                            placeholder={m.entry_adif_field()}
                            autocomplete="off"
                            autocapitalize="characters"
                            spellcheck="false"
                            class="control h-10 w-full px-2 font-mono text-xs uppercase"
                            bind:value={extra.name}
                        />
                        <label for={`e-extra-value-${i}`} class="sr-only">{m.entry_value()}</label>
                        <input
                            id={`e-extra-value-${i}`}
                            class="control h-10"
                            bind:value={extra.value}
                        />
                        <button
                            type="button"
                            aria-label={m.entry_remove_field()}
                            class="inline-flex size-10 items-center justify-center rounded-lg text-muted hover:bg-surface-2"
                            onclick={() => draft?.extra.splice(i, 1)}
                            ><Icon name="close" size={16} /></button
                        >
                    </div>
                {:else}
                    <span class="text-xs text-muted">{m.entry_editor_none()}</span>
                {/each}
                <div>
                    <Button onclick={() => draft?.extra.push({ name: '', value: '' })}
                        >{m.entry_add_adif_field()}</Button
                    >
                </div>
                <datalist id="e-adif-fields">
                    {#each extraFieldNames(logbook.area) as name (name)}
                        <option value={name}></option>
                    {/each}
                </datalist>
            </fieldset>

            {#if problem}
                <p class="text-xs text-error-text">{problem}</p>
            {/if}
            <button type="submit" class="hidden">{m.common_save()}</button>
            {#if qso.deleted_at === null}
                <button
                    type="button"
                    class="h-11 rounded-lg border border-error bg-transparent font-medium text-error"
                    onclick={() => (confirmDelete = true)}>{m.entry_editor_delete_qso()}</button
                >
            {/if}
        </form>
    {:else}
        <p class="text-muted">{m.common_loading()}</p>
    {/if}
</main>

<ConfirmDialog
    bind:open={confirmDelete}
    title={m.entry_editor_delete_title()}
    description={m.entry_editor_delete_description()}
    confirm={m.common_delete()}
    danger
    onconfirm={remove}
/>
