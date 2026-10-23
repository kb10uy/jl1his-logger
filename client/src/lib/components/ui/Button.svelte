<script lang="ts">
    import type { Snippet } from 'svelte';
    import type { HTMLButtonAttributes } from 'svelte/elements';

    type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'link' | 'toggle';
    type Size = 'sm' | 'md' | 'lg' | 'xl';

    let {
        variant = 'secondary',
        size = 'md',
        href,
        pressed,
        class: className = '',
        children,
        ...rest
    }: HTMLButtonAttributes & {
        variant?: Variant;
        size?: Size;
        href?: string;
        pressed?: boolean;
        children: Snippet;
    } = $props();

    const sizes: Record<Size, string> = {
        sm: 'h-8 px-2.5 text-[13px]',
        md: 'h-9 px-3.5',
        lg: 'h-10 px-3.5',
        xl: 'h-11 px-5 text-[15px] rounded-lg'
    };

    const variants: Record<Variant, string> = {
        primary:
            'border-0 bg-accent font-semibold text-accent-text disabled:bg-surface-2 disabled:text-muted',
        secondary: 'border border-border bg-surface-2 font-medium text-text',
        ghost: 'border-0 bg-transparent text-text',
        danger: 'border border-error bg-transparent font-medium text-error',
        link: 'border-0 bg-transparent px-2 text-info',
        toggle: ''
    };

    const toggle = $derived(
        pressed
            ? 'border border-info-strong bg-selected font-semibold text-info'
            : 'border border-border bg-surface text-text'
    );

    const classes = $derived([
        'inline-flex shrink-0 items-center justify-center gap-2 rounded-md whitespace-nowrap no-underline disabled:cursor-not-allowed',
        sizes[size],
        variant === 'toggle' ? toggle : variants[variant],
        className
    ]);
</script>

{#if href !== undefined}
    <a {href} class={classes}>{@render children()}</a>
{:else}
    <button type="button" aria-pressed={pressed} class={classes} {...rest}>
        {@render children()}
    </button>
{/if}
