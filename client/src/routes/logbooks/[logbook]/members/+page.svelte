<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import type { LogbookRole, Member } from '#lib/api/types.ts';
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsFrame from '#lib/components/SettingsFrame.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { roleLabel } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    const logbookId = page.params.logbook ?? '';
    const canEdit = $derived(session.logbook(logbookId)?.role === 'owner');

    let members = $state<Member[] | undefined>(undefined);
    let callsign = $state('');
    let role = $state<LogbookRole>('operator');
    let adding = $state(false);
    let removing = $state<Member | undefined>(undefined);
    let confirming = $state(false);

    const owners = $derived((members ?? []).filter((u) => u.role === 'owner' && !u.disabled));
    const locked = (u: Member) => u.role === 'owner' && !u.disabled && owners.length === 1;
    const note = (u: Member) =>
        [
            u.user_id === session.me?.id && m.settings_members_you(),
            locked(u) && m.settings_members_last_owner(),
            u.disabled && m.settings_members_disabled()
        ]
            .filter((s) => s)
            .join(' · ');

    onMount(() => {
        api.members(logbookId)
            .then((found) => (members = found))
            .catch((e) => toasts.error((e as Error).message));
    });

    function failed(e: unknown) {
        const status = (e as { status?: number }).status;
        toasts.error(
            status === 404
                ? m.settings_members_no_user()
                : status === 409
                  ? m.settings_members_owner_needed()
                  : (e as Error).message
        );
    }

    async function afterOwnChange(member: Member | undefined) {
        if (member?.user_id !== session.me?.id) return;
        await session.load();
        if (!session.logbook(logbookId)) goto('/');
    }

    async function add(event: Event) {
        event.preventDefault();
        const call = callsign.trim().toUpperCase();
        if (call === '') return;
        adding = true;
        try {
            members = await api.setMember(logbookId, call, role);
            callsign = '';
            toasts.show(
                role === 'owner'
                    ? m.settings_members_now_owner({ callsign: call })
                    : m.settings_members_now_operator({ callsign: call })
            );
        } catch (e) {
            failed(e);
        } finally {
            adding = false;
        }
    }

    async function setRole(member: Member, next: LogbookRole) {
        try {
            members = await api.setMember(logbookId, member.callsign, next);
            await afterOwnChange(member);
        } catch (e) {
            failed(e);
            members = [...(members ?? [])];
        }
    }

    async function remove() {
        const member = removing;
        if (member === undefined) return;
        try {
            await api.removeMember(logbookId, member.callsign);
            members = (members ?? []).filter((u) => u.user_id !== member.user_id);
            toasts.show(m.settings_members_removed({ callsign: member.callsign }));
            await afterOwnChange(member);
        } catch (e) {
            failed(e);
        }
    }
</script>

<SettingsFrame {logbookId}>
    {#if canEdit}
        <SettingsSection title={m.settings_members_add_title()} form onsubmit={add}>
            <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[160px_minmax(0,1fr)]">
                <SettingsField label={m.settings_members_callsign()} for="mem-call">
                    <input
                        id="mem-call"
                        class="control h-9 w-full font-mono uppercase md:w-60"
                        placeholder="JA1ABC"
                        autocomplete="off"
                        spellcheck="false"
                        bind:value={callsign}
                    />
                </SettingsField>
                <SettingsField label={m.settings_members_role()} for="mem-role">
                    <select id="mem-role" class="control h-9 w-full md:w-60" bind:value={role}>
                        <option value="operator">{roleLabel('operator')}</option>
                        <option value="owner">{roleLabel('owner')}</option>
                    </select>
                </SettingsField>
            </div>
            <div class="flex flex-wrap items-center justify-end gap-3">
                <span class="mr-auto text-2xs text-muted">{m.settings_members_must_exist()}</span>
                <Button type="submit" variant="primary" disabled={adding || callsign.trim() === ''}
                    >{m.settings_members_add()}</Button
                >
            </div>
        </SettingsSection>
    {:else}
        <Notice tone="info">{m.settings_members_owners_only()}</Notice>
    {/if}

    <SettingsSection title={m.settings_members()} padded={false}>
        {#each members ?? [] as member (member.user_id)}
            <div
                class="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-surface-2 px-4 py-3 last:border-b-0"
            >
                <span class={['font-mono font-semibold', member.disabled && 'text-muted']}
                    >{member.callsign}</span
                >
                <span class="min-w-0 flex-1 text-2xs text-muted">{note(member)}</span>
                {#if canEdit}
                    <label for={`role-${member.user_id}`} class="sr-only"
                        >{m.settings_members_role_of({ callsign: member.callsign })}</label
                    >
                    <select
                        id={`role-${member.user_id}`}
                        class="control h-8"
                        disabled={locked(member)}
                        value={member.role}
                        onchange={(e) => setRole(member, e.currentTarget.value as LogbookRole)}
                    >
                        <option value="owner">{roleLabel('owner')}</option>
                        <option value="operator">{roleLabel('operator')}</option>
                    </select>
                    <Button
                        size="sm"
                        variant="danger"
                        disabled={locked(member)}
                        onclick={() => {
                            removing = member;
                            confirming = true;
                        }}>{m.common_remove()}</Button
                    >
                {:else}
                    <span class="text-sm">{roleLabel(member.role)}</span>
                {/if}
            </div>
        {:else}
            <p class="px-4 py-3 text-muted">
                {members === undefined ? m.common_loading() : ''}
            </p>
        {/each}
    </SettingsSection>
</SettingsFrame>

<ConfirmDialog
    bind:open={confirming}
    title={removing
        ? m.settings_members_remove_title({ callsign: removing.callsign })
        : m.settings_members_remove_title_unnamed()}
    description={removing?.user_id === session.me?.id
        ? m.settings_members_remove_self()
        : m.settings_members_remove_other()}
    confirm={m.common_remove()}
    danger
    onconfirm={remove}
/>
