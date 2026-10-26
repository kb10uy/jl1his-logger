import { describe, expect, it } from 'vitest';
import { splitFavorites } from './favorite.ts';

const item = (name: string, favorite: boolean) => ({ name, favorite });

describe('splitFavorites', () => {
    it('keeps the order within each group', () => {
        const { favorites, others } = splitFavorites([
            item('Home', true),
            item('Park', false),
            item('Club', true),
            item('Mobile', false)
        ]);
        expect(favorites.map((i) => i.name)).toEqual(['Home', 'Club']);
        expect(others.map((i) => i.name)).toEqual(['Park', 'Mobile']);
    });

    it('leaves everything in the others without favorites', () => {
        const items = [item('Home', false), item('Park', false)];
        expect(splitFavorites(items)).toEqual({ favorites: [], others: items });
    });
});
