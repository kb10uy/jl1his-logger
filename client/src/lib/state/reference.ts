import * as api from '#lib/api/api.ts';
import type { Subdivision } from '#lib/api/types.ts';

const cache = new Map<number, Promise<Subdivision[]>>();

export function subdivisionsOf(dxcc: number): Promise<Subdivision[]> {
    let found = cache.get(dxcc);
    if (found === undefined) {
        found = api.subdivisions(dxcc);
        cache.set(dxcc, found);
        found.catch(() => cache.delete(dxcc));
    }
    return found;
}
