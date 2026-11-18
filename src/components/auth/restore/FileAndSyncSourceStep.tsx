import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    AlertTriangle,
    ArrowRight,
    FileArchive,
    FolderSync,
    RefreshCw,
    Server,
    UploadCloud,
} from 'lucide-react';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { SeedPhraseInput, SeedPhraseValue } from '../SeedPhraseInput';

export interface WordsAndSyncSourceFormProps {
    initialPayload?: string;
    focusPrefixTrigger: number;
    seedIsValid: boolean;
    syncInput: string;
    isBusy: boolean;
    onSeedChange: (value: SeedPhraseValue) => void;
    onSyncInputChange: (value: string) => void;
    onSubmit: (e?: React.FormEvent) => void;
}

/**
 * Recovery words, emergency sheet QR/link, and optional sync service input step.
 */
export const WordsAndSyncSourceForm: React.FC<WordsAndSyncSourceFormProps> = ({
    initialPayload,
    focusPrefixTrigger,
    seedIsValid,
    syncInput,
    isBusy,
    onSeedChange,
    onSyncInputChange,
    onSubmit,
}) => {
    const { t } = useTranslation();

    return (
        <form onSubmit={onSubmit} className="space-y-5">
            <SeedPhraseInput
                allowKitUri
                showUserPrefix
                initialPayload={initialPayload}
                focusPrefixTrigger={focusPrefixTrigger}
                onChange={onSeedChange}
                idPrefix="restore-words"
            />

            <div className="p-4 bg-white border border-theme-subtle rounded-2xl space-y-2 shadow-sm">
                <label
                    htmlFor="restore-sync-input"
                    className="text-xs font-bold text-theme-secondary flex items-center gap-1.5"
                >
                    <Server size={14} className="text-theme-primary" />
                    <span>{t('backup.optionalSyncAddressLabel')}</span>
                </label>
                <Input
                    id="restore-sync-input"
                    data-testid="restore-sync-input"
                    type="text"
                    value={syncInput}
                    onChange={(e) => onSyncInputChange(e.target.value)}
                    placeholder={t('profile.backupRestoreSyncAddressPlaceholder')}
                    disabled={isBusy}
                />
                <p className="text-[11px] text-theme-light leading-relaxed">
                    {t('backup.optionalSyncAddressHint')}
                </p>
            </div>

            <Button
                type="submit"
                variant="primary"
                data-testid="words-continue-btn"
                disabled={!seedIsValid || isBusy}
                className="w-full py-3.5"
            >
                {isBusy ? (
                    <>
                        <RefreshCw size={15} className="animate-spin mr-2" />
                        {t('profile.backupRestoreFetchingSync')}
                    </>
                ) : (
                    <>
                        <span>{t('profile.nextStep')}</span>
                        <ArrowRight size={16} className="ml-2" />
                    </>
                )}
            </Button>
        </form>
    );
};

export interface SyncUnreachableCardProps {
    errorMessage?: string | null;
    isBusy: boolean;
    onRetry: () => void;
    onEditAddress: () => void;
    onProceedWithoutBackup: () => void;
}

/**
 * Hard-stop alert card shown when the configured sync service cannot be reached,
 * offering retry, address edit, or explicit bypass to open an empty local profile.
 */
export const SyncUnreachableCard: React.FC<SyncUnreachableCardProps> = ({
    errorMessage,
    isBusy,
    onRetry,
    onEditAddress,
    onProceedWithoutBackup,
}) => {
    const { t } = useTranslation();

    return (
        <div
            data-testid="sync-unreachable-card"
            className="p-6 bg-amber-50 border border-amber-200 rounded-3xl space-y-4"
        >
            <div className="flex items-start gap-3">
                <AlertTriangle size={20} className="text-amber-600 shrink-0 mt-0.5" />
                <div className="space-y-1">
                    <h4 className="text-sm font-black text-amber-900">
                        {t('backup.syncUnreachableTitle')}
                    </h4>
                    <p className="text-xs text-amber-800 leading-relaxed">
                        {errorMessage || t('backup.syncUnreachableDesc')}
                    </p>
                </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 pt-1">
                <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    data-testid="retry-sync-probe-btn"
                    disabled={isBusy}
                    onClick={onRetry}
                >
                    <RefreshCw size={14} className={`mr-1.5 ${isBusy ? 'animate-spin' : ''}`} />
                    {t('common.retry')}
                </Button>
                <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    data-testid="edit-sync-address-btn"
                    onClick={onEditAddress}
                >
                    {t('backup.editSyncAddressBtn')}
                </Button>
                <button
                    type="button"
                    data-testid="proceed-without-backup-btn"
                    onClick={onProceedWithoutBackup}
                    className="text-xs font-semibold text-theme-light hover:text-theme-secondary underline ml-auto cursor-pointer"
                >
                    {t('backup.proceedWithoutBackupLink')}
                </button>
            </div>
        </div>
    );
};

export interface FileSourceDropzoneStepProps {
    isDragging: boolean;
    onDragChange: (dragging: boolean) => void;
    onDrop: (e: React.DragEvent) => void;
    onSelectFile: () => void;
    onSelectFolder: () => void;
}

/**
 * Backup archive drag-and-drop area plus native file/folder picker cards.
 */
export const FileSourceDropzoneStep: React.FC<FileSourceDropzoneStepProps> = ({
    isDragging,
    onDragChange,
    onDrop,
    onSelectFile,
    onSelectFolder,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4">
            <div
                data-testid="backup-file-dropzone"
                onDragEnter={(e) => {
                    e.preventDefault();
                    onDragChange(true);
                }}
                onDragOver={(e) => e.preventDefault()}
                onDragLeave={(e) => {
                    e.preventDefault();
                    onDragChange(false);
                }}
                onDrop={onDrop}
                className={`p-8 border-2 border-dashed rounded-3xl text-center space-y-3 transition-all ${
                    isDragging
                        ? 'border-theme-primary bg-theme-primary/10'
                        : 'border-theme-subtle bg-slate-50/70'
                }`}
            >
                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center mx-auto">
                    <UploadCloud size={24} />
                </div>
                <div className="space-y-1">
                    <p className="text-sm font-bold text-theme-secondary">
                        {t('backup.dropBackupHereTitle')}
                    </p>
                    <p className="text-xs text-theme-light">
                        {t('backup.dropBackupHereDesc')}
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <button
                    type="button"
                    data-testid="select-backup-file-btn"
                    onClick={onSelectFile}
                    className="p-5 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex items-center gap-4 cursor-pointer shadow-sm"
                >
                    <div className="w-11 h-11 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0">
                        <FileArchive size={22} />
                    </div>
                    <div className="space-y-0.5 min-w-0">
                        <h4 className="text-sm font-black text-theme-secondary">
                            {t('profile.backupRestoreSelectFile')}
                        </h4>
                        <p className="text-xs text-theme-light">
                            {t('profile.backupRestoreSelectFileDesc')}
                        </p>
                    </div>
                </button>

                <button
                    type="button"
                    data-testid="select-backup-folder-btn"
                    onClick={onSelectFolder}
                    className="p-5 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex items-center gap-4 cursor-pointer shadow-sm"
                >
                    <div className="w-11 h-11 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0">
                        <FolderSync size={22} />
                    </div>
                    <div className="space-y-0.5 min-w-0">
                        <h4 className="text-sm font-black text-theme-secondary">
                            {t('profile.backupRestoreSelectFolder')}
                        </h4>
                        <p className="text-xs text-theme-light">
                            {t('profile.backupRestoreSelectFolderDesc')}
                        </p>
                    </div>
                </button>
            </div>
        </div>
    );
};
