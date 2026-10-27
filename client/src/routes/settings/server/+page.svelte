<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { ReferenceKind, ReferenceVersion, ServerVersion } from '#lib/api/types.ts';
    import Button from '#lib/components/ui/Button.svelte';
    import FileDrop from '#lib/components/ui/FileDrop.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { ctyArchiveProblem } from '#lib/reference/cty.ts';
    import { session } from '#lib/state/session.svelte.ts';

    const KINDS: { kind: ReferenceKind; label: () => string }[] = [
        { kind: 'cty', label: () => 'CTY' }
    ];

    let server = $state<ServerVersion | undefined>(undefined);
    let versions = $state<ReferenceVersion[] | undefined>(undefined);
    let error = $state<string | undefined>(undefined);
    let archive = $state<File | undefined>(undefined);
    let installing = $state(false);
    let installed = $state<string | undefined>(undefined);
    let refused = $state<string | undefined>(undefined);
    const problem = $derived(archive && ctyArchiveProblem(archive));

    $effect(() => {
        if (archive) {
            installed = undefined;
            refused = undefined;
        }
    });

    async function install() {
        if (!archive || problem) return;
        installing = true;
        try {
            const version = await api.installCty(archive);
            installed = version.version;
            archive = undefined;
            versions = await api.references();
        } catch (e) {
            refused = (e as Error).message;
        } finally {
            installing = false;
        }
    }

    onMount(() => {
        api.version()
            .then((found) => (server = found))
            .catch((e) => (error = (e as Error).message));
        api.references()
            .then((found) => (versions = found))
            .catch((e) => (error = (e as Error).message));
    });
</script>

<section aria-label={m.settings_server()} class="flex flex-col gap-6">
    {#if error}
        <Notice tone="error">{error}</Notice>
    {/if}
    <dl
        class="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-6 gap-y-2 rounded-lg border border-border bg-surface px-3 py-2.5 text-[13px]"
    >
        <dt class="text-muted">{m.settings_server_version()}</dt>
        <dd class="font-mono">{server?.version ?? '…'}</dd>
        <dt class="text-muted">{m.settings_server_commit()}</dt>
        <dd
            class={['truncate font-mono', server?.commit === null && 'text-muted']}
            title={server?.commit ?? undefined}
        >
            {server === undefined
                ? '…'
                : (server.commit?.slice(0, 12) ?? m.settings_server_commit_unknown())}
        </dd>
        <dt class="text-muted">{m.settings_server_adif()}</dt>
        <dd class="font-mono">{server?.adif ?? '…'}</dd>
    </dl>
    <section aria-label={m.settings_reference()} class="flex flex-col gap-2">
        <h2 class="eyebrow">{m.settings_reference()}</h2>
        <div class="overflow-x-auto rounded-lg border border-border bg-surface">
            <table class="w-full border-collapse text-[13px]">
                <thead>
                    <tr class="text-left text-xs text-muted">
                        <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                            >{m.settings_kind()}</th
                        >
                        <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                            >{m.settings_reference_version()}</th
                        >
                        <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                            >{m.settings_reference_installed()}</th
                        >
                    </tr>
                </thead>
                <tbody>
                    {#each KINDS as { kind, label } (kind)}
                        {@const found = versions?.find((v) => v.kind === kind)}
                        <tr class="border-b border-surface-2 last:border-b-0">
                            <td class="px-3 py-2.5">{label()}</td>
                            <td class={['px-3 py-2.5 font-mono', !found && 'text-muted']}
                                >{versions === undefined
                                    ? '…'
                                    : (found?.version ?? m.settings_reference_not_installed())}</td
                            >
                            <td class="px-3 py-2.5 font-mono text-muted"
                                >{found?.updated_at.slice(0, 10) ?? '—'}</td
                            >
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        <span class="text-xs text-muted"
            ><Slotted text={m.settings_reference_updated_with({ command: SLOT })}
                ><code class="font-mono">reference install</code></Slotted
            ></span
        >
    </section>
    {#if session.me?.role === 'admin'}
        <section aria-label={m.settings_reference_cty_install()} class="flex flex-col gap-2">
            <h2 class="eyebrow">{m.settings_reference_cty_install()}</h2>
            <FileDrop
                bind:file={archive}
                accept=".zip,application/zip"
                hint={m.settings_reference_cty_hint()}
            />
            {#if problem}
                <Notice tone="error">{problem}</Notice>
            {:else if refused}
                <Notice tone="error">{refused}</Notice>
            {:else if installed}
                <Notice tone="info"
                    >{m.settings_reference_cty_installed({ version: installed })}</Notice
                >
            {/if}
            <div>
                <Button
                    variant="primary"
                    disabled={archive === undefined || problem !== undefined || installing}
                    onclick={install}
                    >{installing
                        ? m.settings_reference_cty_uploading()
                        : m.settings_reference_cty_upload()}</Button
                >
            </div>
        </section>
    {/if}
</section>
