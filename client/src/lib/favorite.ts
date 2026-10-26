export function splitFavorites<T extends { favorite: boolean }>(
    items: T[]
): { favorites: T[]; others: T[] } {
    return {
        favorites: items.filter((item) => item.favorite),
        others: items.filter((item) => !item.favorite)
    };
}
