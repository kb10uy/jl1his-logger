<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import type { LogbookRole, Member } from '#lib/api/types.ts';
    import SettingsFrame from '#lib/components/SettingsFrame.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
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

    const owners = $derived((members ?? []).filter((m) => m.role === 'owner' && !m.disabled));
    const locked = (m: Member) => m.role === 'owner' && !m.disabled && owners.length === 1;
    const note = (m: Member) =>
        [
            m.user_id === session.me?.id && 'you',
            locked(m) && 'last active owner',
            m.disabled && 'disabled'
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
                ? 'No active user has that callsign on this instance.'
                : status === 409
                  ? 'The Logbook needs at least one active owner.'
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
            toasts.show(`${call} is now ${role === 'owner' ? 'an owner' : 'an operator'}`);
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
            members = (members ?? []).filter((m) => m.user_id !== member.user_id);
            toasts.show(`Removed ${member.callsign}`);
            await afterOwnChange(member);
        } catch (e) {
            failed(e);
        }
    }
</script>

<SettingsFrame {logbookId}>
    <section aria-label="Members" class="flex max-w-2xl flex-col gap-4">
        {#if canEdit}
            <form
                class="flex flex-wrap items-end gap-3 rounded-lg border border-border bg-surface p-4"
                onsubmit={add}
            >
                <div class="flex flex-col gap-1">
                    <label for="mem-call" class="text-xs text-muted">Callsign</label>
                    <input
                        id="mem-call"
                        class="control h-10 w-40 font-mono uppercase"
                        placeholder="JA1ABC"
                        autocomplete="off"
                        spellcheck="false"
                        bind:value={callsign}
                    />
                </div>
                <div class="flex flex-col gap-1">
                    <label for="mem-role" class="text-xs text-muted">Role</label>
                    <select id="mem-role" class="control h-10" bind:value={role}>
                        <option value="operator">operator</option>
                        <option value="owner">owner</option>
                    </select>
                </div>
                <Button
                    type="submit"
                    variant="primary"
                    size="lg"
                    disabled={adding || callsign.trim() === ''}>Add member</Button
                >
                <span class="basis-full text-xs text-muted"
                    >The user must already exist on this instance.</span
                >
            </form>
        {:else}
            <Notice tone="info">Only owners of this Logbook change its members.</Notice>
        {/if}

        <div class="flex flex-col rounded-lg border border-border bg-surface">
            {#each members ?? [] as member (member.user_id)}
                <div
                    class="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-surface-2 px-4 py-3 last:border-b-0"
                >
                    <span class={['font-mono font-semibold', member.disabled && 'text-muted']}
                        >{member.callsign}</span
                    >
                    <span class="min-w-0 flex-1 text-xs text-muted">{note(member)}</span>
                    {#if canEdit}
                        <label for={`role-${member.user_id}`} class="sr-only"
                            >Role of {member.callsign}</label
                        >
                        <select
                            id={`role-${member.user_id}`}
                            class="control h-9"
                            disabled={locked(member)}
                            value={member.role}
                            onchange={(e) => setRole(member, e.currentTarget.value as LogbookRole)}
                        >
                            <option value="owner">owner</option>
                            <option value="operator">operator</option>
                        </select>
                        <Button
                            size="sm"
                            variant="danger"
                            disabled={locked(member)}
                            onclick={() => {
                                removing = member;
                                confirming = true;
                            }}>Remove</Button
                        >
                    {:else}
                        <span class="text-[13px]">{member.role}</span>
                    {/if}
                </div>
            {:else}
                <p class="px-4 py-3 text-muted">{members === undefined ? 'Loading…' : ''}</p>
            {/each}
        </div>
    </section>
</SettingsFrame>

<ConfirmDialog
    bind:open={confirming}
    title={`Remove ${removing?.callsign ?? 'this member'}?`}
    description={removing?.user_id === session.me?.id
        ? 'You lose access to this Logbook and its QSOs.'
        : 'They lose access to this Logbook. Their QSOs stay.'}
    confirm="Remove"
    danger
    onconfirm={remove}
/>
