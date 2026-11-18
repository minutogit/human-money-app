// src/components/marketplace/ProfileCardBioSkillsSection.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Briefcase, ChevronDown, ChevronUp, Plus, Trash2 } from 'lucide-react';
import { Skill } from '../../types';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';

export interface ProfileCardBioSkillsSectionProps {
    skills: Skill[];
    onAddSkill: () => void;
    onRemoveSkill: (index: number) => void;
    onMoveSkill: (index: number, direction: 'up' | 'down') => void;
    onSkillChange: <K extends keyof Skill>(index: number, field: K, value: Skill[K]) => void;
}

/**
 * Editable list of up to 8 permanent business-card skills and offerings.
 */
export const ProfileCardBioSkillsSection: React.FC<ProfileCardBioSkillsSectionProps> = ({
    skills,
    onAddSkill,
    onRemoveSkill,
    onMoveSkill,
    onSkillChange,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4 pt-4 border-t border-theme-subtle/60">
            <div className="flex items-center justify-between border-b border-theme-subtle pb-2">
                <div>
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                        <Briefcase size={14} className="text-theme-primary" />
                        <span>{t('marketplace.sectionSkills', 'Fähigkeiten & Angebote')}</span>
                    </h3>
                    <p className="text-[11px] text-theme-light">
                        {t('marketplace.skillsDesc', 'Was du dauerhaft kannst und für andere tust (max. 8)')}
                    </p>
                </div>
                <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    data-testid="add-skill-button"
                    onClick={onAddSkill}
                    disabled={skills.length >= 8}
                    className="flex items-center gap-1"
                >
                    <Plus size={14} />
                    <span>{t('marketplace.addSkillBtn', 'Fähigkeit hinzufügen')}</span>
                </Button>
            </div>

            {skills.length === 0 ? (
                <div className="p-4 rounded-xl bg-bg-app border border-theme-subtle text-center text-xs text-theme-light">
                    {t('marketplace.noSkillsYet', 'Noch keine Fähigkeiten eingetragen. Füge oben dein Können hinzu.')}
                </div>
            ) : (
                <div className="space-y-3">
                    {skills.map((skill, index) => (
                        <div
                            key={index}
                            className="p-3.5 rounded-2xl bg-white border border-theme-subtle space-y-2.5 shadow-xs"
                        >
                            <div className="flex items-center justify-between gap-2">
                                <span className="text-[10px] font-bold text-theme-light uppercase">
                                    #{index + 1}
                                </span>
                                <div className="flex items-center gap-1">
                                    <button
                                        type="button"
                                        onClick={() => onMoveSkill(index, 'up')}
                                        disabled={index === 0}
                                        className="p-1 rounded hover:bg-bg-app text-theme-light disabled:opacity-30"
                                        title={t('marketplace.moveUpBtn', 'Nach oben')}
                                    >
                                        <ChevronUp size={14} />
                                    </button>
                                    <button
                                        type="button"
                                        onClick={() => onMoveSkill(index, 'down')}
                                        disabled={index === skills.length - 1}
                                        className="p-1 rounded hover:bg-bg-app text-theme-light disabled:opacity-30"
                                        title={t('marketplace.moveDownBtn', 'Nach unten')}
                                    >
                                        <ChevronDown size={14} />
                                    </button>
                                    <button
                                        type="button"
                                        onClick={() => onRemoveSkill(index)}
                                        className="p-1 rounded hover:bg-rose-50 text-rose-600 ml-1"
                                        title={t('marketplace.deleteSkillBtn', 'Löschen')}
                                    >
                                        <Trash2 size={14} />
                                    </button>
                                </div>
                            </div>

                            <Input
                                placeholder={t('marketplace.skillTitlePlaceholder', 'Titel, z. B. Fahrradreparatur (max 60)')}
                                value={skill.title}
                                data-testid={`skill-title-${index}`}
                                onChange={(e) => onSkillChange(index, 'title', e.target.value)}
                                maxLength={60}
                                required
                            />
                            <Input
                                placeholder={t('marketplace.skillTextPlaceholder', 'Kurzbeschreibung (optional, max 160)')}
                                value={skill.text || ''}
                                data-testid={`skill-text-${index}`}
                                onChange={(e) => onSkillChange(index, 'text', e.target.value)}
                                maxLength={160}
                            />
                            <Input
                                placeholder={t('marketplace.skillPricingPlaceholder', 'Preis / Richtwert, z. B. 1 Minuto/Std. (optional, max 40)')}
                                value={skill.pricing || ''}
                                data-testid={`skill-pricing-${index}`}
                                onChange={(e) => onSkillChange(index, 'pricing', e.target.value)}
                                maxLength={40}
                            />
                            <label className="flex items-center gap-2 pt-1 cursor-pointer">
                                <input
                                    type="checkbox"
                                    checked={skill.isRemote ?? false}
                                    onChange={(e) => onSkillChange(index, 'isRemote', e.target.checked)}
                                    className="rounded text-theme-primary focus:ring-theme-primary cursor-pointer"
                                    data-testid={`skill-remote-checkbox-${index}`}
                                />
                                <span className="text-xs text-theme-secondary font-medium">
                                    {t('marketplace.skillIsRemoteLabel', 'Ortsunabhängig / Online möglich (überregional)')}
                                </span>
                            </label>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
};
