// src/components/ui/SeedPhraseInput.tsx
import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { MnemonicLanguage } from '../../types';
import { InspectedEmergencyKitDto } from '../../types/recovery';
import { profileService } from '../../services/profileService';
import { recoveryService } from '../../services/recoveryService';
import { cleanSeedText } from '../../utils/seedUtils';
import { translateError } from '../../utils/errorHelper';
import { Input } from './Input';
import { HelpIcon } from './HelpIcon';
import { PrefixInfoModal } from './PrefixInfoModal';
import { AdvancedDisclosure } from './AdvancedDisclosure';
import {
    KeyRound,
    CheckCircle2,
    AlertCircle,
    HelpCircle,
    Languages,
    SlidersHorizontal,
} from 'lucide-react';

export interface SeedPhraseValue {
    mnemonic: string;
    wordCount: 12 | 24;
    language: MnemonicLanguage;
    passphrase: string;
    userPrefix: string;
    isValid: boolean;
    detectedKit: InspectedEmergencyKitDto | null;
}

export interface SeedPhraseInputProps {
    onChange: (value: SeedPhraseValue) => void;
    allowKitUri?: boolean;
    showUserPrefix?: boolean;
    initialPayload?: string;
    forceOpenAdvanced?: boolean;
    focusPrefixTrigger?: number;
    idPrefix?: string;
}

const ALL_LANGUAGES: Array<{ value: MnemonicLanguage; label: string }> = [
    { value: 'english', label: 'English' },
    { value: 'german', label: 'Deutsch' },
    { value: 'spanish', label: 'Español' },
    { value: 'french', label: 'Français' },
    { value: 'italian', label: 'Italiano' },
    { value: 'japanese', label: '日本語' },
    { value: 'korean', label: '한국어' },
    { value: 'portuguese', label: 'Português' },
    { value: 'czech', label: 'Čeština' },
    { value: 'chineseSimplified', label: '简体中文' },
    { value: 'chineseTraditional', label: '繁體中文' },
];

function detectSystemLanguage(): MnemonicLanguage {
    if (typeof navigator === 'undefined') return 'english';
    const sys = navigator.language || 'en';
    const langMap: Record<string, MnemonicLanguage> = {
        de: 'german',
        es: 'spanish',
        fr: 'french',
        it: 'italian',
        ja: 'japanese',
        ko: 'korean',
        pt: 'portuguese',
        cs: 'czech',
        'zh-CN': 'chineseSimplified',
        'zh-TW': 'chineseTraditional',
    };
    for (const [prefix, mapped] of Object.entries(langMap)) {
        if (sys.startsWith(prefix)) return mapped;
    }
    return 'english';
}

export const SeedPhraseInput: React.FC<SeedPhraseInputProps> = ({
    onChange,
    allowKitUri = true,
    showUserPrefix = true,
    initialPayload,
    forceOpenAdvanced = false,
    focusPrefixTrigger = 0,
    idPrefix = 'seed',
}) => {
    const { t } = useTranslation();

    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const [language, setLanguage] = useState<MnemonicLanguage>(() => detectSystemLanguage());
    const [words, setWords] = useState<string[]>(() => Array(12).fill(''));
    const [passphrase, setPassphrase] = useState<string>('');
    const [userPrefix, setUserPrefix] = useState<string>('');
    const [detectedKit, setDetectedKit] = useState<InspectedEmergencyKitDto | null>(null);
    const [isValid, setIsValid] = useState<boolean>(false);
    const [validationError, setValidationError] = useState<string>('');
    const [wordlist, setWordlist] = useState<string[]>([]);
    const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
    const [showPrefixInfo, setShowPrefixInfo] = useState<boolean>(false);

    const onChangeRef = useRef(onChange);
    onChangeRef.current = onChange;

    const prefixInputRef = useRef<HTMLInputElement | null>(null);
    const passphraseInputRef = useRef<HTMLInputElement | null>(null);
    const lastProcessedPayloadRef = useRef<string | null>(null);
    const prevFocusTriggerRef = useRef<number>(focusPrefixTrigger);

    // Fetch BIP-39 wordlist for suggestion pills
    useEffect(() => {
        let active = true;
        profileService
            .getWordlist(language)
            .then((list) => {
                if (active && Array.isArray(list)) {
                    setWordlist(list);
                }
            })
            .catch(() => {
                if (active) setWordlist([]);
            });
        return () => {
            active = false;
        };
    }, [language]);

    const applyKitUri = useCallback(
        async (rawText: string) => {
            const match = rawText.match(/humoco:\/\/kit[^\s"'>]+/i);
            const uri = match ? match[0] : rawText.trim();
            try {
                const kit = await recoveryService.inspectEmergencyKitUri(uri);
                const kitWords = kit.phrase.trim().split(/\s+/).filter(Boolean);
                const nextCount: 12 | 24 = kit.wordCount === 24 || kitWords.length > 12 ? 24 : 12;
                setWordCount(nextCount);
                setWords(Array(nextCount).fill('').map((_, i) => kitWords[i] || ''));
                setLanguage(kit.language);
                setUserPrefix(kit.userPrefix ?? '');
                setDetectedKit(kit);
                setIsValid(true);
                setValidationError('');
                if (kit.hasBip39Passphrase) {
                    setTimeout(() => passphraseInputRef.current?.focus(), 60);
                }
            } catch (err) {
                setValidationError(translateError(err, t));
                setIsValid(false);
            }
        },
        [t]
    );

    // Inspect initialPayload if it is a humoco://kit link
    useEffect(() => {
        if (
            allowKitUri &&
            initialPayload &&
            initialPayload.includes('humoco://kit') &&
            lastProcessedPayloadRef.current !== initialPayload
        ) {
            lastProcessedPayloadRef.current = initialPayload;
            void applyKitUri(initialPayload);
        }
    }, [allowKitUri, initialPayload, applyKitUri]);

    // Focus prefix input when requested by parent (e.g. from collision card -> subaccount)
    useEffect(() => {
        if (
            showUserPrefix &&
            focusPrefixTrigger > 0 &&
            focusPrefixTrigger !== prevFocusTriggerRef.current
        ) {
            prevFocusTriggerRef.current = focusPrefixTrigger;
            const timer = setTimeout(() => {
                prefixInputRef.current?.focus();
            }, 60);
            return () => clearTimeout(timer);
        }
    }, [focusPrefixTrigger, showUserPrefix]);

    // Debounced validation + auto-language detection across all 11 languages
    useEffect(() => {
        let active = true;
        const sliced = words.slice(0, wordCount).map((w) => w.trim().toLowerCase());
        const filledCount = sliced.filter(Boolean).length;

        if (filledCount < wordCount || sliced.some((w) => !w)) {
            setIsValid(false);
            setValidationError('');
            return () => {
                active = false;
            };
        }

        const candidatePhrase = sliced.join(' ');
        const timer = setTimeout(async () => {
            try {
                await profileService.validateMnemonic(candidatePhrase, language);
                if (!active) return;
                setIsValid(true);
                setValidationError('');
            } catch (primaryErr) {
                // Auto-detect across the remaining BIP-39 languages
                for (const opt of ALL_LANGUAGES) {
                    if (opt.value === language) continue;
                    try {
                        await profileService.validateMnemonic(candidatePhrase, opt.value);
                        if (!active) return;
                        setLanguage(opt.value);
                        setIsValid(true);
                        setValidationError('');
                        return;
                    } catch {
                        // continue trying next language
                    }
                }
                if (!active) return;
                setIsValid(false);
                setValidationError(translateError(primaryErr, t));
            }
        }, 120);

        return () => {
            active = false;
            clearTimeout(timer);
        };
    }, [words, wordCount, language, t]);

    // Emit normalized value to parent
    useEffect(() => {
        const sliced = words.slice(0, wordCount).map((w) => w.trim()).filter(Boolean);
        const mnemonic = sliced.join(' ');
        const filledAll = sliced.length === wordCount && isValid;

        onChangeRef.current({
            mnemonic,
            wordCount,
            language,
            passphrase,
            userPrefix,
            isValid: filledAll,
            detectedKit,
        });
    }, [words, wordCount, language, passphrase, userPrefix, isValid, detectedKit]);

    const handleWordCountChange = (nextCount: 12 | 24) => {
        setWordCount(nextCount);
        setWords((prev) => Array(nextCount).fill('').map((_, i) => prev[i] || ''));
    };

    const handleWordChange = (index: number, rawValue: string) => {
        if (allowKitUri && /humoco:\/\/kit/i.test(rawValue)) {
            void applyKitUri(rawValue);
            return;
        }

        if (!rawValue) {
            setWords((prev) => {
                const next = [...prev];
                next[index] = '';
                return next;
            });
            return;
        }

        const cleaned = cleanSeedText(rawValue);
        const parts = cleaned.split(' ').filter(Boolean);

        if (parts.length > 1) {
            const needed = index + parts.length;
            const targetCount: 12 | 24 = needed > 12 || wordCount === 24 ? 24 : 12;
            if (targetCount !== wordCount) {
                setWordCount(targetCount);
            }
            setWords((prev) => {
                const next = Array(targetCount).fill('').map((_, i) => prev[i] || '');
                for (let i = 0; i < parts.length && index + i < targetCount; i++) {
                    next[index + i] = parts[i];
                }
                return next;
            });
            const nextFocus = index + parts.length;
            if (nextFocus < targetCount) {
                setTimeout(() => {
                    document.getElementById(`${idPrefix}-word-${nextFocus}`)?.focus();
                }, 0);
            }
            return;
        }

        const endsWithSpace = /\s$/.test(rawValue);
        const single = cleaned;
        setWords((prev) => {
            const next = [...prev];
            next[index] = single;
            return next;
        });

        if (endsWithSpace && single.length > 0 && index + 1 < wordCount) {
            document.getElementById(`${idPrefix}-word-${index + 1}`)?.focus();
        }
    };

    const handleClearKit = () => {
        setDetectedKit(null);
        setTimeout(() => {
            document.getElementById(`${idPrefix}-word-0`)?.focus();
        }, 0);
    };

    const activePrefixQuery =
        focusedIndex !== null ? (words[focusedIndex] || '').trim().toLowerCase() : '';
    const suggestions =
        activePrefixQuery.length >= 2 &&
        wordlist.length > 0 &&
        !wordlist.includes(activePrefixQuery)
            ? wordlist.filter((w) => w.startsWith(activePrefixQuery)).slice(0, 4)
            : [];

    const filledCount = words.slice(0, wordCount).filter((w) => w.trim().length > 0).length;
    const isCustomAdvanced = Boolean(
        passphrase || (showUserPrefix && userPrefix.trim())
    );
    const shouldForceOpenAdvanced = Boolean(
        forceOpenAdvanced ||
            detectedKit?.hasBip39Passphrase ||
            focusPrefixTrigger > 0
    );

    return (
        <div className="space-y-4">
            {/* Compact Emerald Banner when a humoco://kit link was detected */}
            {detectedKit && (
                <div
                    data-testid="detected-kit-banner"
                    className="p-3.5 bg-emerald-50 border border-emerald-200 rounded-2xl flex items-center justify-between gap-3"
                >
                    <div className="flex items-center gap-2.5 min-w-0">
                        <CheckCircle2 size={18} className="text-emerald-600 shrink-0" />
                        <div className="min-w-0">
                            <p className="text-xs font-bold text-emerald-900">
                                {t('backup.kitDetectedTitle')}
                            </p>
                            <p className="text-[11px] text-emerald-700 truncate">
                                {t('backup.kitDetectedDesc', {
                                    count: detectedKit.wordCount,
                                })}
                            </p>
                        </div>
                    </div>
                    <button
                        type="button"
                        onClick={handleClearKit}
                        data-testid="clear-detected-kit-btn"
                        className="px-2.5 py-1 text-[11px] font-bold bg-white border border-emerald-300 text-emerald-800 hover:bg-emerald-100 rounded-lg shrink-0 cursor-pointer"
                    >
                        {t('common.change')}
                    </button>
                </div>
            )}

            {/* Word Grid Card */}
            <div className="p-4 sm:p-5 bg-white border border-theme-subtle rounded-2xl space-y-4 shadow-sm">
                <div className="flex flex-wrap items-center justify-between gap-2 pb-2 border-b border-theme-subtle/70">
                    <div className="flex items-center gap-2">
                        <KeyRound size={15} className="text-theme-primary" />
                        <span className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                            {t('backup.seedInputTitle')}
                        </span>
                        <HelpIcon topic="mnemonic" size={13} />
                    </div>

                    <div className="flex bg-slate-100 p-0.5 rounded-lg text-[11px] font-bold">
                        <button
                            type="button"
                            data-testid="seed-word-count-12"
                            onClick={() => handleWordCountChange(12)}
                            className={`px-2.5 py-1 rounded-md transition-all cursor-pointer ${
                                wordCount === 12
                                    ? 'bg-white shadow-xs text-theme-primary'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            {t('backup.words12Label')}
                        </button>
                        <button
                            type="button"
                            data-testid="seed-word-count-24"
                            onClick={() => handleWordCountChange(24)}
                            className={`px-2.5 py-1 rounded-md transition-all cursor-pointer ${
                                wordCount === 24
                                    ? 'bg-white shadow-xs text-theme-primary'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            {t('backup.words24Label')}
                        </button>
                    </div>
                </div>

                {allowKitUri && !detectedKit && (
                    <p className="text-[11px] text-theme-light leading-relaxed">
                        {t('backup.seedInputSmartPasteHint')}
                    </p>
                )}

                <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
                    {Array.from({ length: wordCount }).map((_, idx) => (
                        <div key={idx} className="relative flex items-center">
                            <span className="absolute left-2.5 text-[10px] font-mono text-slate-400 select-none pointer-events-none">
                                {idx + 1}.
                            </span>
                            <input
                                id={`${idPrefix}-word-${idx}`}
                                data-testid={`${idPrefix}-word-${idx}`}
                                type="text"
                                value={words[idx] || ''}
                                onFocus={() => setFocusedIndex(idx)}
                                onBlur={() => {
                                    setTimeout(() => {
                                        setFocusedIndex((prev) => (prev === idx ? null : prev));
                                    }, 120);
                                }}
                                onChange={(e) => handleWordChange(idx, e.target.value)}
                                placeholder={idx === 0 && allowKitUri ? t('backup.firstWordPlaceholder') : ''}
                                className="w-full pl-7 pr-2 py-2 text-xs font-mono bg-slate-50 border border-slate-200 focus:bg-white focus:border-theme-primary rounded-xl outline-hidden transition-all text-theme-secondary"
                                autoCapitalize="none"
                                autoCorrect="off"
                                autoComplete="off"
                                spellCheck={false}
                            />
                        </div>
                    ))}
                </div>

                {/* BIP-39 Word Suggestions */}
                {suggestions.length > 0 && focusedIndex !== null && (
                    <div className="flex flex-wrap items-center gap-1.5 pt-1">
                        {suggestions.map((suggestion) => (
                            <button
                                key={suggestion}
                                type="button"
                                onMouseDown={(e) => {
                                    e.preventDefault();
                                    const targetIdx = focusedIndex;
                                    setWords((prev) => {
                                        const next = [...prev];
                                        next[targetIdx] = suggestion;
                                        return next;
                                    });
                                    if (targetIdx + 1 < wordCount) {
                                        document.getElementById(`${idPrefix}-word-${targetIdx + 1}`)?.focus();
                                    }
                                }}
                                className="px-2.5 py-0.5 text-[11px] font-mono font-semibold bg-theme-primary/10 text-theme-primary hover:bg-theme-primary/20 rounded-full cursor-pointer"
                            >
                                {suggestion}
                            </button>
                        ))}
                    </div>
                )}

                {/* Live validation status */}
                {filledCount === wordCount && (
                    <div
                        data-testid="seed-validation-status"
                        className={`flex items-center gap-2 text-xs font-semibold pt-1 ${
                            isValid
                                ? 'text-emerald-700'
                                : validationError
                                  ? 'text-rose-600'
                                  : 'text-theme-light'
                        }`}
                    >
                        {isValid ? (
                            <>
                                <CheckCircle2 size={14} className="shrink-0" />
                                <span>{t('auth.seedPhraseValid')}</span>
                            </>
                        ) : validationError ? (
                            <>
                                <AlertCircle size={14} className="shrink-0" />
                                <span>{validationError}</span>
                            </>
                        ) : null}
                    </div>
                )}
            </div>

            {/* Passphrase, Subaccount Prefix & Language Disclosure */}
            <AdvancedDisclosure
                icon={<SlidersHorizontal size={15} className="text-theme-primary" />}
                title={
                    <span className="text-xs font-bold text-theme-secondary">
                        {showUserPrefix
                            ? t('backup.seedAdvancedTitleWithPrefix')
                            : t('backup.seedAdvancedTitleNoPrefix')}
                    </span>
                }
                isCustom={isCustomAdvanced}
                forceOpen={shouldForceOpenAdvanced}
                toggleTestId="seed-advanced-toggle"
                contentTestId="seed-advanced-content"
            >
                <div className="space-y-4">
                    {/* Optional BIP-39 Passphrase (13th / 25th word) */}
                    <div className="space-y-1.5">
                        <div className="flex items-center gap-1.5">
                            <label
                                htmlFor={`${idPrefix}-passphrase`}
                                className="text-xs font-bold text-theme-secondary"
                            >
                                {t('profile.optionalPassphraseLabel')}
                            </label>
                            <HelpIcon topic="passphrase" size={12} />
                        </div>
                        <Input
                            id={`${idPrefix}-passphrase`}
                            ref={passphraseInputRef}
                            data-testid="seed-passphrase-input"
                            type="password"
                            showToggle
                            value={passphrase}
                            onChange={(e) => setPassphrase(e.target.value)}
                            placeholder={t('auth.passphrasePlaceholder')}
                            className="text-xs"
                            autoComplete="off"
                        />
                        {detectedKit?.hasBip39Passphrase && !passphrase && (
                            <p
                                data-testid="kit-passphrase-required-hint"
                                className="text-[11px] font-semibold text-amber-700"
                            >
                                {t('backup.kitRequiresPassphraseHint')}
                            </p>
                        )}
                    </div>

                    {/* Optional Subaccount Prefix */}
                    {showUserPrefix && (
                        <div className="space-y-1.5">
                            <div className="flex items-center justify-between gap-2">
                                <div className="flex items-center gap-1.5">
                                    <label
                                        htmlFor={`${idPrefix}-user-prefix`}
                                        className="text-xs font-bold text-theme-secondary"
                                    >
                                        {t('profile.devicePrefixLabel')} ({t('common.optional')})
                                    </label>
                                    <HelpIcon topic="subaccount" size={12} />
                                </div>
                                <button
                                    type="button"
                                    onClick={() => setShowPrefixInfo(true)}
                                    className="text-[10px] font-bold text-theme-primary hover:underline flex items-center gap-1 cursor-pointer"
                                >
                                    <HelpCircle size={12} />
                                    <span>{t('profile.readInstructions')}</span>
                                </button>
                            </div>
                            <Input
                                id={`${idPrefix}-user-prefix`}
                                ref={prefixInputRef}
                                data-testid="seed-user-prefix-input"
                                type="text"
                                value={userPrefix}
                                onChange={(e) => setUserPrefix(e.target.value)}
                                placeholder={t('profile.backupRestoreUserPrefixPlaceholder')}
                                className="text-xs font-mono"
                                autoComplete="off"
                            />
                            <p className="text-[11px] text-theme-light leading-relaxed">
                                {t('backup.subaccountPrefixHint')}
                            </p>
                        </div>
                    )}

                    {/* BIP-39 Language */}
                    <div className="space-y-1.5">
                        <label
                            htmlFor={`${idPrefix}-language`}
                            className="text-xs font-bold text-theme-secondary flex items-center gap-1.5"
                        >
                            <Languages size={13} className="text-theme-primary" />
                            <span>{t('common.dictionary')}</span>
                        </label>
                        <select
                            id={`${idPrefix}-language`}
                            data-testid="seed-language-select"
                            value={language}
                            onChange={(e) => setLanguage(e.target.value as MnemonicLanguage)}
                            className="w-full px-3 py-2 text-xs bg-white border border-theme-subtle rounded-xl text-theme-secondary focus:ring-2 focus:ring-theme-primary/20 outline-none"
                        >
                            {ALL_LANGUAGES.map((opt) => (
                                <option key={opt.value} value={opt.value}>
                                    {opt.label}
                                </option>
                            ))}
                        </select>
                    </div>
                </div>
            </AdvancedDisclosure>

            {showUserPrefix && (
                <PrefixInfoModal
                    isOpen={showPrefixInfo}
                    onClose={() => setShowPrefixInfo(false)}
                />
            )}
        </div>
    );
};
