<script lang="ts" module>
    import AlertTriangle from '@tabler/icons-svelte-runes/icons/alert-triangle';
    import Antenna from '@tabler/icons-svelte-runes/icons/antenna';
    import ArrowNarrowDown from '@tabler/icons-svelte-runes/icons/arrow-narrow-down';
    import ArrowNarrowUp from '@tabler/icons-svelte-runes/icons/arrow-narrow-up';
    import ArrowsSort from '@tabler/icons-svelte-runes/icons/arrows-sort';
    import Book2 from '@tabler/icons-svelte-runes/icons/book-2';
    import Check from '@tabler/icons-svelte-runes/icons/check';
    import ChevronDown from '@tabler/icons-svelte-runes/icons/chevron-down';
    import ChevronLeft from '@tabler/icons-svelte-runes/icons/chevron-left';
    import Columns3 from '@tabler/icons-svelte-runes/icons/columns-3';
    import Dots from '@tabler/icons-svelte-runes/icons/dots';
    import GripVertical from '@tabler/icons-svelte-runes/icons/grip-vertical';
    import LayoutSidebarLeftCollapse from '@tabler/icons-svelte-runes/icons/layout-sidebar-left-collapse';
    import LayoutSidebarLeftExpand from '@tabler/icons-svelte-runes/icons/layout-sidebar-left-expand';
    import List from '@tabler/icons-svelte-runes/icons/list';
    import Loader2 from '@tabler/icons-svelte-runes/icons/loader-2';
    import Logout from '@tabler/icons-svelte-runes/icons/logout';
    import Mail from '@tabler/icons-svelte-runes/icons/mail';
    import MapPin from '@tabler/icons-svelte-runes/icons/map-pin';
    import Microphone from '@tabler/icons-svelte-runes/icons/microphone';
    import Plus from '@tabler/icons-svelte-runes/icons/plus';
    import Radio from '@tabler/icons-svelte-runes/icons/radio';
    import Search from '@tabler/icons-svelte-runes/icons/search';
    import Settings from '@tabler/icons-svelte-runes/icons/settings';
    import Star from '@tabler/icons-svelte-runes/icons/star';
    import StarFilled from '@tabler/icons-svelte-runes/icons/star-filled';
    import TransferVertical from '@tabler/icons-svelte-runes/icons/transfer-vertical';
    import Users from '@tabler/icons-svelte-runes/icons/users';
    import WifiOff from '@tabler/icons-svelte-runes/icons/wifi-off';
    import X from '@tabler/icons-svelte-runes/icons/x';

    const ICONS = {
        log: List,
        entry: Radio,
        voice: Microphone,
        stations: MapPin,
        instruments: Antenna,
        qsl: Mail,
        transfer: TransferVertical,
        settings: Settings,
        members: Users,
        more: Dots,
        close: X,
        search: Search,
        warning: AlertTriangle,
        back: ChevronLeft,
        chevron: ChevronDown,
        grip: GripVertical,
        columns: Columns3,
        offline: WifiOff,
        spinner: Loader2,
        check: Check,
        plus: Plus,
        signout: Logout,
        logbook: Book2,
        collapse: LayoutSidebarLeftCollapse,
        expand: LayoutSidebarLeftExpand,
        star: Star,
        starred: StarFilled,
        ascending: ArrowNarrowUp,
        descending: ArrowNarrowDown,
        sortable: ArrowsSort
    } as const;

    export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
    let {
        name,
        size = 18,
        strokeWidth = 2,
        class: className = ''
    }: { name: IconName; size?: number; strokeWidth?: number; class?: string } = $props();

    const Component = $derived(ICONS[name]);
</script>

<Component
    aria-hidden="true"
    {size}
    stroke={strokeWidth}
    class={['shrink-0', name === 'spinner' && 'animate-spin', className].filter(Boolean).join(' ')}
/>
