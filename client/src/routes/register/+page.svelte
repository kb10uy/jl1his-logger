<script lang="ts">
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import * as api from '#lib/api/api.ts';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Field from '#lib/components/ui/Field.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let ceremony = $state<Awaited<ReturnType<typeof api.startRegistration>> | undefined>();
    let name = $state('');
    let busy = $state(false);
    let error = $state<string | undefined>(undefined);

    onMount(async () => {
        const token = location.hash.slice(1);
        history.replaceState(null, '', location.pathname);
        if (token === '') {
            error = m.register_needs_link();
            return;
        }
        try {
            ceremony = await api.startRegistration(token);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    });

    async function register() {
        if (ceremony === undefined) return;
        busy = true;
        error = undefined;
        try {
            await session.signedIn(await api.finishRegistration(ceremony, name.trim()));
            goto('/', { replace: true });
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<PageTitle title={m.register_title()} />

<main class="flex min-h-dvh items-center justify-center p-4">
    <form
        class="flex w-full max-w-sm flex-col gap-4 rounded-lg border border-border bg-surface p-6"
        onsubmit={(e) => {
            e.preventDefault();
            register();
        }}
    >
        <div class="flex flex-col gap-1">
            <h1 class="text-[22px] font-semibold">{m.register_title()}</h1>
            {#if ceremony}
                <p class="text-muted">
                    <Slotted text={m.register_for({ callsign: SLOT })}
                        ><span class="font-mono font-semibold text-text">{ceremony.callsign}</span
                        ></Slotted
                    >
                </p>
            {/if}
        </div>
        {#if ceremony}
            <Field label={m.register_passkey_name()} for="passkey-name">
                <input
                    id="passkey-name"
                    class="control"
                    placeholder={m.register_passkey_placeholder()}
                    autocomplete="off"
                    bind:value={name}
                />
            </Field>
        {/if}
        {#if error}
            <p role="alert" class="text-[13px] text-error-text">{error}</p>
        {/if}
        <Button type="submit" variant="primary" size="xl" disabled={busy || !ceremony}>
            {busy ? m.login_waiting() : m.register_button()}
        </Button>
    </form>
</main>
