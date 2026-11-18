// src/components/contacts/PendingHandshakesList.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Check, ChevronDown, ChevronUp, Handshake, RefreshCw } from 'lucide-react';
import { FrontendPendingWotHandshake } from '../../types';
import { truncateUserId } from '../../utils/userIdHelper';
import { Button } from '../ui/Button';
import { TechId } from '../ui/TechId';
import { TrustLevelRadioCards } from './TrustLevelRadioCards';

export interface PendingHandshakesListProps {
    pendingHandshakes: FrontendPendingWotHandshake[];
    isExpanded: boolean;
    onToggleExpanded: () => void;
    acceptedLevels: Record<string, number>;
    onLevelChange: (msgId: string, level: number) => void;
    returnFlags: Record<string, boolean>;
    onReturnFlagChange: (msgId: string, checked: boolean) => void;
    isAcceptingHandshake: string | null;
    onAcceptHandshake: (handshake: FrontendPendingWotHandshake) => void;
    onDismissHandshake: (msgId: string) => void;
}

/**
 * Collapsible list of incoming WoT trust confirmation requests.
 */
export const PendingHandshakesList: React.FC<PendingHandshakesListProps> = ({
    pendingHandshakes,
    isExpanded,
    onToggleExpanded,
    acceptedLevels,
    onLevelChange,
    returnFlags,
    onReturnFlagChange,
    isAcceptingHandshake,
    onAcceptHandshake,
    onDismissHandshake,
}) => {
    const { t } = useTranslation();

    if (pendingHandshakes.length === 0) {
        return null;
    }

    return (
        <div
            className="p-4 sm:p-5 bg-sky-50/80 border border-sky-200 rounded-3xl space-y-4"
            data-testid="pending-handshakes-section"
        >
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div className="flex items-center gap-3">
                    <div className="w-10 h-10 rounded-2xl bg-sky-100 border border-sky-200 text-sky-700 flex items-center justify-center shrink-0">
                        <Handshake size={20} />
                    </div>
                    <div>
                        <div className="flex items-center gap-2">
                            <h4 className="text-sm font-extrabold text-sky-950">
                                {t('contacts.incomingHandshakesTitle', 'Eingehende Vertrauensbestätigungen')}
                            </h4>
                            <span className="px-2 py-0.5 rounded-full text-xs font-black bg-sky-600 text-white">
                                {pendingHandshakes.length}
                            </span>
                        </div>
                        <p className="text-xs text-sky-900/80 mt-0.5">
                            {pendingHandshakes.length === 1
                                ? t('contacts.incomingHandshakesBanner', '1 Kontakt möchte dich als vertraut eintragen.')
                                : t('contacts.incomingHandshakesBannerPlural', '{{count}} Kontakte möchten dich als vertraut eintragen.', { count: pendingHandshakes.length })}
                        </p>
                    </div>
                </div>
                <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    onClick={onToggleExpanded}
                    className="self-start sm:self-auto bg-white border-sky-200 text-sky-900 font-bold"
                    data-testid="toggle-handshakes-button"
                >
                    <span>
                        {isExpanded
                            ? t('contacts.hideHandshakesBtn', 'Ausblenden')
                            : t('contacts.viewHandshakesBtn', 'Ansehen')}
                    </span>
                    {isExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                </Button>
            </div>

            {isExpanded && (
                <div className="space-y-4 pt-2 border-t border-sky-200/60 animate-in fade-in duration-200">
                    {pendingHandshakes.map((h) => {
                        const name = h.senderName || truncateUserId(h.senderDid);
                        const currentLevel =
                            acceptedLevels[h.msgId] ?? (h.proposedLevel <= 3 ? h.proposedLevel : 1);
                        const returnChecked = returnFlags[h.msgId] ?? h.replyRequested ?? true;
                        const isAccepting = isAcceptingHandshake === h.msgId;

                        return (
                            <div
                                key={h.msgId}
                                className="p-5 bg-white rounded-2xl border border-sky-100 shadow-xs space-y-4"
                                data-testid={`handshake-item-${h.msgId}`}
                            >
                                <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-3">
                                    <div className="space-y-1">
                                        <p className="font-extrabold text-sm text-theme-primary">
                                            {t(
                                                'contacts.handshakeProposalPhrase',
                                                '{{name}} möchte euch gegenseitig als vertraut eintragen.',
                                                { name },
                                            )}
                                        </p>
                                        <TechId value={h.senderDid} kind="did" />
                                    </div>
                                </div>

                                <div className="space-y-2">
                                    <label className="block text-xs font-bold text-theme-secondary">
                                        {t('contacts.acceptAsLevel', 'Wie möchtest du diesen Kontakt einstufen?')}
                                    </label>
                                    <TrustLevelRadioCards
                                        value={currentLevel}
                                        onChange={(lvl) => onLevelChange(h.msgId, lvl ?? 1)}
                                        allowUnrated={false}
                                        idPrefix={`handshake-${h.msgId}`}
                                    />
                                </div>

                                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pt-2 border-t border-slate-100">
                                    <label className="flex items-center gap-2 text-xs font-medium text-theme-secondary cursor-pointer">
                                        <input
                                            type="checkbox"
                                            checked={returnChecked}
                                            onChange={(e) => onReturnFlagChange(h.msgId, e.target.checked)}
                                            className="rounded accent-sky-600"
                                        />
                                        <span>
                                            {t(
                                                'contacts.sendReturnHandshakeLabel',
                                                'Gegenseitige Vertrauensbestätigung zurücksenden',
                                            )}
                                        </span>
                                    </label>

                                    <div className="flex items-center gap-2 self-end sm:self-auto">
                                        <Button
                                            size="xs"
                                            variant="outline"
                                            onClick={() => onDismissHandshake(h.msgId)}
                                            data-testid="dismiss-handshake-button"
                                            className="text-theme-light hover:text-rose-600"
                                        >
                                            {t('common.dismiss', 'Verwerfen')}
                                        </Button>
                                        <Button
                                            size="xs"
                                            variant="primary"
                                            disabled={isAccepting}
                                            onClick={() => onAcceptHandshake(h)}
                                            data-testid="accept-handshake-button"
                                            className="bg-sky-600 hover:bg-sky-700 text-white gap-1"
                                        >
                                            {isAccepting ? (
                                                <RefreshCw size={12} className="animate-spin" />
                                            ) : (
                                                <Check size={12} />
                                            )}
                                            <span>{t('common.confirm', 'Bestätigen')}</span>
                                        </Button>
                                    </div>
                                </div>
                            </div>
                        );
                    })}
                </div>
            )}
        </div>
    );
};
