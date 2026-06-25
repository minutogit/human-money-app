// src/components/BugReportView.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { PageLayout } from './ui/PageLayout';
import { Card } from './ui/Card';
import { Button } from './ui/Button';
import { Terminal, Copy, ExternalLink, AlertTriangle, Check, Loader2 } from 'lucide-react';
import { logger } from '../utils/log';
import { translateError, stringifyError } from '../utils/errorHelper';

interface BugReportViewProps {
    onBack: () => void;
}

export const BugReportView: React.FC<BugReportViewProps> = ({ onBack }) => {
    const { t } = useTranslation();
    const [logs, setLogs] = useState<string>('');
    const [isLoading, setIsLoading] = useState<boolean>(true);
    const [error, setError] = useState<string>('');
    const [copied, setCopied] = useState<boolean>(false);

    useEffect(() => {
        async function fetchLogs() {
            try {
                logger.info("BugReportView: Fetching application logs.");
                const latestLogs = await invoke<string>("get_latest_logs");
                setLogs(latestLogs);
            } catch (err) {
                logger.error(`Failed to fetch logs: ${stringifyError(err)}`);
                setError(translateError(err, t));
            } finally {
                setIsLoading(false);
            }
        }
        fetchLogs();
    }, [t]);

    const handleCopy = async () => {
        try {
            await navigator.clipboard.writeText(logs);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        } catch (err) {
            logger.error(`Failed to copy logs to clipboard: ${stringifyError(err)}`);
        }
    };

    const handleOpenGithub = async () => {
        try {
            await openUrl('https://github.com/minutogit/human-money-app/issues/new');
        } catch (err) {
            logger.error(`Failed to open GitHub issues link: ${stringifyError(err)}`);
        }
    };

    return (
        <PageLayout
            title={t('common.bugReportTitle')}
            description={t('common.bugReportDescription')}
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500 pb-10">
                {/* Privacy Warning Banner */}
                <div className="flex gap-4 p-5 rounded-2xl bg-amber-500/10 border border-amber-500/20 text-amber-900 dark:text-amber-200">
                    <AlertTriangle className="w-6 h-6 text-amber-500 shrink-0 mt-0.5" />
                    <div className="space-y-1">
                        <h4 className="font-black text-sm tracking-tight">{t('common.bugReportWarnTitle')}</h4>
                        <p className="text-xs leading-relaxed opacity-90">{t('common.bugReportWarnDesc')}</p>
                    </div>
                </div>

                {/* Log Viewer Card */}
                <Card 
                    header={
                        <div className="flex items-center justify-between w-full">
                            <div className="flex items-center gap-2">
                                <Terminal size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">{t('common.bugReportLabel')}</span>
                            </div>
                            {logs && !isLoading && !error && (
                                <Button 
                                    onClick={handleCopy} 
                                    variant={copied ? "primary" : "outline"} 
                                    className="px-4 py-2 text-xs font-black uppercase tracking-widest gap-2 flex items-center transition-all"
                                >
                                    {copied ? <Check size={14} /> : <Copy size={14} />}
                                    {copied ? t('common.copied') : t('common.bugReportCopyButton')}
                                </Button>
                            )}
                        </div>
                    }
                >
                    {isLoading ? (
                        <div className="flex flex-col items-center justify-center py-20 space-y-4">
                            <Loader2 className="w-8 h-8 text-theme-primary animate-spin" />
                            <p className="text-xs font-bold text-theme-light uppercase tracking-widest animate-pulse">Loading Logs...</p>
                        </div>
                    ) : error ? (
                        <div className="p-8 text-center space-y-2">
                            <p className="font-black text-rose-500">{t('common.bugReportErrorTitle')}</p>
                            <p className="text-xs text-theme-light">{error}</p>
                        </div>
                    ) : (
                        <div className="relative rounded-2xl border border-theme-subtle bg-slate-950 p-5 font-mono text-[11px] text-slate-300 leading-relaxed overflow-auto max-h-[350px] shadow-inner select-text">
                            <pre className="whitespace-pre">{logs || "No logs recorded."}</pre>
                        </div>
                    )}
                </Card>

                {/* External Actions Card */}
                {!isLoading && !error && (
                    <div className="flex justify-center pt-4">
                        <Button
                            onClick={handleOpenGithub}
                            className="px-8 py-5 rounded-3xl shadow-premium text-lg gap-2 inline-flex items-center group transition-all"
                        >
                            <ExternalLink size={20} className="group-hover:translate-x-0.5 group-hover:-translate-y-0.5 transition-transform duration-300" />
                            {t('common.bugReportOpenIssue')}
                        </Button>
                    </div>
                )}
            </div>
        </PageLayout>
    );
};
