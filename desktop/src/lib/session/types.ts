import type { Area } from '#client/api/types.ts';

export type Logbook = {
    id: string;
    name: string;
    role: string;
    area_field: Area;
};

export type Session =
    | { state: 'signedOut'; url: string | null; rejected: boolean }
    | { state: 'signedIn'; url: string; callsign: string | null; logbooks: Logbook[] | null };

export type CommandError =
    | { kind: 'badUrl'; detail: string }
    | { kind: 'unauthorized' }
    | { kind: 'wl2Token' }
    | { kind: 'noLogbook' }
    | { kind: 'unreachable'; detail: string }
    | { kind: 'server'; status: number; detail: string; body: Record<string, unknown> | null }
    | { kind: 'credentials'; detail: string }
    | { kind: 'config'; detail: string }
    | { kind: 'unexpected'; detail: string };
