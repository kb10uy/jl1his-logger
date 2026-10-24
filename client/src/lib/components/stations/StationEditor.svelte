<script lang="ts">
    import { untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import { ApiError } from '#lib/api/client.ts';
    import type { Station, Subdivision } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { count } from '#lib/qso/format.ts';
    import {
        EXTRA_FIELDS,
        FIELD_NAMES,
        MAIN_FIELDS,
        draftOf,
        sameInput,
        splitParks,
        stationInput,
        stationProblem
    } from '#lib/station/form.ts';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { subdivisionsOf } from '#lib/state/reference.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let {
        logbookId,
        station,
        canEdit,
        onsaved
    }: {
        logbookId: string;
        station: Station | undefined;
        canEdit: boolean;
        onsaved: (station: Station) => void;
    } = $props();

    let draft = $state(draftOf(untrack(() => station)));
    let parkText = $state('');
    let saving = $state(false);
    let saveError = $state<string | undefined>(undefined);
    let subdivisions = $state<Subdivision[]>([]);
    let from = $state('');
    let until = $state('');
    let preview = $state<number | undefined>(undefined);
    let rewriting = $state(false);
    let rewritten = $state<number | undefined>(undefined);
    let confirmRewrite = $state(false);

    const input = $derived(stationInput(draft));
    const original = $derived(stationInput(draftOf(station)));
    const changed = $derived(station === undefined || !sameInput(input, original));
    const problem = $derived(stationProblem(draft));
    const logbook = useLogbook();
    const countyLabel = $derived(logbook.area?.name === 'CNTY' ? logbook.area.label : 'County');
    const dxcc = $derived(Number(draft.main.MY_DXCC?.trim() || 'NaN'));
    const label = (name: string) =>
        [...MAIN_FIELDS, ...EXTRA_FIELDS].find((f) => f.name === name.trim().toUpperCase())?.label;
    const addable = $derived(
        EXTRA_FIELDS.filter((f) => !draft.extra.some((e) => e.name.trim().toUpperCase() === f.name))
    );
    const stateChoices = $derived(
        subdivisions.length > 0 &&
            (draft.main.MY_STATE === '' || subdivisions.some((s) => s.code === draft.main.MY_STATE))
    );

    $effect(() => {
        const code = dxcc;
        subdivisions = [];
        if (!Number.isInteger(code)) return;
        let live = true;
        subdivisionsOf(code)
            .then((found) => {
                if (live) subdivisions = found;
            })
            .catch(() => undefined);
        return () => {
            live = false;
        };
    });

    function addParks() {
        const added = splitParks(parkText);
        draft.parks = [...draft.parks, ...added.filter((p) => !draft.parks.includes(p))];
        parkText = '';
    }

    function addField(name: string) {
        if (name === '') return;
        draft.extra = [...draft.extra, { name: name === '*' ? '' : name, value: '' }];
    }

    function message(e: unknown): string {
        return e instanceof Error ? e.message : String(e);
    }

    async function save(event?: Event) {
        event?.preventDefault();
        if (!canEdit || problem !== undefined || !changed || saving) return;
        saving = true;
        saveError = undefined;
        try {
            const saved =
                station === undefined
                    ? await api.createStation(logbookId, input)
                    : await api.updateStation(logbookId, station.id, input);
            toasts.show(station === undefined ? `Created ${saved.name}` : `Saved ${saved.name}`);
            onsaved(saved);
        } catch (e) {
            saveError =
                e instanceof ApiError && e.status === 409
                    ? `Another Station already has Wavelog ID ${input.wavelog_id}.`
                    : message(e);
        } finally {
            saving = false;
        }
    }

    async function setArchived(archived: boolean) {
        if (station === undefined) return;
        try {
            const saved = await api.archiveStation(logbookId, station.id, archived);
            toasts.show(archived ? `Archived ${saved.name}` : `Restored ${saved.name}`);
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
        if (station === undefined) return;
        rewritten = undefined;
        try {
            preview = (await api.previewRewrite(logbookId, station.id, range())).count;
        } catch (e) {
            toasts.error(message(e));
        }
    }

    async function runRewrite() {
        if (station === undefined || preview === undefined) return;
        rewriting = true;
        try {
            rewritten = (await api.rewrite(logbookId, station.id, range(), preview)).count;
            preview = undefined;
        } catch (e) {
            if (e instanceof ApiError && e.status === 409) {
                toasts.error('The QSOs in the range changed. Preview again.');
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
            {station === undefined ? 'New Station' : station.name}
        </h2>
        {#if station?.archived_at}
            <span class="text-xs text-muted">archived {station.archived_at.slice(0, 10)}</span>
        {/if}
        {#if station && canEdit}
            {#if station.archived_at}
                <Button size="sm" onclick={() => setArchived(false)}>Restore</Button>
            {:else}
                <Button size="sm" variant="danger" onclick={() => setArchived(true)}>Archive</Button
                >
            {/if}
        {/if}
    </div>

    <fieldset
        disabled={!canEdit}
        class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[168px_minmax(0,1fr)]"
    >
        {@render labelFor('st-name', 'Name')}
        <input id="st-name" class="control h-10" autocomplete="off" bind:value={draft.name} />
        {@render labelFor('st-call', 'Callsign', 'STATION_CALLSIGN')}
        <input
            id="st-call"
            class="control h-10 font-mono uppercase"
            autocomplete="off"
            spellcheck="false"
            bind:value={draft.callsign}
        />
        {@render labelFor('st-wl', 'Wavelog ID', '/wl2 station_id')}
        <input
            id="st-wl"
            inputmode="numeric"
            placeholder="none"
            class="control h-10 font-mono"
            bind:value={draft.wavelogId}
        />

        <div class="border-t border-border md:col-span-2"></div>

        {@render labelFor('st-MY_GRIDSQUARE', 'Grid locator', 'MY_GRIDSQUARE')}
        <input
            id="st-MY_GRIDSQUARE"
            class="control h-10 font-mono"
            autocomplete="off"
            spellcheck="false"
            bind:value={draft.main.MY_GRIDSQUARE}
        />
        {@render labelFor('st-MY_CITY', 'City', 'MY_CITY')}
        <input id="st-MY_CITY" class="control h-10" bind:value={draft.main.MY_CITY} />
        {@render labelFor('st-MY_STATE', 'State', 'MY_STATE')}
        {#if stateChoices}
            <select id="st-MY_STATE" class="control h-10" bind:value={draft.main.MY_STATE}>
                <option value="">—</option>
                {#each subdivisions as s (s.code)}
                    <option value={s.code}>{s.code} · {s.name}</option>
                {/each}
            </select>
        {:else}
            <input
                id="st-MY_STATE"
                class="control h-10 font-mono"
                bind:value={draft.main.MY_STATE}
            />
        {/if}
        {@render labelFor('st-MY_CNTY', countyLabel, 'MY_CNTY')}
        <input id="st-MY_CNTY" class="control h-10 font-mono" bind:value={draft.main.MY_CNTY} />
        {@render labelFor('st-park', 'POTA parks', 'MY_POTA_REF')}
        <div class="flex min-w-0 flex-wrap items-center gap-1.5">
            {#each draft.parks as park (park)}
                <span
                    class="inline-flex h-8 items-center gap-1 rounded-md border border-border bg-surface-2 pr-1 pl-2.5 font-mono text-[13px]"
                >
                    {park}
                    <button
                        type="button"
                        aria-label={`Remove ${park}`}
                        class="inline-flex size-6 items-center justify-center rounded text-muted hover:text-text"
                        onclick={() => (draft.parks = draft.parks.filter((p) => p !== park))}
                        ><Icon name="close" size={12} strokeWidth={2.5} /></button
                    >
                </span>
            {/each}
            <input
                id="st-park"
                placeholder="Add park, US-0001…"
                class="control h-8 w-48 font-mono uppercase placeholder:normal-case"
                autocomplete="off"
                spellcheck="false"
                bind:value={parkText}
                onkeydown={(e) => {
                    if (e.key === 'Enter' || e.key === ',') {
                        e.preventDefault();
                        addParks();
                    }
                }}
                onblur={addParks}
            />
        </div>
        {@render labelFor('st-MY_DXCC', 'DXCC entity', 'MY_DXCC')}
        <div class="flex min-w-0 flex-wrap items-center gap-3">
            <input
                id="st-MY_DXCC"
                inputmode="numeric"
                class="control h-10 w-28 font-mono"
                bind:value={draft.main.MY_DXCC}
            />
            <span class="text-xs text-muted">State choices follow this</span>
        </div>

        {#each draft.extra as field, i (i)}
            {#if label(field.name)}
                {@render labelFor(`st-extra-${i}`, label(field.name) ?? '', field.name)}
            {:else}
                <input
                    aria-label="Field name"
                    list="st-adif-fields"
                    placeholder="FIELD_NAME"
                    class="control h-10 font-mono text-xs uppercase"
                    autocomplete="off"
                    spellcheck="false"
                    bind:value={field.name}
                />
            {/if}
            <div class="flex min-w-0 items-center gap-2">
                <input id={`st-extra-${i}`} class="control h-10 flex-1" bind:value={field.value} />
                <button
                    type="button"
                    aria-label={`Remove ${field.name || 'field'}`}
                    class="inline-flex size-9 items-center justify-center rounded-md text-muted hover:text-text"
                    onclick={() => (draft.extra = draft.extra.filter((_, j) => j !== i))}
                    ><Icon name="close" size={16} /></button
                >
            </div>
        {/each}

        {#if canEdit}
            <span class="hidden md:block"></span>
            <div>
                <label for="st-add" class="sr-only">Add field</label>
                <select
                    id="st-add"
                    class="control h-10"
                    value=""
                    onchange={(e) => {
                        addField(e.currentTarget.value);
                        e.currentTarget.value = '';
                    }}
                >
                    <option value="">+ Add field…</option>
                    {#each addable as f (f.name)}
                        <option value={f.name}>{f.label} · {f.name}</option>
                    {/each}
                    <option value="*">Another ADIF field…</option>
                </select>
                <datalist id="st-adif-fields">
                    {#each FIELD_NAMES as name (name)}
                        <option value={name}></option>
                    {/each}
                </datalist>
            </div>
        {/if}
    </fieldset>

    {#if saveError}
        <Notice tone="error">{saveError}</Notice>
    {:else if problem && changed && canEdit}
        <Notice>{problem}</Notice>
    {/if}

    {#if canEdit}
        <div class="flex flex-wrap items-center justify-end gap-2 border-t border-border pt-4">
            <span class="mr-auto text-xs text-muted">
                {station === undefined
                    ? 'New QSOs at this Station get these values.'
                    : 'Saving changes new QSOs only.'}
            </span>
            {#if station}
                <Button disabled={!changed || saving} onclick={() => (draft = draftOf(station))}
                    >Revert</Button
                >
            {/if}
            <Button
                type="submit"
                variant="primary"
                disabled={!changed || problem !== undefined || saving}
                >{saving ? 'Saving…' : station === undefined ? 'Create Station' : 'Save'}</Button
            >
        </div>
    {:else}
        <p class="text-xs text-muted">Only owners of this Logbook edit its Stations.</p>
    {/if}
</form>

{#if station && canEdit}
    <section
        aria-label="Rewrite past QSOs"
        class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
    >
        <h2 class="text-[15px] font-semibold">Rewrite past QSOs</h2>
        <p class="text-[13px] text-muted">
            Sets STATION_CALLSIGN and every saved field above on this Station's QSOs in the range.
            Other fields stay.
        </p>
        <div class="flex flex-wrap items-end gap-3">
            <div class="flex flex-col gap-1">
                <label for="rw-from" class="text-xs text-muted">From (UTC)</label>
                <input
                    id="rw-from"
                    type="date"
                    class="control h-10 font-mono"
                    bind:value={from}
                    onchange={resetPreview}
                />
            </div>
            <div class="flex flex-col gap-1">
                <label for="rw-until" class="text-xs text-muted">Until (UTC, exclusive)</label>
                <input
                    id="rw-until"
                    type="date"
                    class="control h-10 font-mono"
                    bind:value={until}
                    onchange={resetPreview}
                />
            </div>
            <Button size="lg" disabled={changed} onclick={previewRewrite}>Preview</Button>
            {#if preview !== undefined}
                <span class="text-[13px]"
                    ><strong class="font-mono text-base">{preview}</strong> QSOs would change</span
                >
                <Button
                    size="lg"
                    variant="danger"
                    disabled={preview === 0 || rewriting}
                    onclick={() => (confirmRewrite = true)}
                    >{rewriting ? 'Rewriting…' : `Rewrite ${count(preview, 'QSO')}`}</Button
                >
            {/if}
            {#if rewritten !== undefined}
                <span role="status" class="text-[13px] text-ok"
                    >Rewrote {count(rewritten, 'QSO')}.</span
                >
            {/if}
        </div>
        {#if changed}
            <span class="text-xs text-muted">Save or revert the changes above first.</span>
        {/if}
    </section>
    <ConfirmDialog
        bind:open={confirmRewrite}
        title="Rewrite past QSOs?"
        description={`This sets the Station's current values on ${count(preview ?? 0, 'QSO')}. It cannot be undone from here.`}
        confirm="Rewrite"
        danger
        onconfirm={runRewrite}
    />
{/if}
