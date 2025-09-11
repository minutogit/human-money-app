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
                },
                
                // Border colors
                'theme-subtle': '#E9D8C0', // Subtile Cream-Braun-Mischung für Ränder
                'theme-light-border': '#8E7C68', // Hellbraun für Ränder
            },
            fontFamily: {
                sans: ['Inter', 'sans-serif'], // Optional: Eine modernere Schriftart (müsste importiert werden)
                mono: ['Menlo', 'Consolas', 'monospace'],
            },
        },
    },
    plugins: [],
}