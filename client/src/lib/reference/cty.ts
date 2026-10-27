import { m } from '#lib/paraglide/messages.js';

export const MAX_CTY_ARCHIVE_MIB = 4;

export function ctyArchiveProblem(file: { name: string; size: number }): string | undefined {
    if (!file.name.toLowerCase().endsWith('.zip')) return m.settings_reference_cty_not_zip();
    if (file.size > MAX_CTY_ARCHIVE_MIB * 1024 * 1024) {
        return m.settings_reference_cty_too_large({ max: MAX_CTY_ARCHIVE_MIB });
    }
    return undefined;
}
