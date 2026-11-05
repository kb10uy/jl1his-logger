<script lang="ts">
    import SectionFrame, { type Group } from '#lib/components/SectionFrame.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';

    let { children } = $props();

    const logbook = useLogbook();
    const root = `/logbooks/${logbook.logbook.id}/import-export`;

    const groups: Group[] = [
        {
            sections: [
                { label: m.transfer_export_adif(), href: `${root}/export` },
                { label: m.transfer_import_adif(), href: `${root}/import` }
            ]
        }
    ];
</script>

<SectionFrame
    title={m.nav_import_export()}
    sectionTitle={(section) => m.transfer_section_title({ section })}
    back={m.transfer_back()}
    index={root}
    label={m.transfer_sections()}
    {groups}
    logbookId={logbook.logbook.id}
>
    {@render children()}
</SectionFrame>
