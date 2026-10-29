import { searchForWorkspaceRoot } from 'vite';
import { defineConfig } from 'vitest/config';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { fonts } from '../client/vite.fonts.ts';

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
            adapter: adapter({ fallback: 'index.html' })
        })
    ],
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        fs: { allow: [searchForWorkspaceRoot(process.cwd()), '../client'] },
        watch: { ignored: ['**/src-tauri/**'] }
    },
    test: {
        expect: { requireAssertions: true },
        environment: 'node',
        include: ['src/**/*.test.ts']
    }
});
