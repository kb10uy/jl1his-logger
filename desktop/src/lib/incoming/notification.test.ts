import { describe, expect, it } from 'vitest';
import { loggedNotification } from './notification.ts';

describe('loggedNotification', () => {
    it('names the call, the program, the band and the mode', () => {
        expect(
            loggedNotification(
                'WSJT-X',
                { CALL: 'JA1ABC', BAND: '20m', MODE: 'MFSK', SUBMODE: 'FT4' },
                []
            )
        ).toEqual({ title: 'Logged JA1ABC', body: 'From WSJT-X · 20m · FT4' });
    });

    it('adds a line for each value kept against the Station', () => {
        expect(
            loggedNotification(null, { CALL: 'JA1ABC', MODE: 'FT8' }, [
                { name: 'STATION_CALLSIGN', given: 'JL1HIS/P', station: 'JL1HIS' }
            ])
        ).toEqual({
            title: 'Logged JA1ABC',
            body: 'From another program · FT8\nSTATION_CALLSIGN kept as JL1HIS/P; the Station has JL1HIS'
        });
    });
});
