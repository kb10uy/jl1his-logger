<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { AdminLogbook } from '#lib/api/types.ts';
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import { formatNumber } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let logbooks = $state<AdminLogbook[] | undefined>(undefined);
    let name = $state('');
    let owner = $state(session.me?.callsign ?? '');
    let creating = $state(false);
    let removing = $state<AdminLogbook | undefined>(undefined);
    let confirming = $state(false);

    const ready = $derived(name.trim() !== '' && owner.trim() !== '');

    onMount(load);

    async function load() {
        try {
            logbooks = await api.adminLogbooks();
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }

    async function create(event: Event) {
        event.preventDefault();
        if (!ready || creating) return;
        creating = true;
        try {
            const created = await api.createLogbook({
                name: name.trim(),
                owner: owner.trim().toUpperCase()
            });
            name = '';
            toasts.show(m.admin_logbooks_created({ name: created.name }));
            await Promise.all([load(), session.load()]);
        } catch (e) {
            const status = (e as { status?: number }).status;
            toasts.error(status === 404 ? m.admin_logbooks_no_user() : (e as Error).message);
        } finally {
            creating = false;
        }
    }

    async function remove() {
        const logbook = removing;
        if (logbook === undefined) return;
        try {
            await api.deleteLogbook(logbook.id);
            toasts.show(m.admin_logbooks_deleted({ name: logbook.name }));
            await Promise.all([load(), session.load()]);
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }
</script>

<SettingsSection title={m.admin_logbooks_create_title()} form onsubmit={create}>
    <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[160px_minmax(0,1fr)]">
        <SettingsField label={m.admin_logbooks_name()} for="lb-name">
            <input
                id="lb-name"
                class="control h-10 w-full md:w-60"
                placeholder={m.admin_logbooks_name_placeholder()}
                autocomplete="off"
                bind:value={name}
            />
        </SettingsField>
        <SettingsField
            label={m.admin_logbooks_owner()}
            for="lb-owner"
            hint={m.admin_logbooks_owner_hint()}
        >
            <input
                id="lb-owner"
                class="control h-10 w-full font-mono uppercase md:w-60"
                placeholder="JA1ABC"
                autocomplete="off"
                spellcheck="false"
                bind:value={owner}
            />
        </SettingsField>
    </div>
    <div class="flex flex-wrap items-center justify-end gap-3">
        <Button type="submit" variant="primary" disabled={!ready || creating}
            >{creating ? m.admin_logbooks_creating() : m.admin_logbooks_create()}</Button
        >
    </div>
</SettingsSection>

<SettingsSection title={m.admin_logbooks()} padded={false}>
    <table class="w-full border-collapse text-[13px]">
        <thead>
            <tr class="text-left text-xs text-muted">
                <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                    >{m.admin_logbooks_name()}</th
                >
                <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                    >{m.admin_logbooks_owners()}</th
                >
                <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                    >{m.admin_logbooks_qsos()}</th
                >
                <th scope="col" class="border-b border-border px-3 py-2"
                    ><span class="sr-only">{m.admin_logbooks_actions()}</span></th
                >
            </tr>
        </thead>
        <tbody>
            {#each logbooks ?? [] as logbook (logbook.id)}
                <tr class="border-b border-surface-2 last:border-b-0">
                    <td class="px-3 py-2.5 font-semibold">{logbook.name}</td>
                    <td
                        class={[
                            'px-3 py-2.5 font-mono',
                            logbook.owners.length === 0 && 'text-muted'
                        ]}
                        >{logbook.owners.length === 0
                            ? m.admin_logbooks_no_owner()
                            : logbook.owners.join(', ')}</td
                    >
                    <td class="px-3 py-2.5 font-mono tabular">{formatNumber(logbook.last_seq)}</td>
                    <td class="px-3 py-2 text-right">
                        <Button
                            size="sm"
                            variant="danger"
                            onclick={() => {
                                removing = logbook;
                                confirming = true;
                            }}>{m.admin_logbooks_delete()}</Button
                        >
                    </td>
                </tr>
            {:else}
                <tr>
                    <td colspan="4" class="px-3 py-4 text-muted"
                        >{logbooks === undefined ? m.common_loading() : m.admin_logbooks_none()}</td
                    >
                </tr>
            {/each}
        </tbody>
    </table>
</SettingsSection>

<ConfirmDialog
    bind:open={confirming}
    title={removing
        ? m.admin_logbooks_delete_title({ name: removing.name })
        : m.admin_logbooks_delete_title_unnamed()}
    description={m.admin_logbooks_delete_description({
        count: formatNumber(removing?.last_seq ?? 0)
    })}
    confirm={m.admin_logbooks_delete()}
    challenge={removing?.name}
    danger
    onconfirm={remove}
/>
