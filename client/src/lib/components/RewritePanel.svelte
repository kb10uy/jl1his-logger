<script lang="ts">
    import { ApiError } from '#lib/api/client.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { SLOT, formatNumber } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { toasts } from '#lib/state/toast.svelte.ts';

    interface Range {
        from: string | undefined;
        until: string | undefined;
    }

    let {
        lead,
        confirmDescription,
        changed,
        count,
        rewrite
    }: {
        lead: string;
        confirmDescription: (qsos: string) => string;
        changed: boolean;
        count: (range: Range) => Promise<number>;
        rewrite: (range: Range, expected: number) => Promise<number>;
    } = $props();

    let from = $state('');
    let until = $state('');
    let preview = $state<number | undefined>(undefined);
    let rewriting = $state(false);
    let rewritten = $state<number | undefined>(undefined);
    let confirmRewrite = $state(false);

    const range = (): Range => ({
        from: from === '' ? undefined : `${from}T00:00:00Z`,
        until: until === '' ? undefined : `${until}T00:00:00Z`
    });

    function message(e: unknown): string {
        return e instanceof Error ? e.message : String(e);
    }

    function resetPreview() {
        preview = undefined;
        rewritten = undefined;
    }

    async function previewRewrite() {
        rewritten = undefined;
        try {
            preview = await count(range());
        } catch (e) {
            toasts.error(message(e));
        }
    }

    async function runRewrite() {
        if (preview === undefined) return;
        rewriting = true;
        try {
            rewritten = await rewrite(range(), preview);
            preview = undefined;
        } catch (e) {
            if (e instanceof ApiError && e.status === 409) {
                toasts.error(m.stations_range_changed());
                preview = undefined;
            } else {
                toasts.error(message(e));
            }
        } finally {
            rewriting = false;
        }
    }
</script>

<section
    aria-label={m.stations_rewrite_title()}
    class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
>
    <h2 class="text-[15px] font-semibold">{m.stations_rewrite_title()}</h2>
    <p class="text-[13px] text-muted">{lead}</p>
    <div class="flex flex-wrap items-end gap-3">
        <div class="flex flex-col gap-1">
            <label for="rw-from" class="text-xs text-muted">{m.stations_rewrite_from()}</label>
            <input
                id="rw-from"
                type="date"
                class="control h-10 font-mono"
                bind:value={from}
                onchange={resetPreview}
            />
        </div>
        <div class="flex flex-col gap-1">
            <label for="rw-until" class="text-xs text-muted">{m.stations_rewrite_until()}</label>
            <input
                id="rw-until"
                type="date"
                class="control h-10 font-mono"
                bind:value={until}
                onchange={resetPreview}
            />
        </div>
        <Button size="lg" disabled={changed} onclick={previewRewrite}>{m.stations_preview()}</Button
        >
        {#if preview !== undefined}
            <span class="text-[13px]"
                ><Slotted text={m.stations_would_change({ count: preview, n: SLOT })}
                    ><strong class="font-mono text-base">{formatNumber(preview)}</strong></Slotted
                ></span
            >
            <Button
                size="lg"
                variant="danger"
                disabled={preview === 0 || rewriting}
                onclick={() => (confirmRewrite = true)}
                >{rewriting
                    ? m.stations_rewriting()
                    : m.stations_rewrite_count({ count: preview })}</Button
            >
        {/if}
        {#if rewritten !== undefined}
            <span role="status" class="text-[13px] text-ok"
                >{m.stations_rewrote({ count: rewritten })}</span
            >
        {/if}
    </div>
    {#if changed}
        <span class="text-xs text-muted">{m.stations_save_first()}</span>
    {/if}
</section>
<ConfirmDialog
    bind:open={confirmRewrite}
    title={m.stations_rewrite_confirm_title()}
    description={confirmDescription(m.common_qso_count({ count: preview ?? 0 }))}
    confirm={m.stations_rewrite_confirm()}
    danger
    onconfirm={runRewrite}
/>
