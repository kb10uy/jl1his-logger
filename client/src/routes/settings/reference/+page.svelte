<script lang="ts">
    import { onMount } from 'svelte';
    import * as api from '#lib/api/api.ts';
    import type { ReferenceKind, ReferenceVersion } from '#lib/api/types.ts';
    import Notice from '#lib/components/ui/Notice.svelte';

    const KINDS: { kind: ReferenceKind; label: string }[] = [
        { kind: 'cty', label: 'cty.dat' },
        { kind: 'dxcc', label: 'DXCC codes' },
        { kind: 'subdivisions', label: 'Subdivisions' },
        { kind: 'parks', label: 'POTA parks' }
    ];

    let versions = $state<ReferenceVersion[] | undefined>(undefined);
    let error = $state<string | undefined>(undefined);

    onMount(() => {
        api.references()
            .then((found) => (versions = found))
            .catch((e) => (error = (e as Error).message));
    });
</script>

<section aria-label="Reference data" class="flex max-w-2xl flex-col gap-3">
    {#if error}
        <Notice tone="error">{error}</Notice>
    {/if}
    <div class="overflow-x-auto rounded-lg border border-border bg-surface">
        <table class="w-full border-collapse text-[13px]">
            <thead>
                <tr class="text-left text-xs text-muted">
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium">Kind</th>
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium">Version</th
                    >
                    <th scope="col" class="border-b border-border px-3 py-2 font-medium"
                        >Installed</th
                    >
                </tr>
            </thead>
            <tbody>
                {#each KINDS as { kind, label } (kind)}
                    {@const found = versions?.find((v) => v.kind === kind)}
                    <tr class="border-b border-surface-2 last:border-b-0">
                        <td class="px-3 py-2.5">{label}</td>
                        <td class={['px-3 py-2.5 font-mono', !found && 'text-muted']}
                            >{versions === undefined
                                ? '…'
                                : (found?.version ?? 'not installed')}</td
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
        >Updated with <code class="font-mono">reference install</code> on the server.</span
    >
</section>
