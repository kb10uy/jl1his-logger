import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { DEFAULT_COLUMNS } from '#lib/log/columns.ts';
import { compact, entry, flag, logColumns, theme, voice, withEntryDefaults } from './device.ts';

describe('device settings', () => {
    it('reads flags written before they were unified', () => {
        const setting = flag('x');
        expect(setting.read('true')).toBe(true);
        expect(setting.read('1')).toBe(true);
        expect(setting.read('false')).toBe(false);
        expect(setting.read(null)).toBe(false);
        expect(setting.write(true)).toBe('1');
        expect(setting.write(false)).toBeNull();
    });

    it('reads only known themes', () => {
        expect(theme.read('dark')).toBe('dark');
        expect(theme.read('sepia')).toBeNull();
    });

    it('fills what an older version did not store', () => {
        expect(logColumns.read(null)).toEqual([...DEFAULT_COLUMNS]);
        expect(voice.read('{"language":"xx"}').language).toMatch(/^(en|ja)$/);
        const settings = entry('log').read('{"band":"20m","contest":{"enabled":true}}');
        expect(settings.band).toBe('20m');
        expect(settings.contest.enabled).toBe(true);
        expect(settings.contest).toEqual({ ...withEntryDefaults({}).contest, enabled: true });
    });

    it('keeps the keys the pre-paint script reads', () => {
        const script = readFileSync('static/theme.js', 'utf8');
        expect(script).toContain(`'${theme.key}'`);
        expect(script).toContain(`'${compact.key}'`);
        expect(script).toContain(`=== '${compact.write(true)}'`);
    });
});
