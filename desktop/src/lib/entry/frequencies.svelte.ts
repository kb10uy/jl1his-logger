import type { FavoriteFrequency, FrequencyInput } from '#client/api/types.ts';
import { byFrequency } from '#client/qso/favorites.ts';
import * as api from '#lib/api/api.ts';

class FavoriteFrequencies {
    items = $state<FavoriteFrequency[]>([]);
    failed = $state(false);

    async load() {
        try {
            this.items = (await api.favoriteFrequencies()).sort(byFrequency);
            this.failed = false;
        } catch {
            this.failed = true;
        }
    }

    async add(input: FrequencyInput) {
        const added = await api.addFavoriteFrequency(input);
        this.items = [...this.items.filter((f) => f.id !== added.id), added].sort(byFrequency);
    }

    async remove(favorite: FavoriteFrequency) {
        await api.removeFavoriteFrequency(favorite.id);
        this.items = this.items.filter((f) => f.id !== favorite.id);
    }
}

export const favoriteFrequencies = new FavoriteFrequencies();
