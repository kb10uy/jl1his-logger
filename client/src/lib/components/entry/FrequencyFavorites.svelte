<script lang="ts">
    import { Popover } from 'bits-ui';
    import { onMount } from 'svelte';
    import type { FavoriteFrequency, FrequencyInput } from '#lib/api/types.ts';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import {
        canSaveFavorite,
        favoriteBand,
        favoriteLabel,
        freqLabel,
        isFavorite
    } from '#lib/qso/favorites.ts';
    import { favoriteFrequencies } from '#lib/state/frequencies.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let {
        tuning,
        onselect,
        iconOnly = false,
        class: className = ''
    }: {
        tuning: FrequencyInput;
        onselect: (favorite: FavoriteFrequency) => void;
        iconOnly?: boolean;
        class?: string;
    } = $props();

    let open = $state(false);
    let removed = $state<FavoriteFrequency | undefined>(undefined);

    const saved = $derived(favoriteFrequencies.items.some((f) => isFavorite(f, tuning)));
    const savable = $derived(canSaveFavorite(tuning) && !saved);

    onMount(() => {
        favoriteFrequencies.load();
    });

    async function attempt(action: () => Promise<void>) {
        try {
            await action();
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
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

<Popover.Root bind:open onOpenChange={(next) => next && favoriteFrequencies.load()}>
    <Popover.Trigger
        aria-label={iconOnly ? m.favorites_title() : undefined}
        title={iconOnly ? m.favorites_title() : undefined}
        class={[
            'inline-flex shrink-0 items-center justify-center rounded-lg border border-border bg-bg text-text data-[state=open]:border-muted data-[state=open]:bg-surface-2',
            iconOnly ? 'gap-0.5 px-2.5' : 'gap-1.5 px-3',
            className
        ]}
    >
        <Icon
            name={saved ? 'starred' : 'star'}
            size={18}
            class={saved ? 'text-warn' : 'text-muted'}
        />{#if !iconOnly}{m.favorites_title()}{/if}<Icon
            name="chevron"
            size={12}
            class="text-muted"
        />
    </Popover.Trigger>
    <Popover.Portal>
        <Popover.Content
            role="dialog"
            aria-label={m.favorites_title()}
            align="end"
            sideOffset={6}
            class="z-40 flex max-h-[min(480px,calc(100dvh-96px))] w-[min(330px,calc(100vw-32px))] flex-col gap-px overflow-y-auto rounded-lg border border-border bg-surface p-1 text-[13px] shadow-lg shadow-shadow"
        >
            <h2 class="flex h-7 items-center px-2.5 eyebrow">{m.favorites_title()}</h2>
            {#if savable}
                <button
                    type="button"
                    class="mb-1 flex h-9 items-center gap-2 rounded-md border border-dashed border-border bg-none px-2.5 text-left text-muted hover:bg-surface-2 hover:text-text md:h-8"
                    onclick={save}
                >
                    <Icon name="plus" size={14} />
                    <span
                        >{m.favorites_save()}
                        <span class="font-mono text-text">{favoriteLabel(tuning)}</span></span
                    >
                </button>
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
                        class="grid h-11 min-w-0 flex-1 grid-cols-[40px_76px_minmax(0,1fr)] items-center gap-2 bg-none px-2.5 text-left font-mono text-[15px] md:h-8 md:grid-cols-[36px_68px_minmax(0,1fr)] md:text-[13px]"
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
                        class="inline-flex size-11 shrink-0 items-center justify-center rounded-md bg-none text-muted hover:text-text md:size-8"
                        onclick={() => remove(favorite)}><Icon name="close" size={14} /></button
                    >
                </div>
            {/each}
            {#if removed}
                {@const gone = removed}
                <div role="status" class="flex h-9 items-center gap-2 px-2.5 text-xs text-muted">
                    <span class="flex-1"
                        >{m.favorites_removed()}
                        <span class="font-mono text-text">{favoriteLabel(gone)}</span></span
                    >
                    <button
                        type="button"
                        class="h-7 rounded-md bg-none px-2 font-semibold text-info hover:bg-surface-2"
                        onclick={() => undo(gone)}>{m.common_undo()}</button
                    >
                </div>
            {/if}
            {#if favoriteFrequencies.failed}
                <p class="px-2.5 py-2 text-xs text-error-text">{m.favorites_failed()}</p>
            {:else if favoriteFrequencies.items.length === 0 && !removed}
                <p class="px-2.5 py-2 text-xs text-muted">{m.favorites_empty()}</p>
            {/if}
        </Popover.Content>
    </Popover.Portal>
</Popover.Root>
