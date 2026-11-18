import React, { FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import {
    AlertTriangle,
    ArrowLeft,
    ArrowRight,
    BookOpen,
    CheckCircle2,
    Info,
    Languages,
    Lock,
    RefreshCw,
    ShieldAlert,
} from 'lucide-react';
import { MnemonicLanguage } from '../../types';
import { cleanSeedText } from '../../utils/seedUtils';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { Input } from '../ui/Input';
import { CreateOrRestoreChoiceHeader } from './CreateProfileCredentialSteps';
import { MNEMONIC_LANGUAGES } from './SeedPhraseInput';

export interface ConfirmationWord {
    index: number;
    value: string;
}

export interface DisplaySeedStepProps {
    profilesCount: number | null;
    generatedSeed: string[];
    selectedLanguage: MnemonicLanguage;
    wordCount: 12 | 24;
    passphrase: string;
    passphraseOrdinal: string;
    showPassphraseAccordion: boolean;
    isLoading: boolean;
    onSwitchToRestore?: (method?: 'social' | 'words_or_sheet' | 'file_or_folder') => void;
    onHideExpertMode: () => void;
    onLanguageChange: (language: MnemonicLanguage) => void;
    onWordCountChange: (count: 12 | 24) => void;
    onTogglePassphraseAccordion: () => void;
    onPassphraseChange: (value: string) => void;
    onProceed: () => void;
    onSwitchToLogin?: () => void;
}

/**
 * Expert onboarding step 1: displays the generated BIP-39 recovery words with
 * dictionary, word-count, and optional passphrase controls.
 */
export const DisplaySeedStep: React.FC<DisplaySeedStepProps> = ({
    profilesCount,
    generatedSeed,
    selectedLanguage,
    wordCount,
    passphrase,
    passphraseOrdinal,
    showPassphraseAccordion,
    isLoading,
    onSwitchToRestore,
    onHideExpertMode,
    onLanguageChange,
    onWordCountChange,
    onTogglePassphraseAccordion,
    onPassphraseChange,
    onProceed,
    onSwitchToLogin,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-6 sm:space-y-8 animate-in fade-in duration-500">
            <CreateOrRestoreChoiceHeader
                profilesCount={profilesCount}
                onSwitchToRestore={onSwitchToRestore}
            />
            <div className="flex justify-end">
                <button
                    type="button"
                    onClick={onHideExpertMode}
                    className="text-xs font-bold text-theme-light hover:text-theme-primary transition-colors cursor-pointer"
                >
                    {t('auth.hideExpertOptions')}
                </button>
            </div>
            <div className="text-center space-y-2">
                <h2 className="text-2xl font-black text-theme-primary tracking-tight flex items-center justify-center gap-2">
                    <span>{t('profile.step1Title')}</span>
                    <HelpIcon topic="mnemonic" size={20} />
                </h2>
                <p className="text-sm font-medium text-theme-light">{t('profile.step1Subtitle')}</p>
            </div>

            <div className="p-6 bg-rose-50 border-2 border-rose-100 rounded-[32px] flex items-start gap-4 shadow-sm">
                <ShieldAlert className="text-rose-500 shrink-0 mt-1" size={24} />
                <div>
                    <h3 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">
                        {t('profile.warning')}
                    </h3>
                    <p className="text-xs text-rose-800 font-medium leading-relaxed">
                        {t('profile.seedWarning')}
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                {generatedSeed.map((word, index) => (
                    <div
                        key={index}
                        data-testid={`word-display-${index}`}
                        className="flex items-center gap-3 p-3 bg-theme-primary/5 border border-theme-primary/10 rounded-2xl shadow-inner-soft group hover:bg-white transition-all hover:shadow-md hover:scale-[1.02] duration-300"
                    >
                        <span className="text-[10px] font-black text-theme-primary/40 w-5">
                            {index + 1}
                        </span>
                        <span className="text-sm font-mono font-bold text-theme-secondary tracking-tight">
                            {word}
                        </span>
                    </div>
                ))}
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-2">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                        <Languages size={10} /> {t('common.dictionary')}
                    </label>
                    <select
                        value={selectedLanguage}
                        onChange={(e) => onLanguageChange(e.target.value as MnemonicLanguage)}
                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-2.5 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none"
                    >
                        {MNEMONIC_LANGUAGES.map((opt) => (
                            <option key={opt.value} value={opt.value}>
                                {opt.label}
                            </option>
                        ))}
                    </select>
                </div>
                <div className="space-y-2">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                        <BookOpen size={10} /> {t('profile.wordCount')}
                    </label>
                    <select
                        value={wordCount}
                        onChange={(e) =>
                            onWordCountChange(parseInt(e.target.value, 10) as 12 | 24)
                        }
                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-2.5 text-xs font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none"
                    >
                        <option value={12}>{t('profile.wordCountStandard')}</option>
                        <option value={24}>{t('profile.wordCountHigh')}</option>
                    </select>
                </div>
            </div>

            <div className="pt-3 border-t border-theme-primary/10">
                <button
                    type="button"
                    onClick={onTogglePassphraseAccordion}
                    className="w-full flex items-center justify-between py-2 text-left text-xs font-bold text-theme-secondary hover:text-theme-primary transition-colors cursor-pointer"
                >
                    <span className="flex items-center gap-1.5">
                        <Lock size={12} className="text-theme-primary" />
                        <span>
                            {t('profile.optionalPassphraseAccordion', {
                                ordinal: passphraseOrdinal,
                            })}
                        </span>
                    </span>
                    <span className="text-[10px] text-theme-light">
                        {showPassphraseAccordion ? '▲' : '▼'}
                    </span>
                </button>
                {showPassphraseAccordion && (
                    <div className="space-y-3 pt-3 animate-in fade-in duration-200">
                        <div className="flex items-center gap-1.5">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1">
                                <Lock size={10} />
                                <span>
                                    {t('profile.optionalPassphrase', { ordinal: passphraseOrdinal })}
                                </span>
                            </label>
                            <HelpIcon topic="passphrase" size={12} />
                        </div>
                        <Input
                            type="text"
                            value={passphrase}
                            onChange={(e) => onPassphraseChange(e.target.value)}
                            placeholder={t('profile.passphrasePlaceholder')}
                            className="rounded-2xl"
                        />
                        <p className="text-[9px] font-bold text-amber-600 bg-amber-50 p-3 rounded-xl border border-amber-100 flex items-start gap-2">
                            <AlertTriangle size={12} className="shrink-0 mt-0.5" />
                            {t('profile.passphraseAdvanced', { wordCount })}
                        </p>
                    </div>
                )}
            </div>

            <div className="flex flex-col items-center gap-4 pt-4">
                <Button
                    size="lg"
                    onClick={onProceed}
                    disabled={isLoading || generatedSeed.length === 0}
                    className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3"
                >
                    {isLoading ? (
                        <RefreshCw className="animate-spin" size={24} />
                    ) : (
                        <CheckCircle2 size={24} />
                    )}
                    {t('profile.savedSeed')}
                </Button>
                <Button
                    type="button"
                    variant="secondary"
                    onClick={onSwitchToLogin}
                    className="w-full py-3 rounded-2xl shadow-sm text-sm gap-2"
                >
                    <ArrowLeft size={18} /> {t('auth.backToLogin')}
                </Button>
            </div>
        </div>
    );
};

export interface ConfirmSeedStepProps {
    isBulkMode: boolean;
    bulkSeedInput: string;
    confirmationWords: ConfirmationWord[];
    userConfirmationInput: Record<number, string>;
    passphrase: string;
    confirmPassphrase: string;
    onToggleBulkMode: () => void;
    onBulkSeedChange: (value: string) => void;
    onWordInputChange: (index: number, value: string) => void;
    onConfirmPassphraseChange: (value: string) => void;
    onBack: () => void;
    onSubmit: (e: FormEvent) => void;
}

/**
 * Expert onboarding step 2: verifies 3 random recovery words or bulk-pasted phrase
 * plus optional extra passphrase confirmation.
 */
export const ConfirmSeedStep: React.FC<ConfirmSeedStepProps> = ({
    isBulkMode,
    bulkSeedInput,
    confirmationWords,
    userConfirmationInput,
    passphrase,
    confirmPassphrase,
    onToggleBulkMode,
    onBulkSeedChange,
    onWordInputChange,
    onConfirmPassphraseChange,
    onBack,
    onSubmit,
}) => {
    const { t } = useTranslation();

    return (
        <form
            onSubmit={onSubmit}
            className="space-y-6 sm:space-y-8 animate-in slide-in-from-right-8 duration-500"
        >
            <div className="text-center space-y-2">
                <h2 className="text-2xl font-black text-theme-primary tracking-tight">
                    {t('profile.step2Title')}
                </h2>
                <p className="text-sm font-medium text-theme-light">{t('profile.step2Subtitle')}</p>
            </div>

            <div className="flex justify-center">
                <button
                    type="button"
                    onClick={onToggleBulkMode}
                    className="px-4 py-1.5 bg-theme-primary/5 border border-theme-primary/10 rounded-full text-[10px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-2"
                >
                    <Languages size={12} />{' '}
                    {isBulkMode ? t('profile.interactiveMode') : t('profile.switchBulkPaste')}
                </button>
            </div>

            {isBulkMode ? (
                <div className="space-y-3">
                    <label
                        htmlFor="bulk-seed-input"
                        className="text-[10px] font-black text-theme-light uppercase tracking-widest"
                    >
                        {t('profile.pasteFullSeed')}
                    </label>
                    <textarea
                        id="bulk-seed-input"
                        value={bulkSeedInput}
                        onChange={(e) => onBulkSeedChange(e.target.value)}
                        onBlur={() => onBulkSeedChange(cleanSeedText(bulkSeedInput))}
                        className="w-full h-32 px-4 py-3 bg-white border border-theme-subtle rounded-2xl text-theme-primary font-mono text-sm focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft transition-all"
                        placeholder={t('profile.pasteHere')}
                        required
                    />
                    <p className="text-[9px] font-bold text-theme-light italic flex items-center gap-1.5">
                        <Info size={10} /> {t('profile.formattingAuto')}
                    </p>
                </div>
            ) : (
                <div className="space-y-5">
                    {confirmationWords.map(({ index }) => (
                        <div key={index} className="space-y-2">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                                {t('profile.verifyWord', { number: index + 1 })}
                            </label>
                            <Input
                                type="text"
                                value={userConfirmationInput[index] || ''}
                                onChange={(e) => onWordInputChange(index, e.target.value)}
                                required
                                className="font-mono font-bold py-4 text-center text-lg"
                                autoComplete="off"
                                data-testid={`word-input-${index}`}
                            />
                        </div>
                    ))}
                </div>
            )}

            {passphrase && (
                <div className="space-y-2 pt-4 border-t border-theme-primary/10">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                        {t('profile.confirmPassphrase')}
                    </label>
                    <Input
                        type="text"
                        value={confirmPassphrase}
                        onChange={(e) => onConfirmPassphraseChange(e.target.value)}
                        required
                        className="font-mono font-bold py-4 text-center text-lg"
                        autoComplete="off"
                        placeholder={t('profile.enterExtraWord')}
                    />
                </div>
            )}

            <div className="flex gap-4 pt-4">
                <Button
                    type="button"
                    variant="secondary"
                    onClick={onBack}
                    className="flex-1 rounded-2xl gap-2"
                >
                    <ArrowLeft size={18} /> {t('profile.back')}
                </Button>
                <Button type="submit" className="flex-[2] rounded-3xl py-4 shadow-premium-lg gap-2">
                    {t('profile.confirmSeed')} <ArrowRight size={18} />
                </Button>
            </div>
        </form>
    );
};
