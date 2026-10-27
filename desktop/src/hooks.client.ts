import { overwriteGetLocale } from '#client/paraglide/runtime.js';
import { getLocale } from '#lib/paraglide/runtime.js';
import { defineLanguageStrategy } from '#lib/settings/language.ts';

defineLanguageStrategy();
overwriteGetLocale(getLocale);
