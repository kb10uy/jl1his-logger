import { describe, expect, it } from 'vitest';
import { toApiError } from './errors.ts';

describe('toApiError', () => {
    it('keeps an unreachable server retryable', () => {
        const error = toApiError({ kind: 'unreachable', detail: 'timed out' });
        expect(error.status).toBe(0);
        expect(error.retryable).toBe(true);
        expect(error.message).toBe('Could not reach the server.');
    });

    it('words a server error from its body', () => {
        const error = toApiError({
            kind: 'server',
            status: 409,
            detail: 'conflict',
            body: { error: 'conflict', code: 'id_taken' }
        });
        expect(error.status).toBe(409);
        expect(error.retryable).toBe(false);
        expect(error.message).not.toBe('conflict');
        expect(error.body).toEqual({ error: 'conflict', code: 'id_taken' });
    });

    it('falls back to the detail of a server error without a known code', () => {
        expect(
            toApiError({ kind: 'server', status: 400, detail: 'call is required', body: null })
                .message
        ).toBe('call is required');
    });

    it('marks a refused token as 401', () => {
        expect(toApiError({ kind: 'unauthorized' }).status).toBe(401);
    });
});
