<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import { entryFields, entryProblem, type Entry } from '#client/entry/record.ts';
    import { expandExchange } from '#client/qso/contest.ts';
    import { utcClock, utcFields } from '#client/qso/format.ts';
    import { defaultRst } from '#client/qso/modes.ts';
    import { QSL_REQUESTS } from '#client/qso/qsl.ts';
    import { withBand, withFreq, withMode } from '#client/qso/tuning.ts';
    import { server } from '#lib/api/request.svelte.ts';
    import BandSelect from '#lib/components/BandSelect.svelte';
    import ContestBar from '#lib/components/ContestBar.svelte';
    import EntryHeader from '#lib/components/EntryHeader.svelte';
    import ExtraFields from '#lib/components/ExtraFields.svelte';
    import LatestList from '#lib/components/LatestList.svelte';
    import LookupLine from '#lib/components/LookupLine.svelte';
    import ModeSelect from '#lib/components/ModeSelect.svelte';
    import StatusBar from '#lib/components/StatusBar.svelte';
    import { EntryState } from '#lib/entry/entry.svelte.ts';
    import { outbox } from '#lib/entry/outbox.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/session/session.svelte.ts';

    const desk: EntryState = new EntryState({
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
    const settings = desk.settings;

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
        area: '',
        qsl: 'none',
        comment: '',
        txPwr: '',
        notes: '',
        extra: []
    });
    let now = $state(new Date());
    let locked = $state<Date | undefined>(undefined);
    let extraOpen = $state(false);
    let callInput = $state<HTMLInputElement>();
    let form = $state<HTMLFormElement>();

    const signedIn = $derived(session.current?.state === 'signedIn' ? session.current : undefined);
    const call = $derived(entry.call.trim().toUpperCase());
    const at = $derived(locked ?? now);
    const clock = $derived(utcClock(at));
    const area = $derived(desk.logbook?.area_field ?? null);
    const previous = $derived(desk.lookup?.previous.fields);
    const problem = $derived(entryProblem(entry, settings.contest, area));
    const cantLog = $derived(problem !== undefined || desk.stationId === '' || desk.logging);
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

    onMount(() => {
        if (signedIn?.logbooks === null) server.reachable = false;
        void desk.load(signedIn?.logbooks ?? null);
        const tick = setInterval(() => (now = new Date()), 1000);
        callInput?.focus();
        return () => clearInterval(tick);
    });

    function clear() {
        entry.call = '';
        entry.received = '';
        entry.name = '';
        entry.qth = '';
        entry.area = '';
        entry.qsl = 'none';
        entry.comment = '';
        entry.notes = '';
        entry.extra = [];
        callInput?.focus();
    }

    async function log(event: Event) {
        event.preventDefault();
        if (cantLog) return;
        const fields = entryFields(entry, settings.contest, at, area);
        await desk.log(fields);
        clear();
    }
</script>

<svelte:window
    onkeydown={(e) => {
        if (e.key === 'Escape' && form?.contains(document.activeElement)) clear();
    }}
/>

<div class="flex h-dvh flex-col gap-2 overflow-hidden px-[12px] py-[10px] text-[13px]">
    <EntryHeader {desk} oninstrument={() => (entry.txPwr = '')} />

    {#if settings.contest.enabled}
        <ContestBar bind:contest={settings.contest} />
    {/if}

    <form
        aria-label={m.entry_new_qso()}
        class="flex flex-none flex-col gap-2 rounded-lg border border-accent bg-surface p-[10px]"
        onsubmit={log}
        bind:this={form}
    >
        <div class="flex h-[32px] items-center gap-1.5">
            <label for="freq" class="text-[11px] text-muted">{m.entry_freq()}</label>
            <input
                id="freq"
                inputmode="decimal"
                autocomplete="off"
                class="field w-[84px] text-right font-mono"
                value={entry.freq}
                oninput={(e) => (entry = withFreq(entry, e.currentTarget.value))}
            />
            <span class="font-mono text-[10px] text-muted">MHz</span>
            <label for="band" class="ml-1.5 text-[11px] text-muted">{m.entry_band()}</label>
            <BandSelect
                id="band"
                class="w-[64px] px-1.5"
                value={entry.band}
                onchange={(band) => (entry = withBand(entry, band))}
            />
            <label for="mode" class="ml-1.5 text-[11px] text-muted">{m.entry_mode()}</label>
            <ModeSelect
                id="mode"
                class="w-[92px] px-1.5"
                mode={entry.mode}
                submode={entry.submode}
                onchange={(mode, submode) => (entry = withMode(entry, mode, submode))}
            />
            <div class="flex-1"></div>
            <span class="text-[11px] text-muted">UTC</span>
            <span class="font-mono text-[12px] text-muted tabular">{clock.date}</span>
            <span class={['font-mono text-[17px] font-semibold tabular', locked && 'text-warn']}
                >{clock.time}</span
            >
            <button
                type="button"
                title={m.entry_lock_title()}
                aria-pressed={locked !== undefined}
                class={[
                    'h-[32px] rounded-md border px-2.5 text-[12px]',
                    locked
                        ? 'border-warn bg-warn-bg font-semibold text-warn'
                        : 'border-border bg-surface'
                ]}
                onclick={() => (locked = locked ? undefined : new Date())}>{m.entry_lock()}</button
            >
        </div>

        <div
            class={[
                'grid items-end gap-2',
                settings.contest.enabled
                    ? 'grid-cols-[minmax(0,1fr)_64px_64px_116px_116px]'
                    : 'grid-cols-[minmax(0,1fr)_64px_64px]'
            ]}
        >
            <div class="flex flex-col gap-[3px]">
                <label for="call" class="text-[11px] text-muted">{m.entry_call()}</label>
                <input
                    id="call"
                    bind:this={callInput}
                    bind:value={entry.call}
                    autocomplete="off"
                    spellcheck="false"
                    class="h-[40px] min-w-0 rounded-md border-2 border-accent bg-bg px-[10px] font-mono text-[22px] font-semibold tracking-[0.04em] uppercase outline-none"
                />
            </div>
            <div class="flex flex-col gap-[3px]">
                <label for="rst-sent" class="text-[11px] text-muted">{m.entry_sent()}</label>
                <input
                    id="rst-sent"
                    inputmode="numeric"
                    autocomplete="off"
                    class="field h-[40px] px-0 text-center font-mono text-[17px]"
                    bind:value={entry.rstSent}
                />
            </div>
            <div class="flex flex-col gap-[3px]">
                <label for="rst-rcvd" class="text-[11px] text-muted">{m.entry_rcvd()}</label>
                <input
                    id="rst-rcvd"
                    inputmode="numeric"
                    autocomplete="off"
                    class="field h-[40px] px-0 text-center font-mono text-[17px]"
                    bind:value={entry.rstRcvd}
                />
            </div>
            {#if settings.contest.enabled}
                <div class="flex flex-col gap-[3px]">
                    <span class="text-[11px] text-info">{m.entry_sent_number()}</span>
                    <output
                        aria-label={m.entry_sent_number_label()}
                        class="flex h-[40px] items-center overflow-hidden rounded-md border border-dashed border-info-strong px-[10px] font-mono text-[17px] whitespace-nowrap"
                        >{sentShown}</output
                    >
                </div>
                <div class="flex flex-col gap-[3px]">
                    <label for="received" class="text-[11px] text-info"
                        >{m.entry_rcvd_number()}</label
                    >
                    <input
                        id="received"
                        required
                        autocomplete="off"
                        spellcheck="false"
                        class={[
                            'field h-[40px] px-[10px] font-mono text-[17px] uppercase outline-none',
                            entry.received.trim() === '' && 'border-2 border-info'
                        ]}
                        bind:value={entry.received}
                    />
                </div>
            {/if}
        </div>

        <LookupLine
            shown={call.length >= 3}
            lookup={desk.lookup}
            last={desk.last}
            duplicates={desk.duplicates}
            today={utcFields(now).QSO_DATE}
        />

        <div
            class={[
                'grid items-end gap-2',
                area
                    ? 'grid-cols-[minmax(0,1fr)_minmax(0,1fr)_112px]'
                    : 'grid-cols-[minmax(0,1fr)_minmax(0,1fr)]'
            ]}
        >
            <div class="flex flex-col gap-[3px]">
                <label for="name" class="text-[11px] text-muted">{m.entry_name()}</label>
                <input
                    id="name"
                    placeholder={previous?.NAME}
                    class="field"
                    bind:value={entry.name}
                />
            </div>
            <div class="flex flex-col gap-[3px]">
                <label for="qth" class="text-[11px] text-muted">{m.entry_qth()}</label>
                <input id="qth" placeholder={previous?.QTH} class="field" bind:value={entry.qth} />
            </div>
            {#if area}
                <div class="flex flex-col gap-[3px]">
                    <label for="area" title={area.label} class="truncate text-[11px] text-muted"
                        >{area.label}</label
                    >
                    <input
                        id="area"
                        placeholder={previous?.[area.name]}
                        autocomplete="off"
                        spellcheck="false"
                        class="field font-mono"
                        bind:value={entry.area}
                    />
                </div>
            {/if}
        </div>

        <div class="grid grid-cols-[minmax(0,1fr)_112px_auto] items-end gap-2">
            <div class="flex flex-col gap-[3px]">
                <label for="comment" class="text-[11px] text-muted">{m.entry_comment()}</label>
                <input id="comment" class="field" bind:value={entry.comment} />
            </div>
            <div class="flex flex-col gap-[3px]">
                <label for="qsl" class="text-[11px] text-muted">{m.entry_qsl()}</label>
                <select id="qsl" class="field px-1.5" bind:value={entry.qsl}>
                    {#each QSL_REQUESTS as [value, label] (value)}
                        <option {value}>{label()}</option>
                    {/each}
                </select>
            </div>
            <div class="flex items-center gap-2">
                <button
                    type="button"
                    aria-expanded={extraOpen}
                    aria-controls="extra"
                    class={[
                        'flex h-[32px] items-center gap-1 rounded-md px-2 text-[12px]',
                        extraOpen ? 'bg-surface-2 text-text' : 'text-muted'
                    ]}
                    onclick={() => (extraOpen = !extraOpen)}
                    >{m.entry_extra()}{extraCount > 0 ? ` · ${extraCount}` : ''}<svg
                        aria-hidden="true"
                        width="12"
                        height="12"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        class={extraOpen ? 'rotate-180' : ''}><path d="M6 9l6 6 6-6" /></svg
                    ></button
                >
                <button
                    type="button"
                    class="h-[32px] rounded-md border border-border bg-surface-2 px-3 text-[12px]"
                    onclick={clear}
                    >{m.entry_clear()}
                    <span class="font-mono text-[10px] text-muted">Esc</span></button
                >
                <button
                    type="submit"
                    title={problem}
                    disabled={cantLog}
                    class={[
                        'h-[32px] rounded-md px-4 text-[13px] font-bold whitespace-nowrap',
                        cantLog
                            ? 'cursor-not-allowed bg-surface-2 text-muted'
                            : 'bg-accent text-accent-text'
                    ]}
                    >{m.entry_log_qso()}
                    <span class="font-mono text-[11px] font-medium opacity-80">↵</span></button
                >
            </div>
        </div>

        {#if extraOpen}
            <ExtraFields bind:entry {area} power={desk.instrument?.tx_pwr ?? ''} />
        {/if}
    </form>

    <LatestList
        unsent={desk.unsent}
        recent={desk.recent}
        contest={settings.contest.enabled}
        onsendnow={() => desk.sendUnsent()}
    />

    {#if signedIn}
        <StatusBar
            url={signedIn.url}
            callsign={signedIn.callsign}
            reachable={server.reachable}
            unsent={outbox.items.length}
            sending={outbox.sending}
            onsignout={() => session.signOut()}
        />
    {/if}
</div>
