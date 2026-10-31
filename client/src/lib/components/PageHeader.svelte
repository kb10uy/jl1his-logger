<script lang="ts">
    import type { Snippet } from 'svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook, type LogbookState } from '#lib/state/logbook.svelte.ts';
    import { outbox } from '#lib/state/outbox.svelte.ts';

    interface Back {
        label: string;
        href?: string;
        onclick?: (e: MouseEvent) => void;
        always?: boolean;
    }

    let {
        title,
        subtitle,
        back,
        actions,
        toolbar,
        toolbarClass = ''
    }: {
        title: string;
        subtitle?: string;
        back?: Back;
        actions?: Snippet;
        toolbar?: Snippet;
        toolbarClass?: string;
    } = $props();

    const logbook: LogbookState | undefined = useLogbook();

    const note = $derived.by(() => {
        if (logbook === undefined) return '';
        const unsent = outbox.items.length;
        return [
            logbook.status === 'offline' ? m.shell_offline() : undefined,
            logbook.status === 'connecting' ? m.shell_connecting() : undefined,
            unsent > 0 ? m.common_unsent({ count: unsent }) : undefined
        ]
            .filter((s) => s !== undefined)
            .join(' · ');
    });

    const backClass = (always: boolean | undefined) => [
        '-ml-2 inline-flex size-10 shrink-0 items-center justify-center rounded-lg text-text',
        !always && 'md:hidden'
    ];
</script>

<header class="sticky top-0 z-20 shrink-0 border-b border-border bg-surface">
    <div class="flex h-14 items-center gap-2 px-4 md:gap-3 md:px-5">
        {#if back}
            {#if back.href !== undefined}
                <a
                    href={back.href}
                    onclick={back.onclick}
                    aria-label={back.label}
                    class={backClass(back.always)}><Icon name="back" size={22} /></a
                >
            {:else}
                <button
                    type="button"
                    onclick={back.onclick}
                    aria-label={back.label}
                    class={backClass(back.always)}><Icon name="back" size={22} /></button
                >
            {/if}
        {/if}
        <div class="flex min-w-0 flex-1 flex-col">
            <h1 class="truncate text-lg leading-tight font-semibold">{title}</h1>
            {#if subtitle}
                <span class="truncate text-2xs text-muted">{subtitle}</span>
            {/if}
        </div>
        {#if logbook}
            <span
                class="flex shrink-0 items-center gap-1.5 text-2xs whitespace-nowrap text-muted md:hidden"
            >
                <span
                    role="img"
                    aria-label={note || m.log_synced()}
                    title={note || m.log_synced()}
                    class={['size-2 rounded-full', note === '' ? 'bg-ok' : 'bg-warn']}
                ></span>
                {note}
            </span>
        {/if}
        {#if actions}
            <div class="flex shrink-0 items-center gap-2">
                {@render actions()}
            </div>
        {/if}
    </div>
    {#if toolbar}
        <div class={['px-4 pb-3 md:px-5', toolbarClass]}>
            {@render toolbar()}
        </div>
    {/if}
</header>
