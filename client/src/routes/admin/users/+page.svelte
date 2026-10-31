<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { AdminUser } from '#lib/api/types.ts';
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import RegistrationLink from '#lib/components/admin/RegistrationLink.svelte';
    import { roleLabel } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let users = $state<AdminUser[] | undefined>(undefined);
    let callsign = $state('');
    let role = $state<AdminUser['role']>('user');
    let adding = $state(false);
    let added = $state<{ callsign: string; url: string } | undefined>(undefined);

    onMount(load);

    async function load() {
        try {
            users = await api.adminUsers();
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }

    async function add(event: Event) {
        event.preventDefault();
        if (adding || callsign.trim() === '') return;
        adding = true;
        try {
            const user = await api.createUser({ callsign: callsign.trim(), role });
            const { url } = await api.issueRegistration(user.id);
            added = { callsign: user.callsign, url };
            callsign = '';
            role = 'user';
            toasts.show(m.admin_users_added({ callsign: user.callsign }));
            await load();
        } catch (e) {
            const status = (e as { status?: number }).status;
            toasts.error(status === 409 ? m.admin_users_exists() : (e as Error).message);
        } finally {
            adding = false;
        }
    }

    const stateOf = (user: AdminUser) =>
        user.disabled_at !== null
            ? m.admin_users_disabled()
            : user.passkeys === 0
              ? m.admin_users_waiting()
              : m.admin_users_active();
</script>

<SettingsSection title={m.admin_users_add_title()} form onsubmit={add}>
    <div class="grid grid-cols-1 gap-x-3 gap-y-2 md:grid-cols-[160px_minmax(0,1fr)]">
        <SettingsField
            label={m.admin_users_callsign()}
            for="user-call"
            hint={m.admin_users_callsign_hint()}
        >
            <input
                id="user-call"
                class="control h-9 w-full font-mono uppercase md:w-60"
                placeholder="JA1ABC"
                autocomplete="off"
                spellcheck="false"
                bind:value={callsign}
            />
        </SettingsField>
        <SettingsField
            label={m.admin_users_role()}
            for="user-role"
            hint={m.admin_users_role_hint()}
        >
            <select id="user-role" class="control h-9 w-full md:w-60" bind:value={role}>
                <option value="user">{roleLabel('user')}</option>
                <option value="admin">{roleLabel('admin')}</option>
            </select>
        </SettingsField>
    </div>
    <div class="flex flex-wrap items-center justify-end gap-3">
        <Button type="submit" variant="primary" disabled={adding || callsign.trim() === ''}
            >{adding ? m.admin_users_adding() : m.admin_users_add()}</Button
        >
    </div>
    {#if added}
        <RegistrationLink callsign={added.callsign} url={added.url} />
    {/if}
</SettingsSection>

<SettingsSection title={m.admin_users_list_title()} padded={false}>
    <table class="w-full border-collapse text-sm">
        <thead>
            <tr class="text-left text-2xs text-muted">
                <th scope="col" class="border-b border-border px-3 py-1.5 font-medium"
                    >{m.admin_users_callsign()}</th
                >
                <th scope="col" class="border-b border-border px-3 py-1.5 font-medium"
                    >{m.admin_users_role()}</th
                >
                <th scope="col" class="border-b border-border px-3 py-1.5 font-medium"
                    >{m.admin_users_state()}</th
                >
                <th scope="col" class="border-b border-border px-3 py-1.5 text-right font-medium"
                    >{m.admin_users_passkeys()}</th
                >
            </tr>
        </thead>
        <tbody>
            {#each users ?? [] as user (user.id)}
                <tr class="border-b border-surface-2 last:border-b-0">
                    <td class="px-3 py-1.5">
                        <a
                            href={`/admin/users/${user.id}`}
                            class={[
                                'font-mono font-semibold',
                                user.disabled_at !== null && 'text-muted'
                            ]}>{user.callsign}</a
                        >
                        {#if user.id === session.me?.id}
                            <span class="ml-1 text-2xs text-muted">{m.admin_users_you()}</span>
                        {/if}
                    </td>
                    <td class="px-3 py-1.5">{roleLabel(user.role)}</td>
                    <td
                        class={[
                            'px-3 py-1.5',
                            user.disabled_at !== null && 'text-muted',
                            user.disabled_at === null && user.passkeys === 0 && 'text-warn'
                        ]}>{stateOf(user)}</td
                    >
                    <td class="px-3 py-1.5 text-right font-mono tabular">{user.passkeys}</td>
                </tr>
            {:else}
                <tr>
                    <td colspan="4" class="px-3 py-4 text-muted"
                        >{users === undefined ? m.common_loading() : m.admin_users_none()}</td
                    >
                </tr>
            {/each}
        </tbody>
    </table>
</SettingsSection>
