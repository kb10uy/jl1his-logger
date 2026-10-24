import { errorText } from './errors.ts';
import { m } from '#lib/paraglide/messages.js';

export class ApiError extends Error {
    constructor(
        message: string,
        readonly status: number,
        readonly body: Record<string, unknown> = {}
    ) {
        super(message);
        this.name = 'ApiError';
    }

    get retryable(): boolean {
        return (
            this.status === 0 || this.status >= 500 || this.status === 408 || this.status === 429
        );
    }
}

export interface RequestOptions {
    body?: unknown;
    raw?: Blob;
    revision?: number;
    signal?: AbortSignal;
}

let unauthorized: () => void = () => {};

export function onUnauthorized(handler: () => void) {
    unauthorized = handler;
}

export async function request<T>(
    method: string,
    path: string,
    { body, raw, revision, signal }: RequestOptions = {}
): Promise<T> {
    const headers: Record<string, string> = {};
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (raw !== undefined) headers['Content-Type'] = 'application/octet-stream';
    if (revision !== undefined) headers['If-Match'] = `"${revision}"`;
    let response: Response;
    try {
        response = await fetch(path, {
            method,
            headers,
            body: raw ?? (body === undefined ? undefined : JSON.stringify(body)),
            credentials: 'same-origin',
            signal
        });
    } catch (e) {
        if (e instanceof DOMException && e.name === 'AbortError') throw e;
        throw new ApiError(m.error_unreachable(), 0);
    }
    if (response.status === 204) return undefined as T;
    const text = await response.text();
    let json: unknown = undefined;
    try {
        json = text === '' ? undefined : JSON.parse(text);
    } catch {
        json = undefined;
    }
    if (!response.ok) {
        const detail = (json ?? {}) as Record<string, unknown>;
        if (response.status === 401 && !path.startsWith('/api/auth/')) unauthorized();
        throw new ApiError(
            errorText(detail) ??
                (typeof detail.error === 'string' ? detail.error : response.statusText),
            response.status,
            detail
        );
    }
    return json as T;
}

export function query(params: Record<string, string | number | undefined | null>): string {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
        if (value !== undefined && value !== null && value !== '') search.set(key, String(value));
    }
    const s = search.toString();
    return s === '' ? '' : `?${s}`;
}
