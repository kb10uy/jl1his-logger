<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Qso, Warning } from '#lib/api/types.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import LatestAtStation from '#lib/components/LatestAtStation.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import WarningList from '#lib/components/WarningList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import PttButton from '#lib/components/voice/PttButton.svelte';
    import { loadEntry, saveEntry } from '#lib/entry/settings.ts';
    import { expandExchange, usesSerial } from '#lib/qso/contest.ts';
    import {
        compareNewestFirst,
        hhmm,
        isoDate,
        stationLabel,
        utcClock,
        utcFields
    } from '#lib/qso/format.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
    import {
        FREE_TEXT_MAX_LENGTH,
        withBand,
        withFreq,
        withMode,
        type FreeTextField
    } from '#lib/voice/draft.ts';
    import { draftFields, draftProblem, queuedDuplicate } from '#lib/voice/record.ts';
    import { LANGUAGE_LABELS } from '#lib/voice/settings.ts';
    import { VoiceState, type LogResult } from '#lib/voice/voice.svelte.ts';

    const RECENT = 20;
    const QSL_TEXT = { none: '', requested: 'QSL requested', oneWay: 'QSL one way' } as const;

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const settings = $state(loadEntry(logbookId));
    const voice = new VoiceState(
        untrack(() => ({
            freq: settings.freq,
            band: settings.band,
            mode: settings.mode,
            submode: settings.submode
        })),
        logDraft
    );

    let now = $state(new Date());
    let recent = $state<Qso[]>([]);
    let duplicates = $state<Qso[]>([]);
    let warnings = $state<Warning[]>([]);
    let correcting = $state(false);
    let command = $state('');
    let logging = $state(false);

    const draft = $derived(voice.draft);
    const stationId = $derived(
        logbook.activeStations.some((s) => s.id === settings.stationId)
            ? settings.stationId
            : (logbook.activeStations[0]?.id ?? '')
    );
    const station = $derived(logbook.station(stationId));
    const clock = $derived(utcClock(now));
    const problem = $derived(draftProblem(draft, settings.contest));
    const sent = $derived(
        expandExchange(settings.contest.exchangeTemplate, settings.contest.nextSerial)
    );
    const unsent = $derived(outbox.items.filter((i) => i.stationId === stationId));
    const queued = $derived(
        draft.call === undefined
            ? undefined
            : queuedDuplicate(unsent, draftFields(draft, settings.contest, now))
    );
    const duplicate = $derived(queued?.fields ?? duplicates[0]?.fields);
    const extras = $derived(
        [
            draft.jcx ? `JCC/JCG ${draft.jcx}` : '',
            QSL_TEXT[draft.qsl],
            draft.name ?? '',
            draft.qth ?? ''
        ].filter((s) => s !== '')
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
        const stop = voice.start();
        return () => {
            clearInterval(tick);
            off();
            stop();
        };
    });

    $effect(() => {
        saveEntry(logbookId, {
            stationId: settings.stationId,
            freq: draft.freq,
            band: draft.band,
            mode: draft.mode,
            submode: draft.submode,
            contest: { ...settings.contest }
        });
    });

    $effect(() => {
        const id = stationId;
        if (id === '') return;
        const controller = new AbortController();
        api.searchQsos(logbookId, { station_id: id, limit: RECENT }, controller.signal)
            .then((page) => (recent = page.qsos))
            .catch(() => undefined);
        return () => controller.abort();
    });

    $effect(() => {
        const call = draft.call;
        const band = draft.band;
        const mode = draft.mode;
        duplicates = [];
        if (call === undefined) return;
        const at = utcFields(untrack(() => now));
        const controller = new AbortController();
        const timer = setTimeout(() => {
            api.duplicates(
                logbookId,
                { call, band, mode, qso_date: at.QSO_DATE, time_on: at.TIME_ON },
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

    async function logDraft(): Promise<LogResult> {
        if (stationId === '') return { ok: false, reason: 'Choose a Station first.' };
        const current = voice.draft;
        const reason = draftProblem(current, settings.contest);
        if (reason !== undefined) {
            toasts.error(reason);
            return { ok: false, reason };
        }
        if (logging) return { ok: false, reason: 'Still logging the previous QSO.' };
        logging = true;
        try {
            const fields = draftFields(current, settings.contest, new Date());
            const result = await outbox.log(logbookId, stationId, fields);
            if (settings.contest.enabled && usesSerial(settings.contest.exchangeTemplate)) {
                settings.contest.nextSerial += 1;
            }
            voice.next();
            correcting = false;
            if ('saved' in result) {
                warnings = result.saved.warnings;
                const qso = result.saved.qso;
                recent = [...recent.filter((r) => r.id !== qso.id), qso]
                    .sort(compareNewestFirst)
                    .slice(0, RECENT);
                toasts.show(`Logged ${fields.CALL}`);
            } else {
                warnings = [];
                toasts.error(
                    `${fields.CALL} is kept on this device: ${result.error.message} Send it again when you can.`
                );
            }
            return { ok: true, call: fields.CALL };
        } finally {
            logging = false;
        }
    }

    function discard() {
        voice.next();
        warnings = [];
        correcting = false;
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

    function submitCommand(event: SubmitEvent) {
        event.preventDefault();
        if (command.trim() === '') return;
        void voice.handleText(command, 'typed');
        command = '';
    }

    function setCall(value: string) {
        const call = value.trim().toUpperCase();
        voice.setDraft({
            ...draft,
            call: call === '' ? undefined : call,
            startedAt: draft.startedAt ?? new Date().toISOString()
        });
    }

    function setText(field: FreeTextField, value: string) {
        voice.setFreeText(field, value);
    }

    function typing(target: EventTarget | null): boolean {
        return (
            target instanceof HTMLElement &&
            (target.isContentEditable || ['INPUT', 'SELECT', 'TEXTAREA'].includes(target.tagName))
        );
    }

    function keydown(event: KeyboardEvent) {
        if (event.code !== 'Space' || typing(event.target) || event.ctrlKey || event.metaKey)
            return;
        event.preventDefault();
        if (!event.repeat) void voice.pttPress();
    }

    function keyup(event: KeyboardEvent) {
        if (event.code !== 'Space' || typing(event.target)) return;
        event.preventDefault();
        void voice.pttRelease();
    }

    const time = (at: Date) => at.toISOString().slice(11, 19);
</script>

<PageTitle title="Voice entry" />

<svelte:window onkeydown={keydown} onkeyup={keyup} onblur={() => void voice.pttCancel()} />

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header
        class="flex flex-wrap items-center gap-2 max-md:sticky max-md:top-0 max-md:z-20 max-md:-mx-4 max-md:-mt-4 max-md:border-b max-md:border-border max-md:bg-surface max-md:px-4 max-md:py-3 md:gap-3"
    >
        <h1 class="text-xl font-semibold">Voice entry</h1>
        <div class="flex-1"></div>
        {#if logbook.status === 'offline'}
            <span
                class="flex h-7 items-center gap-1.5 rounded-full bg-warn-bg px-2.5 text-xs font-medium text-warn md:hidden"
                ><Icon name="offline" size={14} />Offline</span
            >
        {/if}
        <label for="voice-station" class="text-xs text-muted max-md:sr-only">Station</label>
        <select
            id="voice-station"
            class="control max-w-full bg-surface max-md:order-3 max-md:min-w-[60%] max-md:flex-1 max-md:bg-bg"
            value={stationId}
            onchange={(e) => (settings.stationId = e.currentTarget.value)}
        >
            {#each logbook.activeStations as s (s.id)}
                <option value={s.id}>{stationLabel(s)}</option>
            {/each}
        </select>
        <div
            class="flex h-9 items-center gap-2 rounded-md border border-border bg-surface px-3 font-mono max-md:border-0 max-md:bg-transparent max-md:px-0"
        >
            <span class="text-xs text-muted">UTC</span>
            <span class="text-base font-semibold tabular">{clock.time}</span>
        </div>
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
                <label for="voice-contest-id" class="text-xs text-muted">CONTEST_ID</label>
                <input
                    id="voice-contest-id"
                    placeholder="optional"
                    autocomplete="off"
                    spellcheck="false"
                    class="control w-[180px] font-mono uppercase"
                    bind:value={settings.contest.contestId}
                />
            </div>
            <div class="flex flex-col gap-1">
                <label for="voice-contest-sent" class="text-xs text-muted">Sent number</label>
                <input
                    id="voice-contest-sent"
                    placeholder={'{serial}'}
                    autocomplete="off"
                    spellcheck="false"
                    class="control w-40 font-mono"
                    bind:value={settings.contest.exchangeTemplate}
                />
            </div>
            <div class="flex flex-col gap-1">
                <label for="voice-contest-next" class="text-xs text-muted">Next serial</label>
                <input
                    id="voice-contest-next"
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
                    >{(settings.contest.includeRst ? draft.rstSent : '') + sent ||
                        '(nothing)'}</strong
                >
            </div>
        </section>
    {/if}

    <div class="flex flex-wrap items-start gap-4">
        <div class="flex min-w-0 flex-[1_1_440px] flex-col gap-4">
            <section
                aria-label="Parsed QSO"
                class="flex flex-col gap-3 rounded-lg border border-accent bg-surface p-4"
            >
                <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                    <span class="text-xs text-muted">Call</span>
                    <span
                        class={[
                            'font-mono text-[32px] leading-none font-semibold tracking-[0.04em]',
                            draft.call === undefined && 'text-muted'
                        ]}>{draft.call ?? '—'}</span
                    >
                    <span class="text-xs text-muted">Band / Mode</span>
                    <span class="flex flex-col gap-0.5">
                        <span class="flex flex-wrap gap-x-2 font-mono text-lg">
                            <span>{draft.freq ? `${draft.freq} MHz` : '—'} ·</span>
                            <span>{draft.band} ·</span>
                            <span>{draft.mode} <span class="text-muted">{draft.submode}</span></span
                            >
                        </span>
                        <span class="text-xs text-muted">Carried over to the next QSO</span>
                    </span>
                    <span class="text-xs text-muted">RST</span>
                    <span class="font-mono text-lg"
                        ><span class="font-sans text-xs text-muted">Sent</span>
                        {draft.rstSent}
                        <span class="ml-3 font-sans text-xs text-muted">Rcvd</span>
                        {draft.rstRcvd}</span
                    >
                    {#if settings.contest.enabled || draft.received !== undefined}
                        <span class="text-xs text-muted">Number</span>
                        <span class="font-mono text-lg"
                            >{#if settings.contest.enabled}<span
                                    class="font-sans text-xs text-muted">Sent</span
                                >
                                {sent || '—'}
                                <span class="ml-3 font-sans text-xs text-muted">Rcvd</span>
                            {/if}<span class={draft.received === undefined ? 'text-info' : ''}
                                >{draft.received ?? 'say “number …”'}</span
                            ></span
                        >
                    {/if}
                    {#if extras.length > 0}
                        <span class="text-xs text-muted">Also</span>
                        <span class="flex flex-wrap gap-1.5">
                            {#each extras as extra (extra)}
                                <span class="rounded-full bg-surface-2 px-2 py-px text-xs"
                                    >{extra}</span
                                >
                            {/each}
                        </span>
                    {/if}
                    {#if duplicate}
                        <span></span>
                        <Notice>
                            Possible duplicate{queued ? ' in the queue' : ''}: {duplicate.CALL}
                            {duplicate.BAND}
                            {duplicate.MODE}
                            {hhmm(duplicate.TIME_ON)} UTC{duplicate.QSO_DATE ===
                            utcFields(now).QSO_DATE
                                ? ''
                                : ` ${isoDate(duplicate.QSO_DATE)}`}. Logging keeps both.
                        </Notice>
                    {/if}
                </div>

                {#if correcting}
                    <div class="flex flex-col gap-3 border-t border-border pt-3">
                        <form class="flex gap-2" onsubmit={submitCommand}>
                            <label for="voice-command" class="sr-only">Command</label>
                            <input
                                id="voice-command"
                                placeholder="received five seven, or call jh1xyz"
                                autocomplete="off"
                                spellcheck="false"
                                class="control h-10 flex-1 font-mono"
                                bind:value={command}
                            />
                            <Button type="submit" size="lg">Apply</Button>
                        </form>
                        <div class="grid grid-cols-2 gap-2.5 md:grid-cols-4">
                            <div class="flex flex-col gap-1">
                                <label for="voice-call" class="text-xs text-muted">Call</label>
                                <input
                                    id="voice-call"
                                    autocomplete="off"
                                    autocapitalize="characters"
                                    spellcheck="false"
                                    class="control font-mono uppercase"
                                    value={draft.call ?? ''}
                                    onchange={(e) => setCall(e.currentTarget.value)}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-freq" class="text-xs text-muted">Freq (MHz)</label
                                >
                                <input
                                    id="voice-freq"
                                    inputmode="decimal"
                                    placeholder="optional"
                                    class="control text-right font-mono"
                                    value={draft.freq}
                                    onchange={(e) =>
                                        voice.setDraft(
                                            withFreq(draft, e.currentTarget.value.trim())
                                        )}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-band" class="text-xs text-muted">Band</label>
                                <BandSelect
                                    id="voice-band"
                                    value={draft.band}
                                    onchange={(band) => voice.setDraft(withBand(draft, band))}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-mode" class="text-xs text-muted">Mode</label>
                                <ModeSelect
                                    id="voice-mode"
                                    mode={draft.mode}
                                    submode={draft.submode}
                                    onchange={(mode, submode) =>
                                        voice.setDraft(withMode(draft, mode, submode))}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-rsts" class="text-xs text-muted">RST sent</label>
                                <input
                                    id="voice-rsts"
                                    inputmode="numeric"
                                    class="control text-center font-mono"
                                    value={draft.rstSent}
                                    onchange={(e) =>
                                        voice.setDraft({
                                            ...draft,
                                            rstSent: e.currentTarget.value.trim()
                                        })}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-rstr" class="text-xs text-muted">RST rcvd</label>
                                <input
                                    id="voice-rstr"
                                    inputmode="numeric"
                                    class="control text-center font-mono"
                                    value={draft.rstRcvd}
                                    onchange={(e) =>
                                        voice.setDraft({
                                            ...draft,
                                            rstRcvd: e.currentTarget.value.trim()
                                        })}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-name" class="text-xs text-muted">Name</label>
                                <input
                                    id="voice-name"
                                    maxlength={FREE_TEXT_MAX_LENGTH}
                                    class="control"
                                    value={draft.name ?? ''}
                                    onchange={(e) => setText('name', e.currentTarget.value)}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-qth" class="text-xs text-muted">QTH</label>
                                <input
                                    id="voice-qth"
                                    maxlength={FREE_TEXT_MAX_LENGTH}
                                    class="control"
                                    value={draft.qth ?? ''}
                                    onchange={(e) => setText('qth', e.currentTarget.value)}
                                />
                            </div>
                        </div>
                    </div>
                {/if}

                <div class="flex flex-wrap items-center gap-2">
                    <span
                        class={[
                            'flex-[1_1_200px] text-xs',
                            problem && draft.call !== undefined ? 'text-info' : 'text-muted'
                        ]}
                        >{problem && draft.call !== undefined
                            ? problem
                            : 'Say “q s o” or press Log. Speaking a field again replaces it.'}</span
                    >
                    <Button
                        size="lg"
                        variant="toggle"
                        pressed={correcting}
                        onclick={() => (correcting = !correcting)}>Correct</Button
                    >
                    <Button size="lg" variant="ghost" class="border border-border" onclick={discard}
                        >Discard</Button
                    >
                    <Button
                        variant="primary"
                        size="xl"
                        class="font-bold max-md:flex-1"
                        disabled={problem !== undefined || stationId === '' || logging}
                        onclick={() => void logDraft()}>Log QSO</Button
                    >
                </div>

                <WarningList {warnings} />
            </section>

            <section
                aria-label="Speech engine"
                class={[
                    'flex flex-wrap items-center gap-x-3 gap-y-2 text-[13px]',
                    voice.engine !== 'ready' &&
                        'rounded-lg border border-border bg-surface px-4 py-3'
                ]}
            >
                {#if voice.engine === 'ready'}
                    <PttButton {voice} />
                {:else}
                    <div class="flex min-w-0 flex-[1_1_240px] flex-col gap-0.5">
                        <strong class="font-semibold"
                            >{voice.engine === 'loading'
                                ? 'Loading the speech engine…'
                                : 'The speech engine is not loaded'}</strong
                        >
                        <span class={voice.engine === 'error' ? 'text-error-text' : 'text-muted'}
                            >{voice.engine === 'error'
                                ? voice.engineError
                                : 'The model is downloaded once and then kept on this device, so voice entry works offline.'}</span
                        >
                    </div>
                    <Button
                        variant="primary"
                        size="lg"
                        disabled={voice.engine === 'loading'}
                        onclick={() => void voice.loadEngine()}
                        >{#if voice.engine === 'loading'}<Icon
                                name="spinner"
                                size={16}
                            />{/if}{voice.engine === 'error' ? 'Try again' : 'Load'}</Button
                    >
                {/if}
                <div class="flex w-full items-center gap-2 text-xs text-muted">
                    <span
                        >Vosk · {LANGUAGE_LABELS[voice.settings.language]} · {voice.engine ===
                        'ready'
                            ? 'ready'
                            : voice.engine}</span
                    >
                    <a href="/settings/voice">Change the language</a>
                </div>
            </section>

            <section
                aria-label="Heard"
                class="flex flex-col gap-1.5 rounded-lg border border-border bg-surface px-4 py-3"
            >
                <div class="flex items-center justify-between text-xs text-muted">
                    <span class="tracking-[0.06em] uppercase">Heard</span>
                    {#if voice.heard}
                        <span class="font-mono">{time(voice.heard.at)}</span>
                    {/if}
                </div>
                {#if voice.heard}
                    <p class="text-base leading-snug">{voice.heard.heard}</p>
                    <span class={['text-xs', voice.heard.ok ? 'text-muted' : 'text-error-text']}
                        >{voice.heard.message}</span
                    >
                {:else}
                    <p class="text-muted">
                        Hold the button and say the call, such as “juliett lima one hotel india
                        sierra”, then the reports: “received five seven sent five nine”.
                    </p>
                {/if}
            </section>

            {#if voice.utterances.length > 0}
                <section
                    aria-label="Recent utterances"
                    class="flex flex-col gap-2 max-md:order-4 max-md:hidden"
                >
                    <h2 class="eyebrow">Recent utterances</h2>
                    <div
                        class="flex flex-col rounded-lg border border-border bg-surface text-[13px]"
                    >
                        {#each voice.utterances as u (u.id)}
                            <div
                                class="flex gap-3 border-b border-surface-2 px-3 py-2 last:border-b-0"
                            >
                                <span class="font-mono text-muted">{time(u.at)}</span>
                                <span class="flex min-w-0 flex-1 flex-col">
                                    <span>{u.heard}</span>
                                    {#if u.source === 'typed'}
                                        <span class="text-xs text-muted">typed</span>
                                    {/if}
                                </span>
                                <span
                                    class={[
                                        'max-w-[50%] text-right',
                                        u.ok ? 'text-ok' : 'text-error-text'
                                    ]}>{u.message}</span
                                >
                            </div>
                        {/each}
                    </div>
                </section>
            {/if}
        </div>

        <LatestAtStation
            {logbookId}
            stationName={station?.name ?? ''}
            {unsent}
            {recent}
            contest={settings.contest.enabled}
            onsendnow={sendNow}
        />
    </div>
</main>
