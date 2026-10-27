import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Locale } from '#lib/paraglide/runtime.js';
import { asCommandError } from '#lib/session/errors.ts';
import type { CommandError } from '#lib/session/types.ts';
import { applyLanguage, asLocale } from './language.ts';

export interface RigSettings {
    enabled: boolean;
    host: string;
    port: number;
    poll_ms: number;
    modes: Record<string, string>;
}

export interface Settings {
    path: string;
    credentialStore: string;
    language: Locale | null;
    rig: RigSettings;
}

export type RigState =
    | { state: 'off' }
    | { state: 'connecting'; address: string }
    | {
          state: 'connected';
          address: string;
          model: string | null;
          frequencyHz: number;
          mode: string;
      }
    | { state: 'failed'; address: string; error: string };

class SettingsState {
    current = $state<Settings | undefined>();
    rig = $state<RigState>({ state: 'off' });
    error = $state<CommandError | undefined>();

    async load(): Promise<boolean> {
        try {
            const settings = await invoke<Settings>('settings');
            this.current = { ...settings, language: asLocale(settings.language) };
            if (applyLanguage(this.current.language)) return false;
        } catch (e) {
            this.error = asCommandError(e);
        }
        void listen<RigState>('rig-state', (e) => (this.rig = e.payload));
        this.rig = await invoke<RigState>('rig_state').catch(() => this.rig);
        return true;
    }

    async save(change: { language?: Locale | null; rig?: RigSettings }) {
        const current = this.current;
        if (current === undefined) return;
        try {
            const saved = await invoke<Settings>('save_settings', {
                language: change.language === undefined ? current.language : change.language,
                rig: change.rig ?? current.rig
            });
            this.current = { ...saved, language: asLocale(saved.language) };
            this.error = undefined;
            if (change.language !== undefined) applyLanguage(this.current.language);
        } catch (e) {
            this.error = asCommandError(e);
        }
    }

    async reconnect() {
        await invoke('reconnect_rig').catch((e) => (this.error = asCommandError(e)));
    }
}

export const settings = new SettingsState();
