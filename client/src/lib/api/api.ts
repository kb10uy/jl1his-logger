import { query, request } from './client.ts';
import type {
    Fields,
    Logbook,
    Lookup,
    Me,
    Outcome,
    Page,
    Qso,
    Saved,
    Station,
    Target
} from './types.ts';
import {
    authenticationJson,
    creationOptions,
    registrationJson,
    requestOptions
} from './webauthn.ts';

interface Ceremony {
    ceremony: string;
    options: unknown;
}

export const me = () => request<Me>('GET', '/api/me');

export async function signIn(): Promise<Me> {
    const { ceremony, options } = await request<Ceremony>('POST', '/api/auth/login/start');
    const credential = (await navigator.credentials.get(
        requestOptions(options)
    )) as PublicKeyCredential | null;
    if (credential === null) throw new Error('No passkey was chosen.');
    return request<Me>('POST', '/api/auth/login/finish', {
        body: { ceremony, credential: authenticationJson(credential) }
    });
}

export async function startRegistration(token: string) {
    return request<Ceremony & { callsign: string }>('POST', '/api/auth/register/start', {
        body: { token }
    });
}

export async function finishRegistration(
    ceremony: Ceremony & { callsign: string },
    name: string
): Promise<Me> {
    const credential = (await navigator.credentials.create(
        creationOptions(ceremony.options)
    )) as PublicKeyCredential | null;
    if (credential === null) throw new Error('No passkey was created.');
    return request<Me>('POST', '/api/auth/register/finish', {
        body: { ceremony: ceremony.ceremony, credential: registrationJson(credential), name }
    });
}

export const signOut = () => request<void>('POST', '/api/auth/logout');

export const logbooks = () => request<Logbook[]>('GET', '/api/logbooks');

const base = (logbook: string) => `/api/logbooks/${encodeURIComponent(logbook)}`;

export const stations = (logbook: string) => request<Station[]>('GET', `${base(logbook)}/stations`);

export interface Search {
    q?: string;
    station_id?: string;
    cursor?: string | null;
    limit?: number;
}

export const searchQsos = (logbook: string, search: Search, signal?: AbortSignal) =>
    request<Page>('GET', `${base(logbook)}/qsos${query({ ...search })}`, { signal });

export const targets = (logbook: string, q: string) =>
    request<{ qsos: Target[] }>('GET', `${base(logbook)}/targets${query({ q })}`);

export const getQso = (logbook: string, id: string) =>
    request<Qso>('GET', `${base(logbook)}/qsos/${encodeURIComponent(id)}`);

export const createQso = (logbook: string, id: string, stationId: string, fields: Fields) =>
    request<Saved>('POST', `${base(logbook)}/qsos`, {
        body: { id, station_id: stationId, fields }
    });

export const updateQso = (
    logbook: string,
    id: string,
    revision: number,
    stationId: string,
    fields: Fields
) =>
    request<Saved>('PUT', `${base(logbook)}/qsos/${encodeURIComponent(id)}`, {
        body: { station_id: stationId, fields },
        revision
    });

export const deleteQso = (logbook: string, id: string, revision: number) =>
    request<Qso>('DELETE', `${base(logbook)}/qsos/${encodeURIComponent(id)}`, { revision });

export interface Candidate {
    call: string;
    band: string;
    mode: string;
    qso_date: string;
    time_on: string;
}

export const duplicates = (logbook: string, candidate: Candidate, signal?: AbortSignal) =>
    request<{ qsos: Qso[] }>('GET', `${base(logbook)}/duplicates${query({ ...candidate })}`, {
        signal
    });

export const lookup = (logbook: string, call: string, signal?: AbortSignal) =>
    request<Lookup>('GET', `${base(logbook)}/lookup${query({ call })}`, { signal });

export const updateQsl = (logbook: string, qsos: Target[], fields: Fields) =>
    request<Outcome>('POST', `${base(logbook)}/qsl`, { body: { qsos, fields } });

export const eventsUrl = (logbook: string) => `${base(logbook)}/events`;
