<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import type { Warning } from '#lib/api/types.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import LatestAtStation from '#lib/components/LatestAtStation.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import WarningList from '#lib/components/WarningList.svelte';
    import ContestBar from '#lib/components/entry/ContestBar.svelte';
    import LookupSummary from '#lib/components/entry/LookupSummary.svelte';
    import EntryEquipment from '#lib/components/entry/EntryEquipment.svelte';
    import FrequencyFavorites from '#lib/components/entry/FrequencyFavorites.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import PttButton from '#lib/components/voice/PttButton.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { expandExchange } from '#lib/qso/contest.ts';
    import { hhmm, isoDate, utcClock, utcFields } from '#lib/qso/format.ts';
    import { EntrySession } from '#lib/state/entry.svelte.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
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

    const QSL_TEXT = {
        none: () => '',
        requested: m.voice_qsl_requested,
        oneWay: m.voice_qsl_one_way
    } as const;
    const ENGINE_TEXT = {
        unloaded: m.voice_engine_state_unloaded,
        loading: m.voice_engine_state_loading,
        ready: m.voice_engine_state_ready,
        error: m.voice_engine_state_error
    } as const;

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const session: EntrySession = new EntrySession(logbook, {
        get call(): string | undefined {
            return draft.call;
        },
        get freq(): string {
            return draft.freq;
        },
        get band(): string {
            return draft.band;
        },
        get mode(): string {
            return draft.mode;
        },
        get submode(): string {
            return draft.submode;
        },
        get at(): Date {
            return now;
        }
    });
    const settings = session.settings;
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
    let warnings = $state<Warning[]>([]);
    let correcting = $state(false);
    let command = $state('');

    const draft = $derived(voice.draft);
    const stationId = $derived(session.stationId);
    const station = $derived(logbook.station(stationId));
    const lookup = $derived(session.lookup);
    const duplicates = $derived(session.duplicates);
    const recent = $derived(session.recent);
    const clock = $derived(utcClock(now));
    const problem = $derived(draftProblem(draft, settings.contest));
    const sent = $derived(
        expandExchange(settings.contest.exchangeTemplate, settings.contest.nextSerial)
    );
    const unsent = $derived(session.unsent);
    const queued = $derived(
        draft.call === undefined
            ? undefined
            : queuedDuplicate(unsent, draftFields(draft, settings.contest, now, logbook.area))
    );
    const duplicate = $derived(queued?.fields ?? duplicates[0]?.fields);
    const duplicateText = $derived(
        duplicate === undefined
            ? ''
            : [
                  duplicate.CALL,
                  duplicate.BAND,
                  duplicate.MODE,
                  `${hhmm(duplicate.TIME_ON)} UTC${
                      duplicate.QSO_DATE === utcFields(now).QSO_DATE
                          ? ''
                          : ` ${isoDate(duplicate.QSO_DATE)}`
                  }`
              ].join(' ')
    );
    const extras = $derived(
        [
            draft.area
                ? logbook.area
                    ? `${logbook.area.label} ${draft.area}`
                    : m.voice_area_not_recorded({ area: draft.area })
                : '',
            QSL_TEXT[draft.qsl](),
            draft.name ?? '',
            draft.qth ?? ''
        ].filter((s) => s !== '')
    );

    onMount(() => {
        const tick = setInterval(() => (now = new Date()), 1000);
        const stop = voice.start();
        return () => {
            clearInterval(tick);
            stop();
        };
    });

    async function logDraft(): Promise<LogResult> {
        if (stationId === '') return { ok: false, reason: m.voice_choose_station_first() };
        const current = voice.draft;
        const reason = draftProblem(current, settings.contest);
        if (reason !== undefined) {
            toasts.error(reason);
            return { ok: false, reason };
        }
        if (session.logging) return { ok: false, reason: m.voice_still_logging() };
        const fields = draftFields(current, settings.contest, new Date(), logbook.area);
        const result = await session.log(fields);
        voice.next();
        correcting = false;
        if ('saved' in result) {
            warnings = result.saved.warnings;
            toasts.show(m.voice_logged({ call: fields.CALL }));
        } else {
            warnings = [];
            toasts.error(
                m.voice_kept_on_device({ call: fields.CALL, error: result.error.message })
            );
        }
        return { ok: true, call: fields.CALL };
    }

    function discard() {
        voice.next();
        warnings = [];
        correcting = false;
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

<PageTitle title={m.nav_voice_entry()} />

<svelte:window onkeydown={keydown} onkeyup={keyup} onblur={() => void voice.pttCancel()} />

<PageHeader title={m.nav_voice_entry()} subtitle={logbook.logbook.name}>
    {#snippet actions()}
        <Button
            variant="toggle"
            pressed={settings.contest.enabled}
            onclick={() => (settings.contest.enabled = !settings.contest.enabled)}
            >{m.field_contest()}</Button
        >
    {/snippet}
    {#snippet toolbar()}
        <EntryEquipment {session} />
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-5">
    <LogbookLoadError />
    {#if logbook.noActiveStation}
        <Notice>{m.voice_no_active_station()}</Notice>
    {/if}

    {#if settings.contest.enabled}
        <ContestBar bind:contest={settings.contest} rstSent={draft.rstSent} />
    {/if}

    <div class="flex flex-wrap items-start gap-4">
        <div class="flex min-w-0 flex-[1_1_440px] flex-col gap-4">
            <section
                aria-label={m.voice_parsed_qso()}
                class="flex flex-col gap-3 rounded-lg border border-accent bg-surface p-4"
            >
                <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                    <span class="text-2xs text-muted">{m.field_date_utc()}</span>
                    <div class="flex items-center gap-3 font-mono tabular">
                        <span class="text-base text-muted">{clock.date}</span>
                        <span class="text-md font-semibold">{clock.time}</span>
                    </div>
                </div>
                <div class="h-px bg-border"></div>
                <div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
                    <span class="text-2xs text-muted">{m.field_call()}</span>
                    <span
                        class={[
                            'font-mono text-2xl leading-none font-semibold tracking-[0.04em]',
                            draft.call === undefined && 'text-muted'
                        ]}>{draft.call ?? '—'}</span
                    >
                    {#if lookup && lookup.call === draft.call}
                        <span></span>
                        <LookupSummary {lookup} />
                    {/if}
                    <span class="text-2xs text-muted">{m.entry_freq_band()}</span>
                    <span class="flex min-w-0 flex-wrap gap-x-2 font-mono text-md">
                        <span>{draft.freq ? `${draft.freq} MHz` : '—'} ·</span>
                        <span>{draft.band}</span>
                    </span>
                    <span class="text-2xs text-muted">{m.field_mode()}</span>
                    <div class="flex min-w-0 items-center gap-2.5">
                        <span class="min-w-0 flex-1 font-mono text-md"
                            >{draft.mode}
                            <span class="text-muted">{draft.submode}</span></span
                        >
                        <FrequencyFavorites
                            tuning={draft}
                            onselect={(favorite) =>
                                voice.setDraft(
                                    withMode(
                                        withFreq(draft, favorite.freq),
                                        favorite.mode,
                                        favorite.submode
                                    )
                                )}
                            iconOnly
                            class="h-10"
                        />
                    </div>
                    <span class="text-2xs text-muted">{m.field_rst()}</span>
                    <span class="flex flex-wrap items-center gap-x-1.5 font-mono text-md"
                        ><span class="font-sans text-2xs text-muted">{m.voice_sent()}</span>
                        {draft.rstSent}
                        <span class="ml-3 font-sans text-2xs text-muted">{m.voice_rcvd()}</span>
                        {draft.rstRcvd}</span
                    >
                    {#if settings.contest.enabled || draft.received !== undefined}
                        <span class="text-2xs text-muted">{m.voice_number()}</span>
                        <span class="flex flex-wrap items-center gap-x-1.5 font-mono text-md"
                            >{#if settings.contest.enabled}<span
                                    class="font-sans text-2xs text-muted">{m.voice_sent()}</span
                                >
                                {sent || '—'}
                                <span class="ml-3 font-sans text-2xs text-muted"
                                    >{m.voice_rcvd()}</span
                                >
                            {/if}<span class={draft.received === undefined ? 'text-info' : ''}
                                >{draft.received ?? m.voice_say_number()}</span
                            ></span
                        >
                    {/if}
                    {#if extras.length > 0}
                        <span class="text-2xs text-muted">{m.voice_also()}</span>
                        <span class="flex flex-wrap gap-1.5">
                            {#each extras as extra (extra)}
                                <span class="rounded-full bg-surface-2 px-2 py-px text-2xs"
                                    >{extra}</span
                                >
                            {/each}
                        </span>
                    {/if}
                    {#if duplicate}
                        <span></span>
                        <Notice>
                            {queued
                                ? m.voice_possible_duplicate_queued({ qso: duplicateText })
                                : m.voice_possible_duplicate({ qso: duplicateText })}
                        </Notice>
                    {/if}
                </div>

                {#if correcting}
                    <div class="flex flex-col gap-3 border-t border-border pt-3">
                        <form class="flex gap-2" onsubmit={submitCommand}>
                            <label for="voice-command" class="sr-only">{m.voice_command()}</label>
                            <input
                                id="voice-command"
                                placeholder={m.voice_command_placeholder()}
                                autocomplete="off"
                                spellcheck="false"
                                class="control h-9 flex-1 font-mono"
                                bind:value={command}
                            />
                            <Button type="submit" size="lg">{m.common_apply()}</Button>
                        </form>
                        <div class="grid grid-cols-2 gap-2.5 md:grid-cols-4">
                            <div class="flex flex-col gap-1">
                                <label for="voice-call" class="text-2xs text-muted"
                                    >{m.field_call()}</label
                                >
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
                                <label for="voice-freq" class="text-2xs text-muted"
                                    >{m.voice_freq_mhz()}</label
                                >
                                <input
                                    id="voice-freq"
                                    inputmode="decimal"
                                    placeholder={m.voice_optional()}
                                    class="control text-right font-mono"
                                    value={draft.freq}
                                    onchange={(e) =>
                                        voice.setDraft(
                                            withFreq(draft, e.currentTarget.value.trim())
                                        )}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-band" class="text-2xs text-muted"
                                    >{m.field_band()}</label
                                >
                                <BandSelect
                                    id="voice-band"
                                    value={draft.band}
                                    onchange={(band) => voice.setDraft(withBand(draft, band))}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-mode" class="text-2xs text-muted"
                                    >{m.field_mode()}</label
                                >
                                <ModeSelect
                                    id="voice-mode"
                                    mode={draft.mode}
                                    submode={draft.submode}
                                    onchange={(mode, submode) =>
                                        voice.setDraft(withMode(draft, mode, submode))}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-rsts" class="text-2xs text-muted"
                                    >{m.field_rst_sent()}</label
                                >
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
                                <label for="voice-rstr" class="text-2xs text-muted"
                                    >{m.field_rst_rcvd()}</label
                                >
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
                                <label for="voice-name" class="text-2xs text-muted"
                                    >{m.field_name()}</label
                                >
                                <input
                                    id="voice-name"
                                    maxlength={FREE_TEXT_MAX_LENGTH}
                                    class="control"
                                    value={draft.name ?? ''}
                                    onchange={(e) => setText('name', e.currentTarget.value)}
                                />
                            </div>
                            <div class="flex flex-col gap-1">
                                <label for="voice-qth" class="text-2xs text-muted"
                                    >{m.field_qth()}</label
                                >
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
                    <span class="flex-[1_1_200px] text-2xs text-info max-md:basis-full"
                        >{draft.call !== undefined ? problem : ''}</span
                    >
                    <Button
                        size="lg"
                        variant="toggle"
                        class="max-md:order-2 max-md:flex-1"
                        pressed={correcting}
                        onclick={() => (correcting = !correcting)}>{m.voice_correct()}</Button
                    >
                    <Button
                        size="lg"
                        variant="ghost"
                        class="border! border-border max-md:order-1 max-md:flex-1"
                        onclick={discard}>{m.common_discard()}</Button
                    >
                    <Button
                        variant="primary"
                        size="xl"
                        class="font-semibold max-md:order-3 max-md:basis-full"
                        disabled={problem !== undefined || stationId === '' || session.logging}
                        onclick={() => void logDraft()}>{m.voice_log_qso()}</Button
                    >
                </div>

                <WarningList {warnings} />
            </section>

            <section
                aria-label={m.voice_speech_engine()}
                class={[
                    'flex flex-wrap items-center gap-x-3 gap-y-2 text-sm',
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
                                ? m.voice_engine_loading()
                                : m.voice_engine_not_loaded()}</strong
                        >
                        <span class={voice.engine === 'error' ? 'text-error-text' : 'text-muted'}
                            >{voice.engine === 'error'
                                ? voice.engineError
                                : m.voice_engine_offline_hint()}</span
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
                            />{/if}{voice.engine === 'error'
                            ? m.common_try_again()
                            : m.voice_load()}</Button
                    >
                {/if}
                <div class="flex w-full items-center gap-2 text-2xs text-muted">
                    <span
                        >Vosk · {LANGUAGE_LABELS[voice.settings.language]()} · {ENGINE_TEXT[
                            voice.engine
                        ]()}</span
                    >
                    <a href="/settings/voice">{m.voice_change_language()}</a>
                </div>
            </section>

            <section
                aria-label={m.voice_heard()}
                class="flex flex-col gap-1.5 rounded-lg border border-border bg-surface px-4 py-3"
            >
                <div class="flex items-center justify-between text-2xs text-muted">
                    <span>{m.voice_heard()}</span>
                    {#if voice.heard}
                        <span class="font-mono">{time(voice.heard.at)}</span>
                    {/if}
                </div>
                {#if voice.heard}
                    <p class="text-base leading-snug">{voice.heard.heard}</p>
                    <span class={['text-2xs', voice.heard.ok ? 'text-muted' : 'text-error-text']}
                        >{voice.heard.message}</span
                    >
                {:else}
                    <p class="text-muted">
                        {m.voice_heard_hint()}
                    </p>
                {/if}
            </section>

            {#if voice.utterances.length > 0}
                <section
                    aria-label={m.voice_recent_utterances()}
                    class="flex flex-col gap-2 max-md:order-4 max-md:hidden"
                >
                    <h2 class="eyebrow">{m.voice_recent_utterances()}</h2>
                    <div class="flex flex-col rounded-lg border border-border bg-surface text-sm">
                        {#each voice.utterances as u (u.id)}
                            <div
                                class="flex gap-3 border-b border-surface-2 px-3 py-2 last:border-b-0"
                            >
                                <span class="font-mono text-muted">{time(u.at)}</span>
                                <span class="flex min-w-0 flex-1 flex-col">
                                    <span>{u.heard}</span>
                                    {#if u.source === 'typed'}
                                        <span class="text-2xs text-muted">{m.voice_typed()}</span>
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
            onsendnow={() => session.sendNow()}
        />
    </div>
</main>
