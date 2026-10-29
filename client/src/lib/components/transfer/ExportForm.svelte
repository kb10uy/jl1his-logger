<script lang="ts">
    import type { Snippet } from 'svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import type { Matches } from '#lib/state/matches.svelte.ts';

    let {
        id,
        q = $bindable(),
        placeholder,
        matches,
        matchText,
        busy,
        label,
        busyLabel,
        onexport,
        children
    }: {
        id: string;
        q: string;
        placeholder?: string;
        matches: Matches;
        matchText: string;
        busy: boolean;
        label: string;
        busyLabel: string;
        onexport: () => void;
        children?: Snippet;
    } = $props();
</script>

<div class="grid grid-cols-[96px_minmax(0,1fr)] items-center gap-3">
    <label for={id} class="text-xs text-muted">{m.transfer_query()}</label>
    <input
        {id}
        class="control h-10 font-mono"
        {placeholder}
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
        bind:value={q}
    />
    {@render children?.()}
</div>
{#if matches.error}
    <Notice tone="error">{matches.error}</Notice>
{/if}
<div class="flex flex-wrap items-center gap-3">
    <span class="text-[13px]"
        ><Slotted text={matchText}
            ><strong class="font-mono text-base">{matches.total ?? '…'}</strong></Slotted
        ></span
    >
    <div class="flex-1"></div>
    <Button
        variant="primary"
        size="lg"
        disabled={busy || matches.error !== undefined || matches.total === 0}
        onclick={onexport}>{busy ? busyLabel : label}</Button
    >
</div>
