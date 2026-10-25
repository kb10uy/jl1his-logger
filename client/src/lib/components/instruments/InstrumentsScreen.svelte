<script lang="ts">
    import { afterNavigate, goto } from '$app/navigation';
    import type { Instrument } from '#lib/api/types.ts';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import PickList from '#lib/components/PickList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import InstrumentEditor from './InstrumentEditor.svelte';

    let { selected }: { selected: string | undefined } = $props();

    const logbook = useLogbook();
    const logbookId = logbook.logbook.id;
    const root = `/logbooks/${logbookId}/instruments`;
    const canRewrite = $derived(logbook.logbook.role === 'owner');
    const archived = $derived(logbook.instruments.filter((i) => i.archived_at !== null));
    const current = $derived(
        selected === undefined || selected === 'new' ? undefined : logbook.instrument(selected)
    );

    let fromList = $state(false);
    afterNavigate(({ from }) => {
        fromList = from?.url.pathname === root;
    });

    function back(e: MouseEvent) {
        if (!fromList) return;
        e.preventDefault();
        history.back();
    }

    async function saved(instrument: Instrument) {
        await logbook.loadInstruments();
        if (selected !== instrument.id) goto(`${root}/${instrument.id}`);
    }
</script>

<PageTitle
    title={current === undefined
        ? m.nav_instruments()
        : m.stations_instrument_page_title({ name: current.name })}
/>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header
        class={['flex flex-wrap items-center gap-3', selected !== undefined && 'max-md:hidden']}
    >
        <a
            href={`/logbooks/${logbookId}/more`}
            aria-label={m.settings_back_more()}
            class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text md:hidden"
            ><Icon name="back" size={22} /></a
        >
        <h1 class="text-xl font-semibold">{m.nav_instruments()}</h1>
        <span class="text-[13px] text-muted"
            >{m.stations_counts({
                active: logbook.activeInstruments.length,
                archived: archived.length
            })}</span
        >
        <div class="flex-1"></div>
        <Button variant="primary" href={`${root}/new`}>{m.stations_instrument_new()}</Button>
    </header>

    {#if selected !== undefined}
        <header class="flex items-center gap-2 md:hidden">
            <a
                href={root}
                onclick={back}
                aria-label={m.stations_instrument_back()}
                class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text"
                ><Icon name="back" size={22} /></a
            >
            <h1 class="truncate text-lg font-semibold">
                {selected === 'new'
                    ? m.stations_instrument_new()
                    : (current?.name ?? m.field_instrument())}
            </h1>
        </header>
    {/if}

    <div class="grid items-start gap-4 md:grid-cols-[minmax(240px,320px)_minmax(0,1fr)]">
        <PickList
            label={m.stations_instrument_list()}
            items={logbook.instruments}
            {selected}
            href={(instrument) => `${root}/${instrument.id}`}
            detail={(instrument) => (instrument.tx_pwr === null ? '' : `${instrument.tx_pwr} W`)}
            search={(instrument) => [
                instrument.name,
                instrument.my_rig,
                instrument.my_antenna,
                instrument.tx_pwr
            ]}
            placeholder={m.stations_instrument_filter_placeholder()}
            empty={m.stations_instrument_no_active()}
            class={selected !== undefined ? 'max-md:hidden' : ''}
        />

        <div class="flex min-w-0 flex-col gap-4">
            {#if selected === undefined}
                <p class="hidden rounded-lg border border-border p-6 text-muted md:block">
                    {m.stations_instrument_choose()}
                </p>
            {:else if selected !== 'new' && current === undefined}
                <p class="text-muted">
                    {logbook.instruments.length === 0
                        ? m.common_loading()
                        : m.stations_instrument_not_found()}
                </p>
            {:else}
                {#key selected}
                    <InstrumentEditor
                        {logbookId}
                        instrument={current}
                        {canRewrite}
                        onsaved={saved}
                    />
                {/key}
            {/if}
        </div>
    </div>
</main>
