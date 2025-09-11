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
                'primary': '#4F46E5',
                'primary-hover': '#4338CA',
                'secondary': '#6B7280',
                'accent': '#10B981',
                'danger': '#EF4444',
                'surface': '#FFFFFF',
                'background': '#F9FAFB',
            },
        },
    },
    plugins: [],
}