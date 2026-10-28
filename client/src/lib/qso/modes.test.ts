import { describe, expect, it } from 'vitest';
import { defaultRst } from './modes.ts';

describe('defaultRst', () => {
    it.each([
        ['AM', '', '59'],
        ['FM', '', '59'],
        ['SSB', 'USB', '59'],
        ['DIGITALVOICE', 'DSTAR', '59'],
        ['CW', '', '599'],
        ['RTTY', '', '599'],
        ['PSK', 'PSK31', '599'],
        ['OLIVIA', '', '599'],
        ['MFSK', 'MFSK16', '599'],
        ['MFSK', 'MFSK32', '599'],
        ['SSTV', '', '595'],
        ['FT8', '', '-10'],
        ['MFSK', 'FT4', '-10'],
        ['MFSK', 'JS8', '-10'],
        ['JT65', '', '-10'],
        ['JT9', '', '-10']
    ])('gives %s %s the report %s', (mode, submode, report) => {
        expect(defaultRst(mode, submode)).toBe(report);
    });
});
