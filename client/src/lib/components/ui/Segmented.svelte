<script lang="ts" generics="T extends string">
    let {
        label,
        options,
        value = $bindable()
    }: { label: string; options: { value: T; label: string }[]; value: T } = $props();
</script>

<div role="group" aria-label={label} class="flex flex-wrap gap-1.5">
    {#each options as option (option.value)}
        <button
            type="button"
            aria-pressed={value === option.value}
            class={[
                'h-[34px] rounded-md border px-3.5 text-[13px]',
                value === option.value
                    ? 'border-accent bg-warn-bg font-semibold text-accent'
                    : 'border-border bg-bg text-text'
            ]}
            onclick={() => (value = option.value)}>{option.label}</button
        >
    {/each}
</div>
