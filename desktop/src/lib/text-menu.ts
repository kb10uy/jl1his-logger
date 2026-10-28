import { Menu } from '@tauri-apps/api/menu';
import { m } from '#lib/paraglide/messages.js';

type Kind = 'field' | 'text';

const TEXT_INPUTS = new Set(['text', 'search', 'url', 'email', 'tel', 'password', 'number']);

const menus = new Map<Kind, Promise<Menu>>();

function kindOf(target: EventTarget | null): Kind | undefined {
    if (target instanceof HTMLInputElement)
        return TEXT_INPUTS.has(target.type) ? 'field' : undefined;
    if (target instanceof HTMLTextAreaElement) return 'field';
    if (!(target instanceof HTMLElement) || target instanceof HTMLSelectElement) return undefined;
    if (target.isContentEditable) return 'field';
    if (getComputedStyle(target).userSelect === 'none') return undefined;
    return window.getSelection()?.isCollapsed === false ? 'text' : undefined;
}

function build(kind: Kind): Promise<Menu> {
    const copy = { item: 'Copy', text: m.menu_copy() } as const;
    if (kind === 'text') return Menu.new({ items: [copy] });
    return Menu.new({
        items: [
            { item: 'Cut', text: m.menu_cut() },
            copy,
            { item: 'Paste', text: m.menu_paste() },
            { item: 'Separator' },
            { item: 'SelectAll', text: m.menu_select_all() }
        ]
    });
}

export function openTextMenu(e: MouseEvent) {
    e.preventDefault();
    const kind = kindOf(e.target);
    if (kind === undefined) return;
    let menu = menus.get(kind);
    if (menu === undefined) {
        menu = build(kind);
        menus.set(kind, menu);
    }
    menu.then((built) => built.popup()).catch(() => menus.delete(kind));
}
