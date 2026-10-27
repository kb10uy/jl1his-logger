import { ADIF_DXCC_ENTITIES, type AdifDxccEntity } from '#lib/adif/tables.ts';

export function dxccEntity(code: string): AdifDxccEntity | undefined {
    const trimmed = code.trim();
    if (!/^\d+$/.test(trimmed)) return undefined;
    const number = Number(trimmed);
    return ADIF_DXCC_ENTITIES.find((e) => e.code === number);
}

function rank(entity: AdifDxccEntity, query: string): number | undefined {
    if (String(entity.code) === query) return 0;
    const name = entity.name.toUpperCase();
    if (name.startsWith(query)) return 1;
    if (name.split(/[^A-Z0-9]+/).some((word) => word.startsWith(query))) return 2;
    if (name.includes(query)) return 3;
    return undefined;
}

export function searchDxcc(query: string, limit = 8): AdifDxccEntity[] {
    const q = query.trim().toUpperCase();
    if (q === '') return [];
    return ADIF_DXCC_ENTITIES.flatMap((entity) => {
        const r = entity.deleted ? undefined : rank(entity, q);
        return r === undefined ? [] : [{ entity, r }];
    })
        .sort((a, b) => a.r - b.r || a.entity.name.localeCompare(b.entity.name))
        .slice(0, limit)
        .map(({ entity }) => entity);
}
