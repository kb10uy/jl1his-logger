<script lang="ts">
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import * as api from '#lib/api/api.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import { isWebAuthnSupported } from '#lib/api/webauthn.ts';
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
            goto(next(), { replace: true });
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

<main class="flex min-h-dvh items-center justify-center p-4">
    <div class="flex w-full max-w-sm flex-col gap-4 rounded-lg border border-border bg-surface p-6">
        <div class="flex flex-col gap-1">
            <h1 class="text-[22px] font-semibold">JL1HIS Logger</h1>
            <p class="text-muted">Sign in with a passkey registered to your callsign.</p>
        </div>
        {#if !isWebAuthnSupported()}
            <p class="text-error-text">This browser cannot use passkeys.</p>
        {/if}
        {#if error}
            <p role="alert" class="text-[13px] text-error-text">{error}</p>
        {/if}
        <Button variant="primary" size="xl" disabled={busy} onclick={signIn}>
            {busy ? 'Waiting for the passkey…' : 'Sign in with a passkey'}
        </Button>
        <p class="text-xs text-muted">
            New here? Open the registration link an admin gave you on this device.
        </p>
    </div>
</main>
