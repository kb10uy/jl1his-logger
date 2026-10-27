<script lang="ts">
    import { incoming, warns } from '#lib/incoming/incoming.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';

    const notice = $derived(incoming.notice);
    const warning = $derived(notice !== undefined && warns(notice));

    $effect(() => {
        if (notice === undefined || warning) return;
        const shown = notice;
        const timer = setTimeout(() => {
            if (incoming.notice === shown) incoming.notice = undefined;
        }, 10_000);
        return () => clearTimeout(timer);
    });
</script>

{#if notice}
    <div
        role="status"
        class={[
            'flex flex-none items-center gap-2 rounded-lg border px-2.5 py-1 text-[12px]',
            warning ? 'border-warn-border bg-warn-surface' : 'border-border bg-surface'
        ]}
    >
        <span class="min-w-0 flex-1">
            {#if notice.kind === 'waiting'}
                <strong class="font-semibold text-warn"
                    >{m.entry_incoming_waiting({ count: notice.count })}</strong
                >
            {:else}
                <span class={warning ? 'font-semibold text-warn' : 'text-muted'}
                    >{notice.program
                        ? m.entry_incoming_logged({ call: notice.call, program: notice.program })
                        : m.entry_incoming_logged_unnamed({ call: notice.call })}</span
                >
                {#each notice.mismatches as mismatch (mismatch.name)}
                    <span class="text-muted">· {m.entry_incoming_mismatch(mismatch)}</span>
                {/each}
            {/if}
        </span>
        <button
            type="button"
            aria-label={m.entry_incoming_dismiss()}
            class="flex-none text-muted"
            onclick={() => (incoming.notice = undefined)}
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
{/if}
