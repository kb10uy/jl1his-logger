<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Fields, Lookup, Qso } from '#lib/api/types.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import WarningList from '#lib/components/WarningList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import QslBadge from '#lib/components/ui/QslBadge.svelte';
    import { entryFields, entryProblem, type Entry } from '#lib/entry/record.ts';
    import { loadEntry, saveEntry } from '#lib/entry/settings.ts';
    import { bandOf, representativeFreq } from '#lib/qso/bands.ts';
    import { expandExchange, usesSerial } from '#lib/qso/contest.ts';
    import {
        compareNewestFirst,
        hhmm,
        isoDate,
        stationLabel,
        utcClock,
        utcFields
    } from '#lib/qso/format.ts';
    import { defaultRst, followsBand, sidebandFor } from '#lib/qso/modes.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import type { Warning } from '#lib/api/types.ts';

    const RECENT = 20;

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const settings = $state(loadEntry(logbookId));

    let entry = $state<Entry>({
        call: '',
        freq: untrack(() => settings.freq),
        band: untrack(() => settings.band),
        mode: untrack(() => settings.mode),
        submode: untrack(() => settings.submode),
        rstSent: defaultRst(untrack(() => settings.mode)),
        rstRcvd: defaultRst(untrack(() => settings.mode)),
        received: '',
        name: '',
        qth: '',
        comment: ''
    });
    let now = $state(new Date());
    let locked = $state<Date | undefined>(undefined);
    let lookup = $state<Lookup | undefined>(undefined);
    let before = $state<Qso[]>([]);
    let duplicates = $state<Qso[]>([]);
    let recent = $state<Qso[]>([]);
    let warnings = $state<Warning[]>([]);
    let logging = $state(false);
    let callInput = $state<HTMLInputElement>();
    let form = $state<HTMLFormElement>();

    const stationId = $derived(
        logbook.activeStations.some((s) => s.id === settings.stationId)
            ? settings.stationId
            : (logbook.activeStations[0]?.id ?? '')
    );
    const call = $derived(entry.call.trim().toUpperCase());
    const at = $derived(locked ?? now);
    const clock = $derived(utcClock(at));
    const freqBand = $derived(entry.freq.trim() === '' ? undefined : bandOf(entry.freq));
    const bandWarning = $derived.by(() => {
        if (entry.freq.trim() === '') return undefined;
        if (freqBand === undefined)
            return `${entry.freq} MHz is outside every ADIF band. FREQ and BAND are kept as entered.`;
        if (freqBand.name !== entry.band)
            return `${entry.freq} MHz is in ${freqBand.name}, not ${entry.band}. Both are kept as entered.`;
        return undefined;
    });
    const problem = $derived(entryProblem(entry, settings.contest));
    const sent = $derived(
        expandExchange(settings.contest.exchangeTemplate, settings.contest.nextSerial)
    );
    const sentShown = $derived(
        (settings.contest.includeRst ? `${entry.rstSent} ` : '') + (sent || '—')
    );
    const unsent = $derived(outbox.items.filter((i) => i.stationId === stationId));
    const rows = $derived(
        [
            ...unsent
                .filter((i) => !recent.some((q) => q.id === i.id))
                .map((i) => ({ id: i.id, fields: i.fields, unsent: true, error: i.error })),
            ...recent.map((q) => ({ id: q.id, fields: q.fields, unsent: false, error: undefined }))
        ].sort(compareNewestFirst)
    );

    onMount(() => {
        const tick = setInterval(() => (now = new Date()), 1000);
        const off = logbook.onQso((qso) => {
            if (qso.station_id !== stationId) return;
            const others = recent.filter((r) => r.id !== qso.id);
            recent =
                qso.deleted_at === null
                    ? [...others, qso].sort(compareNewestFirst).slice(0, RECENT)
                    : others;
        });
        callInput?.focus();
        return () => {
            clearInterval(tick);
            off();
        };
    });

    $effect(() => {
        saveEntry(logbookId, {
            stationId: settings.stationId,
            freq: entry.freq,
            band: entry.band,
            mode: entry.mode,
            submode: entry.submode,
            contest: { ...settings.contest }
        });
    });

    $effect(() => {
        const station = stationId;
        if (station === '') return;
        const controller = new AbortController();
        api.searchQsos(logbookId, { station_id: station, limit: RECENT }, controller.signal)
            .then((page) => (recent = page.qsos))
            .catch(() => undefined);
        return () => controller.abort();
    });

    $effect(() => {
        const wanted = call;
        lookup = undefined;
        before = [];
        if (wanted.length < 3) return;
        const controller = new AbortController();
        const timer = setTimeout(() => {
            api.lookup(logbookId, wanted, controller.signal)
                .then((found) => (lookup = found))
                .catch(() => undefined);
            api.searchQsos(logbookId, { q: `base:"${wanted}"`, limit: 5 }, controller.signal)
                .then((page) => (before = page.qsos))
                .catch(() => undefined);
        }, 250);
        return () => {
            clearTimeout(timer);
            controller.abort();
        };
    });

    $effect(() => {
        const candidate = {
            call,
            band: entry.band,
            mode: entry.mode,
            ...utcFields(untrack(() => at))
        };
        duplicates = [];
        if (candidate.call.length < 3) return;
        const controller = new AbortController();
        const timer = setTimeout(() => {
            api.duplicates(
                logbookId,
                {
                    call: candidate.call,
                    band: candidate.band,
                    mode: candidate.mode,
                    qso_date: candidate.QSO_DATE,
                    time_on: candidate.TIME_ON
                },
                controller.signal
            )
                .then((found) => (duplicates = found.qsos))
                .catch(() => undefined);
        }, 300);
        return () => {
            clearTimeout(timer);
            controller.abort();
        };
    });

    function setFreq(value: string) {
        entry.freq = value;
        const band = bandOf(value);
        if (band) setBand(band.name);
    }

    function setBand(band: string) {
        entry.band = band;
        if (followsBand(entry.mode, entry.submode)) entry.submode = sidebandFor(band);
    }

    function selectBand(band: string) {
        entry.freq = representativeFreq(band) ?? entry.freq;
        setBand(band);
    }

    function setMode(mode: string, submode: string, previous: string) {
        if (entry.rstSent === defaultRst(previous)) entry.rstSent = defaultRst(mode);
        if (entry.rstRcvd === defaultRst(previous)) entry.rstRcvd = defaultRst(mode);
        if (mode === 'SSB' && submode !== '') entry.submode = sidebandFor(entry.band);
    }

    function clear() {
        entry.call = '';
        entry.received = '';
        entry.name = '';
        entry.qth = '';
        entry.comment = '';
        warnings = [];
        callInput?.focus();
    }

    async function log(event?: Event) {
        event?.preventDefault();
        if (problem !== undefined || stationId === '' || logging) return;
        logging = true;
        const fields: Fields = entryFields(entry, settings.contest, at);
        try {
            const result = await outbox.log(logbookId, stationId, fields);
            if (settings.contest.enabled && usesSerial(settings.contest.exchangeTemplate)) {
                settings.contest.nextSerial += 1;
            }
            clear();
            if ('saved' in result) {
                warnings = result.saved.warnings;
                const qso = result.saved.qso;
                recent = [...recent.filter((r) => r.id !== qso.id), qso]
                    .sort(compareNewestFirst)
                    .slice(0, RECENT);
                toasts.show(`Logged ${qso.fields.CALL}`);
            } else {
                toasts.error(
                    `${fields.CALL} is kept on this device: ${result.error.message} Send it again when you can.`
                );
            }
        } finally {
            logging = false;
        }
    }

    async function sendNow() {
        const { saved, failed } = await outbox.sendAll();
        if (saved.length > 0) {
            recent = [
                ...recent.filter((r) => !saved.some((s) => s.qso.id === r.id)),
                ...saved.map((s) => s.qso)
            ]
                .sort(compareNewestFirst)
                .slice(0, RECENT);
        }
        toasts.show(
            failed > 0
                ? `Sent ${saved.length}; ${failed} still unsent.`
                : `Sent ${saved.length} QSO${saved.length === 1 ? '' : 's'}.`
        );
    }

    const pair = (a: string | undefined, b: string | undefined) => `${a || '—'} / ${b || '—'}`;
    const isToday = (q: Qso) => q.fields.QSO_DATE === utcFields(now).QSO_DATE;
</script>

<svelte:window
    onkeydown={(e) => {
        if (e.key === 'Escape' && form?.contains(document.activeElement)) clear();
    }}
/>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header
        class="flex flex-wrap items-center gap-2 max-md:sticky max-md:top-0 max-md:z-20 max-md:-mx-4 max-md:-mt-4 max-md:border-b max-md:border-border max-md:bg-surface max-md:px-4 max-md:py-3 md:gap-3"
    >
        <h1 class="text-xl font-semibold">Live entry</h1>
        <div class="flex-1"></div>
        <label for="station" class="text-xs text-muted max-md:sr-only">Station</label>
        <select
            id="station"
            class="control max-w-full bg-surface max-md:order-3 max-md:min-w-[60%] max-md:flex-1 max-md:bg-bg"
            value={stationId}
            onchange={(e) => (settings.stationId = e.currentTarget.value)}
        >
            {#each logbook.activeStations as station (station.id)}
                <option value={station.id}>{stationLabel(station)}</option>
            {/each}
        </select>
        <div
            class="flex h-9 items-center gap-2 rounded-md border border-border bg-surface px-3 font-mono max-md:border-0 max-md:bg-transparent max-md:px-0"
        >
            <span class="text-xs text-muted">UTC</span>
            <span class={['text-base font-semibold tabular', locked && 'text-warn']}
                >{clock.time}</span
            >
            <span class="text-xs text-muted max-md:hidden">{clock.date}</span>
        </div>
        <Button
            variant="toggle"
            size="md"
            class="max-md:hidden"
            pressed={locked !== undefined}
            onclick={() => (locked = locked ? undefined : new Date())}>Lock time</Button
        >
        <Button
            variant="toggle"
            class="max-md:order-4"
            pressed={settings.contest.enabled}
            onclick={() => (settings.contest.enabled = !settings.contest.enabled)}>Contest</Button
        >
    </header>

    {#if logbook.activeStations.length === 0}
        <Notice>This Logbook has no active Station. An owner adds one under Stations.</Notice>
    {/if}

    {#if settings.contest.enabled}
        <section
            aria-label="Contest"
            class="flex flex-wrap items-end gap-x-5 gap-y-3 rounded-lg border border-info-strong bg-selected px-4 py-3"
        >
            <div class="flex flex-col gap-1">
                <label for="contest-id" class="text-xs text-muted">CONTEST_ID</label>
                <input
                    id="contest-id"
                    placeholder="optional"
                    autocomplete="off"
                    spellcheck="false"
                    class="control w-[180px] font-mono uppercase"
                    bind:value={settings.contest.contestId}
                />
            </div>
            <div class="flex flex-col gap-1">
                <label for="contest-sent" class="text-xs text-muted">Sent number</label>
                <input
                    id="contest-sent"
                    placeholder={'{serial}'}
                    autocomplete="off"
                    spellcheck="false"
                    class="control w-40 font-mono"
                    bind:value={settings.contest.exchangeTemplate}
                />
            </div>
            <div class="flex flex-col gap-1">
                <label for="contest-next" class="text-xs text-muted">Next serial</label>
                <input
                    id="contest-next"
                    type="number"
                    min="1"
                    step="1"
                    class="control w-24 text-right font-mono"
                    bind:value={settings.contest.nextSerial}
                />
            </div>
            <label class="flex h-9 cursor-pointer items-center gap-2 text-[13px]">
                <input
                    type="checkbox"
                    class="size-4 accent-info"
                    bind:checked={settings.contest.includeRst}
                />
                RST in front
            </label>
            <div class="flex-1"></div>
            <div class="flex flex-col items-end gap-1">
                <span class="text-xs text-muted">Next STX_STRING</span>
                <strong class="flex h-9 items-center font-mono text-lg font-semibold"
                    >{(settings.contest.includeRst ? entry.rstSent : '') + sent ||
                        '(nothing)'}</strong
                >
            </div>
        </section>
    {/if}

    <div class="flex flex-wrap items-start gap-4">
        <div class="flex min-w-0 flex-[1_1_440px] flex-col gap-4">
            <form
                aria-label="New QSO"
                class="flex flex-col gap-3 md:rounded-lg md:border md:border-accent md:bg-surface md:p-4"
                onsubmit={log}
                bind:this={form}
            >
                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="freq" class="text-xs text-muted">Freq / Band</label>
                    <div
                        class="grid grid-cols-2 gap-2.5 md:flex md:flex-wrap md:items-center md:gap-2"
                    >
                        <input
                            id="freq"
                            inputmode="decimal"
                            placeholder="optional"
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
                    <label for="mode" class="text-xs text-muted">Mode</label>
                    <ModeSelect
                        id="mode"
                        class="h-11 rounded-lg text-[15px]"
                        mode={entry.mode}
                        submode={entry.submode}
                        onchange={(mode, submode) => {
                            const previous = entry.mode;
                            entry.mode = mode;
                            entry.submode = submode;
                            setMode(mode, submode, previous);
                        }}
                    />
                </div>

                <div class="h-px bg-border"></div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="call" class="text-xs text-muted">Call</label>
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
                                    >worked before · {lookup.previous.count}</span
                                >
                            {/if}
                        </div>
                    {/if}
                    {#if duplicates.length > 0}
                        {@const d = duplicates[0]}
                        <span class="max-md:hidden"></span>
                        <Notice>
                            <strong class="font-semibold">Possible duplicate.</strong>
                            {d.fields.CALL} · {d.fields.BAND}
                            {d.fields.MODE} · {hhmm(d.fields.TIME_ON)} UTC {isToday(d)
                                ? 'today'
                                : isoDate(d.fields.QSO_DATE)}. Logging keeps both.
                            <a
                                class="ml-1 font-medium text-warn"
                                href={`/logbooks/${logbookId}/log?q=${encodeURIComponent(`base:"${call}"`)}`}
                                >Show in log</a
                            >
                        </Notice>
                    {/if}
                </div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <span class="text-xs text-muted max-md:hidden">RST</span>
                    <div class="grid grid-cols-2 gap-2.5 md:flex md:items-center md:gap-3">
                        <div class="flex flex-col gap-1 md:flex-row md:items-center md:gap-3">
                            <label for="rsts" class="text-xs text-muted"
                                ><span class="md:hidden">{'RST '}</span>Sent</label
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
                                ><span class="md:hidden">{'RST '}</span>Rcvd</label
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
                        <span class="text-xs text-muted">Number</span>
                        <div class="flex flex-wrap items-center gap-3">
                            <span class="text-xs text-muted">Sent</span>
                            <output
                                aria-label="Sent number"
                                class="inline-flex h-11 min-w-24 items-center rounded-lg border border-dashed border-border px-3 font-mono text-xl"
                                >{sentShown}</output
                            >
                            <label for="srx" class="ml-2 text-xs text-muted">Rcvd</label>
                            <input
                                id="srx"
                                required
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="1001H"
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
                        <label for="name" class="text-xs text-muted">Name</label>
                        <input
                            id="name"
                            placeholder={lookup?.previous.fields.NAME ?? 'optional'}
                            class="control h-11 rounded-lg px-3 text-base"
                            bind:value={entry.name}
                        />
                    </div>
                    <div
                        class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3"
                    >
                        <label for="qth" class="text-xs text-muted">QTH</label>
                        <input
                            id="qth"
                            placeholder={lookup?.previous.fields.QTH ?? 'optional'}
                            class="control h-11 rounded-lg px-3 text-base"
                            bind:value={entry.qth}
                        />
                    </div>
                </div>

                <div class="h-px bg-border"></div>

                <div class="grid gap-1 md:grid-cols-[96px_minmax(0,1fr)] md:items-center md:gap-3">
                    <label for="comment" class="text-xs text-muted">Comment</label>
                    <input
                        id="comment"
                        placeholder="P2P JP-0345…"
                        class="control h-11 rounded-lg px-3 text-base"
                        bind:value={entry.comment}
                    />
                </div>

                <div
                    class="mt-1 flex flex-wrap items-center justify-end gap-3 max-md:sticky max-md:bottom-[84px] max-md:z-10 max-md:-mx-4 max-md:flex-nowrap max-md:border-t max-md:border-border max-md:bg-surface max-md:px-4 max-md:py-2.5"
                >
                    <span
                        class={[
                            'flex-1 text-xs max-md:hidden',
                            problem && entry.call !== '' ? 'text-info' : 'text-muted'
                        ]}
                        >{problem && entry.call !== ''
                            ? problem
                            : 'Tab moves down the rows. Enter logs from any field.'}</span
                    >
                    <Button size="lg" class="max-md:h-12" onclick={clear}
                        >Clear <span class="font-mono text-[11px] text-muted max-md:hidden"
                            >Esc</span
                        ></Button
                    >
                    <Button
                        type="submit"
                        variant="primary"
                        size="xl"
                        class="font-bold max-md:h-12 max-md:flex-1"
                        disabled={problem !== undefined || stationId === '' || logging}
                        >Log QSO <span class="font-mono text-xs font-medium opacity-80">↵</span
                        ></Button
                    >
                </div>
            </form>

            <WarningList {warnings} />

            {#if before.length > 0}
                <section aria-label="Previous QSOs with this station" class="flex flex-col gap-2">
                    <h2 class="eyebrow">Before with {call}</h2>
                    <div class="flex flex-col rounded-lg border border-border bg-surface">
                        {#each before as q (q.id)}
                            <a
                                href={`/logbooks/${logbookId}/log?qso=${q.id}&q=${encodeURIComponent(`base:"${call}"`)}`}
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
                                        >today</span
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

        <aside
            aria-label="Latest QSOs at this Station"
            class="flex min-w-0 flex-[1_1_440px] flex-col gap-3"
        >
            {#if unsent.length > 0}
                <section
                    aria-label="Unsent"
                    class="flex flex-col gap-2 rounded-lg border border-warn-border bg-warn-surface px-4 py-3 text-[13px]"
                >
                    <div class="flex items-center gap-3">
                        <Icon name="offline" size={16} class="text-warn" />
                        <span class="flex-1"
                            ><strong class="font-semibold text-warn">{unsent.length} unsent</strong>
                            <span class="text-muted">· kept on this device</span></span
                        >
                        <button
                            type="button"
                            disabled={outbox.sending}
                            class="h-8 rounded-md border border-warn px-3 font-semibold text-warn"
                            onclick={sendNow}>{outbox.sending ? 'Sending…' : 'Send now'}</button
                        >
                    </div>
                    {#each unsent.filter((i) => i.error) as item (item.id)}
                        <div class="flex items-center gap-2 text-xs">
                            <span class="font-mono font-semibold">{item.fields.CALL}</span>
                            <span class="flex-1 text-muted">{item.error}</span>
                            <button
                                type="button"
                                class="text-error-text"
                                onclick={() => outbox.discard(item.id)}>Discard</button
                            >
                        </div>
                    {/each}
                </section>
            {/if}
            <div class="flex items-baseline justify-between gap-2">
                <h2 class="eyebrow">Latest at this Station</h2>
                <a
                    href={`/logbooks/${logbookId}/log?q=${encodeURIComponent(`station:"${logbook.station(stationId)?.name ?? ''}"`)}`}
                    class="text-[13px] no-underline">Open in log</a
                >
            </div>
            <div class="overflow-x-auto rounded-lg border border-border bg-surface">
                <table class="w-full border-collapse text-[13px]">
                    <thead>
                        <tr class="text-left text-xs text-muted">
                            {#each ['UTC', 'Call', 'Band / Mode', 'RST'] as h (h)}
                                <th
                                    scope="col"
                                    class={[
                                        'border-b border-border px-3 py-2 font-medium',
                                        h === 'Band / Mode' && 'max-md:hidden'
                                    ]}>{h}</th
                                >
                            {/each}
                            {#if settings.contest.enabled}
                                <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                                    >Number S / R</th
                                >
                            {/if}
                            <th scope="col" class="border-b border-border px-3 py-2"
                                ><span class="sr-only">Sync</span></th
                            >
                        </tr>
                    </thead>
                    <tbody>
                        {#each rows as row (row.id)}
                            {@const f = row.fields}
                            <tr
                                class="align-top [&>td]:border-b [&>td]:border-surface-2 [&>td]:px-3 [&>td]:py-2"
                            >
                                <td class="font-mono whitespace-nowrap"
                                    ><span class="text-muted">{isoDate(f.QSO_DATE).slice(5)}</span>
                                    {hhmm(f.TIME_ON)}</td
                                >
                                <td>
                                    <span class="block font-mono text-sm font-semibold"
                                        >{f.CALL}</span
                                    >
                                    {#if f.COMMENT}
                                        <span class="mt-0.5 block text-xs text-muted"
                                            >{f.COMMENT}</span
                                        >
                                    {/if}
                                </td>
                                <td class="font-mono whitespace-nowrap max-md:hidden"
                                    >{f.BAND}
                                    {f.MODE} <span class="text-muted">{f.SUBMODE ?? ''}</span></td
                                >
                                <td class="font-mono whitespace-nowrap text-muted"
                                    >{pair(f.RST_SENT, f.RST_RCVD)}</td
                                >
                                {#if settings.contest.enabled}
                                    <td class="font-mono whitespace-nowrap"
                                        >{pair(f.STX_STRING, f.SRX_STRING)}</td
                                    >
                                {/if}
                                <td class="text-right">
                                    <span
                                        title={row.unsent ? 'unsent' : 'synced'}
                                        class={[
                                            'inline-block size-2 rounded-full',
                                            row.unsent ? 'bg-warn' : 'bg-ok'
                                        ]}
                                    ></span>
                                </td>
                            </tr>
                        {:else}
                            <tr
                                ><td colspan="6" class="px-3 py-6 text-center text-muted"
                                    >No QSOs at this Station yet.</td
                                ></tr
                            >
                        {/each}
                    </tbody>
                </table>
            </div>
            <span class="text-xs text-muted"
                >New QSOs appear at the top, including ones other operators log at this Station.
                Changing the Station switches the list.</span
            >
        </aside>
    </div>
</main>
