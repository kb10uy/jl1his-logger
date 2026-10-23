<script lang="ts" module>
    const PATHS = {
        log: 'M4 6h16M4 12h16M4 18h10',
        entry: 'M3 11a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM7 9l10-6M7 14h3M14 14h3',
        voice: 'M9 6a3 3 0 0 1 6 0v6a3 3 0 0 1-6 0zM5 11a7 7 0 0 0 14 0M12 18v3',
        stations:
            'M12 21s7-6.2 7-11a7 7 0 0 0-14 0c0 4.8 7 11 7 11zM12 7.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5z',
        qsl: 'M5 5h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM3 8l9 6 9-6',
        transfer: 'M12 3v12M7 10l5 5 5-5M4 21h16',
        settings:
            'M12 9a3 3 0 1 1 0 6 3 3 0 0 1 0-6zM12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1',
        members:
            'M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21a7 7 0 0 1 14 0M17 4a4 4 0 0 1 0 7M22 21a7 7 0 0 0-4-6.3',
        more: 'M5 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2zM12 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2zM19 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2z',
        close: 'M6 6l12 12M18 6L6 18',
        search: 'M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM20 20l-3.5-3.5',
        warning: 'M12 3L2 21h20L12 3zM12 10v5M12 18v1',
        back: 'M15 5l-7 7 7 7',
        chevron: 'M6 9l6 6 6-6',
        offline:
            'M2 2l20 20M5 12.5a10 10 0 0 1 3.4-2.3M12 8a10 10 0 0 1 7 4.5M8.5 16a5 5 0 0 1 7 0M12 20h.01',
        spinner: 'M12 3a9 9 0 1 0 9 9',
        check: 'M5 12l5 5 9-10',
        signout: 'M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 17l-5-5 5-5M5 12h11'
    } as const;

    export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
    let {
        name,
        size = 18,
        strokeWidth = 2,
        class: className = ''
    }: { name: IconName; size?: number; strokeWidth?: number; class?: string } = $props();
</script>

<svg
    aria-hidden="true"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width={strokeWidth}
    stroke-linecap="round"
    stroke-linejoin="round"
    class={[name === 'spinner' && 'animate-spin', 'shrink-0', className]}
>
    <path d={PATHS[name]} />
</svg>
