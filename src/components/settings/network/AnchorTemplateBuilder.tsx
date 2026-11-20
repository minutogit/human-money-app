// src/components/settings/network/AnchorTemplateBuilder.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus } from 'lucide-react';
import {
    AnchorServerInputDto,
    LockServerProbeResult,
    NominatedLockServerMeta,
    StoredAnchorTemplateMeta,
} from '../../../types';
import { settingsService } from '../../../services/settingsService';
import { protectCommandOptions } from '../../../services/commandPolicy';
import { useSession } from '../../../context/SessionContext';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { TechId } from '../../ui/TechId';
import { translateError } from '../../../utils/errorHelper';
import { defaultQuarterYearsAhead, formatQuarter, parseQuarterStr } from '../../../utils/retentionQuarter';
import { formatAgeLabel } from './LockServerProbeSection';

interface DraftServerSelection {
    writeUrl: string;
    nominatedBy: 'creator' | 'guarantor';
    asn: string;
}

export interface AnchorTemplateBuilderProps {
    passingProbes: LockServerProbeResult[];
    templates: StoredAnchorTemplateMeta[];
    ownNominatedLockServer?: NominatedLockServerMeta | null;
    onSuccess: (message: string) => void;
    onError: (message: string) => void;
    onClearFeedback: () => void;
    onRefreshSettings: () => Promise<void>;
}

/**
 * Configures and inspects 1-to-5 server L2 anchor committee templates.
 */
export const AnchorTemplateBuilder: React.FC<AnchorTemplateBuilderProps> = ({
    passingProbes,
    templates,
    ownNominatedLockServer,
    onSuccess,
    onError,
    onClearFeedback,
    onRefreshSettings,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [tplLabel, setTplLabel] = useState('');
    const [tplQuarter, setTplQuarter] = useState(() => defaultQuarterYearsAhead(5));
    const [tplThreshold, setTplThreshold] = useState('');
    const [tplGuarantorKeys, setTplGuarantorKeys] = useState('');
    const [selectedServers, setSelectedServers] = useState<DraftServerSelection[]>([]);
    const [isAssembling, setIsAssembling] = useState(false);

    const toggleServerInDraft = (writeUrl: string) => {
        setSelectedServers((prev) => {
            const exists = prev.some((s) => s.writeUrl === writeUrl);
            if (exists) {
                return prev.filter((s) => s.writeUrl !== writeUrl);
            }
            if (prev.length >= 5) return prev;
            const isOwn = ownNominatedLockServer?.writeUrl === writeUrl;
            return [
                ...prev,
                {
                    writeUrl,
                    nominatedBy: isOwn ? 'guarantor' : 'creator',
                    asn: isOwn ? ownNominatedLockServer?.asn || '' : '',
                },
            ];
        });
    };

    const updateDraftServer = (writeUrl: string, patch: Partial<DraftServerSelection>) => {
        setSelectedServers((prev) =>
            prev.map((s) => (s.writeUrl === writeUrl ? { ...s, ...patch } : s))
        );
    };

    const handleAssembleTemplate = async () => {
        if (selectedServers.length === 0) return;
        const parsedQ = parseQuarterStr(tplQuarter);
        if (parsedQ === null) {
            onError(t('settings.invalidQuarterFormat', 'Invalid quarter format. Expected YYYY-QN (e.g. 2031-Q4).'));
            return;
        }
        setIsAssembling(true);
        onClearFeedback();
        try {
            let guarantorIdx = 0;
            const serverInputs: AnchorServerInputDto[] = selectedServers.map((s) => ({
                writeUrl: s.writeUrl,
                nominatedBy: s.nominatedBy,
                guarantorIndex: s.nominatedBy === 'guarantor' ? guarantorIdx++ : null,
                asn: s.asn.trim() || null,
            }));
            const guarantorKeys = tplGuarantorKeys
                .split(/[\s,;]+/)
                .map((k) => k.trim())
                .filter(Boolean);
            const parsedThresh = tplThreshold.trim()
                ? Number.parseInt(tplThreshold.trim(), 10)
                : null;

            await protectAction(async (pwd) => {
                return await settingsService.assembleAnchorTemplate({
                    label: tplLabel.trim() || `Set (${selectedServers.length})`,
                    retainUntilQuarterStr: tplQuarter.trim().toUpperCase(),
                    servers: serverInputs,
                    guarantorMigPubsHex: guarantorKeys,
                    threshold: Number.isFinite(parsedThresh as number) ? parsedThresh : null,
                    password: pwd || null,
                });
            }, protectCommandOptions('assemble_anchor_template'));

            setTplLabel('');
            setTplThreshold('');
            setTplGuarantorKeys('');
            setSelectedServers([]);
            onSuccess(t('settings.anchorTemplateSavedMsg', 'Anchor set template saved.'));
            await onRefreshSettings();
        } catch (e) {
            onError(translateError(e, t));
        } finally {
            setIsAssembling(false);
        }
    };

    return (
        <div className="space-y-3 p-4 rounded-xl bg-white dark:bg-slate-900 border border-theme-subtle">
            <h4 className="text-xs font-black uppercase tracking-wider text-theme-primary flex items-center gap-1.5">
                <Plus size={13} />
                {t('settings.anchorTemplatesTitle', 'Lock Committee Templates (1–5 Servers)')}
            </h4>
            <p className="text-[11px] text-theme-light leading-relaxed">
                {t(
                    'settings.anchorTemplatesDesc',
                    'Combine 1 to 5 independently verified servers into a reusable double-spend protection template for new vouchers.'
                )}
            </p>

            <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                <Input
                    value={tplLabel}
                    onChange={(e) => setTplLabel(e.target.value)}
                    placeholder={t('settings.anchorLabelPlaceholder', 'Template name (e.g. Regional 2-of-3)')}
                />
                <Input
                    value={tplQuarter}
                    onChange={(e) => setTplQuarter(e.target.value)}
                    placeholder="2031-Q4"
                    aria-label={t('settings.retainUntilQuarterLabel', 'Retain until quarter')}
                />
                <Input
                    value={tplThreshold}
                    onChange={(e) => setTplThreshold(e.target.value)}
                    placeholder={t('settings.quorumOptionalPlaceholder', 'Threshold Q (default majority)')}
                />
            </div>

            {passingProbes.length > 0 ? (
                <div className="space-y-1.5">
                    <p className="text-[10px] font-bold uppercase tracking-wider text-theme-light">
                        {t('settings.selectCommitteeServers', 'Select 1–5 verified servers')}
                    </p>
                    {passingProbes.map((p) => {
                        const draft = selectedServers.find((s) => s.writeUrl === p.writeUrl);
                        return (
                            <div
                                key={p.writeUrl}
                                className="p-2 rounded-lg border border-theme-subtle/70 flex flex-wrap items-center justify-between gap-2 text-xs"
                            >
                                <label className="flex items-center gap-2 cursor-pointer min-w-0 flex-1">
                                    <input
                                        type="checkbox"
                                        checked={Boolean(draft)}
                                        onChange={() => toggleServerInDraft(p.writeUrl)}
                                        className="accent-theme-primary"
                                    />
                                    <span className="truncate font-medium">{p.writeUrl}</span>
                                    <span className="text-[10px] text-theme-light shrink-0">
                                        (≤ {formatQuarter(p.maxRetentionQuarter)})
                                    </span>
                                </label>
                                {draft && (
                                    <div className="flex items-center gap-1.5">
                                        <select
                                            value={draft.nominatedBy}
                                            onChange={(e) =>
                                                updateDraftServer(p.writeUrl, {
                                                    nominatedBy: e.target.value as 'creator' | 'guarantor',
                                                })
                                            }
                                            className="text-[11px] bg-white dark:bg-slate-800 border border-theme-subtle rounded-lg px-2 py-1"
                                        >
                                            <option value="creator">Creator</option>
                                            <option value="guarantor">Guarantor</option>
                                        </select>
                                        <input
                                            type="text"
                                            value={draft.asn}
                                            onChange={(e) => updateDraftServer(p.writeUrl, { asn: e.target.value })}
                                            placeholder="ASN"
                                            className="w-20 text-[11px] bg-white dark:bg-slate-800 border border-theme-subtle rounded-lg px-2 py-1"
                                        />
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>
            ) : (
                <p className="text-[11px] italic text-theme-light">
                    {t('settings.probeServersFirstHint', 'Verify at least one server above before assembling a template.')}
                </p>
            )}

            <Input
                value={tplGuarantorKeys}
                onChange={(e) => setTplGuarantorKeys(e.target.value)}
                placeholder={t('settings.guarantorMigKeysPlaceholder', 'Guarantor rotation keys (optional, comma-separated 64-hex)')}
            />

            <div className="flex justify-end">
                <Button
                    type="button"
                    size="sm"
                    onClick={handleAssembleTemplate}
                    disabled={isAssembling || selectedServers.length === 0}
                >
                    {isAssembling
                        ? t('common.saving', 'Saving...')
                        : t('settings.createAnchorTemplateBtn', 'Assemble Anchor Set')}
                </Button>
            </div>

            {templates.length > 0 && (
                <div className="space-y-2 pt-2 border-t border-theme-subtle/50">
                    <p className="text-[10px] font-bold uppercase tracking-wider text-theme-light">
                        {t('settings.savedAnchorTemplates', 'Saved Anchor Sets')} ({templates.length})
                    </p>
                    {templates.map((tpl) => (
                        <div
                            key={tpl.anchorId}
                            className="p-3 rounded-xl border border-theme-subtle bg-bg-app/30 text-xs space-y-1"
                        >
                            <div className="flex items-center justify-between gap-2 font-bold">
                                <span>{tpl.label}</span>
                                <span className="text-[11px] font-mono">
                                    {tpl.threshold}/{tpl.servers.length} · ≥ {formatQuarter(tpl.retainUntilQuarter)}
                                </span>
                            </div>
                            <div className="flex flex-wrap items-center gap-2 text-[10px] text-theme-light">
                                <span>Anchor:</span>
                                <TechId value={tpl.anchorId} kind="hash" />
                                <span>· {t('settings.probeAgeLabel', 'Age')}: {formatAgeLabel(tpl.createdAtSecs)}</span>
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
};
