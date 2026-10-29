import { defineConfig } from 'vitest/config';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { fonts } from './vite.fonts.ts';

const apiServer = process.env.API_SERVER ?? 'http://127.0.0.1:8080';

export default defineConfig({
    plugins: [
        fonts(),
        paraglideVitePlugin({ project: './project.inlang' }),
        tailwindcss(),
        sveltekit({
            compilerOptions: {
                runes: ({ filename }) =>
                    filename.split(/[/\\]/).includes('node_modules') ? undefined : true
            },
            adapter: adapter({ fallback: 'index.html' }),
            csp: {
                mode: 'hash',
                directives: {
                    'default-src': ['self'],
                    'script-src': ['self', 'wasm-unsafe-eval'],
                    'style-src': ['self', 'unsafe-inline'],
                    'img-src': ['self', 'data:'],
                    'font-src': ['self'],
                    'connect-src': ['self'],
                    'worker-src': ['self'],
                    'manifest-src': ['self']
                }
            }
        })
    ],
    server: {
        proxy: { '/api': apiServer }
    },
    test: {
        expect: { requireAssertions: true },
        environment: 'node',
        include: ['src/**/*.test.ts'],
        setupFiles: ['src/lib/query/setup.test-env.ts']
    }
});
