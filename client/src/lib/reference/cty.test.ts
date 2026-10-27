import { describe, expect, it } from 'vitest';
import { ctyArchiveProblem } from './cty.ts';

describe('CTY archive uploads', () => {
    it.each([
        [{ name: 'bigcty-20261023.zip', size: 330_000 }, undefined],
        [{ name: 'CTY-3633.ZIP', size: 1_400_000 }, undefined],
        [{ name: 'cty.zip', size: 4 * 1024 * 1024 }, undefined],
        [{ name: 'cty.zip', size: 4 * 1024 * 1024 + 1 }, 'The archive is larger than 4 MiB.'],
        [{ name: 'cty.dat', size: 100_000 }, 'Choose a .zip file.'],
        [{ name: 'cty.zip.dat', size: 100_000 }, 'Choose a .zip file.']
    ])('judges %o', (file, problem) => {
        expect(ctyArchiveProblem(file)).toBe(problem);
    });
});
