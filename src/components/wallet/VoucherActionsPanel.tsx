// src/components/wallet/VoucherActionsPanel.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import {
    Shield,
    FileSignature,
    AlertTriangle,
    Info,
    Terminal as Json,
    CheckCircle2,
    Clock,
    ShieldAlert,
    Store,
    FolderOpen,
    X,
} from 'lucide-react';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { AlertBanner } from '../ui/AlertBanner';
import { TrustBadge } from '../ui/TrustBadge';
import { Advanced } from '../ui/Advanced';
import { SignatureRequestBanner } from '../voucher/SignatureRequestBanner';
import type { VoucherDetails, VoucherL2StatusDto } from '../../types';

export interface VoucherStatusInfo {
    id: string;
    label: string;
    color: string;
    bgColor: string;
    icon: React.ElementType;
}

interface VoucherHeaderActionsProps {
    statusId: string;
    canRedeemAtIssuer: boolean;
    canClaimAtGuarantor: boolean;
    isCheckingL2: boolean;
    showJson: boolean;
    onRedeemAtIssuer: () => void;
    onToggleClaimPanel: () => void;
    onCheckL2Status: () => void;
    onOpenExportModal: () => void;
    onToggleJson: () => void;
}

export function VoucherHeaderActions({
    statusId,
    canRedeemAtIssuer,
    canClaimAtGuarantor,
    isCheckingL2,
    showJson,
    onRedeemAtIssuer,
    onToggleClaimPanel,
    onCheckL2Status,
    onOpenExportModal,
    onToggleJson,
}: VoucherHeaderActionsProps) {
    const { t } = useTranslation();

    return (
        <>
            {canRedeemAtIssuer && (
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={onRedeemAtIssuer}
                    className="gap-2 text-xs font-bold bg-white text-theme-primary border border-theme-subtle hover:bg-theme-subtle/20 shadow-sm"
                >
                    <Store size={15} />
                    {t('voucher.redeemAtIssuerBtn')}
                </Button>
            )}
            {canClaimAtGuarantor && (
                <Button
                    data-testid="claim-at-guarantor-btn"
                    variant="secondary"
                    size="sm"
                    onClick={onToggleClaimPanel}
                    className="gap-2 text-xs font-bold bg-amber-50 text-amber-900 border border-amber-300 hover:bg-amber-100 shadow-sm"
                >
                    <Shield size={15} />
                    {t('voucher.claimAtGuarantorBtn')}
                </Button>
            )}
            {statusId === 'active' && (
                <Advanced>
                    <Button
                        variant="outline"
                        size="sm"
                        onClick={onCheckL2Status}
                        disabled={isCheckingL2}
                        className="gap-1.5 text-xs font-bold"
                    >
                        {isCheckingL2 ? (
                            <Clock className="animate-spin" size={14} />
                        ) : (
                            <Shield size={14} />
                        )}
                        {isCheckingL2 ? t('voucher.checkingL2Status') : t('voucher.checkL2Status')}
                    </Button>
                </Advanced>
            )}
            {statusId === 'incomplete' && (
                <Button variant="primary" size="sm" onClick={onOpenExportModal} className="gap-2">
                    <FileSignature size={16} /> {t('voucher.signButton')}
                </Button>
            )}
            <Advanced>
                <button
                    onClick={onToggleJson}
                    className={`p-2 rounded-xl transition-all ${
                        showJson
                            ? 'bg-theme-secondary text-white'
                            : 'bg-white text-theme-light hover:bg-white/80 border border-theme-subtle/50 shadow-sm'
                    }`}
                    title={t('voucher.toggleJson')}
                    aria-label={t('voucher.toggleJson')}
                >
                    <Json size={18} />
                </button>
            </Advanced>
        </>
    );
}

interface VoucherActionsPanelProps {
    details: VoucherDetails;
    statusInfo: VoucherStatusInfo;
    isQuarantined: boolean;
    proofId: string | null;
    signatureImportFeedback: { type: 'success' | 'error'; msg: string } | null;
    onDismissSignatureFeedback: () => void;
    savedAskFilePath: string | null;
    onDismissSavedAskPath: () => void;
    l2Status: VoucherL2StatusDto | null;
    l2Error: string | null;
    signatureHint: string | null;
    guarantorProgress: string | null;
    showJson: boolean;
    onViewProof: (proofId: string) => void;
    onOpenExportModal: () => void;
    onImportSignatureFile: () => void;
}

export function VoucherActionsPanel({
    details,
    statusInfo,
    isQuarantined,
    proofId,
    signatureImportFeedback,
    onDismissSignatureFeedback,
    savedAskFilePath,
    onDismissSavedAskPath,
    l2Status,
    l2Error,
    signatureHint,
    guarantorProgress,
    showJson,
    onViewProof,
    onOpenExportModal,
    onImportSignatureFile,
}: VoucherActionsPanelProps) {
    const { t } = useTranslation();
    const voucher = details.voucher;
    const StatusIcon = statusInfo.icon;

    return (
        <>
            {signatureImportFeedback && (
                <AlertBanner
                    variant={signatureImportFeedback.type}
                    onDismiss={onDismissSignatureFeedback}
                >
                    {signatureImportFeedback.msg}
                </AlertBanner>
            )}

            {savedAskFilePath && (
                <div className="p-4 bg-emerald-50 border border-emerald-200 rounded-2xl flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 text-emerald-900 animate-in fade-in duration-300">
                    <div className="flex items-start gap-3">
                        <CheckCircle2 size={20} className="text-emerald-600 shrink-0 mt-0.5" />
                        <div>
                            <p className="font-bold text-sm text-emerald-950">
                                {t('voucher.askSavedSuccessTitle')}
                            </p>
                            <p className="text-xs text-emerald-800/90 mt-0.5 leading-relaxed">
                                {t('voucher.askSavedSuccessDesc')}
                            </p>
                        </div>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => revealItemInDir(savedAskFilePath).catch(() => {})}
                            className="bg-white hover:bg-white text-xs font-bold gap-1.5 shadow-sm"
                        >
                            <FolderOpen size={14} />
                            {t('common.showInFolder')}
                        </Button>
                        <button
                            onClick={onDismissSavedAskPath}
                            className="p-1.5 text-emerald-700 hover:text-emerald-900 rounded-lg hover:bg-emerald-100/50"
                            title={t('common.close')}
                            aria-label={t('common.close')}
                        >
                            <X size={16} />
                        </button>
                    </div>
                </div>
            )}

            <Card
                variant={isQuarantined ? 'danger' : details.isTestVoucher ? 'warning' : 'glass'}
                className="border-none shadow-premium overflow-hidden"
            >
                <div className="p-8 flex flex-col md:flex-row items-center justify-between gap-8">
                    <div className="flex items-center gap-6">
                        <div
                            className={`p-5 rounded-3xl shadow-xl transition-transform hover:scale-105 ${statusInfo.bgColor} ${statusInfo.color}`}
                        >
                            <StatusIcon size={40} />
                        </div>
                        <div>
                            <div className="flex items-baseline gap-2">
                                <h1 className="text-5xl font-black text-theme-primary tracking-tighter">
                                    {details.currentAmount ?? voucher.nominalValue.amount}
                                </h1>
                                {details.currentAmount &&
                                    details.currentAmount !== voucher.nominalValue.amount && (
                                        <span className="text-sm font-bold text-theme-light">
                                            / {voucher.nominalValue.amount}
                                        </span>
                                    )}
                                <span className="text-xl font-bold text-theme-light uppercase tracking-widest">
                                    {details.displayCurrency}
                                </span>
                            </div>
                            <div className="flex flex-wrap items-center gap-2 mt-2">
                                <span
                                    className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${statusInfo.bgColor} ${statusInfo.color}`}
                                >
                                    {statusInfo.label}
                                </span>
                                {details.isTestVoucher && (
                                    <span className="px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest bg-rose-500 text-white shadow-lg shadow-rose-200">
                                        {t('voucher.testVoucher')}
                                    </span>
                                )}
                                <TrustBadge trust={details.trust} size="md" />
                                {(details.reducedGuarantorProtection ||
                                    l2Status?.reducedGuarantorProtection) && (
                                    <span
                                        data-testid="reduced-guarantor-protection-badge"
                                        title={t('voucher.reducedGuarantorProtectionHint')}
                                        className="inline-flex items-center gap-1 px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-wider bg-amber-100 text-amber-900 border border-amber-300"
                                    >
                                        <AlertTriangle size={12} />
                                        {t('voucher.reducedGuarantorProtectionBadge')}
                                    </span>
                                )}
                                {l2Status && (
                                    <Advanced>
                                        <span
                                            className={`px-3 py-1 rounded-full text-[10px] font-black uppercase tracking-widest ${
                                                l2Status.status === 'unspent'
                                                    ? 'bg-emerald-100 text-emerald-800 border border-emerald-200'
                                                    : l2Status.status === 'spent' ||
                                                        l2Status.status === 'conflict'
                                                      ? 'bg-rose-100 text-rose-800 border border-rose-200'
                                                      : l2Status.status === 'offline'
                                                        ? 'bg-amber-100 text-amber-800 border border-amber-200'
                                                        : 'bg-slate-100 text-slate-800 border border-slate-200'
                                            }`}
                                        >
                                            {l2Status.status === 'unspent' && t('voucher.l2StatusUnspent')}
                                            {l2Status.status === 'spent' && t('voucher.l2StatusSpent')}
                                            {l2Status.status === 'conflict' && t('voucher.statusQuarantined')}
                                            {l2Status.status === 'offline' && t('voucher.l2StatusOffline')}
                                            {l2Status.status === 'inconclusive' && t('voucher.l2StatusInconclusive')}
                                            {l2Status.status === 'offline_only' &&
                                                t('voucher.l2StatusOfflineOnly', 'Offline-only (N=0)')}
                                            {l2Status.status === 'no_servers' &&
                                                t('voucher.l2StatusNoServers')}
                                        </span>
                                    </Advanced>
                                )}
                            </div>
                        </div>
                    </div>

                    <div className="flex flex-col gap-3 w-full md:w-auto">
                        {l2Error && (
                            <Advanced>
                                <p className="text-[10px] font-bold text-rose-500 max-w-[200px] truncate">
                                    {l2Error}
                                </p>
                            </Advanced>
                        )}
                        {isQuarantined && (
                            <Button
                                variant="secondary"
                                size="sm"
                                className="bg-white/20 text-white hover:bg-white/30 border-white/30 backdrop-blur-md"
                                onClick={() => proofId && onViewProof(proofId)}
                                disabled={!proofId}
                            >
                                <ShieldAlert size={16} className="mr-2" /> {t('voucher.viewProof')}
                            </Button>
                        )}
                        <div className="bg-white/20 backdrop-blur-md px-4 py-2 rounded-2xl border border-white/30">
                            <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] mb-1">
                                {t('voucher.standardLabel')}
                            </p>
                            <p className="text-xs font-bold text-theme-secondary truncate max-w-[200px]">
                                {details.displayStandardName}
                            </p>
                        </div>
                    </div>
                </div>

                {signatureHint && (
                    <div className="bg-blue-500/10 border-t border-blue-500/20 p-4 px-8 flex items-start gap-4">
                        <Info size={18} className="text-blue-500 mt-1 shrink-0" />
                        <p className="text-sm text-blue-900 font-medium leading-relaxed">
                            <span className="font-black uppercase text-[10px] tracking-widest block mb-1">
                                {t('voucher.signatureHint')}
                            </span>
                            {signatureHint}
                        </p>
                    </div>
                )}
            </Card>

            {statusInfo.id === 'incomplete' && (
                <SignatureRequestBanner
                    onAction={onOpenExportModal}
                    onImportSignature={onImportSignatureFile}
                    progressText={guarantorProgress}
                />
            )}

            {showJson && (
                <Advanced>
                    <Card
                        variant="glass"
                        className="bg-slate-900 border-none shadow-2xl p-0 overflow-hidden"
                    >
                        <pre className="text-[11px] leading-relaxed text-indigo-300 font-mono p-6 overflow-x-auto selection:bg-indigo-500 selection:text-white max-h-[600px]">
                            {JSON.stringify(details.voucher, null, 2)}
                        </pre>
                    </Card>
                </Advanced>
            )}
        </>
    );
}
