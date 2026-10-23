<script lang="ts">
    import { goto } from '$app/navigation';
    import type { Instrument } from '#lib/api/types.ts';
    import PageTitle from '#lib/components/PageTitle.svelte';
    import Button from '#lib/components/ui/Button.svelte';
    import Icon from '#lib/components/ui/Icon.svelte';
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

    async function saved(instrument: Instrument) {
        await logbook.loadInstruments();
        if (selected !== instrument.id) goto(`${root}/${instrument.id}`);
    }

    const summary = (instrument: Instrument) =>
        [instrument.my_rig, instrument.my_antenna].filter((v) => v !== null).join(' · ');
</script>

<PageTitle title={current === undefined ? 'Instruments' : `${current.name} - Instruments`} />

{#snippet card(instrument: Instrument)}
    <a
        href={`${root}/${instrument.id}`}
        aria-current={instrument.id === selected ? 'page' : undefined}
        class={[
            'flex flex-col gap-1.5 rounded-lg border px-3.5 py-3 text-text no-underline',
            instrument.id === selected
                ? 'border-accent bg-warn-surface shadow-[inset_3px_0_0_var(--accent)]'
                : 'border-border bg-surface'
        ]}
    >
        <span class="flex items-baseline justify-between gap-2">
            <span class="truncate font-semibold">{instrument.name}</span>
            {#if instrument.tx_pwr !== null}
                <span class="font-mono text-xs text-muted">{instrument.tx_pwr} W</span>
            {/if}
        </span>
        {#if summary(instrument) !== ''}
            <span class="truncate text-[13px] text-muted">{summary(instrument)}</span>
        {/if}
    </a>
{/snippet}

<main class="flex min-w-0 flex-col gap-4 px-4 pt-4 pb-6 md:px-6">
    <header
        class={['flex flex-wrap items-center gap-3', selected !== undefined && 'max-md:hidden']}
    >
        <a
            href={`/logbooks/${logbookId}/more`}
            aria-label="Back to More"
            class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text md:hidden"
            ><Icon name="back" size={22} /></a
        >
        <h1 class="text-xl font-semibold">Instruments</h1>
        <span class="text-[13px] text-muted"
            >{logbook.activeInstruments.length} active · {archived.length} archived</span
        >
        <div class="flex-1"></div>
        <Button variant="primary" href={`${root}/new`}>New Instrument</Button>
    </header>

    {#if selected !== undefined}
        <header class="flex items-center gap-2 md:hidden">
            <a
                href={root}
                aria-label="Back to Instruments"
                class="-ml-2 inline-flex size-10 items-center justify-center rounded-lg text-text"
                ><Icon name="back" size={22} /></a
            >
            <h1 class="truncate text-lg font-semibold">
                {selected === 'new' ? 'New Instrument' : (current?.name ?? 'Instrument')}
            </h1>
        </header>
    {/if}

    <div class="flex flex-wrap items-start gap-4">
        <section
            aria-label="Instrument list"
            class={[
                'flex min-w-0 flex-[1_1_280px] flex-col gap-2 md:max-w-[340px]',
                selected !== undefined && 'max-md:hidden'
            ]}
        >
            {#each logbook.activeInstruments as instrument (instrument.id)}
                {@render card(instrument)}
            {:else}
                <p class="text-[13px] text-muted">No active Instrument yet.</p>
            {/each}
            {#if archived.length > 0}
                <h2 class="mt-3 eyebrow">Archived</h2>
                {#each archived as instrument (instrument.id)}
                    <a
                        href={`${root}/${instrument.id}`}
                        aria-current={instrument.id === selected ? 'page' : undefined}
                        class={[
                            'flex items-center gap-2 rounded-lg border px-3.5 py-2.5 text-[13px] text-muted no-underline',
                            instrument.id === selected ? 'border-accent' : 'border-border'
                        ]}
                    >
                        <span class="truncate text-text">{instrument.name}</span>
                        {#if instrument.my_rig !== null}
                            · <span class="truncate">{instrument.my_rig}</span>
                        {/if}
                    </a>
                {/each}
            {/if}
        </section>

        <div class="flex min-w-0 flex-[999_1_480px] flex-col gap-4">
            {#if selected === undefined}
                <p class="hidden rounded-lg border border-border p-6 text-muted md:block">
                    Choose an Instrument to see its values, or create one.
                </p>
            {:else if selected !== 'new' && current === undefined}
                <p class="text-muted">
                    {logbook.instruments.length === 0
                        ? 'Loading…'
                        : 'This Instrument does not exist.'}
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
