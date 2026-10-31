<script lang="ts">
    import { untrack } from 'svelte';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsFrame from '#lib/components/SettingsFrame.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { AREA_PRESETS, areaDraftOf, areaInput, areaProblem } from '#lib/logbook/area.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    const logbookId = page.params.logbook ?? '';
    const logbook = $derived(session.logbook(logbookId));
    const canEdit = $derived(logbook?.role === 'owner');

    let name = $state(untrack(() => session.logbook(logbookId)?.name ?? ''));
    let renaming = $state(false);
    const nameChanged = $derived(name.trim() !== (logbook?.name ?? ''));

    async function rename(event?: Event) {
        event?.preventDefault();
        if (!canEdit || logbook === undefined || !nameChanged || renaming) return;
        renaming = true;
        try {
            await api.setLogbookSettings(logbookId, {
                name,
                area_field: logbook.area_field ?? null
            });
            await session.load();
            name = session.logbook(logbookId)?.name ?? '';
            toasts.show(m.settings_logbook_renamed({ name }));
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        } finally {
            renaming = false;
        }
    }

    let draft = $state(areaDraftOf(untrack(() => session.logbook(logbookId)?.area_field ?? null)));
    let saving = $state(false);

    const input = $derived(areaInput(draft));
    const problem = $derived(areaProblem(draft));
    const changed = $derived(JSON.stringify(input) !== JSON.stringify(logbook?.area_field ?? null));

    async function save(event?: Event) {
        event?.preventDefault();
        if (!canEdit || logbook === undefined || problem !== undefined || !changed || saving)
            return;
        saving = true;
        try {
            await api.setLogbookSettings(logbookId, { name: logbook.name, area_field: input });
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
    {#if !canEdit}
        <Notice tone="info">{m.settings_area_owners_only()}</Notice>
    {/if}

    <SettingsSection title={m.settings_logbook_name()} form onsubmit={rename}>
        <fieldset
            disabled={!canEdit}
            class="grid grid-cols-1 gap-x-3 gap-y-2 md:grid-cols-[160px_minmax(0,1fr)]"
        >
            <SettingsField label={m.settings_logbook_name_label()} for="logbook-name">
                <input
                    id="logbook-name"
                    autocomplete="off"
                    class="control h-9 w-full md:w-80"
                    bind:value={name}
                />
            </SettingsField>
        </fieldset>
        {#if canEdit}
            <div class="flex flex-wrap items-center justify-end gap-3">
                <Button
                    type="submit"
                    variant="primary"
                    disabled={!nameChanged || name.trim() === '' || renaming}
                    >{renaming ? m.common_saving() : m.common_save()}</Button
                >
            </div>
        {/if}
    </SettingsSection>

    <SettingsSection title={m.settings_area_field()} form onsubmit={save}>
        <p class="text-sm text-muted">{m.settings_area_help()}</p>

        <fieldset
            disabled={!canEdit}
            class="grid grid-cols-1 gap-x-3 gap-y-2 md:grid-cols-[160px_minmax(0,1fr)]"
        >
            <SettingsField label={m.settings_area_adif_field()} for="area-name">
                <input
                    id="area-name"
                    list="area-fields"
                    placeholder="—"
                    autocomplete="off"
                    autocapitalize="characters"
                    spellcheck="false"
                    class="control h-9 w-full font-mono uppercase md:w-60"
                    bind:value={draft.name}
                />
            </SettingsField>
            <SettingsField label={m.settings_area_label()} for="area-label">
                <input
                    id="area-label"
                    placeholder={m.settings_area_label_placeholder()}
                    autocomplete="off"
                    class="control h-9 w-full md:w-60"
                    bind:value={draft.label}
                />
            </SettingsField>
            {#if canEdit}
                <span class="hidden md:block"></span>
                <div class="flex flex-wrap gap-2">
                    {#each AREA_PRESETS as preset (preset.label)}
                        <Button
                            size="sm"
                            onclick={() => (draft = { name: preset.name, label: preset.label })}
                            >{preset.label}
                            <span class="font-mono text-2xs text-muted">{preset.name}</span></Button
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
            <div class="flex flex-wrap items-center justify-end gap-3">
                {#if changed}
                    <span class="mr-auto text-2xs text-muted">{m.settings_area_keeps_values()}</span
                    >
                {/if}
                <Button
                    type="submit"
                    variant="primary"
                    disabled={!changed || problem !== undefined || saving}
                    >{saving ? m.common_saving() : m.common_save()}</Button
                >
            </div>
        {/if}
    </SettingsSection>
</SettingsFrame>
