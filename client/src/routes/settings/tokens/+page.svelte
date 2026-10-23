<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Token, TokenKind } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { lastLogbook, session } from '#lib/state/session.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let tokens = $state<Token[] | undefined>(undefined);
    let name = $state('');
    let kind = $state<TokenKind>('native');
    let chosen = $state<string[]>(
        session.logbook(lastLogbook() ?? '') ? [lastLogbook() ?? ''] : []
    );
    let prefix = $state(true);
    let issuing = $state(false);
    let plaintext = $state<string | undefined>(undefined);
    let revoking = $state<Token | undefined>(undefined);
    let confirming = $state(false);

    const logbookIds = $derived(
        kind === 'wl2' ? chosen.slice(0, 1) : chosen.filter((id) => session.logbook(id))
    );
    const problem = $derived(
        name.trim() === ''
            ? 'Name the token after what will use it.'
            : logbookIds.length === 0
              ? 'Choose a Logbook.'
              : undefined
    );
    const logbookNames = (ids: string[]) =>
        ids.map((id) => session.logbook(id)?.name ?? 'another Logbook').join(', ');

    onMount(() => {
        api.tokens()
            .then((found) => (tokens = found))
            .catch((e) => toasts.error((e as Error).message));
    });

    function toggle(id: string, on: boolean) {
        if (kind === 'wl2') chosen = on ? [id] : [];
        else chosen = on ? [...chosen, id] : chosen.filter((c) => c !== id);
    }

    async function issue(event: Event) {
        event.preventDefault();
        if (problem !== undefined) return;
        issuing = true;
        try {
            const issued = await api.issueToken({
                kind,
                name: name.trim(),
                logbook_ids: logbookIds,
                wl2_prefix: kind === 'wl2' ? prefix : undefined
            });
            tokens = [issued.token, ...(tokens ?? [])];
            plaintext = issued.plaintext;
            name = '';
        } catch (e) {
            toasts.error((e as Error).message);
        } finally {
            issuing = false;
        }
    }

    async function copy() {
        try {
            await navigator.clipboard.writeText(plaintext ?? '');
            toasts.show('Copied the token');
        } catch {
            toasts.error('The token could not be copied. Select it and copy it by hand.');
        }
    }

    async function revoke() {
        const token = revoking;
        if (token === undefined) return;
        try {
            await api.revokeToken(token.id);
            tokens = (tokens ?? []).filter((t) => t.id !== token.id);
            toasts.show(`Revoked ${token.name}`);
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }
</script>

<section aria-label="API tokens" class="flex max-w-3xl flex-col gap-4">
    <form
        class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
        onsubmit={issue}
    >
        <h2 class="text-[15px] font-semibold">Issue a token</h2>
        <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[120px_minmax(0,1fr)]">
            <label for="tok-name" class="text-xs text-muted md:pt-2.5">Name</label>
            <input
                id="tok-name"
                class="control h-10"
                placeholder="Logger"
                autocomplete="off"
                bind:value={name}
            />
            <span class="text-xs text-muted md:pt-0.5">Kind</span>
            <div role="radiogroup" aria-label="Kind" class="flex flex-wrap gap-x-5 gap-y-2">
                <label class="flex cursor-pointer items-center gap-1.5 text-[13px]">
                    <input type="radio" name="tok-kind" value="native" bind:group={kind} />
                    Native API
                </label>
                <label class="flex cursor-pointer items-center gap-1.5 text-[13px]">
                    <input type="radio" name="tok-kind" value="wl2" bind:group={kind} />
                    Wavelog compatible (/wl2)
                </label>
            </div>
            <span class="text-xs text-muted md:pt-0.5">Logbooks</span>
            <div class="flex flex-col gap-2">
                {#each session.logbooks as logbook (logbook.id)}
                    <label class="flex cursor-pointer items-center gap-2 text-[13px]">
                        <input
                            type={kind === 'wl2' ? 'radio' : 'checkbox'}
                            name="tok-logbook"
                            checked={logbookIds.includes(logbook.id)}
                            onchange={(e) => toggle(logbook.id, e.currentTarget.checked)}
                        />
                        <span class="font-mono font-semibold">{logbook.name}</span>
                        <span class="text-xs text-muted">{logbook.role}</span>
                    </label>
                {/each}
                {#if kind === 'wl2'}
                    <span class="text-xs text-muted"
                        >A Wavelog compatible token reaches exactly one Logbook.</span
                    >
                {/if}
            </div>
            {#if kind === 'wl2'}
                <span class="max-md:hidden"></span>
                <label class="flex cursor-pointer items-center gap-2 text-[13px]">
                    <input type="checkbox" bind:checked={prefix} />
                    Start with <code class="font-mono">wl2_</code>
                </label>
            {/if}
        </div>
        <div class="flex flex-wrap items-center justify-end gap-3">
            {#if problem && name !== ''}
                <span class="mr-auto text-xs text-muted">{problem}</span>
            {/if}
            <Button type="submit" variant="primary" disabled={problem !== undefined || issuing}
                >{issuing ? 'Issuing…' : 'Issue token'}</Button
            >
        </div>
        {#if plaintext}
            <Notice tone="info">
                <div class="flex flex-col gap-2">
                    <span class="font-semibold">Copy it now. It is not shown again.</span>
                    <div class="flex flex-wrap items-center gap-2">
                        <code
                            class="min-w-0 flex-1 rounded bg-bg px-2 py-1.5 font-mono text-[13px] break-all text-text select-all"
                            >{plaintext}</code
                        >
                        <Button size="sm" onclick={copy}>Copy</Button>
                    </div>
                </div>
            </Notice>
        {/if}
    </form>

    <div class="overflow-x-auto rounded-lg border border-border bg-surface">
        <table class="w-full border-collapse text-[13px]">
            <thead>
                <tr class="text-left text-xs text-muted">
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium">Name</th>
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium">Kind</th>
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >Logbooks</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >Last used</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2"
                        ><span class="sr-only">Actions</span></th
                    >
                </tr>
            </thead>
            <tbody>
                {#each tokens ?? [] as token (token.id)}
                    <tr class="border-b border-surface-2 last:border-b-0">
                        <td class="px-3 py-2.5">{token.name}</td>
                        <td class="px-3 py-2.5 font-mono">{token.kind}</td>
                        <td class="px-3 py-2.5 font-mono">{logbookNames(token.logbook_ids)}</td>
                        <td class="px-3 py-2.5 font-mono text-muted"
                            >{token.last_used_at?.slice(0, 10) ?? 'never'}</td
                        >
                        <td class="px-3 py-2 text-right">
                            <Button
                                size="sm"
                                variant="danger"
                                onclick={() => {
                                    revoking = token;
                                    confirming = true;
                                }}>Revoke</Button
                            >
                        </td>
                    </tr>
                {:else}
                    <tr>
                        <td colspan="5" class="px-3 py-4 text-muted"
                            >{tokens === undefined ? 'Loading…' : 'No tokens yet.'}</td
                        >
                    </tr>
                {/each}
            </tbody>
        </table>
    </div>
</section>

<ConfirmDialog
    bind:open={confirming}
    title="Revoke this token?"
    description={`Whatever uses ${revoking?.name ?? 'it'} stops reaching the server at once.`}
    confirm="Revoke"
    danger
    onconfirm={revoke}
/>
