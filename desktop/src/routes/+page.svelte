<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';
    import { errorText } from '#lib/session/errors.ts';
    import { session } from '#lib/session/session.svelte.ts';

    const current = $derived(session.current?.state === 'signedIn' ? session.current : undefined);
</script>

{#if current}
    <main class="flex min-h-dvh flex-col gap-4 p-4">
        <header class="flex items-center gap-3">
            <h1 class="text-base font-semibold">{m.app_name()}</h1>
            <span class="min-w-0 flex-1 truncate text-xs text-muted"
                >{m.home_connected({ url: current.url })}</span
            >
            <button
                type="button"
                class="h-8 rounded-md border border-border bg-surface px-3 text-xs"
                onclick={() => session.signOut()}>{m.common_sign_out()}</button
            >
        </header>

        {#if session.error}
            <p role="alert" class="rounded-md bg-error-bg px-2.5 py-2 text-xs text-error-text">
                {errorText(session.error)}
            </p>
        {/if}

        {#if current.logbooks}
            <section aria-labelledby="logbooks" class="flex flex-col gap-2">
                <h2 id="logbooks" class="eyebrow">{m.home_logbooks()}</h2>
                <ul class="flex flex-col rounded-lg border border-border bg-surface">
                    {#each current.logbooks as logbook (logbook.id)}
                        <li class="border-b border-surface-2 px-3 py-2 last:border-b-0">
                            {logbook.name}
                        </li>
                    {/each}
                </ul>
            </section>
        {:else}
            <p role="status" class="rounded-md bg-warn-bg px-2.5 py-2 text-xs text-warn">
                {m.home_offline({ url: current.url })}
            </p>
        {/if}
    </main>
{/if}
