// src/components/settings/sync/QuotaPolicyEditors.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { HelpCircle } from 'lucide-react';
import { FrontendMemberPolicyDto } from '../../../types';

interface PolicyHelpPopoverProps {
    title: string;
    content: string;
}

const PolicyHelpPopover: React.FC<PolicyHelpPopoverProps> = ({ title, content }) => {
    const [isOpen, setIsOpen] = useState(false);

    return (
        <span className="relative inline-flex items-center ml-1">
            <button
                type="button"
                onClick={() => setIsOpen(!isOpen)}
                onMouseEnter={() => setIsOpen(true)}
                onMouseLeave={() => setIsOpen(false)}
                className="text-theme-light hover:text-theme-primary transition-colors focus:outline-none p-0.5 rounded-full hover:bg-theme-primary/5 cursor-pointer"
                title={title}
                aria-label={title}
            >
                <HelpCircle size={14} />
            </button>
            {isOpen && (
                <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 w-64 p-2.5 bg-slate-900 text-white text-[11px] font-normal leading-relaxed rounded-xl shadow-lg z-30 pointer-events-none animate-in fade-in zoom-in-95 duration-150">
                    <span className="font-bold text-slate-100 block mb-0.5">{title}</span>
                    <span className="opacity-90 block">{content}</span>
                </span>
            )}
        </span>
    );
};

interface QuotaNumberFieldProps {
    id: string;
    label: string;
    min: number;
    max: number;
    clampMax?: number;
    value: number;
    onChange: (val: number) => void;
}

const QuotaNumberField: React.FC<QuotaNumberFieldProps> = ({
    id,
    label,
    min,
    max,
    clampMax,
    value,
    onChange,
}) => (
    <div className="space-y-1">
        <label htmlFor={id} className="font-bold text-theme-secondary block">
            {label}
        </label>
        <input
            id={id}
            aria-label={label}
            type="number"
            min={min}
            max={max}
            value={value}
            onChange={(e) => {
                const parsed = Number(e.target.value) || min;
                const bounded = clampMax !== undefined ? Math.min(clampMax, parsed) : parsed;
                onChange(Math.max(min, bounded));
            }}
            className="w-full rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
        />
    </div>
);

export interface QuotaEditorProps {
    maxTotalStorageMb: number;
    maxMembers: number;
    maxUserStorageMb: number;
    onChangeTotalStorage: (val: number) => void;
    onChangeMembers: (val: number) => void;
    onChangeUserStorage: (val: number) => void;
    idPrefix: string;
}

/**
 * Edits storage and member limits for a hosted sync service.
 */
export const QuotaEditor: React.FC<QuotaEditorProps> = ({
    maxTotalStorageMb,
    maxMembers,
    maxUserStorageMb,
    onChangeTotalStorage,
    onChangeMembers,
    onChangeUserStorage,
    idPrefix,
}) => {
    const { t } = useTranslation();

    return (
        <div className="pt-2 border-t border-theme-subtle/50 space-y-2 text-xs">
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
                <QuotaNumberField
                    id={`${idPrefix}-maxTotalStorageMb`}
                    label={t('settings.syncHostMaxStorageLabel')}
                    min={10}
                    max={10485760}
                    value={maxTotalStorageMb}
                    onChange={onChangeTotalStorage}
                />
                <QuotaNumberField
                    id={`${idPrefix}-maxMembers`}
                    label={t('settings.syncHostMaxMembersLabel')}
                    min={1}
                    max={100000}
                    clampMax={100000}
                    value={maxMembers}
                    onChange={onChangeMembers}
                />
                <QuotaNumberField
                    id={`${idPrefix}-maxUserStorageMb`}
                    label={t('settings.syncHostMaxUserStorageLabel')}
                    min={10}
                    max={10485760}
                    value={maxUserStorageMb}
                    onChange={onChangeUserStorage}
                />
            </div>
            <p className="text-[11px] text-theme-secondary/80">
                {t('settings.syncHostQuotasHint')}
            </p>
        </div>
    );
};

interface TrustLevelSelectProps {
    label: string;
    helpTitle: string;
    helpContent: string;
    value: number;
    onChange: (val: number) => void;
}

const TrustLevelSelect: React.FC<TrustLevelSelectProps> = ({
    label,
    helpTitle,
    helpContent,
    value,
    onChange,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-1">
            <label className="font-medium text-theme-secondary flex items-center">
                <span>{label}</span>
                <PolicyHelpPopover title={helpTitle} content={helpContent} />
            </label>
            <select
                value={value}
                onChange={(e) => onChange(Number(e.target.value))}
                className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
            >
                <option value={1}>{t('settings.trustLevel1Short')}</option>
                <option value={2}>{t('settings.trustLevel2Short')}</option>
                <option value={3}>{t('settings.trustLevel3Short')}</option>
            </select>
        </div>
    );
};

export interface PolicyEditorProps {
    policy: FrontendMemberPolicyDto;
    onChange: (updater: (prev: FrontendMemberPolicyDto) => FrontendMemberPolicyDto) => void;
    idPrefix: string;
}

/**
 * Edits direct contact and 2-hop Web of Trust bridge policies for a hosted sync service.
 */
export const PolicyEditor: React.FC<PolicyEditorProps> = ({ policy, onChange, idPrefix }) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4 text-xs">
            <div className="space-y-1">
                <label
                    htmlFor={`${idPrefix}-directMin`}
                    className="font-bold text-theme-secondary block"
                >
                    {t('settings.syncHostDirectMinLabel')}
                </label>
                <select
                    id={`${idPrefix}-directMin`}
                    aria-label={t('settings.syncHostDirectMinLabel')}
                    value={policy.directMin}
                    onChange={(e) => {
                        const val = Number(e.target.value);
                        onChange((p) => ({ ...p, directMin: val }));
                    }}
                    className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
                >
                    <option value={1}>{t('settings.syncHostDirectMinLevel1')}</option>
                    <option value={2}>{t('settings.syncHostDirectMinLevel2')}</option>
                    <option value={3}>{t('settings.syncHostDirectMinLevel3')}</option>
                </select>
            </div>

            <div className="pt-2 border-t border-theme-subtle/50 space-y-3">
                <label className="flex items-center gap-2 cursor-pointer">
                    <input
                        type="checkbox"
                        checked={policy.hop2Enabled}
                        onChange={(e) => onChange((p) => ({ ...p, hop2Enabled: e.target.checked }))}
                        className="rounded text-theme-primary focus:ring-theme-primary"
                    />
                    <span className="font-bold text-theme-secondary">
                        {t('settings.syncHostHop2Label')}
                    </span>
                </label>

                {policy.hop2Enabled && (
                    <div className="pl-6 space-y-3 pt-1 animate-in fade-in duration-150">
                        <TrustLevelSelect
                            label={t('settings.syncHostBridgeMinLabel')}
                            helpTitle={t('settings.syncHostBridgeMinTitle')}
                            helpContent={t('settings.syncHostBridgeMinDesc')}
                            value={policy.bridgeMin}
                            onChange={(val) => onChange((p) => ({ ...p, bridgeMin: val }))}
                        />
                        <TrustLevelSelect
                            label={t('settings.syncHostHop2MinLabel')}
                            helpTitle={t('settings.syncHostHop2MinTitle')}
                            helpContent={t('settings.syncHostHop2MinDesc')}
                            value={policy.hop2Min}
                            onChange={(val) => onChange((p) => ({ ...p, hop2Min: val }))}
                        />
                    </div>
                )}
            </div>
        </div>
    );
};
