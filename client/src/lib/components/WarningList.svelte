<script lang="ts">
    import type { Warning } from '#lib/api/types.ts';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { m } from '#lib/paraglide/messages.js';

    let { warnings }: { warnings: Warning[] } = $props();

    function text(w: Warning): string {
        switch (w.kind) {
            case 'possible_duplicate':
                return m.warning_possible_duplicate({ count: w.ids.length });
            case 'frequency_outside_band':
                return m.warning_frequency_outside_band({ freq: w.freq, band: w.band });
            case 'unknown_mode':
                return m.warning_unknown_mode({ mode: w.mode });
            case 'unknown_submode':
                return m.warning_unknown_submode({ submode: w.submode });
            case 'submode_of_another_mode':
                return m.warning_submode_of_another_mode({
                    submode: w.submode,
                    expected: w.expected,
                    mode: w.mode
                });
        }
    }
</script>

{#each warnings as warning, i (i)}
    <Notice>{text(warning)}</Notice>
{/each}
