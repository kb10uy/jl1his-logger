<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Fields, Qso } from '#lib/api/types.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import LatestAtStation from '#lib/components/LatestAtStation.svelte';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import WarningList from '#lib/components/WarningList.svelte';
    import ContestBar from '#lib/components/entry/ContestBar.svelte';
    import EntryEquipment from '#lib/components/entry/EntryEquipment.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import QslBadge from '#lib/components/ui/QslBadge.svelte';
    import {
        clearedEntry,
        entryFields,
        entryProblem,
        extraFieldNames,
        type Entry
    } from '#lib/entry/record.ts';
    import { bandOf } from '#lib/qso/bands.ts';
    import { expandExchange } from '#lib/qso/contest.ts';
    import { hhmm, isoDate, utcClock, utcFields } from '#lib/qso/format.ts';
    import { defaultRst } from '#lib/qso/modes.ts';
    import { QSL_REQUESTS } from '#lib/qso/qsl.ts';
    import { withBand, withFreq, withMode } from '#lib/qso/tuning.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { EntrySession } from '#lib/state/entry.svelte.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import type { Warning } from '#lib/api/types.ts';

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const session: EntrySession = new EntrySession(logbook, {
        get call(): string | undefined {
            return call.length < 3 ? undefined : call;
        },
        get freq(): string {
            return entry.freq;
        },
        get band(): string {
            return entry.band;
        },
        get mode(): string {
            return entry.mode;
        },
        get submode(): string {
            return entry.submode;
        },
        get at(): Date {
            return at;
        }
    });
    const settings = session.settings;

    let entry = $state<Entry>({
        call: '',
        freq: untrack(() => settings.freq),
        band: untrack(() => settings.band),
        mode: untrack(() => settings.mode),
        submode: untrack(() => settings.submode),
        rstSent: untrack(() => defaultRst(settings.mode, settings.submode)),
        rstRcvd: untrack(() => defaultRst(settings.mode, settings.submode)),
        received: '',
        name: '',
        qth: '',
        area: '',
        qsl: 'none',
        comment: '',
        txPwr: '',
        notes: '',
        extra: []
    });
    let now = $state(new Date());
    let locked = $state<Date | undefined>(undefined);
    let before = $state<Qso[]>([]);
    let warnings = $state<Warning[]>([]);
    let callInput = $state<HTMLInputElement>();
    let form = $state<HTMLFormElement>();

    const stationId = $derived(session.stationId);
    const instrumentId = $derived(session.instrumentId);
    const lookup = $derived(session.lookup);
    const duplicates = $derived(session.duplicates);
    const recent = $derived(session.recent);
    const instrumentPower = $derived(logbook.instrument(instrumentId)?.tx_pwr ?? '');
    const call = $derived(entry.call.trim().toUpperCase());
    const at = $derived(locked ?? now);
    const clock = $derived(utcClock(at));
    const freqBand = $derived(entry.freq.trim() === '' ? undefined : bandOf(entry.freq));
    const bandWarning = $derived.by(() => {
        if (entry.freq.trim() === '') return undefined;
        if (freqBand === undefined) return m.entry_band_outside({ freq: entry.freq });
        if (freqBand.name !== entry.band)
            return m.entry_band_mismatch({
                freq: entry.freq,
                band: freqBand.name,
                chosen: entry.band
            });
        return undefined;
    });
    const area = $derived(logbook.area);
    const problem = $derived(entryProblem(entry, settings.contest, area));
    const extraCount = $derived(
        [entry.txPwr, entry.notes, ...entry.extra.map((f) => f.value)].filter(
            (v) => v.trim() !== ''
        ).length
    );
    const sent = $derived(
        expandExchange(settings.contest.exchangeTemplate, settings.contest.nextSerial)
    );
    const sentShown = $derived(
        (settings.contest.includeRst ? `${entry.rstSent} ` : '') + (sent || '—')
    );
    const unsent = $derived(session.unsent);

    onMount(() => {
        const tick = setInterval(() => (now = new Date()), 1000);
        callInput?.focus();
        return () => clearInterval(tick);
    });

    $effect(() => {
        const wanted = call;
        before = [];
        if (wanted.length < 3) return;
        const controller = new AbortController();
        const timer = setTimeout(() => {
            api.searchQsos(logbookId, { q: `base:"${wanted}"`, limit: 5 }, controller.signal)
                .then((page) => (before = page.qsos))
                .catch(() => undefined);
        }, 250);
        return () => {
            clearTimeout(timer);
            controller.abort();
        };
    });

    function setFreq(value: string) {
        entry = withFreq(entry, value);
    }

    function selectBand(band: string) {
        entry = withBand(entry, band);
    }

    function setMode(mode: string, submode: string) {
        entry = withMode(entry, mode, submode);
    }

    function clear() {
        entry = clearedEntry(entry);
        warnings = [];
        callInput?.focus();
    }

    async function log(event?: Event) {
        event?.preventDefault();
        if (problem !== undefined || stationId === '' || session.logging) return;
        const fields: Fields = entryFields(entry, settings.contest, at, area);
        clear();
        const result = await session.log(fields);
        if ('saved' in result) {
            warnings = result.saved.warnings;
            toasts.show(m.entry_logged({ call: result.saved.qso.fields.CALL }));
        } else {
            toasts.error(m.entry_kept({ call: fields.CALL, error: result.error.message }));
        }
    }

    async function sendNow() {
        const { saved, failed } = await session.sendUnsent();
        toasts.show(
            failed > 0
                ? m.entry_sent_some_unsent({ sent: saved.length, failed })
                : m.entry_sent({ count: saved.length })
        );
    }

    const isToday = (q: Qso) => q.fields.QSO_DATE === utcFields(now).QSO_DATE;
</script>

<PageTitle title={m.nav_live_entry()} />

<svelte:window
    onkeydown={(e) => {
        if (e.key === 'Escape' && form?.contains(document.activeElement)) clear();
    }}
/>

<PageHeader title={m.nav_live_entry()} subtitle={logbook.logbook.name}>
    {#snippet actions()}
        <Button
            variant="toggle"
            pressed={settings.contest.enabled}
            onclick={() => (settings.contest.enabled = !settings.contest.enabled)}
            >{m.field_contest()}</Button
        >
    {/snippet}
    {#snippet toolbar()}
        <EntryEquipment {session} oninstrument={() => (entry.txPwr = '')} />
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <LogbookLoadError />
    {#if logbook.noActiveStation}
        <Notice>{m.entry_no_station()}</Notice>
    {/if}

    {#if settings.contest.enabled}
        <ContestBar bind:contest={settings.contest} rstSent={entry.rstSent} />
    {/if}

    <div class="flex flex-wrap items-start gap-4">
        <div class="flex min-w-0 flex-[1_1_440px] flex-col gap-4">
            <form
                aria-label={m.entry_new_qso()}
                class="flex flex-col gap-3 md:rounded-lg md:border md:border-accent md:bg-surface md:p-4"
                onsubmit={log}
                bind:this={form}
            >
                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <span class="text-xs text-muted">{m.field_date_utc()}</span>
                    <div class="flex items-center gap-3 font-mono tabular">
                        <span class="text-[15px] text-muted">{clock.date}</span>
                        <span class={['text-lg font-semibold', locked && 'text-warn']}
                            >{clock.time}</span
                        >
                        <div class="flex-1"></div>
                        <Button
                            variant="toggle"
                            size="sm"
                            class="font-sans"
                            pressed={locked !== undefined}
                            onclick={() => (locked = locked ? undefined : new Date())}
                            >{m.entry_lock_time()}</Button
                        >
                    </div>
                </div>

                <div class="h-px bg-border"></div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="freq" class="text-xs text-muted">{m.entry_freq_band()}</label>
                    <div
                        class="grid grid-cols-2 gap-2.5 md:flex md:flex-wrap md:items-center md:gap-2"
                    >
                        <input
                            id="freq"
                            inputmode="decimal"
                            placeholder={m.entry_optional()}
                            class="control h-11 w-full rounded-lg px-3 text-right font-mono text-lg md:w-[140px]"
                            value={entry.freq}
                            oninput={(e) => setFreq(e.currentTarget.value)}
                        />
                        <span class="mr-2 text-xs text-muted max-md:hidden">MHz</span>
                        <BandSelect
                            class="h-11 rounded-lg text-[15px] md:min-w-[168px] md:flex-1"
                            detailed
                            value={entry.band}
                            onchange={selectBand}
                        />
                    </div>
                    {#if bandWarning}
                        <span class="max-md:hidden"></span>
                        <Notice class="text-xs">{bandWarning}</Notice>
                    {/if}
                </div>
                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="mode" class="text-xs text-muted">{m.field_mode()}</label>
                    <ModeSelect
                        id="mode"
                        class="h-11 rounded-lg text-[15px]"
                        mode={entry.mode}
                        submode={entry.submode}
                        onchange={setMode}
                    />
                </div>

                <div class="h-px bg-border"></div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="call" class="text-xs text-muted">{m.field_call()}</label>
                    <input
                        id="call"
                        bind:this={callInput}
                        bind:value={entry.call}
                        autocomplete="off"
                        autocapitalize="characters"
                        spellcheck="false"
                        class="control h-14 rounded-lg border-2 border-accent px-3.5 font-mono text-[28px] font-semibold tracking-wider uppercase outline-none"
                    />
                    {#if lookup && call.length >= 3}
                        <span class="max-md:hidden"></span>
                        <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 text-[13px]">
                            <span class="font-mono font-semibold">{lookup.call}</span>
                            <span class="text-muted">
                                {[
                                    lookup.entity?.name,
                                    lookup.entity?.dxcc ? `DXCC ${lookup.entity.dxcc}` : undefined,
                                    lookup.entity ? `CQ ${lookup.entity.cq_zone}` : undefined,
                                    lookup.entity ? `ITU ${lookup.entity.itu_zone}` : undefined,
                                    lookup.previous.fields.NAME,
                                    lookup.previous.fields.QTH
                                ]
                                    .filter((s) => s)
                                    .join(' · ')}
                            </span>
                            {#if lookup.previous.count > 0}
                                <span class="rounded-full bg-info-bg px-2 py-px text-xs text-info"
                                    >{m.entry_worked_before({ count: lookup.previous.count })}</span
                                >
                            {/if}
                        </div>
                    {/if}
                    {#if duplicates.length > 0}
                        {@const d = duplicates[0]}
                        <span class="max-md:hidden"></span>
                        <Notice>
                            <strong class="font-semibold">{m.entry_possible_duplicate()}</strong>
                            {m.entry_duplicate_detail({
                                call: d.fields.CALL,
                                band: d.fields.BAND,
                                mode: d.fields.MODE,
                                time: hhmm(d.fields.TIME_ON),
                                when: isToday(d) ? m.entry_today() : isoDate(d.fields.QSO_DATE)
                            })}
                            <a
                                class="ml-1 font-medium text-warn"
                                href={`/logbooks/${logbookId}?q=${encodeURIComponent(`base:"${call}"`)}`}
                                >{m.entry_show_in_log()}</a
                            >
                        </Notice>
                    {/if}
                </div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <span class="text-xs text-muted max-md:hidden">{m.field_rst()}</span>
                    <div class="grid grid-cols-2 gap-2.5 md:flex md:items-center md:gap-3">
                        <div class="flex flex-col gap-1 md:flex-row md:items-center md:gap-3">
                            <label for="rsts" class="text-xs text-muted"
                                ><span class="md:hidden">{'RST '}</span
                                >{m.entry_sent_short()}</label
                            >
                            <input
                                id="rsts"
                                inputmode="numeric"
                                class="control h-11 w-full rounded-lg text-center font-mono text-xl md:w-24"
                                bind:value={entry.rstSent}
                            />
                        </div>
                        <div
                            class="flex flex-col gap-1 md:ml-2 md:flex-row md:items-center md:gap-3"
                        >
                            <label for="rstr" class="text-xs text-muted"
                                ><span class="md:hidden">{'RST '}</span
                                >{m.entry_rcvd_short()}</label
                            >
                            <input
                                id="rstr"
                                inputmode="numeric"
                                class="control h-11 w-full rounded-lg text-center font-mono text-xl md:w-24"
                                bind:value={entry.rstRcvd}
                            />
                        </div>
                    </div>
                </div>

                {#if settings.contest.enabled}
                    <div
                        class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3"
                    >
                        <span class="text-xs text-muted">{m.entry_number()}</span>
                        <div class="flex flex-wrap items-center gap-3">
                            <span class="text-xs text-muted">{m.entry_sent_short()}</span>
                            <output
                                aria-label={m.entry_sent_number()}
                                class="inline-flex h-11 min-w-24 items-center rounded-lg border border-dashed border-border px-3 font-mono text-xl"
                                >{sentShown}</output
                            >
                            <label for="srx" class="ml-2 text-xs text-muted"
                                >{m.entry_rcvd_short()}</label
                            >
                            <input
                                id="srx"
                                required
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="001"
                                class={[
                                    'control h-11 w-[140px] rounded-lg font-mono text-xl uppercase',
                                    entry.received.trim() === '' && 'border-2 border-info'
                                ]}
                                bind:value={entry.received}
                            />
                        </div>
                    </div>
                {/if}

                <div class="h-px bg-border"></div>

                <div class="grid grid-cols-2 gap-2.5 md:grid-cols-1 md:gap-3">
                    <div
                        class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3"
                    >
                        <label for="name" class="text-xs text-muted">{m.field_name()}</label>
                        <input
                            id="name"
                            placeholder={lookup?.previous.fields.NAME ?? m.entry_optional()}
                            class="control h-11 rounded-lg px-3 text-base"
                            bind:value={entry.name}
                        />
                    </div>
                    <div
                        class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3"
                    >
                        <label for="qth" class="text-xs text-muted">{m.field_qth()}</label>
                        <input
                            id="qth"
                            placeholder={lookup?.previous.fields.QTH ?? m.entry_optional()}
                            class="control h-11 rounded-lg px-3 text-base"
                            bind:value={entry.qth}
                        />
                    </div>
                </div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <span class="text-xs text-muted max-md:hidden">{area?.label ?? 'QSL'}</span>
                    <div class="grid grid-cols-2 gap-2.5 md:flex md:items-center md:gap-3">
                        {#if area}
                            <div class="flex flex-col gap-1">
                                <label for="area" class="text-xs text-muted md:hidden"
                                    >{area.label}</label
                                >
                                <input
                                    id="area"
                                    placeholder={lookup?.previous.fields[area.name] ??
                                        m.entry_optional()}
                                    autocomplete="off"
                                    spellcheck="false"
                                    class="control h-11 w-full rounded-lg px-3 font-mono text-base md:w-40"
                                    bind:value={entry.area}
                                />
                            </div>
                        {/if}
                        <div class="flex flex-col gap-1 md:flex-row md:items-center md:gap-3">
                            <label for="qsl" class={['text-xs text-muted', !area && 'md:hidden']}
                                >QSL</label
                            >
                            <select
                                id="qsl"
                                class="control h-11 w-full rounded-lg text-base md:w-48"
                                bind:value={entry.qsl}
                            >
                                {#each QSL_REQUESTS as [value, label] (value)}
                                    <option {value}>{label()}</option>
                                {/each}
                            </select>
                        </div>
                    </div>
                </div>

                <div class="h-px bg-border"></div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="comment" class="text-xs text-muted">{m.field_comment()}</label>
                    <input
                        id="comment"
                        placeholder="P2P US-0001…"
                        class="control h-11 rounded-lg px-3 text-base"
                        bind:value={entry.comment}
                    />
                </div>

                <details class="group">
                    <summary
                        class="-ml-2 inline-flex cursor-pointer list-none items-center gap-1 rounded-md px-2 py-1 text-[13px] text-muted hover:bg-surface-2 hover:text-text [&::-webkit-details-marker]:hidden"
                        >{m.entry_extra_fields()}{#if extraCount > 0}<span
                                >&nbsp;· {extraCount}</span
                            >{/if}<Icon
                            name="chevron"
                            size={14}
                            class="transition-transform group-open:rotate-180"
                        /></summary
                    >
                    <div class="flex flex-col gap-2.5 pt-2.5">
                        <div
                            class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3"
                        >
                            <label for="tx-pwr" class="text-xs text-muted"
                                >{m.entry_tx_power_w()}</label
                            >
                            <input
                                id="tx-pwr"
                                inputmode="decimal"
                                placeholder={instrumentPower}
                                class="control h-10 rounded-lg px-3 font-mono"
                                bind:value={entry.txPwr}
                            />
                        </div>
                        <div
                            class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3"
                        >
                            <label for="notes" class="text-xs text-muted">{m.field_notes()}</label>
                            <input
                                id="notes"
                                placeholder={m.entry_optional()}
                                class="control h-10 rounded-lg px-3"
                                bind:value={entry.notes}
                            />
                        </div>
                        {#each entry.extra as extra, i (i)}
                            <div
                                class="grid grid-cols-[96px_minmax(0,1fr)_auto] items-center gap-2 md:gap-3"
                            >
                                <label for={`extra-name-${i}`} class="sr-only"
                                    >{m.entry_adif_field()}</label
                                >
                                <input
                                    id={`extra-name-${i}`}
                                    list="adif-fields"
                                    placeholder={m.entry_adif_field()}
                                    autocomplete="off"
                                    autocapitalize="characters"
                                    spellcheck="false"
                                    class="control h-10 w-full rounded-lg px-2 font-mono text-xs uppercase"
                                    bind:value={extra.name}
                                />
                                <label for={`extra-value-${i}`} class="sr-only"
                                    >{m.entry_value()}</label
                                >
                                <input
                                    id={`extra-value-${i}`}
                                    class="control h-10 rounded-lg px-3"
                                    bind:value={extra.value}
                                />
                                <button
                                    type="button"
                                    aria-label={m.entry_remove_field()}
                                    class="inline-flex size-10 items-center justify-center rounded-lg text-muted hover:bg-surface-2"
                                    onclick={() => entry.extra.splice(i, 1)}
                                    ><Icon name="close" size={16} /></button
                                >
                            </div>
                        {/each}
                        <div>
                            <Button onclick={() => entry.extra.push({ name: '', value: '' })}
                                >{m.entry_add_adif_field()}</Button
                            >
                        </div>
                        <datalist id="adif-fields">
                            {#each extraFieldNames(area) as name (name)}
                                <option value={name}></option>
                            {/each}
                        </datalist>
                    </div>
                </details>

                <div
                    class="mt-1 flex flex-wrap items-center justify-end gap-3 max-md:sticky max-md:bottom-[84px] max-md:z-10 max-md:-mx-4 max-md:flex-nowrap max-md:border-t max-md:border-border max-md:bg-surface max-md:px-4 max-md:py-2.5"
                >
                    <span
                        class={[
                            'flex-1 text-xs max-md:hidden',
                            problem && entry.call !== '' ? 'text-info' : 'text-muted'
                        ]}>{problem && entry.call !== '' ? problem : m.entry_keys_hint()}</span
                    >
                    <Button size="lg" class="max-md:h-12" onclick={clear}
                        >{m.common_clear()}
                        <span class="font-mono text-[11px] text-muted max-md:hidden">Esc</span
                        ></Button
                    >
                    <Button
                        type="submit"
                        variant="primary"
                        size="xl"
                        class="font-bold max-md:h-12 max-md:flex-1"
                        disabled={problem !== undefined || stationId === '' || session.logging}
                        >{m.entry_log_qso()}
                        <span class="font-mono text-xs font-medium opacity-80">↵</span></Button
                    >
                </div>
            </form>

            <WarningList {warnings} />

            {#if before.length > 0}
                <section aria-label={m.entry_previous_label()} class="flex flex-col gap-2">
                    <h2 class="eyebrow">{m.entry_before_with({ call })}</h2>
                    <div class="flex flex-col rounded-lg border border-border bg-surface">
                        {#each before as q (q.id)}
                            <a
                                href={`/logbooks/${logbookId}?qso=${q.id}&q=${encodeURIComponent(`base:"${call}"`)}`}
                                class="flex items-center gap-3 border-b border-surface-2 px-3 py-2.5 text-[13px] text-text no-underline last:border-b-0"
                            >
                                <span class="font-mono text-muted"
                                    >{isoDate(q.fields.QSO_DATE)} {hhmm(q.fields.TIME_ON)}</span
                                >
                                <span class="font-mono">{q.fields.BAND} {q.fields.MODE}</span>
                                <span class="flex-1"></span>
                                {#if isToday(q)}
                                    <span
                                        class="rounded bg-warn-bg px-1.5 text-xs font-semibold text-warn"
                                        >{m.entry_today()}</span
                                    >
                                {:else}
                                    <QslBadge
                                        value={q.fields.QSL_RCVD}
                                        label={`QSL ${q.fields.QSL_RCVD || 'N'}`}
                                    />
                                {/if}
                            </a>
                        {/each}
                    </div>
                </section>
            {/if}
        </div>

        <LatestAtStation
            {logbookId}
            stationName={logbook.station(stationId)?.name ?? ''}
            {unsent}
            {recent}
            contest={settings.contest.enabled}
            onsendnow={sendNow}
        />
    </div>
</main>
