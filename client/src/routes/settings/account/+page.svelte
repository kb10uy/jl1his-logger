<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import * as api from '#lib/api/api.ts';
    import type { Passkey } from '#lib/api/types.ts';
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import { roleLabel } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let passkeys = $state<Passkey[] | undefined>(undefined);
    let name = $state('');
    let adding = $state(false);
    let removing = $state<Passkey | undefined>(undefined);
    let confirming = $state(false);

    const day = (at: string | null) => (at === null ? m.settings_never() : at.slice(0, 10));

    onMount(load);

    async function load() {
        try {
            passkeys = await api.passkeys();
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }

    async function add(event: Event) {
        event.preventDefault();
        adding = true;
        try {
            const added = await api.addPasskey(name.trim());
            passkeys = [...(passkeys ?? []), added];
            name = '';
            toasts.show(m.settings_account_added({ name: added.name }));
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            adding = false;
        }
    }

    async function remove() {
        const passkey = removing;
        if (passkey === undefined) return;
        try {
            await api.deletePasskey(passkey.id);
            passkeys = (passkeys ?? []).filter((p) => p.id !== passkey.id);
            toasts.show(m.settings_account_removed({ name: passkey.name }));
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }

    async function signOut() {
        await session.signOut();
        goto('/login');
    }
</script>

<SettingsSection title={m.settings_account_session()}>
    <div class="flex flex-wrap items-center gap-3">
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <span class="font-mono text-lg font-semibold">{session.me?.callsign}</span>
            <span class="text-xs text-muted"
                >{m.settings_account_signed_in({ role: roleLabel(session.me?.role ?? '') })}</span
            >
        </div>
        <Button onclick={signOut}>{m.shell_sign_out()}</Button>
    </div>
</SettingsSection>

<SettingsSection title={m.settings_account_add_title()} form onsubmit={add}>
    <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[160px_minmax(0,1fr)]">
        <SettingsField label={m.settings_account_passkey_name()} for="pk-name">
            <input
                id="pk-name"
                class="control h-10 w-full md:w-60"
                placeholder={m.settings_account_passkey_placeholder()}
                autocomplete="off"
                bind:value={name}
            />
        </SettingsField>
    </div>
    <div class="flex flex-wrap items-center justify-end gap-3">
        <Button type="submit" variant="primary" disabled={adding}
            >{adding ? m.settings_account_waiting() : m.settings_account_add_passkey()}</Button
        >
    </div>
</SettingsSection>

<SettingsSection title={m.settings_account_passkeys()} padded={false}>
    {#each passkeys ?? [] as passkey (passkey.id)}
        <div
            class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-surface-2 px-4 py-3 last:border-b-0"
        >
            <span class="min-w-0 flex-1 truncate font-medium">{passkey.name}</span>
            <span class="text-xs text-muted"
                >{m.settings_account_passkey_dates({
                    created: day(passkey.created_at),
                    used: day(passkey.last_used_at)
                })}</span
            >
            <Button
                size="sm"
                variant="danger"
                disabled={passkeys?.length === 1}
                title={passkeys?.length === 1 ? m.settings_account_last_passkey() : undefined}
                onclick={() => {
                    removing = passkey;
                    confirming = true;
                }}>{m.common_remove()}</Button
            >
        </div>
    {:else}
        <p class="px-4 py-3 text-muted">
            {passkeys === undefined ? m.common_loading() : m.settings_account_no_passkeys()}
        </p>
    {/each}
</SettingsSection>

<ConfirmDialog
    bind:open={confirming}
    title={m.settings_account_remove_title()}
    description={removing
        ? m.settings_account_remove_description({
              name: removing.name,
              callsign: session.me?.callsign ?? ''
          })
        : m.settings_account_remove_description_unnamed({ callsign: session.me?.callsign ?? '' })}
    confirm={m.common_remove()}
    danger
    onconfirm={remove}
/>
