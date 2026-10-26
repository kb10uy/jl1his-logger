<script lang="ts">
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';

    let { favorite, onchange }: { favorite: boolean; onchange: (favorite: boolean) => void } =
        $props();
</script>

<Button size="sm" variant="toggle" pressed={favorite} onclick={() => onchange(!favorite)}
    ><Icon
        name={favorite ? 'starred' : 'star'}
        size={16}
        class={favorite ? 'text-warn' : ''}
    />{m.favorite()}</Button
>
