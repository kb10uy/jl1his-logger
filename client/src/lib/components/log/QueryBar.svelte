<script lang="ts">
    import type { Snippet } from 'svelte';
    import { DropdownMenu } from 'bits-ui';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { SYNTAX, chip, templates } from '#lib/query/chips.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { analyze, appendTerm, removeToken } from '#lib/query/index.ts';
    import { utcClock } from '#lib/qso/format.ts';

    let {
        query,
        ready,
        serverError,
        onsearch,
        compact = false,
        aside
    }: {
        query: string;
        ready: boolean;
        serverError?: string;
        onsearch: (query: string) => void;
        compact?: boolean;
        aside?: Snippet;
    } = $props();

    let text = $state('');
    let showHelp = $state(false);
    let input = $state<HTMLInputElement>();
    let timer: ReturnType<typeof setTimeout> | undefined;

    $effect.pre(() => {
        if (document.activeElement !== input) text = query;
    });

    const tokens = $derived(ready ? analyze(text) : []);
    const invalid = $derived(
        tokens.some((t) => t.error !== null && t.error.code !== 'missing_value')
    );
    const chips = $derived(tokens.map((t) => ({ token: t, chip: chip(t) })));

    function schedule(next: string, delay = 350) {
        clearTimeout(timer);
        timer = setTimeout(() => {
            if (!analyze(next).some((t) => t.error !== null)) onsearch(next.trim());
        }, delay);
    }

    function set(next: string, delay = 0) {
        text = next;
        schedule(next, delay);
    }

    function add(term: string) {
        text = appendTerm(text, term);
        input?.focus();
        requestAnimationFrame(() => input?.setSelectionRange(text.length, text.length));
        schedule(text);
    }

    function onkeydown(event: KeyboardEvent) {
        if (
            event.key === '/' &&
            !(event.target instanceof HTMLElement && isEditable(event.target))
        ) {
            event.preventDefault();
            input?.focus();
        }
    }

    function isEditable(element: HTMLElement): boolean {
        return (
            element.isContentEditable ||
            element instanceof HTMLInputElement ||
            element instanceof HTMLTextAreaElement ||
            element instanceof HTMLSelectElement
        );
    }

    const toneClass = {
        normal: 'border border-border bg-surface',
        negated: 'border border-error-border bg-error-bg',
        error: 'border border-error bg-error-bg',
        pending: 'border border-dashed border-muted bg-transparent'
    };
</script>

<svelte:window {onkeydown} />

<div class="flex flex-col gap-2.5">
    <div class="flex flex-wrap items-center gap-2">
        <label for="log-query" class="sr-only">{m.log_query()}</label>
        <div
            class={[
                'flex min-w-0 flex-1 items-center gap-2 rounded-md border px-2.5',
                compact ? 'h-10 rounded-lg bg-bg' : 'h-9 bg-surface',
                invalid || serverError ? 'border-error' : 'border-border'
            ]}
        >
            <Icon name="search" size={compact ? 18 : 16} class="text-muted" />
            <input
                id="log-query"
                bind:this={input}
                type="search"
                value={text}
                oninput={(e) => set(e.currentTarget.value, 350)}
                onkeydown={(e) => {
                    if (e.key === 'Enter') set(text);
                }}
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                placeholder="call:JH1* band:40m,20m -mode:FT8 date:2026-10-01.. sent:R,Q"
                class={[
                    'min-w-0 flex-1 border-0 bg-transparent font-mono outline-none',
                    compact ? 'text-base' : 'text-xs'
                ]}
            />
            {#if !compact}
                <span class="rounded border border-border px-1.5 font-mono text-2xs text-muted"
                    >/</span
                >
            {/if}
        </div>
        {#if !compact}
            <button
                type="button"
                aria-pressed={showHelp}
                class={[
                    'h-9 rounded-md border border-border px-3',
                    showHelp ? 'bg-surface-2' : 'bg-surface'
                ]}
                onclick={() => (showHelp = !showHelp)}>{m.log_syntax()}</button
            >
        {/if}
    </div>

    {#if showHelp}
        <div
            class="grid grid-cols-1 gap-x-6 gap-y-2 rounded-lg border border-border bg-surface px-4 py-3 text-2xs sm:grid-cols-2 lg:grid-cols-3"
        >
            {#each SYNTAX as [example, meaning] (example)}
                <div class="flex gap-2.5">
                    <code class="min-w-[150px] font-mono text-text">{example}</code>
                    <span class="text-muted">{meaning()}</span>
                </div>
            {/each}
        </div>
    {/if}

    <div
        class={[
            'flex items-center gap-1.5 text-sm',
            compact ? 'overflow-x-auto pb-0.5' : 'min-h-8 flex-wrap'
        ]}
    >
        {#each chips as { token, chip: c } (`${token.start}:${token.text}`)}
            <span
                class={[
                    'inline-flex shrink-0 items-center gap-1.5 rounded-md pr-1 pl-2.5',
                    compact ? 'h-8' : 'h-8',
                    toneClass[c.tone]
                ]}
                title={token.error?.message}
            >
                <span class={c.tone === 'error' ? 'font-mono text-error-text' : 'text-muted'}
                    >{c.label}</span
                >
                <span
                    class={c.tone === 'negated' || c.tone === 'error'
                        ? 'font-medium text-error-text'
                        : 'text-muted'}>{c.op}</span
                >
                {#if c.value}
                    <span class="font-mono font-medium">{c.value}</span>
                {/if}
                <button
                    type="button"
                    aria-label={m.log_query_remove({ term: token.text })}
                    class="inline-flex size-6 items-center justify-center rounded text-muted hover:text-text"
                    onclick={() => set(removeToken(text, token))}
                >
                    <Icon name="close" size={12} strokeWidth={2.5} />
                </button>
            </span>
        {/each}
        <DropdownMenu.Root>
            <DropdownMenu.Trigger
                class={[
                    'shrink-0 rounded-md border border-dashed border-border px-2.5 text-muted',
                    compact ? 'h-8' : 'h-8'
                ]}>{m.log_add_condition()}</DropdownMenu.Trigger
            >
            <DropdownMenu.Portal>
                <DropdownMenu.Content
                    align="start"
                    sideOffset={4}
                    class="z-40 flex min-w-[220px] flex-col rounded-lg border border-border bg-surface p-1 shadow-lg shadow-shadow"
                >
                    {#each templates(utcClock(new Date()).date) as t (t.term)}
                        <DropdownMenu.Item
                            onSelect={() => add(t.term)}
                            class="flex h-8 cursor-pointer items-center justify-between gap-3 rounded-md px-2.5 text-sm data-highlighted:bg-surface-2"
                        >
                            <span>{t.label}</span>
                            <span class="font-mono text-2xs text-muted">{t.term}</span>
                        </DropdownMenu.Item>
                    {/each}
                </DropdownMenu.Content>
            </DropdownMenu.Portal>
        </DropdownMenu.Root>
        {#if chips.length > 0 && !compact}
            <button type="button" class="h-8 px-2 text-info" onclick={() => set('')}
                >{m.log_clear_all()}</button
            >
        {/if}
        {#if aside && !compact}
            <div class="flex-1"></div>
            {@render aside()}
        {/if}
    </div>
    {#if serverError}
        <p role="alert" class="text-2xs text-error-text">{serverError}</p>
    {/if}
</div>
