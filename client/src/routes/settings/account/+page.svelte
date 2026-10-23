<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import * as api from '#lib/api/api.ts';
    import type { Passkey } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import { session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let passkeys = $state<Passkey[] | undefined>(undefined);
    let name = $state('');
    let adding = $state(false);
    let removing = $state<Passkey | undefined>(undefined);
    let confirming = $state(false);

    const day = (at: string | null) => (at === null ? 'never' : at.slice(0, 10));

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
            toasts.show(`Added ${added.name}`);
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
            toasts.show(`Removed ${passkey.name}`);
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }

    async function signOut() {
        await session.signOut();
        goto('/login');
    }
</script>

<section aria-label="Account" class="flex max-w-2xl flex-col gap-4">
    <div
        class="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface px-4 py-3.5"
    >
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <span class="font-mono text-lg font-semibold">{session.me?.callsign}</span>
            <span class="text-xs text-muted">{session.me?.role} · signed in with a passkey</span>
        </div>
        <Button onclick={signOut}>Sign out</Button>
    </div>

    <form class="flex flex-wrap items-end gap-3" onsubmit={add}>
        <h2 class="mr-auto self-center text-[15px] font-semibold">Passkeys</h2>
        <div class="flex flex-col gap-1">
            <label for="pk-name" class="text-xs text-muted">Name of the new passkey</label>
            <input
                id="pk-name"
                class="control w-52"
                placeholder="This device"
                autocomplete="off"
                bind:value={name}
            />
        </div>
        <Button type="submit" variant="primary" disabled={adding}
            >{adding ? 'Waiting for the device…' : 'Add passkey'}</Button
        >
    </form>
    <div class="flex flex-col rounded-lg border border-border bg-surface">
        {#each passkeys ?? [] as passkey (passkey.id)}
            <div
                class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-surface-2 px-4 py-3 last:border-b-0"
            >
                <span class="min-w-0 flex-1 truncate font-medium">{passkey.name}</span>
                <span class="text-xs text-muted"
                    >added {day(passkey.created_at)} · used {day(passkey.last_used_at)}</span
                >
                <Button
                    size="sm"
                    variant="danger"
                    disabled={passkeys?.length === 1}
                    title={passkeys?.length === 1
                        ? 'Your last passkey cannot be removed'
                        : undefined}
                    onclick={() => {
                        removing = passkey;
                        confirming = true;
                    }}>Remove</Button
                >
            </div>
        {:else}
            <p class="px-4 py-3 text-muted">
                {passkeys === undefined ? 'Loading…' : 'No passkeys.'}
            </p>
        {/each}
    </div>
</section>

<ConfirmDialog
    bind:open={confirming}
    title="Remove this passkey?"
    description={`${removing?.name ?? 'It'} can no longer sign in as ${session.me?.callsign}.`}
    confirm="Remove"
    danger
    onconfirm={remove}
/>
