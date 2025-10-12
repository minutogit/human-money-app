// src/components/TransferSuccessView.tsx
import { useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
// CORRECTED: Import `writeFile`
import { writeFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { Button } from './ui/Button';

interface TransferSuccessViewProps {
    bundleData: number[];
    recipientId: string;
    summary: string;
    onDone: () => void;
}

export function TransferSuccessView({ bundleData, recipientId, summary, onDone }: TransferSuccessViewProps) {
    const [feedback, setFeedback] = useState('');
    const [isSaving, setIsSaving] = useState(false);

    async function handleSaveFile() {
        setIsSaving(true);
        setFeedback('');

        try {
            const suggestedFilename = `transfer-for-${recipientId.substring(8, 20)}-${new Date().toISOString().split('T')[0]}.minuto-bundle`;

            const filePath = await save({
                title: 'Save Transfer Bundle',
                defaultPath: suggestedFilename,
                filters: [{
                    name: 'Minuto Bundle',
                    extensions: ['minuto-bundle']
                }]
            });

            if (filePath) {
                const content = new Uint8Array(bundleData);
                // CORRECTED: Use `writeFile`
                await writeFile(filePath, content);
                logger.info(`Successfully saved transfer bundle to ${filePath}`);
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