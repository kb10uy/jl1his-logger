import type { Plugin } from 'vite';

export function fonts(): Plugin {
    return {
        name: 'fonts',
        enforce: 'pre',
        config: () => ({
            build: { assetsInlineLimit: (file) => (file.endsWith('.woff2') ? false : undefined) }
        }),
        transform(code, id) {
            if (!id.includes('/@fontsource-variable/m-plus-2/')) return;
            return code.replaceAll(
                'font-display: swap;',
                'font-display: swap;\n  ascent-override: 110%;\n  descent-override: 35%;'
            );
        }
    };
}
