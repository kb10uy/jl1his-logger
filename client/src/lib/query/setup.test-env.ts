import { readFileSync } from 'node:fs';
import { initSync } from './wasm/query.js';

initSync({ module: readFileSync(new URL('./wasm/query_bg.wasm', import.meta.url)) });
