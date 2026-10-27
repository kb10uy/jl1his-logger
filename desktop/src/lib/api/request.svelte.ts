import { invoke } from '@tauri-apps/api/core';
import { asCommandError } from '#lib/session/errors.ts';
import { session } from '#lib/session/session.svelte.ts';
import { toApiError } from './errors.ts';

export type Method = 'GET' | 'POST' | 'PUT' | 'DELETE';

class ServerState {
    reachable = $state(true);
}

export const server = new ServerState();

export async function request<T>(method: Method, path: string, body?: unknown): Promise<T> {
    try {
        const value = await invoke<T>('request', { method, path, body });
        server.reachable = true;
        return value;
    } catch (e) {
        const error = asCommandError(e);
        server.reachable = error.kind !== 'unreachable';
        if (error.kind === 'unauthorized') session.reject();
        throw toApiError(error);
    }
}
