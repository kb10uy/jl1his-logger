import * as api from '#lib/api/api.ts';
import type { Subdivision } from '#lib/api/types.ts';

const cache = new Map<string, Promise<Subdivision[]>>();

export async function subdivisionsOf(dxcc: number): Promise<Subdivision[]> {
    const versions = await api.references();
    const version = versions.find((v) => v.kind === 'subdivisions')?.version;
    if (version === undefined) return [];
    const key = `${version}:${dxcc}`;
    let found = cache.get(key);
    if (found === undefined) {
        found = api.subdivisions(dxcc);
        cache.set(key, found);
        found.catch(() => cache.delete(key));
    }
    return found;
}
