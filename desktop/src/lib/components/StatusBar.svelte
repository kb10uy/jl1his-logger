<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';

    let {
        url,
        callsign,
        reachable,
        unsent,
        sending,
        onsignout
    }: {
        url: string;
        callsign: string | null;
        reachable: boolean;
        unsent: number;
        sending: boolean;
        onsignout: () => void;
    } = $props();

    const host = $derived(URL.canParse(url) ? new URL(url).host : url);
    const dot = 'size-[7px] flex-none rounded-full';
</script>

<footer
    role="status"
    aria-label={m.status_label()}
    class="-mx-[12px] -mb-[10px] flex h-[22px] flex-none items-center gap-[14px] border-t border-border bg-surface px-[12px] text-[11px] whitespace-nowrap text-muted"
>
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
    <button type="button" class="text-muted hover:text-text" onclick={onsignout}
        >{m.common_sign_out()}</button
    >
</footer>
