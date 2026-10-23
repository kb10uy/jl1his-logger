<script lang="ts">
    const SHOWN = 20;

    let {
        title,
        note,
        items,
        qsoHref,
        open = false
    }: {
        title: string;
        note?: string;
        items: { record: number; text: string; ids?: string[] }[];
        qsoHref?: (id: string) => string;
        open?: boolean;
    } = $props();

    let all = $state(false);
    const shown = $derived(all ? items : items.slice(0, SHOWN));
</script>

{#if items.length > 0}
    <details {open} class="rounded-lg border border-border px-3">
        <summary class="flex h-10 cursor-pointer items-center gap-2 text-[13px]">
            {title} · {items.length.toLocaleString('en-US')}
            {#if note}<span class="text-muted">{note}</span>{/if}
        </summary>
        <ul class="flex flex-col gap-1.5 pb-3 font-mono text-[13px]">
            {#each shown as item, i (i)}
                <li class="flex flex-wrap gap-x-2">
                    <span class="text-muted">#{item.record}</span>
                    <span class="min-w-0 break-words">{item.text}</span>
                    {#if qsoHref && item.ids}
                        {#each item.ids as id, n (id)}
                            <a href={qsoHref(id)} class="font-sans">QSO {n + 1}</a>
                        {/each}
                    {/if}
                </li>
            {/each}
            {#if items.length > shown.length}
                <li>
                    <button type="button" class="font-sans text-info" onclick={() => (all = true)}
                        >Show all {items.length.toLocaleString('en-US')}</button
                    >
                </li>
            {/if}
        </ul>
    </details>
{/if}
