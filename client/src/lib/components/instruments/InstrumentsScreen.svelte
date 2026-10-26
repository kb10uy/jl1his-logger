<script lang="ts">
    import { MediaQuery } from 'svelte/reactivity';
    import { afterNavigate, goto } from '$app/navigation';
    import * as api from '#lib/api/api.ts';
    import type { Instrument } from '#lib/api/types.ts';
    import PageHeader from '#lib/components/PageHeader.svelte';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import LogbookLoadError from '#lib/components/LogbookLoadError.svelte';
    import PickList from '#lib/components/PickList.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import { useLogbook } from '#lib/state/logbook.svelte.ts';
    import { toasts } from '#lib/state/toast.svelte.ts';
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

    const desktop = new MediaQuery('min-width: 768px');
    const detail = $derived(selected !== undefined && !desktop.current);

    let fromList = $state(false);
    afterNavigate(({ from }) => {
        fromList = from?.url.pathname === root;
    });

    function back(e: MouseEvent) {
        if (!fromList) return;
        e.preventDefault();
        history.back();
    }

    async function favorite(instrument: Instrument, favorite: boolean) {
        try {
            await api.favoriteInstrument(logbookId, instrument.id, favorite);
            await logbook.loadInstruments();
        } catch (e) {
            toasts.error(e instanceof Error ? e.message : String(e));
        }
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

<PageHeader
    title={detail
        ? selected === 'new'
            ? m.stations_instrument_new()
            : (current?.name ?? m.field_instrument())
        : m.nav_instruments()}
    subtitle={detail
        ? `${logbook.logbook.name} · ${m.nav_instruments()}`
        : `${logbook.logbook.name} · ${m.stations_counts({
              active: logbook.activeInstruments.length,
              archived: archived.length
          })}`}
    back={detail
        ? { href: root, onclick: back, label: m.stations_instrument_back() }
        : { href: `/logbooks/${logbookId}/more`, label: m.settings_back_more() }}
>
    {#snippet actions()}
        {#if selected === undefined || desktop.current}
            <Button variant="primary" href={`${root}/new`} class="max-md:w-9 max-md:px-0"
                ><Icon name="plus" /><span class="max-md:sr-only"
                    >{m.stations_instrument_new()}</span
                ></Button
            >
        {/if}
    {/snippet}
</PageHeader>

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <LogbookLoadError />
    <div class="grid items-start gap-4 md:grid-cols-[minmax(240px,320px)_minmax(0,1fr)]">
        <PickList
            label={m.stations_instrument_list()}
            items={logbook.instruments}
            {selected}
            onfavorite={favorite}
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
                    {logbook.loaded ? m.stations_instrument_not_found() : m.common_loading()}
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
