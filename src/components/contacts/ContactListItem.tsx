// src/components/contacts/ContactListItem.tsx
import React from 'react';
import { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import {
    Building2,
    Edit3,
    Server,
    Share2,
    ShieldCheck,
    Tag,
    Trash2,
    UserCheck,
} from 'lucide-react';
import { Contact, FrontendSyncDescriptor } from '../../types';
import { Advanced } from '../ui/Advanced';
import { TechId } from '../ui/TechId';
import { UserAvatar } from '../ui/UserAvatar';

export interface ContactListItemProps {
    contact: Contact;
    isGuardian: boolean;
    isGuardianWard: boolean;
    onShareWot: (contact: Contact) => void;
    onEdit: (contact: Contact) => void;
    onDelete: (did: string) => void;
}

const TRUST_BADGE_STYLES: Record<number, { style: string; key: string }> = {
    3: { style: 'bg-emerald-50 text-emerald-800 border-emerald-200', key: 'contacts.trustLevel3' },
    2: { style: 'bg-teal-50 text-teal-800 border-teal-200', key: 'contacts.trustLevel2' },
    1: { style: 'bg-amber-50 text-amber-800 border-amber-200', key: 'contacts.trustLevel1' },
    0: { style: 'bg-sky-50 text-sky-800 border-sky-200', key: 'contacts.trustLevel0' },
};

function getSyncEndpointBadgeMeta(syncEp: FrontendSyncDescriptor, t: TFunction) {
    const rawUrl = syncEp.writeUrl || syncEp.readUrl;
    let host = rawUrl;
    try {
        host = new URL(rawUrl).hostname;
    } catch {
        // Keep raw URL when URL parsing fails for custom test fixtures.
    }

    const isPendingAuth = syncEp.status === 'pending_auth';
    const isFailing =
        syncEp.status === 'unreachable' || (!isPendingAuth && (syncEp.failCount ?? 0) > 0);

    if (isFailing) {
        const isHardUnreachable =
            syncEp.status === 'unreachable' || Boolean(syncEp.failCount && syncEp.failCount >= 3);
        const statusLabel = isHardUnreachable
            ? t('contacts.syncUnreachable')
            : t('contacts.syncWarning');
        return {
            host,
            badgeColor: isHardUnreachable
                ? 'bg-rose-50 text-rose-700 border-rose-200'
                : 'bg-amber-50 text-amber-700 border-amber-200',
            dotColor: isHardUnreachable ? 'bg-rose-500' : 'bg-amber-500',
            statusLabel,
            tooltip: `${host} - ${statusLabel} (${t('contacts.syncFailCount')} ${syncEp.failCount ?? 0})`,
        };
    }

    if (isPendingAuth) {
        const statusLabel = t('contacts.syncPendingAuth', 'Erreichbar, nicht freigeschaltet');
        return {
            host,
            badgeColor: 'bg-amber-50 text-amber-700 border-amber-200',
            dotColor: 'bg-amber-500',
            statusLabel,
            tooltip: `${host} - ${statusLabel}`,
        };
    }

    if (syncEp.status === 'active') {
        const statusLabel = t('contacts.syncAvailable', 'Abgleich bereit');
        const dateStr = syncEp.lastSeenAt
            ? new Date(syncEp.lastSeenAt * 1000).toLocaleTimeString([], {
                  hour: '2-digit',
                  minute: '2-digit',
              })
            : '';
        return {
            host,
            badgeColor: 'bg-emerald-50 text-emerald-700 border-emerald-200',
            dotColor: 'bg-emerald-500',
            statusLabel,
            tooltip: `${host} - ${statusLabel}${dateStr ? ` (${t('contacts.syncLastSeen')} ${dateStr})` : ''}`,
        };
    }

    const statusLabel = t('contacts.syncUnverified', 'Ungeprüft');
    return {
        host,
        badgeColor: 'bg-slate-50 text-slate-600 border-slate-200',
        dotColor: 'bg-slate-400',
        statusLabel,
        tooltip: `${host} (${statusLabel})`,
    };
}

/**
 * Address book contact card with trust badge, guardian release action,
 * Expert-mode DID/sync endpoint pills, and progressive tag disclosure.
 */
export const ContactListItem: React.FC<ContactListItemProps> = ({
    contact,
    isGuardian,
    isGuardianWard,
    onShareWot,
    onEdit,
    onDelete,
}) => {
    const { t } = useTranslation();

    const displayName =
        contact.profile.firstName || contact.profile.lastName
            ? `${contact.profile.firstName || ''} ${contact.profile.lastName || ''}`.trim()
            : contact.profile.organization || t('contacts.unnamedIdentity');

    const trustBadge =
        contact.trustLevel !== null && contact.trustLevel !== undefined
            ? TRUST_BADGE_STYLES[contact.trustLevel]
            : undefined;

    const isPeerListStale =
        Boolean(contact.peerListUpdatedAt) &&
        Date.now() / 1000 - (contact.peerListUpdatedAt ?? 0) > 31_536_000;

    return (
        <div className="group h-full">
            <div className="h-full bg-white border border-theme-subtle rounded-[32px] p-6 flex flex-col group-hover:border-theme-primary/30 group-hover:shadow-premium transition-all">
                <div className="flex items-start justify-between mb-4">
                    <div className="relative">
                        <div className="w-16 h-16 rounded-[24px] overflow-hidden border-2 border-theme-subtle group-hover:border-theme-primary/30 transition-all p-1 bg-white shadow-inner flex items-center justify-center">
                            <UserAvatar
                                size={56}
                                rounded="2xl"
                                pictureUrl={contact.profile.pictureUrl}
                                name={
                                    [contact.profile.firstName, contact.profile.lastName]
                                        .filter(Boolean)
                                        .join(' ') ||
                                    contact.profile.organization ||
                                    contact.did
                                }
                            />
                        </div>
                        <div className="absolute -bottom-1 -right-1 bg-white p-1 rounded-full shadow-sm border border-theme-subtle">
                            <div className="bg-emerald-500 w-3 h-3 rounded-full border-2 border-white"></div>
                        </div>
                    </div>

                    <div className="flex gap-1 opacity-100 md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100 transition-opacity">
                        <Advanced>
                            <button
                                onClick={() => onShareWot(contact)}
                                className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                                title={t('contacts.shareWotForContact')}
                            >
                                <Share2 size={18} />
                            </button>
                        </Advanced>
                        <button
                            onClick={() => onEdit(contact)}
                            className="p-2 hover:bg-theme-subtle/20 rounded-xl text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                            title={t('contacts.editProfile')}
                        >
                            <Edit3 size={18} />
                        </button>
                        <button
                            onClick={() => onDelete(contact.did)}
                            className="p-2 hover:bg-rose-50 rounded-xl text-theme-light hover:text-rose-500 transition-all cursor-pointer"
                            title={t('contacts.removeContact')}
                        >
                            <Trash2 size={18} />
                        </button>
                    </div>
                </div>

                <div className="flex-1">
                    <h3 className="text-lg font-black text-theme-primary tracking-tight leading-tight group-hover:text-theme-accent transition-colors">
                        {displayName}
                    </h3>

                    <Advanced>
                        <div className="flex items-center gap-1.5 mt-2 mb-4">
                            <TechId value={contact.did} kind="did" />
                        </div>
                    </Advanced>

                    {(contact.profile.organization || contact.profile.community) && (
                        <div className="flex items-center gap-2 mb-4 p-2.5 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                            <Building2 size={14} className="text-theme-light" />
                            <span className="text-xs font-bold text-theme-secondary truncate">
                                {contact.profile.organization || contact.profile.community}
                            </span>
                        </div>
                    )}

                    <div className="flex items-center gap-1.5 flex-wrap mb-3">
                        {isGuardian && (
                            <span
                                title={t('backup.addressBookGuardianBadge')}
                                className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider border shadow-sm bg-purple-50 text-purple-800 border-purple-200 flex items-center gap-1"
                            >
                                <ShieldCheck size={10} />
                                {t('backup.socialRecoveryStatusLabel')}
                            </span>
                        )}

                        {isGuardianWard && (
                            <button
                                type="button"
                                data-testid={`open-ward-release-${contact.did}`}
                                onClick={() =>
                                    window.dispatchEvent(
                                        new CustomEvent('humoco:open-guardian-release', {
                                            detail: { ownerDid: contact.did },
                                        }),
                                    )
                                }
                                title={t('backup.guardianWardBadgeHint')}
                                className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider border shadow-sm bg-sky-50 hover:bg-sky-100 text-sky-800 border-sky-200 flex items-center gap-1 cursor-pointer transition-colors"
                            >
                                <UserCheck size={10} />
                                {t('backup.addressBookOpenRelease')}
                            </button>
                        )}

                        <span
                            title={t('contacts.trustLevelHint')}
                            className={`px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider border shadow-sm ${
                                trustBadge ? trustBadge.style : 'bg-slate-50 text-slate-500 border-slate-200'
                            }`}
                        >
                            {t(trustBadge ? trustBadge.key : 'contacts.trustLevelUnrated')}
                        </span>

                        <Advanced>
                            {contact.hasPeerList &&
                                (isPeerListStale ? (
                                    <span
                                        title={t('contacts.peerListStaleHint')}
                                        className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-amber-50 text-amber-700 border border-amber-200 shadow-sm"
                                    >
                                        {t('contacts.peerListStale')}
                                    </span>
                                ) : (
                                    <span className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-indigo-50 text-indigo-700 border border-indigo-200 shadow-sm">
                                        {t('contacts.peerListSynced')}
                                    </span>
                                ))}

                            {contact.syncs && contact.syncs.length > 0 && (
                                <div
                                    data-testid="contact-has-sync-badge"
                                    className="flex flex-wrap items-center gap-1.5"
                                >
                                    <span
                                        className="px-2 py-0.5 rounded-md text-[9px] font-black uppercase tracking-wider bg-emerald-50 text-emerald-700 border border-emerald-200 shadow-sm flex items-center gap-1"
                                        title={t('contacts.hasSyncTooltip')}
                                    >
                                        <Server size={9} />
                                        {t('contacts.hasSync')}
                                    </span>
                                    {contact.syncs.map((syncEp, idx) => {
                                        const meta = getSyncEndpointBadgeMeta(syncEp, t);
                                        return (
                                            <span
                                                key={`${syncEp.writeUrl || syncEp.readUrl}-${idx}`}
                                                className={`px-1.5 py-0.5 rounded-md text-[9px] font-semibold border shadow-sm flex items-center gap-1 ${meta.badgeColor}`}
                                                title={meta.tooltip}
                                            >
                                                <span className={`w-1.5 h-1.5 rounded-full ${meta.dotColor}`} />
                                                <span className="font-mono">{meta.host}</span>
                                                <span className="opacity-75 font-normal">({meta.statusLabel})</span>
                                            </span>
                                        );
                                    })}
                                </div>
                            )}
                        </Advanced>
                    </div>

                    <div className="flex flex-wrap gap-1.5 mt-auto">
                        {contact.tags.slice(0, 1).map((tag) => (
                            <span
                                key={tag}
                                className="flex items-center gap-1 px-2.5 py-1 bg-white border border-theme-subtle/50 rounded-lg text-[9px] uppercase tracking-widest font-black text-theme-light shadow-sm"
                            >
                                <Tag size={8} />
                                {tag}
                            </span>
                        ))}
                        <Advanced>
                            {contact.tags.slice(1).map((tag) => (
                                <span
                                    key={tag}
                                    className="flex items-center gap-1 px-2.5 py-1 bg-white border border-theme-subtle/50 rounded-lg text-[9px] uppercase tracking-widest font-black text-theme-light shadow-sm"
                                >
                                    <Tag size={8} />
                                    {tag}
                                </span>
                            ))}
                            {contact.tags.length === 0 && (
                                <span className="text-[9px] font-black uppercase tracking-widest text-theme-light/40 italic">
                                    {t('contacts.noTagsAssigned')}
                                </span>
                            )}
                        </Advanced>
                    </div>
                </div>
            </div>
        </div>
    );
};
