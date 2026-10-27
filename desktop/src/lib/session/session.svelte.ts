import { invoke } from '@tauri-apps/api/core';
import { asCommandError } from './errors.ts';
import type { CommandError, Session } from './types.ts';

class SessionState {
    current = $state<Session | undefined>();
    error = $state<CommandError | undefined>();

    async load(): Promise<void> {
        try {
            this.current = await invoke<Session>('session');
        } catch (e) {
            this.error = asCommandError(e);
            this.current = { state: 'signedOut', url: null, rejected: false };
        }
    }

    async signIn(url: string, token: string): Promise<void> {
        try {
            this.current = await invoke<Session>('sign_in', { url, token });
            this.error = undefined;
        } catch (e) {
            throw asCommandError(e);
        }
    }

    reject() {
        const current = this.current;
        if (current?.state !== 'signedIn') return;
        this.current = { state: 'signedOut', url: current.url, rejected: true };
    }

    async signOut(): Promise<void> {
        try {
            this.current = await invoke<Session>('sign_out');
            this.error = undefined;
        } catch (e) {
            this.error = asCommandError(e);
        }
    }
}

export const session = new SessionState();
