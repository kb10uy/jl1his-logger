<script lang="ts">
    import type { Snippet } from 'svelte';
    import Icon from './Icon.svelte';

    let {
        tone = 'warn',
        class: className = '',
        children
    }: { tone?: 'warn' | 'error' | 'info'; class?: string; children: Snippet } = $props();
</script>

<div
    role="status"
    class={[
        'flex flex-wrap items-center gap-x-3 gap-y-2 rounded-md px-3 py-2 text-sm',
        tone === 'warn' && 'bg-warn-bg text-warn',
        tone === 'error' && 'bg-error-bg text-error-text',
        tone === 'info' && 'bg-info-bg text-info',
        className
    ]}
>
    <Icon name="warning" size={16} />
    <div class="min-w-0 flex-[1_1_200px]">{@render children()}</div>
</div>
