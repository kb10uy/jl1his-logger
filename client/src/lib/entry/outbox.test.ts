import 'fake-indexeddb/auto';
import { describe, expect, it } from 'vitest';
import { ApiError } from '#lib/api/client.ts';
import type { Saved } from '#lib/api/types.ts';
import { Outbox, type Outgoing } from './outbox.ts';

const item = (id: string, createdAt: string, logbookId = 'lb'): Outgoing => ({
    id,
    logbookId,
    stationId: 'st',
    fields: { CALL: 'JA1XYZ' },
    createdAt
});

describe('Outbox', () => {
    it('lists a Logbook oldest first', async () => {
        const outbox = await Outbox.open('list');
        await outbox.put(item('b', '2026-10-19T09:02:00Z'));
        await outbox.put(item('a', '2026-10-19T09:01:00Z'));
        await outbox.put(item('c', '2026-10-19T09:00:00Z', 'other'));
        expect((await outbox.list('lb')).map((i) => i.id)).toEqual(['a', 'b']);
    });

    it('drops what was sent and keeps what failed with its error', async () => {
        const outbox = await Outbox.open('send');
        await outbox.put(item('a', '2026-10-19T09:01:00Z'));
        await outbox.put(item('b', '2026-10-19T09:02:00Z'));
        const saved = { qso: { id: 'a' }, warnings: [] } as unknown as Saved;
        expect(await outbox.send(item('a', '2026-10-19T09:01:00Z'), async () => saved)).toEqual({
            saved
        });
        const failed = await outbox.send(item('b', '2026-10-19T09:02:00Z'), async () => {
            throw new ApiError('The server could not be reached.', 0);
        });
        expect('error' in failed && failed.error.retryable).toBe(true);
        expect(await outbox.list('lb')).toEqual([
            { ...item('b', '2026-10-19T09:02:00Z'), error: 'The server could not be reached.' }
        ]);
    });
});
