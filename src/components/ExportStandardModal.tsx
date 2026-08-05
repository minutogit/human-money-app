import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { standardsService } from '../services/standardsService';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { ContainerConfig } from '../types';
import { Input } from './ui/Input';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { Lock, FileCode, Shield } from 'lucide-react';
import { logger } from '../utils/log';
import { translateError } from '../utils/errorHelper';

interface ExportStandardModalProps {
    isOpen: boolean;
    standardId: string;
    standardName: string;
    onClose: () => void;
    onSuccess: () => void;
}

export function ExportStandardModal({
    isOpen,
    standardId,
    standardName,
    onClose,
    onSuccess
}: ExportStandardModalProps) {
    const { t } = useTranslation();
    const [mode, setMode] = useState<'Cleartext' | 'Password'>('Cleartext');
    const [password, setPassword] = useState('');
    const [confirmPassword, setConfirmPassword] = useState('');
    const [isExporting, setIsExporting] = useState(false);
    const [error, setError] = useState('');

    const handleExport = async () => {
        setError('');
        if (mode === 'Password') {
            if (!password) {
                setError(t('voucher.export.errorNoConfirm'));
                return;
            }
            if (password !== confirmPassword) {
                setError(t('voucher.export.errorPasswordMismatch'));
                return;
            }
        }

        setIsExporting(true);
        try {
            const config: ContainerConfig = mode === 'Password' 
                ? { type: 'Password', value: password }
                : { type: 'Cleartext' };

            const containerBytes = await standardsService.exportStandard(standardId, config);

            const defaultFileName = `${standardName.toLowerCase().replace(/\s+/g, '-')}.standard`;
            const filePath = await save({
                defaultPath: defaultFileName,
                filters: [{ name: t('standards.formatDesc'), extensions: ['standard'] }]
            });

            if (filePath) {
                await writeFile(filePath, new Uint8Array(containerBytes));
                logger.info(`Successfully exported standard to ${filePath}`);
                onSuccess();
                onClose();
            }
        } catch (e) {
            setError(translateError(e, t));
        } finally {
            setIsExporting(false);
        }
    };

    return (
        <ConfirmationModal
            isOpen={isOpen}
            title={t('standards.exportModalTitle')}
            onCancel={onClose}
            onConfirm={handleExport}
            confirmText={t('standards.export')}
            isProcessing={isExporting}
            description={
                <div className="space-y-6 pt-2 text-left">
                    <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10 flex items-center gap-3">
                        <FileCode size={24} className="text-theme-primary shrink-0" />
                        <div>
                            <p className="text-sm font-bold text-theme-primary">{standardName}</p>
                            <p className="text-[10px] font-mono text-theme-light truncate max-w-[260px]">{standardId}</p>
                        </div>
                    </div>

                    <div className="space-y-3">
                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-2">
                            <Shield size={12} /> {t('standards.encryption')}
                        </label>
                        <div className="grid grid-cols-2 gap-3">
                            <button
                                type="button"
                                onClick={() => setMode('Cleartext')}
                                className={`p-3 rounded-2xl border-2 text-xs font-bold transition-all text-center flex flex-col items-center gap-1 ${
                                    mode === 'Cleartext'
                                        ? 'border-theme-primary bg-theme-primary/10 text-theme-primary shadow-sm'
                                        : 'border-theme-subtle bg-white text-theme-light hover:border-theme-primary/30'
                                }`}
                            >
                                <FileCode size={18} />
                                <span>{t('standards.cleartext')}</span>
                            </button>
                            <button
                                type="button"
                                onClick={() => setMode('Password')}
                                className={`p-3 rounded-2xl border-2 text-xs font-bold transition-all text-center flex flex-col items-center gap-1 ${
                                    mode === 'Password'
                                        ? 'border-theme-primary bg-theme-primary/10 text-theme-primary shadow-sm'
                                        : 'border-theme-subtle bg-white text-theme-light hover:border-theme-primary/30'
                                }`}
                            >
                                <Lock size={18} />
                                <span>{t('standards.passwordEncrypted')}</span>
                            </button>
                        </div>
                    </div>

                    {mode === 'Password' && (
                        <div className="space-y-3 animate-in fade-in slide-in-from-top-2 duration-300">
                            <div>
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('standards.password')}</label>
                                <Input
                                    type="password"
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                    placeholder={t('voucher.export.passwordPlaceholder')}
                                    className="mt-1"
                                />
                            </div>
                            <div>
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('voucher.export.confirmPasswordPlaceholder')}</label>
                                <Input
                                    type="password"
                                    value={confirmPassword}
                                    onChange={(e) => setConfirmPassword(e.target.value)}
                                    placeholder={t('voucher.export.confirmPasswordPlaceholder')}
                                    className="mt-1"
                                />
                            </div>
                        </div>
                    )}

                    {error && (
                        <p className="text-xs font-bold text-rose-500 animate-shake">{error}</p>
                    )}
                </div>
            }
        />
    );
}
