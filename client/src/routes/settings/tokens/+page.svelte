<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { Token, TokenKind } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import ConfirmDialog from '#lib/components/ui/ConfirmDialog.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { roleLabel, SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
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
            ? m.settings_tokens_name_problem()
            : logbookIds.length === 0
              ? m.settings_tokens_choose_logbook()
              : undefined
    );
    const logbookNames = (ids: string[]) =>
        ids
            .map((id) => session.logbook(id)?.name ?? m.settings_tokens_another_logbook())
            .join(', ');

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
            toasts.show(m.settings_tokens_copied());
        } catch {
            toasts.error(m.settings_tokens_copy_failed());
        }
    }

    async function revoke() {
        const token = revoking;
        if (token === undefined) return;
        try {
            await api.revokeToken(token.id);
            tokens = (tokens ?? []).filter((t) => t.id !== token.id);
            toasts.show(m.settings_tokens_revoked({ name: token.name }));
        } catch (e) {
            toasts.error((e as Error).message);
        }
    }
</script>

<section aria-label={m.settings_tokens()} class="flex max-w-3xl flex-col gap-4">
    <form
        class="flex flex-col gap-3 rounded-lg border border-border bg-surface p-4"
        onsubmit={issue}
    >
        <h2 class="text-[15px] font-semibold">{m.settings_tokens_issue_title()}</h2>
        <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[120px_minmax(0,1fr)]">
            <label for="tok-name" class="text-xs text-muted md:pt-2.5"
                >{m.settings_tokens_name()}</label
            >
            <input
                id="tok-name"
                class="control h-10"
                placeholder={m.settings_tokens_name_placeholder()}
                autocomplete="off"
                bind:value={name}
            />
            <span class="text-xs text-muted md:pt-0.5">{m.settings_kind()}</span>
            <div
                role="radiogroup"
                aria-label={m.settings_kind()}
                class="flex flex-wrap gap-x-5 gap-y-2"
            >
                <label class="flex cursor-pointer items-center gap-1.5 text-[13px]">
                    <input type="radio" name="tok-kind" value="native" bind:group={kind} />
                    {m.settings_tokens_native()}
                </label>
                <label class="flex cursor-pointer items-center gap-1.5 text-[13px]">
                    <input type="radio" name="tok-kind" value="wl2" bind:group={kind} />
                    {m.settings_tokens_wl2()}
                </label>
            </div>
            <span class="text-xs text-muted md:pt-0.5">{m.settings_tokens_logbooks()}</span>
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
                        <span class="text-xs text-muted">{roleLabel(logbook.role)}</span>
                    </label>
                {/each}
                {#if kind === 'wl2'}
                    <span class="text-xs text-muted">{m.settings_tokens_wl2_one_logbook()}</span>
                {/if}
            </div>
            {#if kind === 'wl2'}
                <span class="max-md:hidden"></span>
                <label class="flex cursor-pointer items-center gap-2 text-[13px]">
                    <input type="checkbox" bind:checked={prefix} />
                    <Slotted text={m.settings_tokens_wl2_prefix({ prefix: SLOT })}
                        ><code class="font-mono">wl2_</code></Slotted
                    >
                </label>
            {/if}
        </div>
        <div class="flex flex-wrap items-center justify-end gap-3">
            {#if problem && name !== ''}
                <span class="mr-auto text-xs text-muted">{problem}</span>
            {/if}
            <Button type="submit" variant="primary" disabled={problem !== undefined || issuing}
                >{issuing ? m.settings_tokens_issuing() : m.settings_tokens_issue()}</Button
            >
        </div>
        {#if plaintext}
            <Notice tone="info">
                <div class="flex flex-col gap-2">
                    <span class="font-semibold">{m.settings_tokens_shown_once()}</span>
                    <div class="flex flex-wrap items-center gap-2">
                        <code
                            class="min-w-0 flex-1 rounded bg-bg px-2 py-1.5 font-mono text-[13px] break-all text-text select-all"
                            >{plaintext}</code
                        >
                        <Button size="sm" onclick={copy}>{m.common_copy()}</Button>
                    </div>
                </div>
            </Notice>
        {/if}
    </form>

    <div class="overflow-x-auto rounded-lg border border-border bg-surface">
        <table class="w-full border-collapse text-[13px]">
            <thead>
                <tr class="text-left text-xs text-muted">
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >{m.settings_tokens_name()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >{m.settings_kind()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >{m.settings_tokens_logbooks()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >{m.settings_tokens_last_used()}</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2"
                        ><span class="sr-only">{m.settings_tokens_actions()}</span></th
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
                            >{token.last_used_at?.slice(0, 10) ?? m.settings_never()}</td
                        >
                        <td class="px-3 py-2 text-right">
                            <Button
                                size="sm"
                                variant="danger"
                                onclick={() => {
                                    revoking = token;
                                    confirming = true;
                                }}>{m.settings_tokens_revoke()}</Button
                            >
                        </td>
                    </tr>
                {:else}
                    <tr>
                        <td colspan="5" class="px-3 py-4 text-muted"
                            >{tokens === undefined
                                ? m.common_loading()
                                : m.settings_tokens_none()}</td
                        >
                    </tr>
                {/each}
            </tbody>
        </table>
    </div>
</section>

<ConfirmDialog
    bind:open={confirming}
    title={m.settings_tokens_revoke_title()}
    description={revoking
        ? m.settings_tokens_revoke_description({ name: revoking.name })
        : m.settings_tokens_revoke_description_unnamed()}
    confirm={m.settings_tokens_revoke()}
    danger
    onconfirm={revoke}
/>
