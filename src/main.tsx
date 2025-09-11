import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './App.css';
import { attachConsole } from '@tauri-apps/plugin-log';

// WICHTIG: Konsole so früh wie möglich anhängen, um alle Logs zu erfassen
const detach = await attachConsole();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>
);

// Optional: Konsole beim Beenden der App wieder freigeben
// window.addEventListener('beforeunload', () => {
//   detach();
// });