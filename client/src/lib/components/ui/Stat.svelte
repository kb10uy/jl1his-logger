<script lang="ts">
    import { formatNumber } from '#lib/i18n.ts';

    let {
        label,
        value,
        tone
    }: { label: string; value: string | number; tone?: 'ok' | 'warn' | 'error' } = $props();
</script>

<div class="flex min-w-0 flex-col gap-0.5 rounded-md bg-bg px-3 py-2.5">
    <span class="truncate text-xs text-muted">{label}</span>
    <span
        class={[
            'font-mono text-lg',
            tone === 'ok' && 'text-ok',
            tone === 'warn' && 'text-warn',
            tone === 'error' && 'text-error-text'
        ]}>{typeof value === 'number' ? formatNumber(value) : value}</span
    >
</div>
