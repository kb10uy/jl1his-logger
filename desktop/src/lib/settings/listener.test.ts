import { describe, expect, it } from 'vitest';
import { isLoopback } from './listener.ts';

describe('isLoopback', () => {
    it.each(['127.0.0.1', '127.1.2.3', '::1', '[::1]', 'localhost', ' LocalHost '])(
        'takes %j as this machine only',
        (host) => {
            expect(isLoopback(host)).toBe(true);
        }
    );

    it.each(['0.0.0.0', '::', '192.168.1.10', 'example.org', '127.0.0.1.example.org'])(
        'takes %j as reachable from other machines',
        (host) => {
            expect(isLoopback(host)).toBe(false);
        }
    );
});
