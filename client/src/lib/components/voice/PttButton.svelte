<script lang="ts">
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import type { VoiceState } from '#lib/voice/voice.svelte.ts';

    let { voice, class: className = '' }: { voice: VoiceState; class?: string } = $props();

    let failed = $state(false);

    $effect(() =>
        voice.onApplied(({ source, ok }) => {
            if (ok || source !== 'voice') return;
            failed = true;
            setTimeout(() => (failed = false), 600);
        })
    );

    function down(e: PointerEvent) {
        if (e.button !== 0) return;
        e.preventDefault();
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        void voice.pttPress();
    }

    const label = $derived(
        {
            idle: m.voice_ptt_idle,
            opening: m.voice_ptt_opening,
            listening: m.voice_ptt_listening,
            finishing: m.voice_ptt_finishing
        }[voice.ptt]()
    );
    const active = $derived(voice.ptt === 'listening' || voice.ptt === 'opening');
    const meter = $derived(Math.min(1, Math.sqrt(voice.level) * 2.5));
</script>

<button
    type="button"
    aria-label={m.voice_ptt_label()}
    aria-pressed={active}
    class={[
        'relative flex h-32 w-full touch-none flex-col items-center justify-center gap-1.5 overflow-hidden rounded-xl border-0 text-white transition-[background-color,scale] duration-75 select-none [-webkit-touch-callout:none]',
        active && 'scale-[0.99] bg-ptt-active shadow-[0_0_0_4px_var(--warn-bg)]',
        voice.ptt === 'finishing' && 'bg-surface-2 text-text',
        voice.ptt === 'idle' && (failed ? 'bg-error' : 'bg-ptt'),
        className
    ]}
    onpointerdown={down}
    onpointerup={() => void voice.pttRelease()}
    onpointercancel={() => void voice.pttCancel()}
    oncontextmenu={(e) => e.preventDefault()}
>
    <span
        aria-hidden="true"
        class="absolute bottom-0 left-0 h-1.5 w-full origin-left bg-white/85 transition-transform duration-75 ease-linear"
        style:transform="scaleX({voice.ptt === 'listening' ? meter : 0})"
    ></span>
    <Icon name="voice" size={28} />
    <span class="text-md font-semibold">{label}</span>
    {#if voice.ptt === 'listening' && voice.partial}
        <span class="px-4 text-base opacity-90">{voice.partial}</span>
    {/if}
</button>
