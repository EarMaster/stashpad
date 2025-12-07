/** @type {import('tailwindcss').Config} */
export default {
    content: ['./src/**/*.{html,js,svelte,ts}'],
    darkMode: 'class',
    theme: {
        extend: {
            colors: {
                background: '#18181b', // Deep Charcoal
                foreground: '#d8d8d9', // Terminal White

                muted: '#27272a',      // Lighter Charcoal
                'muted-foreground': '#a1a1aa', // Standard muted text (zinc-400 equivalent for contrast)

                card: '#2c373d',       // Midnight Graphite
                'card-foreground': '#d8d8d9',

                primary: {
                    DEFAULT: '#8b5cf6', // Electric Violet
                    foreground: '#ffffff'
                },
                secondary: {
                    DEFAULT: '#2c373d', // Midnight Graphite
                    foreground: '#d8d8d9'
                },
                accent: {
                    DEFAULT: '#f59e0b', // Amber
                    foreground: '#ffffff'
                },
                border: '#27272a',
                input: '#27272a',
                ring: '#8b5cf6',
            }
        },
    },
    plugins: [],
}
