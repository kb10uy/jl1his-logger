<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';
    import type { RigState } from '#lib/settings/settings.svelte.ts';

    let {
        url,
        callsign,
        reachable,
        unsent,
        sending,
        rig,
        pollMs,
        onsettings
    }: {
        url: string;
        callsign: string | null;
        reachable: boolean;
        unsent: number;
        sending: boolean;
        rig: RigState;
        pollMs: number;
        onsettings?: () => void;
    } = $props();

    const host = $derived(URL.canParse(url) ? new URL(url).host : url);
    const dot = 'size-[7px] flex-none rounded-full';
</script>

<footer
    role="status"
    aria-label={m.status_label()}
    class="flex h-[22px] flex-none items-center gap-[14px] border-t border-border bg-surface px-[12px] text-[11px] whitespace-nowrap text-muted"
>
    {#if rig.state === 'connected'}
        <span
            title={m.status_rig_title({ seconds: pollMs / 1000, mode: rig.mode })}
            class="flex items-center gap-[5px]"
            ><span class={[dot, 'bg-ok']}></span>{['rigctld', rig.model, rig.address]
                .filter((s) => s)
                .join(' · ')}</span
        >
    {:else}
        <span
            title={rig.state === 'failed' ? rig.error : undefined}
            class="flex items-center gap-[5px]"
            ><span class={[dot, 'bg-[#6e7681]']}></span>{rig.state === 'off'
                ? m.status_rig_off()
                : m.status_rig_down()}</span
        >
    {/if}
    {#if reachable}
        <span
            title={m.status_server_title({ url })}
            class="flex min-w-0 items-center gap-[5px] overflow-hidden"
            ><span class={[dot, 'bg-ok']}></span><span class="truncate"
                >{[callsign, host].filter((s) => s).join(' · ')}</span
            ></span
        >
    {:else}
        <span
            title={m.status_unreachable_title()}
            class="flex min-w-0 items-center gap-[5px] overflow-hidden"
            ><span class={[dot, 'bg-error']}></span><span class="truncate"
                >{m.status_unreachable({ host })}</span
            ></span
        >
    {/if}
    <div class="flex-1"></div>
    {#if unsent > 0}
        <span title={m.status_queued_title({ count: unsent })} class="flex items-center gap-[5px]"
            ><span class={[dot, 'bg-warn']}></span>{sending
                ? m.status_sending({ count: unsent })
                : m.status_queued({ count: unsent })}</span
        >
    {:else}
        <span title={m.status_synced_title()} class="flex items-center gap-[5px]"
            ><span class={[dot, 'bg-ok']}></span>{m.status_synced()}</span
        >
    {/if}
    {#if onsettings}
        <button
            type="button"
            title={m.status_settings()}
            aria-label={m.status_settings()}
            class="-mr-[6px] flex size-[22px] items-center justify-center text-muted hover:text-text"
            onclick={onsettings}
            ><svg
                aria-hidden="true"
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><circle cx="12" cy="12" r="3" /><path
                    d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"
                /></svg
            ></button
        >
    {/if}
</footer>
