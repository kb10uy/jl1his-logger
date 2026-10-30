<script lang="ts">
    import type { Lookup, Qso } from '#client/api/types.ts';
    import { hhmm, isoDate } from '#client/qso/format.ts';
    import { lookupLine } from '#client/entry/lookup.ts';
    import { m } from '#lib/paraglide/messages.js';

    let {
        shown,
        lookup,
        last,
        duplicates,
        today
    }: {
        shown: boolean;
        lookup: Lookup | undefined;
        last: Qso | undefined;
        duplicates: Qso[];
        today: string;
    } = $props();

    const duplicate = $derived(duplicates[0]);
    const lastFields = $derived(last?.fields);
</script>

<div
    role="status"
    aria-label={m.entry_lookup()}
    class={[
        '-mt-0.5 flex h-[20px] items-center gap-2 text-[12px] whitespace-nowrap',
        !shown && 'invisible'
    ]}
>
    {#if lookup}
        <span class="min-w-0 flex-[0_1_auto] truncate text-muted">{lookupLine(lookup)}</span>
        {#if lookup.previous.count > 0}
            <span class="flex-none rounded-full bg-info-bg px-[7px] py-px text-[11px] text-info"
                >{m.entry_worked({ count: lookup.previous.count })}</span
            >
            {#if lastFields}
                <span class="min-w-0 flex-[0_1_auto] truncate text-muted"
                    >{m.entry_last({
                        time: `${isoDate(lastFields.QSO_DATE)} ${hhmm(lastFields.TIME_ON)}`,
                        band: lastFields.BAND ?? '',
                        mode: lastFields.SUBMODE || lastFields.MODE || '',
                        qsl: lastFields.QSL_RCVD || '—'
                    })}</span
                >
            {/if}
            <div class="flex-1"></div>
            {#if duplicate}
                <span
                    title={m.entry_duplicate_detail({
                        call: duplicate.fields.CALL,
                        band: duplicate.fields.BAND,
                        mode: duplicate.fields.MODE,
                        time: hhmm(duplicate.fields.TIME_ON),
                        when:
                            duplicate.fields.QSO_DATE === today
                                ? m.entry_today()
                                : isoDate(duplicate.fields.QSO_DATE)
                    })}
                    class="flex-none rounded-full bg-warn-bg px-[7px] py-px text-[11px] font-semibold text-warn"
                    >{m.entry_possible_duplicate()}</span
                >
            {/if}
        {:else}
            <span class="flex-none rounded-full bg-ok-bg px-[7px] py-px text-[11px] text-ok"
                >{m.entry_first_qso()}</span
            >
        {/if}
    {/if}
</div>
