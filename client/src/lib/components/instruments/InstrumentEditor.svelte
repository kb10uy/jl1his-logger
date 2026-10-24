<script lang="ts">
    import { untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import { ApiError } from '#lib/api/client.ts';
    import type { Instrument } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { SLOT, formatNumber } from '#lib/i18n.ts';
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
    let from = $state('');
    let until = $state('');
    let preview = $state<number | undefined>(undefined);
    let rewriting = $state(false);
    let rewritten = $state<number | undefined>(undefined);
    let confirmRewrite = $state(false);

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

    const range = () => ({
        from: from === '' ? undefined : `${from}T00:00:00Z`,
        until: until === '' ? undefined : `${until}T00:00:00Z`
    });

    function resetPreview() {
        preview = undefined;
        rewritten = undefined;
    }

    async function previewRewrite() {
        if (instrument === undefined) return;
        rewritten = undefined;
        try {
            preview = (await api.previewInstrumentRewrite(logbookId, instrument.id, range())).count;
        } catch (e) {
            toasts.error(message(e));
        }
    }

    async function runRewrite() {
        if (instrument === undefined || preview === undefined) return;
        rewriting = true;
        try {
            rewritten = (await api.rewriteInstrument(logbookId, instrument.id, range(), preview))
                .count;
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
    <section
        aria-label={m.stations_rewrite_title()}
        class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
    >
        <h2 class="text-[15px] font-semibold">{m.stations_rewrite_title()}</h2>
        <p class="text-[13px] text-muted">
            {m.stations_instrument_rewrite_lead()}
        </p>
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
                <label for="rw-until" class="text-xs text-muted">{m.stations_rewrite_until()}</label
                >
                <input
                    id="rw-until"
                    type="date"
                    class="control h-10 font-mono"
                    bind:value={until}
                    onchange={resetPreview}
                />
            </div>
            <Button size="lg" disabled={changed} onclick={previewRewrite}
                >{m.stations_preview()}</Button
            >
            {#if preview !== undefined}
                <span class="text-[13px]"
                    ><Slotted text={m.stations_would_change({ count: preview, n: SLOT })}
                        ><strong class="font-mono text-base">{formatNumber(preview)}</strong
                        ></Slotted
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
        description={m.stations_instrument_rewrite_confirm_description({
            qsos: m.common_qso_count({ count: preview ?? 0 })
        })}
        confirm={m.stations_rewrite_confirm()}
        danger
        onconfirm={runRewrite}
    />
{/if}
