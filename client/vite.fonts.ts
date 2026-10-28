import type { Plugin } from 'vite';

export function fontMetrics(): Plugin {
    return {
        name: 'font-metrics',
        enforce: 'pre',
        transform(code, id) {
            if (!id.includes('/@fontsource-variable/m-plus-2/')) return;
            return code.replaceAll(
                'font-display: swap;',
                'font-display: swap;\n  ascent-override: 110%;\n  descent-override: 35%;'
            );
        }
    };
}
