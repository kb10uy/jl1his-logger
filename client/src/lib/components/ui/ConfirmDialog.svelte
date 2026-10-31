<script lang="ts">
    import { AlertDialog } from 'bits-ui';
    import Slotted from '#lib/components/ui/Slotted.svelte';
    import { SLOT } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        open = $bindable(false),
        title,
        description,
        confirm,
        danger = false,
        challenge,
        onconfirm
    }: {
        open?: boolean;
        title: string;
        description: string;
        confirm: string;
        danger?: boolean;
        challenge?: string;
        onconfirm: () => void;
    } = $props();

    let typed = $state('');
    const ready = $derived(challenge === undefined || typed.trim() === challenge);

    $effect(() => {
        if (!open) typed = '';
    });
</script>

<AlertDialog.Root bind:open>
    <AlertDialog.Portal>
        <AlertDialog.Overlay class="fixed inset-0 z-50 bg-black/50" />
        <AlertDialog.Content
            class="fixed top-1/2 left-1/2 z-50 flex w-[min(420px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-lg border border-border bg-surface p-5 shadow-xl shadow-shadow"
        >
            <AlertDialog.Title class="text-base font-semibold">{title}</AlertDialog.Title>
            <AlertDialog.Description class="text-sm text-muted">
                {description}
            </AlertDialog.Description>
            {#if challenge !== undefined}
                <label class="flex flex-col gap-1 text-2xs text-muted">
                    <span
                        ><Slotted text={m.common_type_to_confirm({ name: SLOT })}
                            ><span class="font-mono text-text">{challenge}</span></Slotted
                        ></span
                    >
                    <input
                        class="control h-10 font-mono"
                        autocomplete="off"
                        spellcheck="false"
                        bind:value={typed}
                    />
                </label>
            {/if}
            <div class="mt-2 flex justify-end gap-2">
                <AlertDialog.Cancel
                    class="h-9 rounded-md border border-border bg-transparent px-3.5 text-text"
                    >{m.common_cancel()}</AlertDialog.Cancel
                >
                <AlertDialog.Action
                    disabled={!ready}
                    class={[
                        'h-9 rounded-md px-3.5 font-semibold disabled:opacity-50',
                        danger ? 'border border-error text-error' : 'bg-accent text-accent-text'
                    ]}
                    onclick={() => {
                        open = false;
                        onconfirm();
                    }}>{confirm}</AlertDialog.Action
                >
            </div>
        </AlertDialog.Content>
    </AlertDialog.Portal>
</AlertDialog.Root>
