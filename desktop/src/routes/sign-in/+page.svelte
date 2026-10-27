<script lang="ts">
    import { untrack } from 'svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { asCommandError, errorText, refusesToken } from '#lib/session/errors.ts';
    import { session } from '#lib/session/session.svelte.ts';
    import type { CommandError } from '#lib/session/types.ts';

    const saved = untrack(() =>
        session.current?.state === 'signedOut' ? session.current : undefined
    );

    let url = $state(saved?.url ?? '');
    let token = $state('');
    let revealed = $state(false);
    let checking = $state(false);
    let error = $state<CommandError | undefined>(untrack(() => session.error));

    const cantConnect = $derived(url.trim() === '' || token.trim() === '' || checking);

    async function connect(event: SubmitEvent) {
        event.preventDefault();
        if (cantConnect) return;
        checking = true;
        error = undefined;
        try {
            await session.signIn(url, token);
        } catch (e) {
            error = asCommandError(e);
        } finally {
            checking = false;
        }
    }
</script>

<main class="flex min-h-dvh items-center justify-center p-4">
    <form
        aria-labelledby="sign-in-title"
        novalidate
        class="flex w-full max-w-100 flex-col gap-3"
        onsubmit={connect}
    >
        <div class="mb-1 flex flex-col gap-0.5">
            <h1 id="sign-in-title" class="text-lg font-semibold">{m.app_name()}</h1>
            <p class="text-xs text-muted">{m.signin_lead()}</p>
        </div>

        <div class="flex flex-col gap-1">
            <label for="url" class="text-[11px] text-muted">{m.signin_url()}</label>
            <input
                id="url"
                type="url"
                placeholder="https://log.example.com"
                autocomplete="url"
                spellcheck="false"
                class="control font-mono text-[13px]"
                bind:value={url}
            />
        </div>

        <div class="flex flex-col gap-1">
            <label for="token" class="text-[11px] text-muted">{m.signin_token()}</label>
            <div class="flex">
                <input
                    id="token"
                    type={revealed ? 'text' : 'password'}
                    autocomplete="off"
                    spellcheck="false"
                    class={[
                        'control flex-1 rounded-r-none font-mono text-[13px]',
                        refusesToken(error) && 'border-error'
                    ]}
                    bind:value={token}
                />
                <button
                    type="button"
                    aria-label={revealed ? m.signin_hide_token() : m.signin_show_token()}
                    aria-pressed={revealed}
                    class="flex w-10 items-center justify-center self-stretch rounded-r-md border border-l-0 border-border bg-surface text-muted"
                    onclick={() => (revealed = !revealed)}
                >
                    <svg
                        aria-hidden="true"
                        width="16"
                        height="16"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        ><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z" /><circle
                            cx="12"
                            cy="12"
                            r="3"
                        /></svg
                    >
                </button>
            </div>
            <span class="text-[11px] text-muted">{m.signin_token_hint()}</span>
        </div>

        {#if error}
            <p role="alert" class="rounded-md bg-error-bg px-2.5 py-2 text-xs text-error-text">
                {errorText(error)}
            </p>
        {:else if saved?.rejected}
            <p role="status" class="rounded-md bg-warn-bg px-2.5 py-2 text-xs text-warn">
                {m.signin_rejected()}
            </p>
        {/if}

        <button
            type="submit"
            disabled={cantConnect}
            class="mt-1 h-10 rounded-md bg-accent text-sm font-bold text-accent-text disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-muted"
            >{checking ? m.signin_checking() : m.signin_connect()}</button
        >
        <p class="text-center text-[11px] text-muted">{m.signin_keychain()}</p>
    </form>
</main>
