import type { Candidate, Search } from '#client/api/api.ts';
import { query } from '#client/api/client.ts';
import type {
    FavoriteFrequency,
    Fields,
    FrequencyInput,
    Instrument,
    Lookup,
    Page,
    Qso,
    Saved,
    Station
} from '#client/api/types.ts';
import { request } from './request.svelte.ts';

const base = (logbook: string) => `/api/logbooks/${encodeURIComponent(logbook)}`;

export const stations = (logbook: string) => request<Station[]>('GET', `${base(logbook)}/stations`);

export const instruments = (logbook: string) =>
    request<Instrument[]>('GET', `${base(logbook)}/instruments`);

export const searchQsos = (logbook: string, search: Search) =>
    request<Page>('GET', `${base(logbook)}/qsos${query({ ...search })}`);

export const createQso = (
    logbook: string,
    id: string,
    stationId: string,
    instrumentId: string | null,
    fields: Fields
) =>
    request<Saved>('POST', `${base(logbook)}/qsos`, {
        id,
        station_id: stationId,
        instrument_id: instrumentId,
        fields
    });

export const duplicates = (logbook: string, candidate: Candidate) =>
    request<{ qsos: Qso[] }>('GET', `${base(logbook)}/duplicates${query({ ...candidate })}`);

export const lookup = (logbook: string, call: string) =>
    request<Lookup>('GET', `${base(logbook)}/lookup${query({ call })}`);

export const favoriteFrequencies = () => request<FavoriteFrequency[]>('GET', '/api/frequencies');

export const addFavoriteFrequency = (input: FrequencyInput) =>
    request<FavoriteFrequency>('POST', '/api/frequencies', input);

export const removeFavoriteFrequency = (id: string) =>
    request<null>('DELETE', `/api/frequencies/${encodeURIComponent(id)}`);
