<script lang="ts">
    import { goto } from '$app/navigation';
    import AppShell from '#lib/components/AppShell.svelte';
    import SectionFrame from '#lib/components/SectionFrame.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { lastLogbook, session } from '#lib/state/session.svelte.ts';

    let { children } = $props();

    const admin = $derived(session.me?.role === 'admin');
    const logbookId = $derived(
        session.logbook(lastLogbook() ?? '')?.id ?? session.logbooks[0]?.id ?? null
    );

    $effect(() => {
        if (!admin) goto('/settings', { replace: true });
    });
</script>

{#if admin}
    <AppShell {logbookId}>
        <SectionFrame
            title={m.admin_title()}
            sectionTitle={(section) => m.admin_section_title({ section })}
            back={m.admin_back()}
            index="/admin"
            label={m.admin_sections()}
            groups={[
                {
                    label: m.admin_scope_instance(),
                    sections: [{ label: m.admin_server(), href: '/admin/server' }]
                }
            ]}
            {logbookId}
        >
            {@render children()}
        </SectionFrame>
    </AppShell>
{/if}
