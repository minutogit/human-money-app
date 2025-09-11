// vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite"; // NEU: Vite-Plugin importieren

export default defineConfig(async () => ({
    // ACHTUNG: Das Plugin wird hier hinzugefügt
    plugins: [react(), tailwindcss()],

    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        watch: {
            ignored: ["**/src-tauri/**"],
        },
    },
}));