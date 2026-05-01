// src/components/TransferSuccessView.tsx
import { useState, useEffect } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { AppSettings } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { invoke } from '@tauri-apps/api/core';
import { 
    Download, 
    Share2, 
    ShieldCheck, 
    User,
    ArrowUpRight
} from 'lucide-react';

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
            const recipientNameMatch = recipientId.match(/(.+)@/);
            const recipientName = recipientNameMatch ? recipientNameMatch[1] : 'transfer';
            const now = new Date();
            const dateTimePart = now.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, '');
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: 'Export Transfer Bundle',
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
                await writeFile(filePath, content);
                logger.info(`Successfully saved transfer bundle to ${filePath}`);
                
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        invoke<AppSettings>('get_app_settings').then(setSettings).catch(() => {});
                    });
                }
                setFeedback(`Vault synchronization successful.`);
            }
        } catch (e) {
            setFeedback(`Error: ${e}`);
        } finally {
            setIsSaving(false);
        }
    }

    return (
        <div className="flex flex-col items-center justify-center min-h-[80vh] py-12 px-4 animate-in fade-in zoom-in-95 duration-500">
            <div className="w-full max-w-xl space-y-8">
                {/* Status Hero */}
                <div className="text-center space-y-4">
                    <div className="relative inline-block">
                        <div className="p-6 rounded-[32px] bg-emerald-500 text-white shadow-premium-lg animate-bounce-subtle">
                            <ArrowUpRight size={64} />
                        </div>
                        <div className="absolute -bottom-2 -right-2 bg-white p-1.5 rounded-full shadow-md">
                            <div className="bg-emerald-500 w-4 h-4 rounded-full border-2 border-white"></div>
                        </div>
                    </div>
                    <div className="space-y-1">
                        <h1 className="text-3xl font-black text-theme-primary tracking-tight">Transfer Manifest Created</h1>
                        <p className="text-theme-light font-medium">The cryptographic payload is ready for distribution.</p>
                    </div>
                </div>

                {/* Transfer Info Card */}
                <div className="bg-white border border-theme-subtle rounded-[40px] p-8 shadow-premium space-y-8">
                    {/* Amount Hero */}
                    <div className="text-center space-y-2">
                        <span className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">Net Outbound Value</span>
                        <div className="text-4xl font-black text-theme-primary tracking-tighter">
                            {summary}
                        </div>
                    </div>

                    <div className="space-y-4">
                        <div className="space-y-2">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5 px-1">
                                <User size={10} /> Recipient Identifier
                            </span>
                            <div className="p-4 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                                <p className="text-sm font-mono font-bold text-theme-secondary break-all">
                                    {recipientId}
                                </p>
                            </div>
                        </div>

                        <div className="p-5 bg-blue-50/50 border border-blue-100 rounded-3xl flex items-start gap-4">
                            <div className="p-2 bg-white rounded-xl shadow-sm text-blue-500">
                                <Share2 size={20} />
                            </div>
                            <div className="space-y-1">
                                <h4 className="text-xs font-black text-blue-900 uppercase tracking-widest">Next Phase</h4>
                                <p className="text-xs text-blue-800/80 font-medium leading-relaxed">
                                    Export the bundle file and deliver it to the recipient via your preferred secure channel.
                                </p>
                            </div>
                        </div>
                    </div>
                </div>

                {/* Action Controls */}
                <div className="flex flex-col items-center gap-4">
                    <Button 
                        size="lg" 
                        onClick={handleSaveFile} 
                        disabled={isSaving} 
                        className="w-full max-w-sm py-4 rounded-3xl shadow-premium-lg text-lg gap-2"
                    >
                        {isSaving ? <Download className="animate-bounce" size={20} /> : <Download size={20} />}
                        {isSaving ? 'Synchronizing File...' : 'Export Transfer Bundle'}
                    </Button>
                    <Button 
                        variant="secondary" 
                        onClick={onDone} 
                        className="w-full max-w-sm py-3 rounded-2xl text-sm font-black uppercase tracking-widest"
                    >
                        Complete Session
                    </Button>
                    
                    {feedback && (
                        <p className="text-xs font-bold text-emerald-600 flex items-center gap-2 animate-in slide-in-from-bottom-2">
                            <ShieldCheck size={14} />
                            {feedback}
                        </p>
                    )}
                </div>
            </div>
        </div>
    );
}