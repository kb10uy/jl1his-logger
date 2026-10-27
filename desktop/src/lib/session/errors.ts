import { m } from '#lib/paraglide/messages.js';
import type { CommandError } from './types.ts';

const KINDS = new Set([
    'badUrl',
    'unauthorized',
    'wl2Token',
    'noLogbook',
    'unreachable',
    'server',
    'credentials',
    'config'
]);

export function asCommandError(e: unknown): CommandError {
    if (typeof e === 'object' && e !== null && 'kind' in e && KINDS.has(String(e.kind))) {
        return e as CommandError;
    }
    return { kind: 'unexpected', detail: e instanceof Error ? e.message : String(e) };
}

export function errorText(e: CommandError): string {
    switch (e.kind) {
        case 'badUrl':
            return m.signin_error_bad_url({ detail: e.detail });
        case 'unauthorized':
            return m.signin_error_unauthorized();
        case 'wl2Token':
            return m.signin_error_wl2();
        case 'noLogbook':
            return m.signin_error_no_logbook();
        case 'unreachable':
            return m.signin_error_unreachable({ detail: e.detail });
        case 'server':
            return m.signin_error_server({ status: e.status, detail: e.detail });
        case 'credentials':
            return m.signin_error_credentials({ detail: e.detail });
        case 'config':
            return m.signin_error_config({ detail: e.detail });
        case 'unexpected':
            return m.common_unexpected({ detail: e.detail });
    }
}

export const refusesToken = (e: CommandError | undefined): boolean =>
    e?.kind === 'unauthorized' || e?.kind === 'wl2Token' || e?.kind === 'noLogbook';
