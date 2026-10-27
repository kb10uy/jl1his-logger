import { describe, expect, it } from 'vitest';
import { gridSquare } from './grid.ts';

describe('grid squares', () => {
    it.each([
        [36.4, 138.38, 'PM96ej'],
        [35.6895, 139.6917, 'PM95uq'],
        [27.0944, 142.1917, 'QL17cc'],
        [-33.8688, 151.2093, 'QF56od'],
        [40.7128, -74.006, 'FN20xr'],
        [0, 0, 'JJ00aa'],
        [-90, -180, 'AA00aa'],
        [90, 180, 'RR99xx']
    ])('locates %f, %f in %s', (latitude, longitude, expected) => {
        expect(gridSquare(latitude, longitude)).toBe(expected);
    });

    it('refuses positions outside the globe', () => {
        expect(gridSquare(91, 0)).toBeUndefined();
        expect(gridSquare(0, -181)).toBeUndefined();
        expect(gridSquare(Number.NaN, 0)).toBeUndefined();
    });
});
