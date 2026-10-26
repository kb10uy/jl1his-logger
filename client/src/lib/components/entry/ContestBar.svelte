<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';
    import { expandExchange, type ContestSettings } from '#lib/qso/contest.ts';

    let { contest = $bindable(), rstSent }: { contest: ContestSettings; rstSent: string } =
        $props();

    const sent = $derived(expandExchange(contest.exchangeTemplate, contest.nextSerial));
</script>

<section
    aria-label={m.field_contest()}
    class="flex flex-wrap items-end gap-x-5 gap-y-3 rounded-lg border border-info-strong bg-selected px-4 py-3"
>
    <div class="flex flex-col gap-1">
        <label for="contest-id" class="text-xs text-muted">CONTEST_ID</label>
        <input
            id="contest-id"
            placeholder={m.entry_optional()}
            autocomplete="off"
            spellcheck="false"
            class="control w-[180px] font-mono uppercase"
            bind:value={contest.contestId}
        />
    </div>
    <div class="flex flex-col gap-1">
        <label for="contest-sent" class="text-xs text-muted">{m.entry_sent_number()}</label>
        <input
            id="contest-sent"
            placeholder={'{serial}'}
            autocomplete="off"
            spellcheck="false"
            class="control w-40 font-mono"
            bind:value={contest.exchangeTemplate}
        />
    </div>
    <div class="flex flex-col gap-1">
        <label for="contest-next" class="text-xs text-muted">{m.entry_next_serial()}</label>
        <input
            id="contest-next"
            type="number"
            min="1"
            step="1"
            class="control w-24 text-right font-mono"
            bind:value={contest.nextSerial}
        />
    </div>
    <label class="flex h-9 cursor-pointer items-center gap-2 text-[13px]">
        <input type="checkbox" class="size-4 accent-info" bind:checked={contest.includeRst} />
        {m.entry_rst_in_front()}
    </label>
    <div class="flex-1"></div>
    <div class="flex flex-col items-end gap-1">
        <span class="text-xs text-muted">{m.entry_next_stx()}</span>
        <strong class="flex h-9 items-center font-mono text-lg font-semibold"
            >{(contest.includeRst ? rstSent : '') + sent || m.entry_nothing()}</strong
        >
    </div>
</section>
