// src/components/marketplace/ProfileCardVisibilitySection.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Eye, EyeOff, Users } from 'lucide-react';
import { CardFieldVisibility, FieldAudience } from '../../types';
import { Advanced } from '../ui/Advanced';
import { InfoHint } from '../ui/InfoHint';
import { PresetOrCustom } from '../ui/PresetOrCustom';
import { CardPresetId, detectCardPreset } from './useProfileCardEditor';

export interface VisibilityCycleButtonProps {
    field: keyof CardFieldVisibility;
    visibility: CardFieldVisibility;
    onCycle: (key: keyof CardFieldVisibility) => void;
}

const AUDIENCE_STYLE_MAP: Record<FieldAudience, string> = {
    everyone: 'bg-emerald-50 text-emerald-800 border-emerald-200 hover:bg-emerald-100',
    contacts: 'bg-sky-50 text-sky-800 border-sky-200 hover:bg-sky-100',
    nobody: 'bg-slate-100 text-slate-700 border-slate-200 hover:bg-slate-200',
};

/**
 * Expert-mode pill button cycling per-field audience visibility while keeping street clamped off 'everyone'.
 */
export const VisibilityCycleButton: React.FC<VisibilityCycleButtonProps> = ({
    field,
    visibility,
    onCycle,
}) => {
    const { t } = useTranslation();
    const current = visibility[field];

    const label =
        current === 'contacts'
            ? t('marketplace.visibilityContacts', 'Nur Kontakte')
            : current === 'nobody'
              ? t('marketplace.visibilityNobody', 'Niemand')
              : t('marketplace.visibilityEveryone', 'Alle');

    const icon =
        current === 'contacts' ? (
            <Users size={13} className="text-sky-600" />
        ) : current === 'nobody' ? (
            <EyeOff size={13} className="text-slate-500" />
        ) : (
            <Eye size={13} className="text-emerald-600" />
        );

    const titleText =
        field === 'street'
            ? t('marketplace.visibilityCycleStreetTooltip', 'Klicken zum Wechseln: Nur Kontakte ➔ Niemand')
            : t('marketplace.visibilityCycleTooltip', 'Klicken zum Wechseln: Alle ➔ Nur Kontakte ➔ Niemand');

    return (
        <Advanced>
            <button
                type="button"
                onClick={() => onCycle(field)}
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-semibold border transition-all cursor-pointer ${AUDIENCE_STYLE_MAP[current]}`}
                title={titleText}
                data-testid={`visibility-btn-${field}`}
            >
                {icon}
                <span>{label}</span>
            </button>
        </Advanced>
    );
};

export interface ProfileCardVisibilitySectionProps {
    published: boolean;
    onPublishedChange: (published: boolean) => void;
    visibility: CardFieldVisibility;
    onApplyPreset: (presetId: CardPresetId) => void;
}

export const ProfileCardVisibilitySection: React.FC<ProfileCardVisibilitySectionProps> = ({
    published,
    onPublishedChange,
    visibility,
    onApplyPreset,
}) => {
    const { t } = useTranslation();

    return (
        <>
            <label className="flex items-start gap-3 p-4 rounded-2xl border border-theme-primary/25 bg-theme-primary/5 cursor-pointer hover:bg-theme-primary/10 transition-colors">
                <input
                    type="checkbox"
                    checked={published}
                    onChange={(e) => onPublishedChange(e.target.checked)}
                    data-testid="editor-publish-checkbox"
                    className="mt-0.5 w-4 h-4 rounded accent-theme-primary cursor-pointer shrink-0"
                />
                <div className="min-w-0 space-y-0.5">
                    <span className="block text-xs font-bold text-theme-secondary">
                        {t('marketplace.publishCardSwitchLabel', 'Visitenkarte veröffentlichen & weiterreichen')}
                    </span>
                    <span className="block text-[11px] text-theme-light leading-relaxed">
                        {t(
                            'marketplace.publishCardSwitchHint',
                            'Aus: Nur lokal auf diesem Gerät gespeichert. · An: Signiert, teilbar und gemäß Sichtbarkeit im Netzwerk auffindbar.',
                        )}
                    </span>
                </div>
            </label>

            <div className="p-4 rounded-2xl bg-bg-app border border-theme-subtle space-y-3">
                <PresetOrCustom<CardPresetId>
                    value={detectCardPreset(visibility)}
                    onChange={onApplyPreset}
                    gridClassName="grid grid-cols-1 sm:grid-cols-3 gap-3"
                    presets={[
                        {
                            id: 'private',
                            title: t('marketplace.cardPresetPrivateTitle', 'Privat'),
                            description: t(
                                'marketplace.cardPresetPrivateDesc',
                                'Nur Name & Ort öffentlich, Kontaktangaben nur für deine Kontakte.',
                            ),
                        },
                        {
                            id: 'local',
                            title: t('marketplace.cardPresetLocalTitle', 'Lokal anbieten'),
                            description: t(
                                'marketplace.cardPresetLocalDesc',
                                'Name, Angebote, Bild & Ort öffentlich – Telefon, E-Mail & Straße nur für Kontakte.',
                            ),
                            recommended: true,
                        },
                        {
                            id: 'business',
                            title: t('marketplace.cardPresetBusinessTitle', 'Geschäftlich'),
                            description: t(
                                'marketplace.cardPresetBusinessDesc',
                                'Kontaktdaten öffentlich auffindbar, Straße nur für Kontakte.',
                            ),
                        },
                    ]}
                />
                <InfoHint topic="cardVisibility" variant="inline" />
            </div>
        </>
    );
};
