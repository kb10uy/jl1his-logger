<script lang="ts">
    import { toasts } from '#lib/state/toast.svelte.ts';
    import { m } from '#lib/paraglide/messages.js';
    import Icon from './Icon.svelte';
</script>

<div
    aria-live="polite"
    class={[
        'pointer-events-none fixed inset-x-0 bottom-24 z-50 flex flex-col items-center gap-2 px-4',
        toasts.lifted ? 'md:bottom-[88px]' : 'md:bottom-6'
    ]}
>
    {#each toasts.items as toast (toast.id)}
        <div
            role="status"
            class={[
                'pointer-events-auto flex max-w-full items-center gap-4 rounded-lg border bg-surface-2 py-2.5 pr-3 pl-4 text-[13px] shadow-lg shadow-shadow',
                toast.tone === 'error' ? 'border-error text-error-text' : 'border-border'
            ]}
        >
            <span class="min-w-0">{toast.text}</span>
            {#if toast.action}
                {@const action = toast.action}
                <button
                    type="button"
                    class="h-7 rounded-md px-2.5 font-semibold text-info"
                    onclick={() => {
                        toasts.dismiss(toast.id);
                        action.run();
                    }}>{action.label}</button
                >
            {/if}
            <button
                type="button"
                aria-label={m.common_dismiss()}
                class="inline-flex size-7 items-center justify-center rounded-md text-muted"
                onclick={() => toasts.dismiss(toast.id)}
            >
                <Icon name="close" size={12} strokeWidth={2.5} />
            </button>
        </div>
    {/each}
</div>
