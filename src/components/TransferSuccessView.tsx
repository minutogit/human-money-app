// src/components/TransferSuccessView.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { AppSettings, IntentDispatchObservation } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { settingsService } from '../services/settingsService';
import { 
    Download, 
    Share2, 
    ShieldCheck, 
    User,
    ArrowUpRight,
    FolderOpen
} from 'lucide-react';
import { suggestFilename } from '../utils/userIdHelper';
import { stringifyError } from '../utils/errorHelper';

interface TransferSuccessViewProps {
    bundleData: number[];
    recipientId: string;
    summary: string;
    l2Dispatch?: IntentDispatchObservation[];
    onDone: () => void;
}

export function TransferSuccessView({ bundleData, recipientId, summary, l2Dispatch, onDone }: TransferSuccessViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [feedback, setFeedback] = useState('');
    const [isSaving, setIsSaving] = useState(false);
    const [savedFilePath, setSavedFilePath] = useState<string | null>(null);
    const [settings, setSettings] = useState<AppSettings | null>(null);

    useEffect(() => {
        async function fetchSettings() {
            try {
                const currentSettings = await settingsService.getSettings().catch(() => null);
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
            const recipientName = suggestFilename(recipientId);
            const now = new Date();
            const dateTimePart = now.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, '');
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: t('transfer.success.saveTitle'),
                defaultPath: settings?.lastUsedDirectory 
                    ? `${settings.lastUsedDirectory}/${suggestedFilename}`
                    : suggestedFilename,
                filters: [{
                    name: t('transfer.success.fileFilter'),
                    extensions: ['transfer']
                }]
            });

            if (filePath) {
                const content = new Uint8Array(bundleData);
                await writeFile(filePath, content);
                setSavedFilePath(filePath);
                logger.info(`Successfully saved transfer bundle to ${filePath}`);
                
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        settingsService.getSettings().then(setSettings).catch(() => {});
                    });
                }
                setFeedback(t('transfer.success.synced'));
            }
        } catch (e) {
            setFeedback(`Error: ${stringifyError(e)}`);
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
                        <h1 className="text-3xl font-black text-theme-primary tracking-tight">{t('transfer.success.title')}</h1>
                        <p className="text-theme-light font-medium">{t('transfer.success.description')}</p>
                    </div>
                </div>

                {/* Transfer Info Card */}
                <div className="bg-white border border-theme-subtle rounded-[40px] p-8 shadow-premium space-y-8">
                    {/* Amount Hero */}
                    <div className="text-center space-y-2">
                        <span className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">{t('transfer.success.totalAmount')}</span>
                        <div className="text-4xl font-black text-theme-primary tracking-tighter">
                            {summary}
                        </div>
                    </div>

                    <div className="space-y-4">
                        <div className="space-y-2">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5 px-1">
                                <User size={10} /> {t('transfer.success.recipientId')}
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
                                <h4 className="text-xs font-black text-blue-900 uppercase tracking-widest">{t('transfer.success.nextStep')}</h4>
                                <p className="text-xs text-blue-800/80 font-medium leading-relaxed">
                                    {t('transfer.success.nextStepDesc')}
                                </p>
                            </div>
                        </div>

                        {l2Dispatch && l2Dispatch.length > 0 && (() => {
                            const isAnyDispatched = l2Dispatch.some(obs => obs.httpStatus === 200 || obs.httpStatus === 201);
                            
                            if (isAnyDispatched) {
                                return (
                                    <div className="p-5 bg-emerald-50/60 border border-emerald-200 rounded-3xl space-y-3">
                                        <div className="flex items-center justify-between">
                                            <span className="text-[10px] font-black uppercase tracking-widest text-emerald-900 flex items-center gap-1.5">
                                                <ShieldCheck size={14} className="text-emerald-600" />
                                                {t('transfer.l2DispatchedTitle')}
                                            </span>
                                            <span className="text-[9px] font-bold px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800">
                                                {l2Dispatch.length} {l2Dispatch.length === 1 ? 'Lock-Intent' : 'Lock-Intents'}
                                            </span>
                                        </div>
                                        <p className="text-xs text-emerald-900/80 font-medium leading-relaxed">
                                            {t('transfer.l2DispatchedDesc')}
                                        </p>
                                        <div className="space-y-1.5 pt-1">
                                            {l2Dispatch.map((obs, idx) => (
                                                <div key={`${obs.localInstanceId}-${idx}`} className="flex items-center justify-between text-xs p-2.5 bg-white/80 rounded-xl border border-emerald-100">
                                                    <span className="font-mono text-[11px] text-theme-light truncate max-w-[220px]" title={obs.localInstanceId}>
                                                        {obs.localInstanceId.slice(0, 16)}...
                                                    </span>
                                                    <span className={`text-[10px] font-bold px-2 py-0.5 rounded ${obs.httpStatus && obs.httpStatus >= 200 && obs.httpStatus < 300 ? 'bg-emerald-100 text-emerald-800' : 'bg-amber-100 text-amber-800'}`}>
                                                        {obs.httpStatus ? `HTTP ${obs.httpStatus}` : obs.status}
                                                    </span>
                                                </div>
                                            ))}
                                        </div>
                                        <p className="text-[10px] text-theme-light/70 italic border-t border-emerald-200/60 pt-2">
                                            {t('transfer.l2TransportDisclaimer')}
                                        </p>
                                    </div>
                                );
                            }

                            return (
                                <div className="p-5 bg-slate-50 border border-slate-200 rounded-3xl space-y-3">
                                    <div className="flex items-center justify-between">
                                        <span className="text-[10px] font-black uppercase tracking-widest text-theme-secondary flex items-center gap-1.5">
                                            <ShieldCheck size={14} className="text-theme-primary" />
                                            {t('transfer.l2PreparedOfflineTitle')}
                                        </span>
                                        <span className="text-[9px] font-bold px-2 py-0.5 rounded-full bg-slate-200 text-slate-700">
                                            {l2Dispatch.length} {l2Dispatch.length === 1 ? 'Lock-Intent' : 'Lock-Intents'}
                                        </span>
                                    </div>
                                    <p className="text-xs text-theme-light font-medium leading-relaxed">
                                        {t('transfer.l2PreparedOfflineDesc')}
                                    </p>
                                    <div className="space-y-1.5 pt-1">
                                        {l2Dispatch.map((obs, idx) => (
                                            <div key={`${obs.localInstanceId}-${idx}`} className="flex items-center justify-between text-xs p-2.5 bg-white rounded-xl border border-slate-100">
                                                <span className="font-mono text-[11px] text-theme-light truncate max-w-[220px]" title={obs.localInstanceId}>
                                                    {obs.localInstanceId.slice(0, 16)}...
                                                </span>
                                                <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-slate-100 text-slate-700">
                                                    {obs.status}
                                                </span>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            );
                        })()}
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
                        {isSaving ? t('transfer.success.savingFile') : t('transfer.success.saveFile')}
                    </Button>

                    {savedFilePath && (
                        <Button
                            variant="secondary"
                            onClick={() => {
                                revealItemInDir(savedFilePath).catch((err) => {
                                    logger.error(`Failed to reveal item in dir: ${stringifyError(err)}`);
                                });
                            }}
                            className="w-full max-w-sm py-3 rounded-2xl text-sm font-bold gap-2 animate-in fade-in duration-200"
                        >
                            <FolderOpen size={18} />
                            {t('transfer.success.revealInFolder')}
                        </Button>
                    )}

                    <Button 
                        variant="secondary" 
                        onClick={onDone} 
                        className="w-full max-w-sm py-3 rounded-2xl text-sm font-black uppercase tracking-widest"
                    >
                        {t('transfer.success.finish')}
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