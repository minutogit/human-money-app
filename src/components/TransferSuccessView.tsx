// src/components/TransferSuccessView.tsx
import { useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
// CORRECTED: Import `writeFile`
import { writeFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { AppSettings } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { invoke } from '@tauri-apps/api/core';
import { useEffect } from 'react';

interface TransferSuccessViewProps {
    bundleData: number[];
    recipientId: string;
    summary: string;
    onDone: () => void;
}

export function TransferSuccessView({ bundleData, recipientId, summary, onDone }: TransferSuccessViewProps) {
    const { protectAction } = useSession();
    const [feedback, setFeedback] = useState('');
    const [isSaving, setIsSaving] = useState(false);
    const [settings, setSettings] = useState<AppSettings | null>(null);

    useEffect(() => {
        async function fetchSettings() {
            try {
                const currentSettings = await invoke<AppSettings>('get_app_settings').catch(() => null);
                setSettings(currentSettings);
            } catch (e) {
                console.error("Failed to load settings in TransferSuccessView:", e);
            }
        }
        fetchSettings();
    }, []);

    async function handleSaveFile() {
        setIsSaving(true);
        setFeedback('');

        try {
            // NEUE LOGIK: Format [RECIPIENT_NAME]_[DATUM-ZEIT].transfer
            // 1. Extrahiere den Teil vor dem "@" (z.B. hans-tRB)
            const recipientNameMatch = recipientId.match(/(.+)@/);
            const recipientName = recipientNameMatch ? recipientNameMatch[1] : 'transfer';

            // 2. Erzeuge den Zeitstempel: YYYYMMDD_HHmm (ohne Doppelpunkte)
            const now = new Date();
            const dateTimePart = now.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, ''); // z.B. 20251106_2119
            
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: 'Save Transfer Bundle',
                defaultPath: settings?.last_used_directory 
                    ? `${settings.last_used_directory}/${suggestedFilename}`
                    : suggestedFilename,
                filters: [{
                    name: 'Transfer Bundle',
                    extensions: ['transfer']
                }]
            });

            if (filePath) {
                const content = new Uint8Array(bundleData);
                // CORRECTED: Use `writeFile`
                await writeFile(filePath, content);
                logger.info(`Successfully saved transfer bundle to ${filePath}`);
                
                // Save directory for next time
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        invoke<AppSettings>('get_app_settings').then(setSettings).catch(() => {});
                    });
                }

                setFeedback(`File saved successfully to ${filePath}`);
            } else {
                logger.info('File save dialog was cancelled by the user.');
            }
        } catch (e) {
            const msg = `Failed to save file: ${e}`;
            logger.error(msg);
            setFeedback(`Error: ${msg}`);
        } finally {
            setIsSaving(false);
        }
    }

    return (
        <div className="flex flex-col h-full max-w-2xl mx-auto items-center justify-center text-center">
            <div className="bg-bg-card p-8 rounded-lg shadow-lg border border-theme-subtle">
                <svg className="w-16 h-16 mx-auto text-theme-success" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>
                <h1 className="text-2xl font-bold text-theme-primary mt-4">Transfer Prepared!</h1>
                <p className="text-theme-light mt-2">
                    A transfer of <span className="font-semibold text-theme-secondary">{summary}</span> for <br />
                    <span className="font-mono text-sm text-theme-accent break-all">{recipientId}</span><br />
                    has been created and saved to your history.
                </p>
                <p className="text-theme-secondary mt-6">
                    Next step: Save the bundle file and send it to the recipient.
                </p>

                <div className="mt-8 space-y-4">
                    <Button size="lg" onClick={handleSaveFile} disabled={isSaving} className="w-full">
                        {isSaving ? 'Saving...' : 'Save Bundle as File'}
                    </Button>
                    <Button variant="secondary" onClick={onDone} className="w-full">
                        Done
                    </Button>
                </div>
                {feedback && <p className="text-sm mt-4 text-theme-light">{feedback}</p>}
            </div>
        </div>
    );
}