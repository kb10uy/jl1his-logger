<script lang="ts">
    import { qslTone, qslValue } from '#lib/qso/qsl.ts';

    let {
        value,
        label,
        small = false
    }: { value: string | undefined; label?: string; small?: boolean } = $props();

    const tone = $derived(qslTone(value));
</script>

<span
    class={[
        'inline-block min-w-[22px] rounded text-center font-semibold',
        small ? 'px-1.5 text-2xs leading-[18px]' : 'px-1.5 py-px text-2xs',
        tone === 'ok' && 'bg-ok-bg text-ok',
        tone === 'warn' && 'bg-warn-bg text-warn',
        tone === 'none' && 'bg-surface-2 text-muted'
    ]}>{label ?? qslValue(value)}</span
>
