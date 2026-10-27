import { defineConfig } from 'vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
    plugins: [
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
        watch: { ignored: ['**/src-tauri/**'] }
    }
});
