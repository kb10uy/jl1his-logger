<script lang="ts">
    import type { ContestSettings } from '#client/qso/contest.ts';
    import { m } from '#lib/paraglide/messages.js';

    let { contest = $bindable() }: { contest: ContestSettings } = $props();
</script>

<div
    role="group"
    aria-label={m.entry_contest()}
    class="flex flex-none items-center gap-2 rounded-md border border-info-strong bg-selected px-2 py-[5px] text-[11px] text-muted"
>
    <label for="contest-id">CONTEST_ID</label>
    <input
        id="contest-id"
        autocomplete="off"
        spellcheck="false"
        class="field h-[28px] w-[120px] font-mono text-[12px] uppercase"
        bind:value={contest.contestId}
    />
    <label for="contest-template" class="ml-1.5">{m.entry_contest_template()}</label>
    <input
        id="contest-template"
        autocomplete="off"
        spellcheck="false"
        placeholder={'{serial}'}
        class="field h-[28px] w-[100px] font-mono text-[12px]"
        bind:value={contest.exchangeTemplate}
    />
    <label for="contest-next" class="ml-1.5">{m.entry_contest_next()}</label>
    <input
        id="contest-next"
        type="number"
        min="1"
        step="1"
        class="field h-[28px] w-[60px] px-1.5 text-right font-mono text-[12px]"
        bind:value={contest.nextSerial}
    />
    <label class="ml-1.5 flex items-center gap-1.5">
        <input
            type="checkbox"
            class="m-0 size-[14px] accent-info"
            bind:checked={contest.includeRst}
        />
        {m.entry_rst_in_front()}
    </label>
</div>
