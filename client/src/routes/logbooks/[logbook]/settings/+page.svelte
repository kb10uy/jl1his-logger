<script lang="ts">
    import { untrack } from 'svelte';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import SettingsFrame from '#lib/components/SettingsFrame.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { AREA_PRESETS, areaDraftOf, areaInput, areaProblem } from '#lib/logbook/area.ts';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    const logbookId = page.params.logbook ?? '';
    const logbook = $derived(session.logbook(logbookId));
    const canEdit = $derived(logbook?.role === 'owner');

    let draft = $state(areaDraftOf(untrack(() => session.logbook(logbookId)?.area_field ?? null)));
    let saving = $state(false);

    const input = $derived(areaInput(draft));
    const problem = $derived(areaProblem(draft));
    const changed = $derived(JSON.stringify(input) !== JSON.stringify(logbook?.area_field ?? null));

    async function save(event?: Event) {
        event?.preventDefault();
        if (!canEdit || problem !== undefined || !changed || saving) return;
        saving = true;
        try {
            await api.setLogbookSettings(logbookId, { area_field: input });
            await session.load();
            draft = areaDraftOf(session.logbook(logbookId)?.area_field ?? null);
            toasts.show(input === null ? 'Removed the area code field' : `Saved ${input.label}`);
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        } finally {
            saving = false;
        }
    }
</script>

<SettingsFrame {logbookId}>
    <form
        aria-label="Area code field"
        class="flex max-w-2xl flex-col gap-4 rounded-lg border border-border bg-surface p-4"
        onsubmit={save}
    >
        <div class="flex flex-col gap-1">
            <h2 class="text-base font-semibold">Area code field</h2>
            <p class="text-[13px] text-muted">
                The regional code of the other station that Live entry, voice entry and the QSO
                editor take next to the name and QTH, such as JCC / JCG in Japan or the county in
                the US. Leave it empty to show no such field.
            </p>
        </div>

        <fieldset
            disabled={!canEdit}
            class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[168px_minmax(0,1fr)]"
        >
            <label for="area-name" class="flex flex-col text-xs text-muted md:pt-2">
                <span>ADIF field</span>
            </label>
            <input
                id="area-name"
                list="area-fields"
                placeholder="none"
                autocomplete="off"
                autocapitalize="characters"
                spellcheck="false"
                class="control h-10 w-full font-mono uppercase md:w-60"
                bind:value={draft.name}
            />
            <label for="area-label" class="flex flex-col text-xs text-muted md:pt-2">
                <span>Label</span>
            </label>
            <input
                id="area-label"
                placeholder="JCC / JCG"
                autocomplete="off"
                class="control h-10 w-full md:w-60"
                bind:value={draft.label}
            />
            {#if canEdit}
                <span class="hidden md:block"></span>
                <div class="flex flex-wrap gap-2">
                    {#each AREA_PRESETS as preset (preset.label)}
                        <Button
                            size="sm"
                            onclick={() => (draft = { name: preset.name, label: preset.label })}
                            >{preset.label}
                            <span class="font-mono text-xs text-muted">{preset.name}</span></Button
                        >
                    {/each}
                    <Button size="sm" onclick={() => (draft = { name: '', label: '' })}>None</Button
                    >
                </div>
            {/if}
        </fieldset>
        <datalist id="area-fields">
            {#each AREA_PRESETS as preset (preset.name + preset.label)}
                <option value={preset.name}></option>
            {/each}
        </datalist>

        {#if problem && changed && canEdit}
            <Notice>{problem}</Notice>
        {/if}

        {#if canEdit}
            <div class="flex flex-wrap items-center justify-end gap-2 border-t border-border pt-4">
                <span class="mr-auto text-xs text-muted"
                    >Changing the field does not move values already recorded.</span
                >
                <Button
                    type="submit"
                    variant="primary"
                    disabled={!changed || problem !== undefined || saving}
                    >{saving ? 'Saving…' : 'Save'}</Button
                >
            </div>
        {:else}
            <p class="text-xs text-muted">Only owners of this Logbook change its settings.</p>
        {/if}
    </form>
</SettingsFrame>
