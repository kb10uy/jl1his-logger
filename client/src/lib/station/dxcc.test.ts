import { describe, expect, it } from 'vitest';
import { dxccEntity, searchDxcc } from './dxcc.ts';

describe('DXCC entities', () => {
    it('finds an entity by its code', () => {
        expect(dxccEntity('339')?.name).toBe('JAPAN');
        expect(dxccEntity(' 192 ')?.name).toBe('OGASAWARA');
        expect(dxccEntity('2')).toEqual({ code: 2, name: 'ABU AIL IS.', deleted: true });
        expect(dxccEntity('1000')).toBeUndefined();
        expect(dxccEntity('Japan')).toBeUndefined();
        expect(dxccEntity('')).toBeUndefined();
    });

    it('searches names ignoring case, best matches first', () => {
        expect(searchDxcc('japan').map((e) => e.code)).toEqual([339]);
        expect(searchDxcc('ogasa').map((e) => e.code)).toEqual([192]);
        expect(searchDxcc('united').map((e) => e.name)).toContain('UNITED STATES OF AMERICA');
        expect(searchDxcc('russia').map((e) => e.name)).toEqual(
            expect.arrayContaining(['ASIATIC RUSSIA', 'EUROPEAN RUSSIA'])
        );
        expect(searchDxcc('339')[0]?.code).toBe(339);
    });

    it('ranks a name prefix before a word prefix and a word prefix before a substring', () => {
        const names = searchDxcc('guinea', 20).map((e) => e.name);
        expect(names[0]).toBe('GUINEA');
        expect(names.indexOf('PAPUA NEW GUINEA')).toBeGreaterThan(0);
        const iran = searchDxcc('iran', 20).map((e) => e.name);
        expect(iran[0]).toBe('IRAN');
    });

    it('leaves out deleted entities and limits the results', () => {
        expect(searchDxcc('abu ail')).toEqual([]);
        expect(searchDxcc('is', 5)).toHaveLength(5);
        expect(searchDxcc('  ')).toEqual([]);
    });
});
