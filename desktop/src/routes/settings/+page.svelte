<script lang="ts">
    import { goto } from '$app/navigation';
    import { MODE_CHOICES, modeKey } from '#client/qso/modes.ts';
    import { server } from '#lib/api/request.svelte.ts';
    import StatusBar from '#lib/components/StatusBar.svelte';
    import { outbox } from '#lib/entry/outbox.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { locales } from '#lib/paraglide/runtime.js';
    import { RIG_MODES, fitting, loggedFor } from '#lib/rig/modes.ts';
    import { errorText } from '#lib/session/errors.ts';
    import { session } from '#lib/session/session.svelte.ts';
    import { LOCALE_NAMES, asLocale, systemLanguage } from '#lib/settings/language.ts';
    import { isLoopback } from '#lib/settings/listener.ts';
    import {
        settings,
        type ListenerSettings,
        type ListenerState,
        type RigSettings
    } from '#lib/settings/settings.svelte.ts';

    const signedIn = $derived(session.current?.state === 'signedIn' ? session.current : undefined);
    const current = $derived(settings.current);
    const rig = $derived(settings.rig);
    const system = systemLanguage(navigator.languages);
    const POLLS = [500, 1000, 2000, 5000];

    const label = (key: string) =>
        MODE_CHOICES.find((c) => modeKey(c.mode, c.submode) === key)?.label ?? key;

    const status = $derived.by(() => {
        if (current?.rig.enabled !== true) return { tone: 'off', text: m.settings_rig_off() };
        switch (rig.state) {
            case 'off':
            case 'connecting':
                return {
                    tone: 'busy',
                    text: m.settings_rig_connecting({
                        address: rig.state === 'off' ? '' : rig.address
                    })
                };
            case 'connected':
                return {
                    tone: 'ok',
                    text: [
                        m.settings_rig_connected({ address: rig.address }),
                        rig.model,
                        `${(rig.frequencyHz / 1_000_000).toFixed(6)} MHz`,
                        rig.mode
                    ]
                        .filter((s) => s)
                        .join(' · ')
                };
            case 'failed':
                return { tone: 'error', text: m.settings_rig_failed({ error: rig.error }) };
        }
    });

    function saveRig(change: Partial<RigSettings>) {
        if (current === undefined) return;
        void settings.save({ rig: { ...current.rig, ...change } });
    }

    function saveListeners(listeners: ListenerSettings[]) {
        void settings.save({ listeners });
    }

    function saveListener(index: number, change: Partial<ListenerSettings>) {
        if (current === undefined) return;
        saveListeners(current.listeners.map((l, i) => (i === index ? { ...l, ...change } : l)));
    }

    const exposed = $derived((current?.listeners ?? []).filter((l) => !isLoopback(l.host)));

    function listenerStatus(listener: ListenerSettings, state: ListenerState | undefined) {
        if (!listener.enabled || state === undefined || state.state === 'off')
            return { tone: 'off', text: m.settings_listen_off() };
        if (state.state === 'listening')
            return {
                tone: 'ok',
                text: listener.subscribe
                    ? m.settings_listen_subscribed({ count: state.subscribers })
                    : m.settings_listen_listening()
            };
        return { tone: 'error', text: m.settings_listen_failed({ error: state.error }) };
    }
</script>

<svelte:window
    onkeydown={(e) => {
        if (e.key === 'Escape') goto('/');
    }}
/>

<div class="flex h-dvh flex-col text-[13px]">
    <header class="flex h-[44px] flex-none items-center gap-2.5 border-b border-border px-[12px]">
        <button
            type="button"
            class="flex h-[30px] items-center gap-1.5 rounded-md border border-border bg-surface px-2.5 text-[12px]"
            onclick={() => goto('/')}
            ><svg
                aria-hidden="true"
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"><path d="M15 6l-6 6 6 6" /></svg
            >{m.settings_back()}
            <span class="font-mono text-[10px] text-muted">Esc</span></button
        >
        <h1 class="m-0 text-[15px] font-semibold">{m.settings_title()}</h1>
        <div class="flex-1"></div>
        {#if current}
            <span title={current.path} class="text-[11px] text-muted"
                >{m.settings_saved({ file: current.path.split(/[\\/]/).at(-1) ?? '' })}</span
            >
        {/if}
    </header>

    <main class="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-[12px]">
        {#if settings.error}
            <p
                role="alert"
                class="m-0 rounded-md bg-error-bg px-2.5 py-2 text-[12px] text-error-text"
            >
                {errorText(settings.error)}
            </p>
        {/if}

        {#if current}
            <section
                aria-labelledby="settings-rig"
                class="flex flex-col gap-2.5 rounded-lg border border-border bg-surface p-[12px]"
            >
                <div class="flex items-center gap-2.5">
                    <h2 id="settings-rig" class="m-0 text-[13px] font-semibold">
                        {m.settings_rig()}
                    </h2>
                    <span class="text-[12px] text-muted">{m.settings_rig_lead()}</span>
                    <div class="flex-1"></div>
                    <button
                        type="button"
                        role="switch"
                        aria-checked={current.rig.enabled}
                        aria-label={m.settings_rig_switch()}
                        class={[
                            'relative h-[20px] w-[36px] flex-none rounded-full',
                            current.rig.enabled ? 'bg-info-strong' : 'bg-border'
                        ]}
                        onclick={() => saveRig({ enabled: !current.rig.enabled })}
                        ><span
                            class={[
                                'absolute top-[2px] size-[16px] rounded-full bg-text',
                                current.rig.enabled ? 'left-[18px]' : 'left-[2px]'
                            ]}
                        ></span></button
                    >
                </div>

                <div class={['flex flex-col gap-2.5', !current.rig.enabled && 'opacity-50']}>
                    <div class="flex items-end gap-2">
                        <div class="flex flex-col gap-[3px]">
                            <label for="rig-host" class="text-[11px] text-muted"
                                >{m.settings_rig_host()}</label
                            >
                            <input
                                id="rig-host"
                                spellcheck="false"
                                autocomplete="off"
                                class="field w-[180px] font-mono"
                                value={current.rig.host}
                                onchange={(e) => {
                                    const host = e.currentTarget.value.trim();
                                    if (host !== '') saveRig({ host });
                                }}
                            />
                        </div>
                        <div class="flex flex-col gap-[3px]">
                            <label for="rig-port" class="text-[11px] text-muted"
                                >{m.settings_rig_port()}</label
                            >
                            <input
                                id="rig-port"
                                type="number"
                                min="1"
                                max="65535"
                                class="field w-[76px] text-right font-mono"
                                value={current.rig.port}
                                onchange={(e) => {
                                    const port = Number(e.currentTarget.value);
                                    if (Number.isInteger(port) && port >= 1 && port <= 65535)
                                        saveRig({ port });
                                }}
                            />
                        </div>
                        <div class="flex flex-col gap-[3px]">
                            <label for="rig-poll" class="text-[11px] text-muted"
                                >{m.settings_rig_poll()}</label
                            >
                            <select
                                id="rig-poll"
                                class="field w-[84px] px-1.5"
                                value={current.rig.poll_ms}
                                onchange={(e) =>
                                    saveRig({ poll_ms: Number(e.currentTarget.value) })}
                            >
                                {#each POLLS as poll (poll)}
                                    <option value={poll}>{poll / 1000} s</option>
                                {/each}
                            </select>
                        </div>
                        <button
                            type="button"
                            disabled={!current.rig.enabled}
                            class="h-[32px] rounded-md border border-border bg-surface-2 px-3 text-[12px]"
                            onclick={() => settings.reconnect()}
                            >{m.settings_rig_reconnect()}</button
                        >
                    </div>

                    <div role="status" class="flex h-[20px] items-center gap-1.5 text-[12px]">
                        <span
                            class={[
                                'size-[7px] flex-none rounded-full',
                                status.tone === 'ok' && 'bg-ok',
                                status.tone === 'busy' && 'bg-info',
                                status.tone === 'error' && 'bg-error',
                                status.tone === 'off' && 'bg-[#6e7681]'
                            ]}
                        ></span>
                        <span
                            title={status.text}
                            class={[
                                'truncate',
                                status.tone === 'ok' && 'text-text',
                                status.tone === 'error' && 'text-error-text',
                                (status.tone === 'off' || status.tone === 'busy') && 'text-muted'
                            ]}>{status.text}</span
                        >
                    </div>

                    <div class="flex flex-col gap-1.5 border-t border-border pt-2.5">
                        <div class="flex items-baseline gap-2">
                            <span class="flex-none eyebrow text-[11px]"
                                >{m.settings_rig_modes()}</span
                            >
                            <span class="text-[11px] text-muted">{m.settings_rig_modes_hint()}</span
                            >
                        </div>
                        <div class="grid grid-cols-3 gap-x-3 gap-y-1.5">
                            {#each RIG_MODES as mode (mode)}
                                <div class="flex items-center gap-2">
                                    <span class="w-[56px] font-mono text-[12px] text-muted"
                                        >{mode}</span
                                    >
                                    <span aria-hidden="true" class="text-[#6e7681]">→</span>
                                    <select
                                        aria-label={m.settings_rig_mode_label({ mode })}
                                        class="field h-[28px] min-w-0 flex-1 px-1.5 font-mono text-[12px]"
                                        value={loggedFor(mode, current.rig.modes)}
                                        onchange={(e) =>
                                            saveRig({
                                                modes: {
                                                    ...current.rig.modes,
                                                    [mode]: e.currentTarget.value
                                                }
                                            })}
                                    >
                                        {#each fitting(mode) as key (key)}
                                            <option value={key}>{label(key)}</option>
                                        {/each}
                                    </select>
                                </div>
                            {/each}
                        </div>
                    </div>
                </div>
            </section>

            <section
                aria-labelledby="settings-listen"
                class="flex flex-col gap-2 rounded-lg border border-border bg-surface p-[12px]"
            >
                <div class="flex items-center gap-2.5">
                    <h2 id="settings-listen" class="m-0 flex-none text-[13px] font-semibold">
                        {m.settings_listen()}
                    </h2>
                    <span class="truncate text-[12px] text-muted" title={m.settings_listen_lead()}
                        >{m.settings_listen_lead()}</span
                    >
                </div>
                {#each current.listeners as listener, index (index)}
                    {@const status = listenerStatus(listener, settings.listening[index])}
                    <div class="flex items-center gap-2">
                        <button
                            type="button"
                            role="switch"
                            aria-checked={listener.enabled}
                            aria-label={m.settings_listen_switch({
                                address: `${listener.host}:${listener.port}`
                            })}
                            class={[
                                'relative h-[20px] w-[36px] flex-none rounded-full',
                                listener.enabled ? 'bg-info-strong' : 'bg-border'
                            ]}
                            onclick={() => saveListener(index, { enabled: !listener.enabled })}
                            ><span
                                class={[
                                    'absolute top-[2px] size-[16px] rounded-full bg-text',
                                    listener.enabled ? 'left-[18px]' : 'left-[2px]'
                                ]}
                            ></span></button
                        >
                        <input
                            aria-label={m.settings_listen_host()}
                            spellcheck="false"
                            autocomplete="off"
                            class="field h-[28px] w-[150px] font-mono text-[12px]"
                            value={listener.host}
                            onchange={(e) => {
                                const host = e.currentTarget.value.trim();
                                if (host !== '') saveListener(index, { host });
                            }}
                        />
                        <input
                            aria-label={m.settings_listen_port()}
                            type="number"
                            min="1"
                            max="65535"
                            class="field h-[28px] w-[76px] text-right font-mono text-[12px]"
                            value={listener.port}
                            onchange={(e) => {
                                const port = Number(e.currentTarget.value);
                                if (Number.isInteger(port) && port >= 1 && port <= 65535)
                                    saveListener(index, { port });
                            }}
                        />
                        <label
                            title={m.settings_listen_subscribe_title()}
                            class={[
                                'flex flex-none items-center gap-[5px] text-[12px]',
                                !listener.enabled && 'text-muted'
                            ]}
                            ><input
                                type="checkbox"
                                class="m-0 size-[14px] accent-info-strong"
                                checked={listener.subscribe}
                                disabled={!listener.enabled}
                                onchange={(e) =>
                                    saveListener(index, { subscribe: e.currentTarget.checked })}
                            />{m.settings_listen_subscribe()}</label
                        >
                        <span class="flex min-w-0 flex-1 items-center gap-1.5 text-[12px]">
                            <span
                                class={[
                                    'size-[7px] flex-none rounded-full',
                                    status.tone === 'ok' && 'bg-ok',
                                    status.tone === 'error' && 'bg-error',
                                    status.tone === 'off' && 'bg-[#6e7681]'
                                ]}
                            ></span>
                            <span
                                title={status.text}
                                class={[
                                    'truncate',
                                    status.tone === 'ok' && 'text-text',
                                    status.tone === 'error' && 'text-error-text',
                                    status.tone === 'off' && 'text-muted'
                                ]}>{status.text}</span
                            >
                        </span>
                        <button
                            type="button"
                            aria-label={m.settings_listen_remove()}
                            class="flex-none text-muted"
                            onclick={() =>
                                saveListeners(current.listeners.filter((_, i) => i !== index))}
                            ><svg
                                aria-hidden="true"
                                width="12"
                                height="12"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg
                            ></button
                        >
                    </div>
                {/each}
                {#if exposed.length > 0}
                    <p class="m-0 text-[12px] text-warn">
                        {m.settings_listen_exposed({
                            addresses: exposed.map((l) => `${l.host}:${l.port}`).join(', ')
                        })}
                    </p>
                {/if}
                <div>
                    <button
                        type="button"
                        class="h-[28px] rounded-md border border-border bg-surface-2 px-2.5 text-[12px]"
                        onclick={() =>
                            saveListeners([
                                ...current.listeners,
                                { enabled: false, host: '127.0.0.1', port: 2237, subscribe: false }
                            ])}>{m.settings_listen_add()}</button
                    >
                </div>
            </section>

            {#if signedIn}
                <section
                    aria-labelledby="settings-server"
                    class="flex flex-col gap-2.5 rounded-lg border border-border bg-surface p-[12px]"
                >
                    <div class="flex items-center justify-between gap-2.5">
                        <h2 id="settings-server" class="m-0 text-[13px] font-semibold">
                            {m.settings_server()}
                        </h2>
                        <button
                            type="button"
                            class="h-[28px] rounded-md border border-error-border px-2.5 text-[12px] text-error-text"
                            onclick={() => session.signOut()}>{m.common_sign_out()}</button
                        >
                    </div>
                    <div
                        class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-x-3 gap-y-2 text-[12px]"
                    >
                        <span class="text-muted">{m.settings_server_url()}</span>
                        <span class="font-mono">{signedIn.url}</span>
                        <span class="text-muted">{m.settings_server_signed_in()}</span>
                        <span
                            ><strong class="font-mono font-semibold"
                                >{signedIn.callsign ?? '—'}</strong
                            >{#if signedIn.logbooks}
                                <span class="text-muted"
                                    >{` · ${m.settings_server_logbooks({
                                        count: signedIn.logbooks.length
                                    })}`}</span
                                >{/if}</span
                        >
                        <span class="text-muted">{m.settings_server_token()}</span>
                        <span class="text-muted"
                            >{m.settings_server_token_store({
                                store: current.credentialStore
                            })}</span
                        >
                    </div>
                </section>
            {/if}

            <section
                aria-labelledby="settings-tray"
                class="flex items-center gap-3 rounded-lg border border-border bg-surface p-[12px]"
            >
                <h2 id="settings-tray" class="m-0 w-[96px] flex-none text-[13px] font-semibold">
                    {m.settings_tray()}
                </h2>
                <button
                    type="button"
                    role="switch"
                    aria-checked={current.closeToTray}
                    aria-label={m.settings_tray_switch()}
                    class={[
                        'relative h-[20px] w-[36px] flex-none rounded-full',
                        current.closeToTray ? 'bg-info-strong' : 'bg-border'
                    ]}
                    onclick={() => settings.save({ closeToTray: !current.closeToTray })}
                    ><span
                        class={[
                            'absolute top-[2px] size-[16px] rounded-full bg-text',
                            current.closeToTray ? 'left-[18px]' : 'left-[2px]'
                        ]}
                    ></span></button
                >
                <span class="text-[12px] text-muted"
                    >{current.closeToTray ? m.settings_tray_on() : m.settings_tray_off()}</span
                >
            </section>

            <section
                aria-labelledby="settings-language"
                class="flex items-center gap-3 rounded-lg border border-border bg-surface p-[12px]"
            >
                <h2 id="settings-language" class="m-0 w-[96px] text-[13px] font-semibold">
                    <label for="language">{m.settings_language()}</label>
                </h2>
                <select
                    id="language"
                    class="field w-[200px] px-1.5"
                    value={current.language ?? ''}
                    onchange={(e) => settings.save({ language: asLocale(e.currentTarget.value) })}
                >
                    <option value=""
                        >{m.settings_language_system({ language: LOCALE_NAMES[system] })}</option
                    >
                    {#each locales as locale (locale)}
                        <option value={locale}>{LOCALE_NAMES[locale]}</option>
                    {/each}
                </select>
                <span class="text-[12px] text-muted"
                    >{current.language === null
                        ? m.settings_language_follows()
                        : m.settings_language_reload()}</span
                >
            </section>
        {/if}
    </main>

    {#if signedIn}
        <StatusBar
            url={signedIn.url}
            callsign={signedIn.callsign}
            reachable={server.reachable}
            unsent={outbox.items.length}
            sending={outbox.sending}
            {rig}
            pollMs={current?.rig.poll_ms ?? 1000}
        />
    {/if}
</div>

<style>
    input[type='number'] {
        appearance: textfield;
    }

    input[type='number']::-webkit-inner-spin-button,
    input[type='number']::-webkit-outer-spin-button {
        appearance: none;
        margin: 0;
    }
</style>
