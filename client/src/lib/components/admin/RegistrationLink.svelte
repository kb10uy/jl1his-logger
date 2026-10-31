<script lang="ts">
    import Button from '#lib/components/ui/Button.svelte';
    import Notice from '#lib/components/ui/Notice.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { toasts } from '#lib/state/toast.svelte.ts';

    let { callsign, url }: { callsign: string; url: string } = $props();

    async function copy() {
        try {
            await navigator.clipboard.writeText(url);
            toasts.show(m.admin_users_link_copied());
        } catch {
            toasts.error(m.admin_users_link_copy_failed());
        }
    }
</script>

<Notice tone="info">
    <div class="flex flex-col gap-2">
        <span class="font-semibold">{m.admin_users_link_shown_once({ callsign })}</span>
        <div class="flex flex-wrap items-center gap-2">
            <code
                class="min-w-0 flex-1 rounded bg-bg px-2 py-1.5 font-mono text-sm break-all text-text select-all"
                >{url}</code
            >
            <Button size="sm" onclick={copy}>{m.common_copy()}</Button>
        </div>
    </div>
</Notice>
