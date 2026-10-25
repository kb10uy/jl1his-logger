<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { ReferenceKind, ReferenceVersion, ServerVersion } from '#lib/api/types.ts';
    import Notice from '#lib/components/ui/Notice.svelte';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';

    const KINDS: { kind: ReferenceKind; label: () => string }[] = [
        { kind: 'cty', label: () => 'cty.dat' },
        { kind: 'dxcc', label: m.settings_reference_dxcc },
        { kind: 'subdivisions', label: m.settings_reference_subdivisions },
        { kind: 'parks', label: m.settings_reference_parks }
    ];

    let server = $state<ServerVersion | undefined>(undefined);
    let versions = $state<ReferenceVersion[] | undefined>(undefined);
    let error = $state<string | undefined>(undefined);

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
</section>
