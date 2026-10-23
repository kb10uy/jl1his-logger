import { query, request } from './client.ts';
import type {
    Batch,
    Exported,
    ExportKind,
    Fields,
    ImportSource,
    Instrument,
    LengthUnit,
    Logbook,
    LogbookRole,
    Lookup,
    Me,
    Member,
    MergeReport,
    NewReport,
    Outcome,
    Page,
    Passkey,
    Qso,
    ReferenceVersion,
    Saved,
    Station,
    StationInput,
    Subdivision,
    Target,
    Token,
    TokenKind
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

export const passkeys = () => request<Passkey[]>('GET', '/api/passkeys');

export async function addPasskey(name: string): Promise<Passkey> {
    const { ceremony, options } = await request<Ceremony>('POST', '/api/passkeys/register/start');
    const credential = (await navigator.credentials.create(
        creationOptions(options)
    )) as PublicKeyCredential | null;
    if (credential === null) throw new Error('No passkey was created.');
    return request<Passkey>('POST', '/api/passkeys/register/finish', {
        body: { ceremony, credential: registrationJson(credential), name }
    });
}

export const deletePasskey = (id: string) =>
    request<void>('DELETE', `/api/passkeys/${encodeURIComponent(id)}`);

export const references = () => request<ReferenceVersion[]>('GET', '/api/reference');

export const subdivisions = (dxcc: number) =>
    request<Subdivision[]>('GET', `/api/reference/subdivisions/${dxcc}`);

export const tokens = () => request<Token[]>('GET', '/api/tokens');

export interface TokenRequest {
    kind: TokenKind;
    name: string;
    logbook_ids: string[];
    wl2_prefix?: boolean;
}

export const issueToken = (token: TokenRequest) =>
    request<{ token: Token; plaintext: string }>('POST', '/api/tokens', { body: token });

export const revokeToken = (id: string) =>
    request<void>('DELETE', `/api/tokens/${encodeURIComponent(id)}`);

export const logbooks = () => request<Logbook[]>('GET', '/api/logbooks');

const base = (logbook: string) => `/api/logbooks/${encodeURIComponent(logbook)}`;

export const stations = (logbook: string) => request<Station[]>('GET', `${base(logbook)}/stations`);

const station = (logbook: string, id: string) =>
    `${base(logbook)}/stations/${encodeURIComponent(id)}`;

export const createStation = (logbook: string, input: StationInput) =>
    request<Station>('POST', `${base(logbook)}/stations`, { body: input });

export const updateStation = (logbook: string, id: string, input: StationInput) =>
    request<Station>('PUT', station(logbook, id), { body: input });

export const archiveStation = (logbook: string, id: string, archived: boolean) =>
    request<Station>(archived ? 'POST' : 'DELETE', `${station(logbook, id)}/archive`);

export const instruments = (logbook: string) =>
    request<Instrument[]>('GET', `${base(logbook)}/instruments`);

export interface Range {
    from?: string;
    until?: string;
}

export const previewRewrite = (logbook: string, id: string, range: Range) =>
    request<{ count: number }>('GET', `${station(logbook, id)}/rewrite${query({ ...range })}`);

export const rewrite = (logbook: string, id: string, range: Range, count: number) =>
    request<{ count: number }>('POST', `${station(logbook, id)}/rewrite`, {
        body: { ...range, count }
    });

export const members = (logbook: string) => request<Member[]>('GET', `${base(logbook)}/members`);

export const setMember = (logbook: string, callsign: string, role: LogbookRole) =>
    request<Member[]>('PUT', `${base(logbook)}/members/${encodeURIComponent(callsign)}`, {
        body: { role }
    });

export const removeMember = (logbook: string, callsign: string) =>
    request<void>('DELETE', `${base(logbook)}/members/${encodeURIComponent(callsign)}`);

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

export interface ExportRequest {
    kind: ExportKind;
    q?: string;
    park?: string;
    length?: LengthUnit;
}

export const createExport = (logbook: string, req: ExportRequest) =>
    request<Exported>('POST', `${base(logbook)}/exports`, { body: req });

export const batches = (logbook: string) => request<Batch[]>('GET', `${base(logbook)}/exports`);

export const applyBatch = (logbook: string, id: string) =>
    request<Outcome>('POST', `${base(logbook)}/exports/${encodeURIComponent(id)}/apply`);

export interface ImportRequest {
    station_id?: string;
    format: 'adi' | 'adx';
    length?: LengthUnit;
    apply: boolean;
}

export function importAdif(
    logbook: string,
    source: 'new',
    req: ImportRequest,
    file: Blob
): Promise<NewReport>;
export function importAdif(
    logbook: string,
    source: Exclude<ImportSource, 'new'>,
    req: ImportRequest,
    file: Blob
): Promise<MergeReport>;
export function importAdif(logbook: string, source: ImportSource, req: ImportRequest, file: Blob) {
    const params = query({ source, ...req, apply: req.apply ? 'true' : undefined });
    return request<NewReport | MergeReport>('POST', `${base(logbook)}/imports${params}`, {
        raw: file
    });
}

export const eventsUrl = (logbook: string) => `${base(logbook)}/events`;
