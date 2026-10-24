import { m } from '#lib/paraglide/messages.js';

export function isPower(value: string): boolean {
    return /^(\d+(\.\d{0,6})?|\.\d{1,6})$/.test(value) && Number(value) > 0;
}

export function powerRule(): string {
    return m.entry_power_rule();
}
