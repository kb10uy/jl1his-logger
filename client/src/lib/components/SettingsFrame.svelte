<script lang="ts">
    import type { Snippet } from 'svelte';
    import SectionFrame, { type Group } from '#lib/components/SectionFrame.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { session } from '#lib/state/session.svelte.ts';

    let { logbookId, children }: { logbookId: string | null; children: Snippet } = $props();

    const logbook = $derived(logbookId === null ? undefined : session.logbook(logbookId));

    const groups = $derived<Group[]>(
        [
            {
                label: m.settings_scope_you(),
                sections: [
                    { label: m.settings_account(), href: '/settings/account' },
                    { label: m.settings_tokens(), href: '/settings/tokens' }
                ]
            },
            {
                label: m.settings_scope_device(),
                sections: [
                    { label: m.settings_voice(), href: '/settings/voice' },
                    { label: m.settings_appearance(), href: '/settings/appearance' }
                ]
            },
            logbook === undefined
                ? undefined
                : {
                      label: `${m.settings_scope_logbook()} · ${logbook.name}`,
                      sections: [
                          { label: m.settings_logbook(), href: `/logbooks/${logbook.id}/settings` },
                          { label: m.settings_members(), href: `/logbooks/${logbook.id}/members` }
                      ]
                  }
        ].filter((group) => group !== undefined)
    );
</script>

<SectionFrame
    title={m.settings_title()}
    sectionTitle={(section) => m.settings_section_title({ section })}
    back={m.settings_back()}
    index="/settings"
    label={m.settings_sections()}
    {groups}
    {logbookId}
>
    {@render children()}
</SectionFrame>
