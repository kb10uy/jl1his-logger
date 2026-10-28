<script lang="ts">
    import { onMount } from 'svelte';
    import type { FavoriteFrequency, FrequencyInput } from '#client/api/types.ts';
    import {
        canSaveFavorite,
        favoriteBand,
        favoriteLabel,
        freqLabel,
        isFavorite
    } from '#client/qso/favorites.ts';
    import { favoriteFrequencies } from '#lib/entry/frequencies.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        tuning,
        rig,
        onselect
    }: {
        tuning: FrequencyInput;
        rig: boolean;
        onselect: (favorite: FavoriteFrequency) => void;
    } = $props();

    let open = $state(false);
    let removed = $state<FavoriteFrequency | undefined>(undefined);
    let problem = $state<string | undefined>(undefined);
    let root = $state<HTMLDivElement>();

    const saved = $derived(favoriteFrequencies.items.some((f) => isFavorite(f, tuning)));
    const savable = $derived(canSaveFavorite(tuning) && !saved);

    onMount(() => {
        void favoriteFrequencies.load();
    });

    function toggle() {
        open = !open;
        if (open) void favoriteFrequencies.load();
    }

    async function attempt(action: () => Promise<void>) {
        try {
            await action();
            problem = undefined;
        } catch (e) {
            problem = e instanceof Error ? e.message : String(e);
        }
    }

    const save = () =>
        attempt(async () => {
            await favoriteFrequencies.add({
                freq: tuning.freq.trim(),
                mode: tuning.mode,
                submode: tuning.submode
            });
            removed = undefined;
        });

    const remove = (favorite: FavoriteFrequency) =>
        attempt(async () => {
            await favoriteFrequencies.remove(favorite);
            removed = favorite;
        });

    const undo = (favorite: FavoriteFrequency) =>
        attempt(async () => {
            await favoriteFrequencies.add(favorite);
            removed = undefined;
        });

    function select(favorite: FavoriteFrequency) {
        onselect({ ...favorite, freq: freqLabel(favorite.freq) });
        removed = undefined;
        open = false;
    }
</script>

<svelte:window
    onpointerdown={(e) => {
        if (open && root && !root.contains(e.target as Node)) open = false;
    }}
/>

<div
    bind:this={root}
    class="relative flex"
    role="presentation"
    onkeydown={(e) => {
        if (open && e.key === 'Escape') {
            e.stopPropagation();
            open = false;
        }
    }}
>
    <button
        type="button"
        aria-label={m.favorites_title()}
        title={m.favorites_title()}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls="frequency-favorites"
        class={[
            'inline-flex h-[32px] w-[40px] items-center justify-center gap-0.5 rounded-[6px] border',
            open ? 'border-muted bg-surface-2' : 'border-border bg-bg',
            saved ? 'text-warn' : 'text-muted'
        ]}
        onclick={toggle}
    >
        {#if saved}
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"
                ><path
                    d="M8.243 7.34l-6.38 .925l-.113 .023a1 1 0 0 0 -.44 1.684l4.622 4.499l-1.09 6.355l-.013 .11a1 1 0 0 0 1.464 .944l5.706 -3l5.693 3l.1 .046a1 1 0 0 0 1.352 -1.1l-1.091 -6.355l4.624 -4.5l.078 -.085a1 1 0 0 0 -.633 -1.62l-6.38 -.926l-2.852 -5.78a1 1 0 0 0 -1.794 0l-2.853 5.78z"
                /></svg
            >
        {:else}
            <svg
                aria-hidden="true"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path
                    d="M12 17.75l-6.172 3.245l1.179 -6.873l-5 -4.867l6.9 -1l3.086 -6.253l3.086 6.253l6.9 1l-5 4.867l1.179 6.873l-6.158 -3.245"
                /></svg
            >
        {/if}
        <svg
            aria-hidden="true"
            width="10"
            height="10"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"><path d="M6 9l6 6l6 -6" /></svg
        >
    </button>
    {#if open}
        <div
            id="frequency-favorites"
            role="dialog"
            aria-label={m.favorites_title()}
            class="absolute top-[38px] left-0 z-40 flex max-h-[420px] w-[300px] flex-col gap-px overflow-y-auto rounded-lg border border-border bg-surface p-1 text-[12px] whitespace-normal shadow-lg shadow-shadow"
        >
            <h2
                class="flex h-[24px] items-center px-2 text-[11px] font-medium tracking-[0.06em] text-muted uppercase"
            >
                {m.favorites_title()}
            </h2>
            {#if savable}
                <button
                    type="button"
                    class="mb-1 flex h-[28px] items-center gap-2 rounded-md border border-dashed border-border bg-none px-2 text-left text-muted hover:bg-surface-2 hover:text-text"
                    onclick={save}
                >
                    <svg
                        aria-hidden="true"
                        width="12"
                        height="12"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"><path d="M12 5l0 14M5 12l14 0" /></svg
                    >
                    <span
                        >{m.favorites_save()}
                        <span class="font-mono text-text">{favoriteLabel(tuning)}</span></span
                    >
                </button>
            {:else if saved}
                <p class="mb-1 flex h-[24px] items-center gap-2 px-2 text-muted">
                    <span class="font-mono text-text">{favoriteLabel(tuning)}</span>
                    {m.favorites_saved()}
                </p>
            {/if}
            {#each favoriteFrequencies.items as favorite (favorite.id)}
                {@const active = isFavorite(favorite, tuning)}
                <div
                    class={[
                        'flex items-center rounded-md hover:bg-surface-2',
                        active ? 'bg-surface-2 font-semibold text-accent' : 'text-text'
                    ]}
                >
                    <button
                        type="button"
                        aria-current={active ? 'true' : undefined}
                        class="grid h-[28px] min-w-0 flex-1 grid-cols-[34px_64px_minmax(0,1fr)] items-center gap-2 bg-none px-2 text-left font-mono text-[13px]"
                        onclick={() => select(favorite)}
                    >
                        <span class="text-[11px] font-normal text-muted"
                            >{favoriteBand(favorite)}</span
                        >
                        <span class="text-right">{freqLabel(favorite.freq)}</span>
                        <span>{favorite.submode || favorite.mode}</span>
                    </button>
                    <button
                        type="button"
                        aria-label={m.favorites_remove({ favorite: favoriteLabel(favorite) })}
                        title={m.favorites_remove({ favorite: favoriteLabel(favorite) })}
                        class="inline-flex size-[28px] shrink-0 items-center justify-center rounded-md bg-none text-muted hover:text-text"
                        onclick={() => remove(favorite)}
                        ><svg
                            aria-hidden="true"
                            width="13"
                            height="13"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"><path d="M18 6l-12 12M6 6l12 12" /></svg
                        ></button
                    >
                </div>
            {/each}
            {#if removed}
                {@const gone = removed}
                <div role="status" class="flex h-[28px] items-center gap-2 px-2 text-muted">
                    <span class="flex-1"
                        >{m.favorites_removed()}
                        <span class="font-mono text-text">{favoriteLabel(gone)}</span></span
                    >
                    <button
                        type="button"
                        class="h-[24px] rounded-md bg-none px-1.5 font-semibold text-info hover:bg-surface-2"
                        onclick={() => undo(gone)}>{m.favorites_undo()}</button
                    >
                </div>
            {/if}
            {#if problem}
                <p role="alert" class="px-2 py-1.5 text-error-text">{problem}</p>
            {/if}
            {#if favoriteFrequencies.failed}
                <p class="px-2 py-1.5 text-error-text">{m.favorites_failed()}</p>
            {:else if favoriteFrequencies.items.length === 0 && !removed}
                <p class="px-2 py-1.5 text-muted">{m.favorites_empty()}</p>
            {/if}
            {#if rig}
                <p class="mt-1 border-t border-border px-2 pt-1.5 pb-0.5 text-[11px] text-muted">
                    {m.favorites_rig()}
                </p>
            {/if}
        </div>
    {/if}
</div>
