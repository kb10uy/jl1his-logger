<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';

    let {
        file = $bindable(),
        hint = m.file_drop_hint(),
        accept = '.adi,.adx'
    }: { file: File | undefined; hint?: string; accept?: string } = $props();

    let over = $state(false);

    const size = (n: number) =>
        n < 1024 * 1024
            ? m.file_size_kb({ size: Math.max(1, Math.round(n / 1024)) })
            : m.file_size_mb({ size: (n / 1048576).toFixed(1) });
</script>

<label
    class={[
        'relative flex h-24 cursor-pointer flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed bg-bg px-3 text-center',
        over ? 'border-accent' : 'border-border'
    ]}
    ondragover={(e) => {
        e.preventDefault();
        over = true;
    }}
    ondragleave={() => (over = false)}
    ondrop={(e) => {
        e.preventDefault();
        over = false;
        const dropped = e.dataTransfer?.files[0];
        if (dropped) file = dropped;
    }}
>
    {#if file}
        <span class="max-w-full truncate font-mono text-xs"
            >{file.name} <span class="text-muted">· {size(file.size)}</span></span
        >
    {/if}
    <span class="text-2xs text-muted">{hint}</span>
    <input
        type="file"
        {accept}
        class="absolute size-px opacity-0"
        onchange={(e) => {
            const chosen = e.currentTarget.files?.[0];
            if (chosen) file = chosen;
            e.currentTarget.value = '';
        }}
    />
</label>
