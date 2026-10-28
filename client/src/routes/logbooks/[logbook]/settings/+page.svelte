<script lang="ts">
    import { untrack } from 'svelte';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import SettingsFrame from '#lib/components/SettingsFrame.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { AREA_PRESETS, areaDraftOf, areaInput, areaProblem } from '#lib/logbook/area.ts';
    import { m } from '#lib/paraglide/messages.js';
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
            toasts.show(
                input === null
                    ? m.settings_area_removed()
                    : m.settings_area_saved({ label: input.label })
            );
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        } finally {
            saving = false;
        }
    }
</script>

<SettingsFrame {logbookId}>
    <form
        aria-label={m.settings_area_field()}
        class="flex flex-col gap-4 rounded-lg border border-border bg-surface p-4"
        onsubmit={save}
    >
        <div class="flex flex-col gap-1">
            <h2 class="text-base font-semibold">{m.settings_area_field()}</h2>
            <p class="text-[13px] text-muted">
                {m.settings_area_help()}
            </p>
        </div>

        <fieldset
            disabled={!canEdit}
            class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[168px_minmax(0,1fr)]"
        >
            <label for="area-name" class="flex flex-col text-xs text-muted md:pt-2">
                <span>{m.settings_area_adif_field()}</span>
            </label>
            <input
                id="area-name"
                list="area-fields"
                placeholder="—"
                autocomplete="off"
                autocapitalize="characters"
                spellcheck="false"
                class="control h-10 w-full font-mono uppercase md:w-60"
                bind:value={draft.name}
            />
            <label for="area-label" class="flex flex-col text-xs text-muted md:pt-2">
                <span>{m.settings_area_label()}</span>
            </label>
            <input
                id="area-label"
                placeholder={m.settings_area_label_placeholder()}
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
                    <Button size="sm" onclick={() => (draft = { name: '', label: '' })}
                        >{m.common_none()}</Button
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
                    >{changed ? m.settings_area_keeps_values() : ''}</span
                >
                <Button
                    type="submit"
                    variant="primary"
                    disabled={!changed || problem !== undefined || saving}
                    >{saving ? m.common_saving() : m.common_save()}</Button
                >
            </div>
        {:else}
            <p class="text-xs text-muted">{m.settings_area_owners_only()}</p>
        {/if}
    </form>
</SettingsFrame>
