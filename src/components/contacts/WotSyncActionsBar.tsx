// src/components/contacts/WotSyncActionsBar.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    ChevronDown,
    ChevronUp,
    RefreshCw,
    Share2,
    ShieldCheck,
    SlidersHorizontal,
    Upload,
    UserCheck,
} from 'lucide-react';
import { WotSummaryDto } from '../../types';
import { Button } from '../ui/Button';
import { InfoHint } from '../ui/InfoHint';
import { ModalShell } from '../ui/ModalShell';

export interface WotSyncActionsBarProps {
    isSyncingServices: boolean;
    isCoolingDown: boolean;
    cooldownSeconds: number;
    isNetworkOptionsOpen: boolean;
    onSyncServices: () => void;
    onToggleNetworkOptions: () => void;
    onOpenGuardianSetup: () => void;
}

/**
 * Secondary/advanced header actions for sync refresh, WoT reach options,
 * and social recovery guardian workflows.
 */
export const WotSyncActionsBar: React.FC<WotSyncActionsBarProps> = ({
    isSyncingServices,
    isCoolingDown,
    cooldownSeconds,
    isNetworkOptionsOpen,
    onSyncServices,
    onToggleNetworkOptions,
    onOpenGuardianSetup,
}) => {
    const { t } = useTranslation();

    const syncLabel = isCoolingDown
        ? t('contacts.syncRateLimitWait', {
              seconds: cooldownSeconds,
              defaultValue: `Bitte noch ${cooldownSeconds}s warten...`,
          })
        : isSyncingServices
          ? t('settings.syncing', 'Synchronisiere...')
          : t('contacts.syncServicesButton', 'Über Sync-Dienste synchronisieren');

    return (
        <>
            <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={onSyncServices}
                disabled={isSyncingServices || isCoolingDown}
                className="gap-1.5"
                title={syncLabel}
                aria-label={syncLabel}
                data-testid="sync-services-button"
            >
                <RefreshCw size={16} className={isSyncingServices ? 'animate-spin' : ''} />
                <span>
                    {isCoolingDown
                        ? `${cooldownSeconds}s`
                        : isSyncingServices
                          ? t('settings.syncing', 'Synchronisiere...')
                          : t('contacts.refreshSyncBtn', 'Aktualisieren')}
                </span>
            </Button>
            <Button
                type="button"
                variant={isNetworkOptionsOpen ? 'primary' : 'secondary'}
                size="sm"
                onClick={onToggleNetworkOptions}
                className="gap-1.5"
                title={t('contacts.wotCustomizeOptions', 'Netzwerk-Optionen anpassen')}
                aria-label={t('contacts.wotCustomizeOptions', 'Netzwerk-Optionen anpassen')}
                data-testid="toggle-network-options-button"
            >
                <SlidersHorizontal size={16} />
                <span>{t('contacts.wotNetworkAndReachBtn', 'Netzwerk & Reichweite')}</span>
                {isNetworkOptionsOpen ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
            </Button>
            <Button
                size="sm"
                variant="secondary"
                onClick={onOpenGuardianSetup}
                className="gap-2"
                title={t('backup.manageGuardiansButton')}
                data-testid="manage-guardians-btn"
            >
                <ShieldCheck size={18} />
                <span>{t('backup.socialRecoveryStatusLabel')}</span>
            </Button>
            <Button
                size="sm"
                variant="secondary"
                onClick={() => window.dispatchEvent(new CustomEvent('humoco:open-guardian-release'))}
                className="gap-2"
                title={t('backup.helpContactRecoveryBtn')}
                data-testid="open-guardian-release-btn"
            >
                <UserCheck size={18} />
                <span>{t('backup.helpContactRecoveryBtn')}</span>
            </Button>
        </>
    );
};

export interface WotTrustPackageModalsProps {
    wotSummary: WotSummaryDto | null;
    isTrustListModalOpen: boolean;
    onCloseTrustListModal: () => void;
    onImportWot: () => void;
    onOpenExportModal: () => void;
    isExportModalOpen: boolean;
    onCloseExportModal: () => void;
    exportMode: 'contacts' | 'password';
    onExportModeChange: (mode: 'contacts' | 'password') => void;
    exportPassword: string;
    onExportPasswordChange: (password: string) => void;
    isExporting: boolean;
    onConfirmExport: () => void;
    isWotPasswordModalOpen: boolean;
    onCloseWotPasswordModal: () => void;
    wotContainerPassword: string;
    onWotContainerPasswordChange: (password: string) => void;
    isImportingWot: boolean;
    onConfirmEncryptedImport: () => void;
}

/**
 * Offline .wot trust-list menu, export mode selector, and encrypted import password modals.
 */
export const WotTrustPackageModals: React.FC<WotTrustPackageModalsProps> = ({
    wotSummary,
    isTrustListModalOpen,
    onCloseTrustListModal,
    onImportWot,
    onOpenExportModal,
    isExportModalOpen,
    onCloseExportModal,
    exportMode,
    onExportModeChange,
    exportPassword,
    onExportPasswordChange,
    isExporting,
    onConfirmExport,
    isWotPasswordModalOpen,
    onCloseWotPasswordModal,
    wotContainerPassword,
    onWotContainerPasswordChange,
    isImportingWot,
    onConfirmEncryptedImport,
}) => {
    const { t } = useTranslation();

    return (
        <>
            <ModalShell
                isOpen={isTrustListModalOpen}
                onClose={onCloseTrustListModal}
                maxWidth="md"
                title={t('contacts.trustListMenuBtn', 'Vertrauensliste')}
                description={t('contacts.trustListMenuDesc', 'Vertrauenslisten offline teilen oder einlesen')}
            >
                <div className="space-y-4 py-2">
                    <InfoHint topic="visibilityReach" variant="banner" />

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                        <button
                            type="button"
                            onClick={() => {
                                onCloseTrustListModal();
                                onImportWot();
                            }}
                            className="p-4 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-2 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                        >
                            <div className="w-10 h-10 rounded-xl bg-theme-primary/5 text-theme-primary flex items-center justify-center shrink-0">
                                <Upload size={20} />
                            </div>
                            <div>
                                <h4 className="font-extrabold text-sm text-theme-primary">
                                    {t('contacts.importWotButton', 'Vertrauensliste einlesen')}
                                </h4>
                                <p className="text-xs text-theme-light mt-0.5 leading-relaxed">
                                    {t('contacts.trustListImportDesc', 'Eine empfangene Vertrauensliste importieren')}
                                </p>
                            </div>
                        </button>

                        <button
                            type="button"
                            onClick={() => {
                                onCloseTrustListModal();
                                onOpenExportModal();
                            }}
                            className="p-4 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-2 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                        >
                            <div className="w-10 h-10 rounded-xl bg-theme-primary/5 text-theme-primary flex items-center justify-center shrink-0">
                                <Share2 size={20} />
                            </div>
                            <div>
                                <h4 className="font-extrabold text-sm text-theme-primary">
                                    {t('contacts.exportWotButton', 'Vertrauensliste teilen')}
                                </h4>
                                <p className="text-xs text-theme-light mt-0.5 leading-relaxed">
                                    {t('contacts.trustListExportDesc', 'Deine Vertrauenseinstufungen als Datei exportieren')}
                                </p>
                            </div>
                        </button>
                    </div>
                </div>
            </ModalShell>

            <ModalShell
                isOpen={isExportModalOpen}
                onClose={onCloseExportModal}
                title={t('contacts.exportWotModalTitle')}
                description={t('contacts.exportWotModalDesc')}
                footer={
                    <div className="flex justify-end gap-3">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onCloseExportModal}
                            disabled={isExporting}
                        >
                            {t('common.cancel')}
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={onConfirmExport}
                            disabled={isExporting || (exportMode === 'password' && !exportPassword.trim())}
                        >
                            {isExporting ? t('common.loading') : t('contacts.exportWotButton')}
                        </Button>
                    </div>
                }
            >
                <div className="space-y-4">
                    <div className="grid grid-cols-1 gap-2.5">
                        <button
                            type="button"
                            disabled={(wotSummary?.ratedContactsCount ?? 0) === 0}
                            onClick={() => onExportModeChange('contacts')}
                            className={`w-full text-left p-3 rounded-xl border transition-all flex flex-col gap-1 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed ${
                                exportMode === 'contacts'
                                    ? 'bg-emerald-50/70 border-emerald-500 text-emerald-950 ring-2 ring-emerald-400/20 shadow-sm'
                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/30'
                            }`}
                        >
                            <span className="font-bold text-xs flex items-center justify-between">
                                <span>
                                    {t('contacts.exportModeAllContacts', {
                                        count: wotSummary?.ratedContactsCount ?? 0,
                                    })}
                                </span>
                                {exportMode === 'contacts' && (
                                    <span className="text-[10px] font-black uppercase tracking-wider text-emerald-600">
                                        ✓
                                    </span>
                                )}
                            </span>
                            <span className="text-[11px] text-theme-light leading-tight">
                                {t('contacts.exportModeAllContactsDesc')}
                            </span>
                        </button>

                        <button
                            type="button"
                            onClick={() => onExportModeChange('password')}
                            className={`w-full text-left p-3 rounded-xl border transition-all flex flex-col gap-1 cursor-pointer ${
                                exportMode === 'password'
                                    ? 'bg-theme-secondary/5 border-theme-secondary text-theme-secondary ring-2 ring-theme-secondary/20 shadow-sm'
                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/30'
                            }`}
                        >
                            <span className="font-bold text-xs flex items-center justify-between">
                                <span>{t('contacts.exportModePassword')}</span>
                                {exportMode === 'password' && (
                                    <span className="text-[10px] font-black uppercase tracking-wider text-theme-secondary">
                                        ✓
                                    </span>
                                )}
                            </span>
                            <span className="text-[11px] text-theme-light leading-tight">
                                {t('contacts.exportModePasswordDesc')}
                            </span>
                        </button>
                    </div>

                    {exportMode === 'password' && (
                        <div className="pt-2 animate-in fade-in duration-200">
                            <input
                                type="password"
                                value={exportPassword}
                                onChange={(e) => onExportPasswordChange(e.target.value)}
                                placeholder={t('contacts.exportWotPasswordPlaceholder')}
                                className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 text-sm shadow-sm"
                                autoFocus
                            />
                        </div>
                    )}
                </div>
            </ModalShell>

            <ModalShell
                isOpen={isWotPasswordModalOpen}
                onClose={onCloseWotPasswordModal}
                title={t('transfer.wotImport.title')}
                description={t('transfer.importFile.passwordPlaceholder')}
                footer={
                    <div className="flex justify-end gap-3">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onCloseWotPasswordModal}
                            disabled={isImportingWot}
                        >
                            {t('common.cancel')}
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={onConfirmEncryptedImport}
                            disabled={isImportingWot}
                        >
                            {isImportingWot ? t('common.loading') : t('transfer.importFile.confirmImport')}
                        </Button>
                    </div>
                }
            >
                <div className="space-y-4">
                    <input
                        type="password"
                        value={wotContainerPassword}
                        onChange={(e) => onWotContainerPasswordChange(e.target.value)}
                        placeholder={t('transfer.importFile.passwordPlaceholder')}
                        className="w-full bg-white border border-theme-subtle rounded-xl px-4 py-2.5 text-theme-secondary placeholder:text-theme-placeholder focus:outline-none focus:ring-2 focus:ring-theme-primary/20 text-sm shadow-sm"
                        autoFocus
                    />
                </div>
            </ModalShell>
        </>
    );
};
