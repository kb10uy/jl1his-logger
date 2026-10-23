<script lang="ts">
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import { ApiError } from '#lib/api/client.ts';
    import type { Entity, Qso, Warning } from '#lib/api/types.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import InstrumentSelect from '#lib/components/InstrumentSelect.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import WarningList from '#lib/components/WarningList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Field from '#lib/components/ui/Field.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { bandOf, representativeFreq } from '#lib/qso/bands.ts';
    import { EDITED, draftOf, fieldsOf, missing, type Draft } from '#lib/qso/edit.ts';
    import { QSL_RCVD_VALUES, QSL_SENT_VALUES, QSL_VIA_VALUES } from '#lib/qso/qsl.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const qsoId = $derived(page.params.qso ?? '');
    const logHref = `/logbooks/${logbookId}/log`;

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
        draft = draftOf(found.fields);
        instrumentId = found.instrument_id ?? '';
        conflict = undefined;
        saveError = undefined;
    }

    function back() {
        if (history.length > 1) history.back();
        else goto(logHref);
    }

    const absent = $derived(draft === undefined ? [] : missing(draft));
    const instruments = $derived(
        logbook.instruments.filter((i) => i.archived_at === null || i.id === qso?.instrument_id)
    );
    const station = $derived(qso === undefined ? [] : stationFields(qso));
    const others = $derived(
        qso === undefined
            ? []
            : Object.entries(qso.fields).filter(
                  ([name]) => !EDITED.includes(name) && !station.some(([n]) => n === name)
              )
    );

    function stationFields(q: Qso): [string, string][] {
        return Object.entries(q.fields).filter(
            ([name]) => name === 'STATION_CALLSIGN' || name === 'OPERATOR' || name.startsWith('MY_')
        );
    }

    async function save() {
        if (qso === undefined || draft === undefined || absent.length > 0) return;
        saving = true;
        saveError = undefined;
        try {
            const saved = await api.updateQso(
                logbookId,
                qso.id,
                qso.revision,
                qso.station_id,
                instrumentId || null,
                fieldsOf(draft, qso.fields)
            );
            reset(saved.qso);
            warnings = saved.warnings;
            toasts.show(`Saved ${saved.qso.fields.CALL} · rev ${saved.qso.revision}`);
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
            toasts.show(`Deleted ${qso.fields.CALL}`);
            goto(logHref, { replace: true });
        } catch (e) {
            if (e instanceof ApiError && e.body.current) conflict = e.body.current as Qso;
            else saveError = e instanceof Error ? e.message : String(e);
        }
    }

    function onFreq(value: string) {
        if (draft === undefined) return;
        draft.freq = value;
        const band = bandOf(value);
        if (band) draft.band = band.name;
    }

    function onBand(band: string) {
        if (draft === undefined) return;
        draft.band = band;
        draft.freq = representativeFreq(band) ?? draft.freq;
    }
</script>

<PageTitle title="Edit QSO" />

<header
    class="sticky top-0 z-20 flex items-center gap-3 border-b border-border bg-surface px-2 py-2 md:px-6 md:py-3"
>
    <button
        type="button"
        aria-label="Back"
        class="inline-flex size-11 items-center justify-center rounded-lg text-text"
        onclick={back}
    >
        <Icon name="back" size={22} />
    </button>
    <div class="flex min-w-0 flex-1 flex-col">
        <h1 class="text-lg font-semibold">Edit QSO</h1>
        {#if qso}
            <span class="truncate text-xs text-muted"
                >rev {qso.revision} · updated {qso.updated_at.replace('T', ' ').slice(0, 16)}
                {qso.updated_by_callsign ? `by ${qso.updated_by_callsign}` : ''}</span
            >
        {/if}
    </div>
    <Button
        variant="primary"
        size="lg"
        disabled={saving || draft === undefined || absent.length > 0 || qso?.deleted_at !== null}
        onclick={save}>{saving ? 'Saving…' : 'Save'}</Button
    >
</header>

<main class="mx-auto flex w-full max-w-2xl flex-col gap-4 px-4 py-4 md:px-6">
    {#if loadError}
        <Notice tone="error">{loadError}</Notice>
    {:else if qso && draft}
        {#if qso.deleted_at}
            <Notice tone="error">This QSO was deleted at {qso.deleted_at}.</Notice>
        {/if}
        {#if conflict}
            <Notice tone="error">
                Someone else saved this QSO (rev {conflict.revision}) while you were editing.
                <button
                    type="button"
                    class="ml-2 font-semibold underline"
                    onclick={() => reset(conflict!)}>Load their version</button
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
            <Field label="Call" adif="CALL" for="e-call">
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
                <Field label="Date (UTC)" adif="QSO_DATE" for="e-date">
                    <input
                        id="e-date"
                        type="date"
                        class="control h-11 font-mono"
                        bind:value={draft.date}
                    />
                </Field>
                <Field label="Time (UTC)" adif="TIME_ON" for="e-time">
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
                <Field label="Freq MHz" adif="FREQ" for="e-freq">
                    <input
                        id="e-freq"
                        inputmode="decimal"
                        placeholder="optional"
                        class="control h-11 font-mono"
                        value={draft.freq}
                        oninput={(e) => onFreq(e.currentTarget.value)}
                    />
                </Field>
                <Field label="Band" adif="BAND" for="e-band">
                    <BandSelect id="e-band" class="h-11" value={draft.band} onchange={onBand} />
                </Field>
            </div>
            <Field label="Mode / Submode" adif="MODE · SUBMODE" for="e-mode">
                <ModeSelect
                    id="e-mode"
                    class="h-11"
                    bind:mode={draft.mode}
                    bind:submode={draft.submode}
                />
            </Field>
            <div class="grid grid-cols-2 gap-3">
                <Field label="RST sent" adif="RST_SENT" for="e-rsts">
                    <input id="e-rsts" class="control h-11 font-mono" bind:value={draft.rstSent} />
                </Field>
                <Field label="RST rcvd" adif="RST_RCVD" for="e-rstr">
                    <input id="e-rstr" class="control h-11 font-mono" bind:value={draft.rstRcvd} />
                </Field>
            </div>
            <div class="grid grid-cols-2 gap-3">
                <Field label="Name" adif="NAME" for="e-name">
                    <input id="e-name" class="control h-11 text-base" bind:value={draft.name} />
                </Field>
                <Field label="QTH" adif="QTH" for="e-qth">
                    <input id="e-qth" class="control h-11 text-base" bind:value={draft.qth} />
                </Field>
            </div>
            <Field label="Comment" adif="COMMENT" for="e-comment">
                <textarea
                    id="e-comment"
                    rows="2"
                    class="control h-auto py-2 text-base"
                    bind:value={draft.comment}></textarea>
            </Field>

            <fieldset class="m-0 flex flex-col gap-3 rounded-lg border border-border p-3">
                <legend class="px-1.5 eyebrow">QSL</legend>
                <div class="grid grid-cols-2 gap-3">
                    <Field label="Sent" adif="QSL_SENT" for="e-qsls">
                        <select id="e-qsls" class="control h-11" bind:value={draft.qslSent}>
                            <option value="">—</option>
                            {#each QSL_SENT_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label}</option>
                            {/each}
                        </select>
                    </Field>
                    <Field label="Received" adif="QSL_RCVD" for="e-qslr">
                        <select id="e-qslr" class="control h-11" bind:value={draft.qslRcvd}>
                            <option value="">—</option>
                            {#each QSL_RCVD_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label}</option>
                            {/each}
                        </select>
                    </Field>
                </div>
                <div class="grid grid-cols-2 gap-3">
                    <Field label="Via" adif="QSL_SENT_VIA" for="e-via">
                        <select id="e-via" class="control h-11" bind:value={draft.qslSentVia}>
                            <option value="">—</option>
                            {#each QSL_VIA_VALUES as [code, label] (code)}
                                <option value={code}>{code} · {label}</option>
                            {/each}
                        </select>
                    </Field>
                    <Field label="Sent date" adif="QSLSDATE" for="e-qslsdate">
                        <input
                            id="e-qslsdate"
                            type="date"
                            class="control h-11 font-mono"
                            bind:value={draft.qslSentDate}
                        />
                    </Field>
                </div>
                <div class="flex justify-between text-[13px] text-muted">
                    <span>LoTW sent / rcvd</span>
                    <span class="font-mono"
                        >{qso.fields.LOTW_QSL_SENT || '—'} / {qso.fields.LOTW_QSL_RCVD || '—'}</span
                    >
                </div>
            </fieldset>

            <div class="flex flex-col gap-2 rounded-lg border border-border bg-surface p-3">
                <div class="flex justify-between">
                    <span class="eyebrow">Station at the time</span>
                    <span class="text-xs text-muted">read only</span>
                </div>
                <div class="flex flex-wrap gap-x-4 gap-y-1 text-[13px]">
                    {#each station as [name, value] (name)}
                        <span
                            ><span class="font-mono text-[11px] text-muted">{name}</span>
                            <span class="font-mono">{value}</span></span
                        >
                    {/each}
                </div>
                <span class="text-xs text-muted"
                    >Baked at creation. Editing the Station or the Instrument, or choosing another
                    Instrument here, does not change these values.</span
                >
            </div>

            <Field label="Instrument" for="e-instrument">
                <InstrumentSelect
                    id="e-instrument"
                    class="h-11"
                    value={instrumentId}
                    {instruments}
                    onchange={(id) => (instrumentId = id)}
                />
            </Field>

            {#if others.length > 0}
                <details class="rounded-lg border border-border bg-surface p-3">
                    <summary class="cursor-pointer text-[13px]"
                        >Other ADIF fields · {others.length}</summary
                    >
                    <div class="mt-2 flex flex-col gap-1 text-[13px]">
                        {#each others as [name, value] (name)}
                            <span class="break-all"
                                ><span class="font-mono text-[11px] text-muted">{name}</span>
                                {value}</span
                            >
                        {/each}
                    </div>
                </details>
            {/if}

            {#if absent.length > 0}
                <p class="text-xs text-error-text">Needed before saving: {absent.join(', ')}.</p>
            {/if}
            <button type="submit" class="hidden">Save</button>
            {#if qso.deleted_at === null}
                <button
                    type="button"
                    class="h-11 rounded-lg border border-error bg-transparent font-medium text-error"
                    onclick={() => (confirmDelete = true)}>Delete QSO</button
                >
            {/if}
        </form>
    {:else}
        <p class="text-muted">Loading…</p>
    {/if}
</main>

<ConfirmDialog
    bind:open={confirmDelete}
    title="Delete this QSO?"
    description="It leaves the log and every export. Nobody can bring it back from the web UI."
    confirm="Delete"
    danger
    onconfirm={remove}
/>
