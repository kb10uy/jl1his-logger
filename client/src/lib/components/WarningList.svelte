<script lang="ts">
    import type { Warning } from '#lib/api/types.ts';
    import Notice from '#lib/components/ui/Notice.svelte';

    let { warnings }: { warnings: Warning[] } = $props();

    function text(w: Warning): string {
        switch (w.kind) {
            case 'possible_duplicate':
                return `Possible duplicate of ${w.ids.length} QSO${w.ids.length === 1 ? '' : 's'} within 15 minutes. Both are kept.`;
            case 'frequency_outside_band':
                return `${w.freq} MHz is outside ${w.band}. Both are kept as entered.`;
            case 'unknown_mode':
                return `${w.mode} is not an ADIF mode.`;
            case 'unknown_submode':
                return `${w.submode} is not an ADIF submode.`;
            case 'submode_of_another_mode':
                return `${w.submode} belongs to ${w.expected}, not ${w.mode}.`;
        }
    }
</script>

{#each warnings as warning, i (i)}
    <Notice>{text(warning)}</Notice>
{/each}
