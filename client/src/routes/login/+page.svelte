<script lang="ts">
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import { isWebAuthnSupported } from '#lib/api/webauthn.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let busy = $state(false);
    let error = $state<string | undefined>(undefined);

    function next(): string {
        const wanted = page.url.searchParams.get('next') ?? '/';
        return wanted.startsWith('/') && !wanted.startsWith('//') ? wanted : '/';
    }

    async function signIn() {
        busy = true;
        error = undefined;
        try {
            await session.signedIn(await api.signIn());
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }

    $effect(() => {
        if (session.me) goto(next(), { replace: true });
    });
</script>

<PageTitle title={m.login_title()} />

<main class="flex min-h-dvh items-center justify-center p-4">
    <div class="flex w-full max-w-sm flex-col gap-4 rounded-lg border border-border bg-surface p-6">
        <div class="flex flex-col gap-1">
            <h1 class="font-logo text-[22px] font-semibold">JL1HIS Logger</h1>
            <p class="text-muted">{m.login_lead()}</p>
        </div>
        {#if !isWebAuthnSupported()}
            <p class="text-error-text">{m.login_unsupported()}</p>
        {/if}
        {#if error}
            <p role="alert" class="text-[13px] text-error-text">{error}</p>
        {/if}
        <Button variant="primary" size="xl" disabled={busy} onclick={signIn}>
            {busy ? m.login_waiting() : m.login_button()}
        </Button>
        <p class="text-xs text-muted">
            {m.login_new_here()}
        </p>
    </div>
</main>
