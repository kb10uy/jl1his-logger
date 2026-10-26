<script lang="ts">
    import { untrack } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import { ApiError } from '#lib/api/client.ts';
    import type { Station, Subdivision } from '#lib/api/types.ts';
    import FavoriteButton from '#lib/components/FavoriteButton.svelte';
    import RewritePanel from '#lib/components/RewritePanel.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { m } from '#lib/paraglide/messages.js';
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

    const input = $derived(stationInput(draft));
    const original = $derived(stationInput(draftOf(station)));
    const changed = $derived(station === undefined || !sameInput(input, original));
    const problem = $derived(stationProblem(draft));
    const logbook = useLogbook();
    const countyLabel = $derived(
        logbook.area?.name === 'CNTY' ? logbook.area.label : m.stations_field_county()
    );
    const dxcc = $derived(Number(draft.main.MY_DXCC?.trim() || 'NaN'));
    const label = (name: string) =>
        [...MAIN_FIELDS, ...EXTRA_FIELDS]
            .find((f) => f.name === name.trim().toUpperCase())
            ?.label();
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
            toasts.show(
                station === undefined
                    ? m.stations_created({ name: saved.name })
                    : m.stations_saved({ name: saved.name })
            );
            onsaved(saved);
        } catch (e) {
            saveError =
                e instanceof ApiError && e.status === 409
                    ? m.stations_wavelog_id_taken({ id: String(input.wavelog_id) })
                    : message(e);
        } finally {
            saving = false;
        }
    }

    async function setFavorite(favorite: boolean) {
        if (station === undefined) return;
        try {
            onsaved(await api.favoriteStation(logbookId, station.id, favorite));
        } catch (e) {
            toasts.error(message(e));
        }
    }

    async function setArchived(archived: boolean) {
        if (station === undefined) return;
        try {
            const saved = await api.archiveStation(logbookId, station.id, archived);
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
            {station === undefined ? m.stations_new() : station.name}
        </h2>
        {#if station?.archived_at}
            <span class="text-xs text-muted"
                >{m.stations_archived_on({ date: station.archived_at.slice(0, 10) })}</span
            >
        {/if}
        {#if station}
            <FavoriteButton favorite={station.favorite} onchange={setFavorite} />
        {/if}
        {#if station && canEdit}
            {#if station.archived_at}
                <Button size="sm" onclick={() => setArchived(false)}>{m.stations_restore()}</Button>
            {:else}
                <Button size="sm" variant="danger" onclick={() => setArchived(true)}
                    >{m.common_archive()}</Button
                >
            {/if}
        {/if}
    </div>

    <fieldset
        disabled={!canEdit}
        class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[168px_minmax(0,1fr)]"
    >
        {@render labelFor('st-name', m.field_name())}
        <input id="st-name" class="control h-10" autocomplete="off" bind:value={draft.name} />
        {@render labelFor('st-call', m.stations_callsign(), 'STATION_CALLSIGN')}
        <input
            id="st-call"
            class="control h-10 font-mono uppercase"
            autocomplete="off"
            spellcheck="false"
            bind:value={draft.callsign}
        />
        {@render labelFor('st-wl', m.stations_wavelog_id(), '/wl2 station_id')}
        <input
            id="st-wl"
            inputmode="numeric"
            placeholder={m.stations_placeholder_none()}
            class="control h-10 font-mono"
            bind:value={draft.wavelogId}
        />

        <div class="border-t border-border md:col-span-2"></div>

        {@render labelFor('st-MY_GRIDSQUARE', m.stations_field_grid(), 'MY_GRIDSQUARE')}
        <input
            id="st-MY_GRIDSQUARE"
            class="control h-10 font-mono"
            autocomplete="off"
            spellcheck="false"
            bind:value={draft.main.MY_GRIDSQUARE}
        />
        {@render labelFor('st-MY_CITY', m.stations_field_city(), 'MY_CITY')}
        <input id="st-MY_CITY" class="control h-10" bind:value={draft.main.MY_CITY} />
        {@render labelFor('st-MY_STATE', m.stations_field_state(), 'MY_STATE')}
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
        {@render labelFor('st-park', m.stations_pota_parks(), 'MY_POTA_REF')}
        <div class="flex min-w-0 flex-wrap items-center gap-1.5">
            {#each draft.parks as park (park)}
                <span
                    class="inline-flex h-8 items-center gap-1 rounded-md border border-border bg-surface-2 pr-1 pl-2.5 font-mono text-[13px]"
                >
                    {park}
                    <button
                        type="button"
                        aria-label={m.stations_remove_named({ name: park })}
                        class="inline-flex size-6 items-center justify-center rounded text-muted hover:text-text"
                        onclick={() => (draft.parks = draft.parks.filter((p) => p !== park))}
                        ><Icon name="close" size={12} strokeWidth={2.5} /></button
                    >
                </span>
            {/each}
            <input
                id="st-park"
                placeholder={m.stations_park_placeholder()}
                class="control h-8 w-52 font-mono uppercase placeholder:normal-case"
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
        {@render labelFor('st-MY_DXCC', m.stations_field_dxcc(), 'MY_DXCC')}
        <div class="flex min-w-0 flex-wrap items-center gap-3">
            <input
                id="st-MY_DXCC"
                inputmode="numeric"
                class="control h-10 w-28 font-mono"
                bind:value={draft.main.MY_DXCC}
            />
            <span class="text-xs text-muted">{m.stations_state_follows()}</span>
        </div>

        {#each draft.extra as field, i (i)}
            {#if label(field.name)}
                {@render labelFor(`st-extra-${i}`, label(field.name) ?? '', field.name)}
            {:else}
                <input
                    aria-label={m.stations_field_name()}
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
                    aria-label={field.name
                        ? m.stations_remove_named({ name: field.name })
                        : m.stations_remove_field()}
                    class="inline-flex size-9 items-center justify-center rounded-md text-muted hover:text-text"
                    onclick={() => (draft.extra = draft.extra.filter((_, j) => j !== i))}
                    ><Icon name="close" size={16} /></button
                >
            </div>
        {/each}

        {#if canEdit}
            <span class="hidden md:block"></span>
            <div>
                <label for="st-add" class="sr-only">{m.stations_add_field()}</label>
                <select
                    id="st-add"
                    class="control h-10 w-full"
                    value=""
                    onchange={(e) => {
                        addField(e.currentTarget.value);
                        e.currentTarget.value = '';
                    }}
                >
                    <option value="">{m.stations_add_field_choice()}</option>
                    {#each addable as f (f.name)}
                        <option value={f.name}>{f.label()} · {f.name}</option>
                    {/each}
                    <option value="*">{m.stations_another_field()}</option>
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
                {station === undefined ? m.stations_new_hint() : m.stations_saving_hint()}
            </span>
            {#if station}
                <Button disabled={!changed || saving} onclick={() => (draft = draftOf(station))}
                    >{m.stations_revert()}</Button
                >
            {/if}
            <Button
                type="submit"
                variant="primary"
                disabled={!changed || problem !== undefined || saving}
                >{saving
                    ? m.common_saving()
                    : station === undefined
                      ? m.stations_create()
                      : m.common_save()}</Button
            >
        </div>
    {:else}
        <p class="text-xs text-muted">{m.stations_owners_edit()}</p>
    {/if}
</form>

{#if station && canEdit}
    <RewritePanel
        lead={m.stations_rewrite_lead()}
        confirmDescription={(qsos) => m.stations_rewrite_confirm_description({ qsos })}
        {changed}
        count={async (range) => (await api.previewRewrite(logbookId, station.id, range)).count}
        rewrite={async (range, expected) =>
            (await api.rewrite(logbookId, station.id, range, expected)).count}
    />
{/if}
