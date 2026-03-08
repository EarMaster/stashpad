import typography from '@tailwindcss/typography';

/** @type {import('tailwindcss').Config} */
export default {
    content: ['./src/**/*.{html,js,svelte,ts}'],
    darkMode: 'class',
    theme: {
        extend: {
            colors: {
                background: 'var(--background)',
                foreground: 'var(--foreground)',

                muted: 'var(--muted)',
                'muted-foreground': 'var(--muted-foreground)',

                card: 'var(--card)',
                'card-foreground': 'var(--card-foreground)',

                popover: 'var(--popover)',
                'popover-foreground': 'var(--popover-foreground)',

                primary: {
                    DEFAULT: 'var(--primary)',
                    foreground: 'var(--primary-foreground)'
                },
                secondary: {
                    DEFAULT: 'var(--secondary)',
                    foreground: 'var(--secondary-foreground)'
                },
                accent: {
                    DEFAULT: 'var(--accent)',
                    foreground: 'var(--accent-foreground)'
                },
                destructive: {
                    DEFAULT: 'var(--destructive)',
                    foreground: 'var(--destructive-foreground)'
                },
                border: 'var(--border)',
                input: 'var(--input)',
                ring: 'var(--ring)',
            },
            typography: {
                DEFAULT: {
                    css: {
                        'code::before': {
                            content: '""'
                        },
                        'code::after': {
                            content: '""'
                        },
                        code: {
                            fontWeight: '400',
                            padding: '0.2em 0.4em',
                            borderRadius: '0.25rem',
                        },
                        'pre code': {
                            backgroundColor: 'transparent',
                            padding: '0',
                        },
                        pre: {
                            backgroundColor: 'transparent',
                            borderRadius: '0.5rem',
                            padding: '1rem',
                        },
                        h1: { fontSize: '1.5em' },
                        h2: { fontSize: '1.25em' },
                        h3: { fontSize: '1.125em' },
                        h4: { fontSize: '1em' },
                        h5: { fontSize: '0.875em' },
                        h6: { fontSize: '0.875em' }
                    },
                },
            },
        },
    },
    plugins: [typography],
}
