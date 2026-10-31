<script lang="ts" generics="T extends string">
    let {
        label,
        tabs,
        value = $bindable(),
        class: className
    }: {
        label: string;
        tabs: { id: T; label: string }[];
        value: T;
        class?: string;
    } = $props();
</script>

<div role="tablist" aria-label={label} class={['grid grid-cols-2 gap-1.5', className]}>
    {#each tabs as tab (tab.id)}
        <button
            type="button"
            role="tab"
            aria-selected={value === tab.id}
            class={[
                'h-10 rounded-md border text-sm',
                value === tab.id
                    ? 'border-accent bg-selected font-semibold text-accent'
                    : 'border-border bg-bg'
            ]}
            onclick={() => (value = tab.id)}>{tab.label}</button
        >
    {/each}
</div>
