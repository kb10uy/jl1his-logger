<script lang="ts">
    import { untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Instrument } from '#lib/api/types.ts';
    import RewritePanel from '#lib/components/RewritePanel.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import {
        FIELD_NAMES,
        draftOf,
        instrumentInput,
        instrumentProblem,
        sameInput
    } from '#lib/instrument/form.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let {
        logbookId,
        instrument,
        canRewrite,
        onsaved
    }: {
        logbookId: string;
        instrument: Instrument | undefined;
        canRewrite: boolean;
        onsaved: (instrument: Instrument) => void;
    } = $props();

    let draft = $state(draftOf(untrack(() => instrument)));
    let saving = $state(false);
    let saveError = $state<string | undefined>(undefined);

    const input = $derived(instrumentInput(draft));
    const original = $derived(instrumentInput(draftOf(instrument)));
    const changed = $derived(instrument === undefined || !sameInput(input, original));
    const problem = $derived(instrumentProblem(draft));

    function message(e: unknown): string {
        return e instanceof Error ? e.message : String(e);
    }

    async function save(event?: Event) {
        event?.preventDefault();
        if (problem !== undefined || !changed || saving) return;
        saving = true;
        saveError = undefined;
        try {
            const saved =
                instrument === undefined
                    ? await api.createInstrument(logbookId, input)
                    : await api.updateInstrument(logbookId, instrument.id, input);
            toasts.show(
                instrument === undefined
                    ? m.stations_created({ name: saved.name })
                    : m.stations_saved({ name: saved.name })
            );
            onsaved(saved);
        } catch (e) {
            saveError = message(e);
        } finally {
            saving = false;
        }
    }

    async function setArchived(archived: boolean) {
        if (instrument === undefined) return;
        try {
            const saved = await api.archiveInstrument(logbookId, instrument.id, archived);
            toasts.show(
                archived
                    ? m.stations_archived_toast({ name: saved.name })
                    : m.stations_restored({ name: saved.name })
            );
            onsaved(saved);
        } catch (e) {
            toasts.error(message(e));
        }
    }
</script>

{#snippet labelFor(id: string, text: string, adif?: string)}
    <label for={id} class="flex flex-col text-xs text-muted md:pt-2">
        <span>{text}</span>
        {#if adif}
            <span class="font-mono text-[10px] tracking-wide opacity-75">{adif}</span>
        {/if}
    </label>
{/snippet}

<form class="flex flex-col gap-4 rounded-lg border border-border bg-surface p-4" onsubmit={save}>
    <div class="flex flex-wrap items-center gap-3">
        <h2 class="min-w-0 flex-1 truncate text-base font-semibold">
            {instrument === undefined ? m.stations_instrument_new() : instrument.name}
        </h2>
        {#if instrument?.archived_at}
            <span class="text-xs text-muted"
                >{m.stations_archived_on({ date: instrument.archived_at.slice(0, 10) })}</span
            >
        {/if}
        {#if instrument}
            {#if instrument.archived_at}
                <Button size="sm" onclick={() => setArchived(false)}>{m.stations_restore()}</Button>
            {:else}
                <Button size="sm" variant="danger" onclick={() => setArchived(true)}
                    >{m.common_archive()}</Button
                >
            {/if}
        {/if}
    </div>

    <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[168px_minmax(0,1fr)]">
        {@render labelFor('in-name', m.field_name())}
        <input id="in-name" class="control h-10" autocomplete="off" bind:value={draft.name} />

        <div class="border-t border-border md:col-span-2"></div>

        {@render labelFor('in-rig', m.stations_instrument_rig(), 'MY_RIG')}
        <input id="in-rig" class="control h-10" autocomplete="off" bind:value={draft.rig} />
        {@render labelFor('in-antenna', m.stations_instrument_antenna(), 'MY_ANTENNA')}
        <input id="in-antenna" class="control h-10" autocomplete="off" bind:value={draft.antenna} />
        {@render labelFor('in-power', m.stations_instrument_power(), 'TX_PWR')}
        <input
            id="in-power"
            inputmode="decimal"
            placeholder={m.stations_placeholder_none()}
            class="control h-10 w-28 font-mono"
            bind:value={draft.power}
        />

        {#each draft.extra as field, i (i)}
            <input
                aria-label={m.stations_field_name()}
                list="in-adif-fields"
                placeholder="FIELD_NAME"
                class="control h-10 font-mono text-xs uppercase"
                autocomplete="off"
                spellcheck="false"
                bind:value={field.name}
            />
            <div class="flex min-w-0 items-center gap-2">
                <input
                    aria-label={field.name
                        ? m.stations_instrument_value_named({ name: field.name })
                        : m.stations_instrument_value_field()}
                    class="control h-10 flex-1"
                    bind:value={field.value}
                />
                <button
                    type="button"
                    aria-label={field.name
                        ? m.stations_remove_named({ name: field.name })
                        : m.stations_remove_field()}
                    class="inline-flex size-9 items-center justify-center rounded-md text-muted hover:text-text"
                    onclick={() => (draft.extra = draft.extra.filter((_, j) => j !== i))}
                    ><Icon name="close" size={16} /></button
                >
            </div>
        {/each}

        <span class="hidden md:block"></span>
        <div>
            <Button
                size="sm"
                onclick={() => (draft.extra = [...draft.extra, { name: '', value: '' }])}
                >{m.stations_instrument_add_field()}</Button
            >
            <datalist id="in-adif-fields">
                {#each FIELD_NAMES as name (name)}
                    <option value={name}></option>
                {/each}
            </datalist>
        </div>
    </div>

    {#if saveError}
        <Notice tone="error">{saveError}</Notice>
    {:else if problem && changed}
        <Notice>{problem}</Notice>
    {/if}

    <div class="flex flex-wrap items-center justify-end gap-2 border-t border-border pt-4">
        <span class="mr-auto text-xs text-muted">
            {instrument === undefined ? m.stations_instrument_new_hint() : m.stations_saving_hint()}
        </span>
        {#if instrument}
            <Button disabled={!changed || saving} onclick={() => (draft = draftOf(instrument))}
                >{m.stations_revert()}</Button
            >
        {/if}
        <Button
            type="submit"
            variant="primary"
            disabled={!changed || problem !== undefined || saving}
            >{saving
                ? m.common_saving()
                : instrument === undefined
                  ? m.stations_instrument_create()
                  : m.common_save()}</Button
        >
    </div>
</form>

{#if instrument && canRewrite}
    <RewritePanel
        lead={m.stations_instrument_rewrite_lead()}
        confirmDescription={(qsos) => m.stations_instrument_rewrite_confirm_description({ qsos })}
        {changed}
        count={async (range) =>
            (await api.previewInstrumentRewrite(logbookId, instrument.id, range)).count}
        rewrite={async (range, expected) =>
            (await api.rewriteInstrument(logbookId, instrument.id, range, expected)).count}
    />
{/if}
