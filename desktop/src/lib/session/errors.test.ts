import { describe, expect, it } from 'vitest';
import { asCommandError, errorText, refusesToken } from './errors.ts';

describe('asCommandError', () => {
    it('keeps an error the Rust side sent', () => {
        expect(asCommandError({ kind: 'unreachable', detail: 'timed out' })).toEqual({
            kind: 'unreachable',
            detail: 'timed out'
        });
    });

    it('wraps anything else as unexpected', () => {
        expect(asCommandError('command sign_in not found')).toEqual({
            kind: 'unexpected',
            detail: 'command sign_in not found'
        });
        expect(asCommandError(new Error('boom'))).toEqual({ kind: 'unexpected', detail: 'boom' });
        expect(asCommandError({ kind: 'nothing' })).toEqual({
            kind: 'unexpected',
            detail: '[object Object]'
        });
    });
});

describe('errorText', () => {
    it('words each error with its details', () => {
        expect(
            errorText({ kind: 'server', status: 503, detail: 'Service Unavailable', body: null })
        ).toBe('The server answered 503: Service Unavailable');
        expect(errorText({ kind: 'wl2Token' })).toBe(
            'This is a Wavelog compatible token. Issue a Native API token instead.'
        );
        expect(errorText({ kind: 'unreachable', detail: 'timed out' })).toContain('timed out');
    });
});

describe('refusesToken', () => {
    it.each([
        [{ kind: 'unauthorized' } as const, true],
        [{ kind: 'wl2Token' } as const, true],
        [{ kind: 'noLogbook' } as const, true],
        [{ kind: 'unreachable', detail: '' } as const, false],
        [undefined, false]
    ])('tells whether %o is about the token', (e, expected) => {
        expect(refusesToken(e)).toBe(expected);
    });
});
