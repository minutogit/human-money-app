// tailwind.config.js
/** @type {import('tailwindcss').Config} */
export default {
    content: [
        "./index.html",
        "./src/**/*.{js,ts,jsx,tsx}",
    ],
    theme: {
        extend: {
            colors: {
                // Theme colors
                'theme': {
                    'primary': '#E63946', // Warmes Korallenrot
                    'secondary': '#2B1B17', // Tiefes Braun für Haupttexte
                    'light': '#5E503F', // Mittelbraun für sekundäre Texte
                    'placeholder': '#8E7C68', // Hellbraun für Platzhalter
                    'success': '#2A9D8F', // Türkisgrün für Erfolgsmeldungen
                    'error': '#E63946', // Warmes Korallenrot für Fehlermeldungen
                    'accent': '#E76F51', // Terrakotta für Akzente
                    'secondary-accent': '#F4A261', // Sanftes Orange
                },
                
                // Background colors
                'bg': {
                    'app': '#FFFAF0', // Warmes Elfenbein für App-Hintergrund
                    'card': '#FFFFFF', // Reines Weiß für Karten
                    'input-readonly': '#FFF8E1', // Sehr helles Cream für ReadOnly-Felder
                    'card-alternate': '#f8ebe1', // Warmes Cream für abwechselnde Karten
                },
                
                // Border colors
                'theme-subtle': '#E9D8C0', // Subtile Cream-Braun-Mischung für Ränder
                'theme-light-border': '#8E7C68', // Hellbraun für Ränder
                
                // Surface colors for premium cards
                'surface': {
                    'primary': '#FFFFFF',
                    'secondary': '#FDFCFB',
                    'accent': '#FFF9F5',
                },
            },
            boxShadow: {
                'premium': '0 10px 30px -10px rgba(43, 27, 23, 0.1), 0 4px 10px -5px rgba(43, 27, 23, 0.04)',
                'premium-hover': '0 20px 40px -15px rgba(43, 27, 23, 0.15), 0 8px 20px -10px rgba(43, 27, 23, 0.08)',
                'inner-soft': 'inset 0 2px 4px 0 rgba(0, 0, 0, 0.02)',
            },
            fontFamily: {
                sans: ['Inter', 'system-ui', 'sans-serif'], // Optional: Eine modernere Schriftart (müsste importiert werden)
                mono: ['Menlo', 'Consolas', 'monospace'],
            },
            borderRadius: {
                '2xl': '1.25rem',
                '3xl': '1.75rem',
            },
        },
    },
    plugins: [],
}