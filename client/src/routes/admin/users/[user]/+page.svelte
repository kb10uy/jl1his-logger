<script lang="ts">
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import type { AdminLogbook, AdminUser, Passkey, Session, Token } from '#lib/api/types.ts';
    import RegistrationLink from '#lib/components/admin/RegistrationLink.svelte';
    import { sectionDetail } from '#lib/components/SectionFrame.svelte';
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { roleLabel } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    type Pending =
        | { kind: 'disable' }
        | { kind: 'passkey'; passkey: Passkey }
        | { kind: 'session'; session: Session }
        | { kind: 'token'; token: Token };

    const detail = sectionDetail();
    const userId = $derived(page.params.user ?? '');

    let user = $state<AdminUser | undefined>(undefined);
    let missing = $state(false);
    let passkeys = $state<Passkey[] | undefined>(undefined);
    let sessions = $state<Session[] | undefined>(undefined);
    let tokens = $state<Token[] | undefined>(undefined);
    let logbooks = $state<AdminLogbook[]>([]);
    let busy = $state(false);
    let link = $state<string | undefined>(undefined);
    let pending = $state<Pending | undefined>(undefined);
    let confirming = $state(false);

    const day = (at: string | null) => (at === null ? m.settings_never() : at.slice(0, 10));
    const logbookNames = (ids: string[]) =>
        ids.map((id) => logbooks.find((l) => l.id === id)?.name ?? id).join(', ');

    $effect(() => {
        detail.name = user?.callsign;
        return () => (detail.name = undefined);
    });

    $effect(() => {
        load(userId);
    });

    async function load(id: string) {
        user = undefined;
        missing = false;
        link = undefined;
        passkeys = sessions = tokens = undefined;
        try {
            const [found, keys, live, issued, all] = await Promise.all([
                api.getAdminUser(id),
                api.passkeys(id),
                api.sessions(id),
                api.tokens(id),
                api.adminLogbooks()
            ]);
            if (id !== userId) return;
            user = found;
            passkeys = keys;
            sessions = live;
            tokens = issued;
            logbooks = all;
        } catch (e) {
            if ((e as { status?: number }).status === 404) missing = true;
            else toasts.error((e as Error).message);
        }
    }

    async function act(work: () => Promise<void>) {
        if (busy) return;
        busy = true;
        try {
            await work();
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            busy = false;
        }
    }

    function setRole(role: AdminUser['role']) {
        const target = user;
        if (target === undefined || role === target.role) return;
        act(async () => {
            try {
                user = await api.setUserRole(target.id, role);
            } catch (e) {
                user = { ...target };
                throw e;
            }
            toasts.show(
                m.admin_users_role_changed({ callsign: target.callsign, role: roleLabel(role) })
            );
            if (target.id === session.me?.id) await session.load();
        });
    }

    function enable() {
        const target = user;
        if (target === undefined) return;
        act(async () => {
            user = await api.setUserDisabled(target.id, false);
            toasts.show(m.admin_users_enabled({ callsign: target.callsign }));
        });
    }

    function issue() {
        const target = user;
        if (target === undefined) return;
        act(async () => {
            link = (await api.issueRegistration(target.id)).url;
        });
    }

    function ask(next: Pending) {
        pending = next;
        confirming = true;
    }

    async function confirm() {
        const target = user;
        const what = pending;
        if (target === undefined || what === undefined) return;
        await act(async () => {
            switch (what.kind) {
                case 'disable':
                    user = await api.setUserDisabled(target.id, true);
                    sessions = [];
                    tokens = [];
                    link = undefined;
                    toasts.show(m.admin_users_disabled_toast({ callsign: target.callsign }));
                    break;
                case 'passkey':
                    await api.deletePasskey(what.passkey.id);
                    passkeys = (passkeys ?? []).filter((p) => p.id !== what.passkey.id);
                    user = { ...target, passkeys: target.passkeys - 1 };
                    toasts.show(m.admin_users_passkey_removed({ name: what.passkey.name }));
                    break;
                case 'session':
                    await api.revokeSession(what.session.id);
                    sessions = (sessions ?? []).filter((s) => s.id !== what.session.id);
                    toasts.show(m.admin_users_session_revoked());
                    break;
                case 'token':
                    await api.revokeToken(what.token.id);
                    tokens = (tokens ?? []).filter((t) => t.id !== what.token.id);
                    toasts.show(m.admin_users_token_revoked({ name: what.token.name }));
                    break;
            }
        });
    }

    const dialog = $derived.by(() => {
        const callsign = user?.callsign ?? '';
        switch (pending?.kind) {
            case 'disable':
                return {
                    title: m.admin_users_disable_title({ callsign }),
                    description: m.admin_users_disable_description(),
                    confirm: m.admin_users_disable()
                };
            case 'passkey': {
                const removed = m.admin_users_remove_passkey_description({
                    callsign,
                    name: pending.passkey.name
                });
                return {
                    title: m.admin_users_remove_passkey_title(),
                    description:
                        passkeys?.length === 1
                            ? `${removed} ${m.admin_users_remove_passkey_last()}`
                            : removed,
                    confirm: m.common_remove()
                };
            }
            case 'session':
                return {
                    title: m.admin_users_revoke_session_title(),
                    description: pending.session.user_agent ?? m.admin_users_session_unknown(),
                    confirm: m.admin_users_revoke()
                };
            case 'token':
                return {
                    title: m.settings_tokens_revoke_title(),
                    description: m.settings_tokens_revoke_description({ name: pending.token.name }),
                    confirm: m.admin_users_revoke()
                };
            default:
                return { title: '', description: '', confirm: '' };
        }
    });
</script>

{#if missing}
    <Notice tone="error">{m.admin_users_not_found()}</Notice>
{:else if user === undefined}
    <p class="text-muted">{m.common_loading()}</p>
{:else}
    <SettingsSection title={m.admin_users_account()}>
        <div class="grid grid-cols-1 gap-x-3 gap-y-2 md:grid-cols-[160px_minmax(0,1fr)]">
            <SettingsField label={m.admin_users_role()} for="user-role">
                <select
                    id="user-role"
                    class="control h-9 w-full md:w-60"
                    disabled={busy}
                    value={user.role}
                    onchange={(e) => setRole(e.currentTarget.value as AdminUser['role'])}
                >
                    <option value="user">{roleLabel('user')}</option>
                    <option value="admin">{roleLabel('admin')}</option>
                </select>
            </SettingsField>
            <SettingsField label={m.admin_users_state()}>
                <div class="flex flex-wrap items-center gap-3 md:min-h-10">
                    <span class={['min-w-0 flex-1', user.disabled_at !== null && 'text-muted']}
                        >{user.disabled_at === null
                            ? user.passkeys === 0
                                ? m.admin_users_waiting()
                                : m.admin_users_active()
                            : m.admin_users_disabled_since({ date: day(user.disabled_at) })}</span
                    >
                    {#if user.disabled_at === null}
                        <Button
                            size="sm"
                            variant="danger"
                            disabled={busy}
                            onclick={() => ask({ kind: 'disable' })}
                            >{m.admin_users_disable()}</Button
                        >
                    {:else}
                        <Button size="sm" disabled={busy} onclick={enable}
                            >{m.admin_users_enable()}</Button
                        >
                    {/if}
                </div>
            </SettingsField>
        </div>
    </SettingsSection>

    <SettingsSection title={m.admin_users_registration()}>
        <p class="text-sm text-muted">{m.admin_users_registration_hint()}</p>
        <div class="flex flex-wrap items-center justify-end gap-3">
            {#if user.disabled_at !== null}
                <span class="mr-auto text-2xs text-muted"
                    >{m.admin_users_registration_disabled()}</span
                >
            {/if}
            <Button variant="primary" disabled={busy || user.disabled_at !== null} onclick={issue}
                >{m.admin_users_issue()}</Button
            >
        </div>
        {#if link}
            <RegistrationLink callsign={user.callsign} url={link} />
        {/if}
    </SettingsSection>

    <SettingsSection title={m.admin_users_passkeys()} padded={false}>
        {#each passkeys ?? [] as passkey (passkey.id)}
            <div
                class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-surface-2 px-3 py-2 last:border-b-0"
            >
                <span class="min-w-0 flex-1 truncate font-medium">{passkey.name}</span>
                <span class="text-2xs text-muted"
                    >{m.settings_account_passkey_dates({
                        created: day(passkey.created_at),
                        used: day(passkey.last_used_at)
                    })}</span
                >
                <Button
                    size="sm"
                    variant="danger"
                    disabled={busy}
                    onclick={() => ask({ kind: 'passkey', passkey })}>{m.common_remove()}</Button
                >
            </div>
        {:else}
            <p class="px-3 py-3 text-muted">
                {passkeys === undefined ? m.common_loading() : m.admin_users_no_passkeys()}
            </p>
        {/each}
    </SettingsSection>

    <SettingsSection title={m.admin_users_sessions()} padded={false}>
        {#each sessions ?? [] as live (live.id)}
            <div
                class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-surface-2 px-3 py-2 last:border-b-0"
            >
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span class="truncate text-sm"
                        >{live.user_agent ?? m.admin_users_session_unknown()}</span
                    >
                    <span class="text-2xs text-muted"
                        >{m.admin_users_session_dates({
                            created: day(live.created_at),
                            used: day(live.last_used_at)
                        })}{live.current ? ` · ${m.admin_users_session_current()}` : ''}</span
                    >
                </div>
                <Button
                    size="sm"
                    variant="danger"
                    disabled={busy || live.current}
                    onclick={() => ask({ kind: 'session', session: live })}
                    >{m.admin_users_revoke()}</Button
                >
            </div>
        {:else}
            <p class="px-3 py-3 text-muted">
                {sessions === undefined ? m.common_loading() : m.admin_users_no_sessions()}
            </p>
        {/each}
    </SettingsSection>

    <SettingsSection title={m.admin_users_tokens()} padded={false}>
        <table class="w-full border-collapse text-sm">
            <thead>
                <tr class="text-left text-2xs text-muted">
                    <th scope="col" class="border-b border-border px-3 py-1.5 font-medium"
                        >{m.settings_tokens_name()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-1.5 font-medium"
                        >{m.admin_users_token_logbooks()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-1.5 font-medium"
                        >{m.admin_users_token_last_used()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-1.5"
                        ><span class="sr-only">{m.admin_users_actions()}</span></th
                    >
                </tr>
            </thead>
            <tbody>
                {#each tokens ?? [] as token (token.id)}
                    <tr class="border-b border-surface-2 last:border-b-0">
                        <td class="px-3 py-1.5">{token.name}</td>
                        <td class="px-3 py-1.5">{logbookNames(token.logbook_ids)}</td>
                        <td class="px-3 py-1.5 font-mono text-muted">{day(token.last_used_at)}</td>
                        <td class="px-3 py-1 text-right">
                            <Button
                                size="sm"
                                variant="danger"
                                disabled={busy}
                                onclick={() => ask({ kind: 'token', token })}
                                >{m.admin_users_revoke()}</Button
                            >
                        </td>
                    </tr>
                {:else}
                    <tr>
                        <td colspan="4" class="px-3 py-4 text-muted"
                            >{tokens === undefined
                                ? m.common_loading()
                                : m.admin_users_no_tokens()}</td
                        >
                    </tr>
                {/each}
            </tbody>
        </table>
    </SettingsSection>
{/if}

<ConfirmDialog
    bind:open={confirming}
    title={dialog.title}
    description={dialog.description}
    confirm={dialog.confirm}
    danger
    onconfirm={confirm}
/>
