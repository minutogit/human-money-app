// src/components/auth/CreateNewProfile.tsx
import { useState, useEffect, useRef, FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, ArrowLeft, Fingerprint, Lock, RefreshCw } from 'lucide-react';
import logo from '../../assets/logo.png';
import { profileService } from '../../services/profileService';
import { authService } from '../../services/authService';
import { recoveryService } from '../../services/recoveryService';
import { ActiveRecoverySessionDto, MnemonicLanguage } from '../../types';
import { translateError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { cleanSeedText } from '../../utils/seedUtils';
import { Button } from '../ui/Button';
import { AuthLayout } from './AuthLayout';
import {
    ExpertDeviceSettingsCard,
    ProfileCredentialsCard,
    StandardQuickCreateStep,
} from './CreateProfileCredentialSteps';
import {
    ConfirmSeedStep,
    ConfirmationWord,
    DisplaySeedStep,
} from './CreateProfileSeedSteps';
import { ResumeSocialRecoveryBanner } from './ResumeSocialRecoveryBanner';
import { detectSystemMnemonicLanguage } from './SeedPhraseInput';

type WizardStep = 'display_seed' | 'confirm_seed' | 'set_password';

interface CreateNewProfileProps {
    onProfileCreated: (profileName?: string, folderName?: string) => void;
    onSwitchToLogin?: () => void;
    onSwitchToRestore?: (method?: 'social' | 'words_or_sheet' | 'file_or_folder') => void;
    defaultExpertMode?: boolean;
}

export function CreateNewProfile({
    onProfileCreated,
    onSwitchToLogin,
    onSwitchToRestore,
    defaultExpertMode,
}: CreateNewProfileProps) {
    const { t, i18n } = useTranslation();
    const [isExpertMode, setIsExpertMode] = useState<boolean>(defaultExpertMode ?? false);
    const [retainMnemonic, setRetainMnemonic] = useState<boolean>(true);
    const [profilesCount, setProfilesCount] = useState<number | null>(null);
    const [activeSocialSession, setActiveSocialSession] =
        useState<ActiveRecoverySessionDto | null>(null);

    const [wizardStep, setWizardStep] = useState<WizardStep>('display_seed');
    const [generatedSeed, setGeneratedSeed] = useState<string[]>([]);
    const [wordCount, setWordCount] = useState<12 | 24>(12);
    const passphraseOrdinal = i18n.language?.startsWith('de')
        ? wordCount === 12
            ? '13.'
            : '25.'
        : wordCount === 12
          ? '13th'
          : '25th';
    const [selectedLanguage, setSelectedLanguage] = useState<MnemonicLanguage>(() =>
        detectSystemMnemonicLanguage()
    );
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [isLoading, setIsLoading] = useState(false);

    const [confirmationWords, setConfirmationWords] = useState<ConfirmationWord[]>([]);
    const [userConfirmationInput, setUserConfirmationInput] = useState<Record<number, string>>({});
    const [isBulkMode, setIsBulkMode] = useState(false);
    const [bulkSeedInput, setBulkSeedInput] = useState('');

    const [passphrase, setPassphrase] = useState<string>('');
    const [confirmPassphrase, setConfirmPassphrase] = useState<string>('');
    const [showPassphraseAccordion, setShowPassphraseAccordion] = useState(false);
    const [profileName, setProfileName] = useState('');
    const [userPrefix, setUserPrefix] = useState('');
    const [isSubAccount, setIsSubAccount] = useState(false);
    const [password, setPassword] = useState('');
    const [confirmPassword, setConfirmPassword] = useState('');
    const [showPrefixInfo, setShowPrefixInfo] = useState(false);
    const passwordInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        logger.info('CreateNewProfile component displayed');
        authService
            .listProfiles()
            .then((list) => setProfilesCount(list.length))
            .catch(() => setProfilesCount(null));
        recoveryService
            .getActiveRecoverySession?.()
            ?.then((session) =>
                setActiveSocialSession(
                    session && session.sessionMode !== 'reset_local_password' ? session : null
                )
            )
            ?.catch(() => setActiveSocialSession(null));
    }, []);

    useEffect(() => {
        let active = true;
        async function generateNewSeed() {
            setIsLoading(true);
            try {
                const newSeed: string = await profileService.generateMnemonic(
                    wordCount,
                    selectedLanguage
                );
                if (!active) return;
                setGeneratedSeed(newSeed.split(' '));
            } catch (e) {
                if (!active) return;
                setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(e, t)}`);
            } finally {
                if (active) setIsLoading(false);
            }
        }
        void generateNewSeed();
        return () => {
            active = false;
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [wordCount, selectedLanguage]);

    useEffect(() => {
        if (!isBulkMode) return;
        const cleaned = cleanSeedText(bulkSeedInput);
        if (
            (/[0-9.,\-:]/.test(bulkSeedInput) || /\s\s/.test(bulkSeedInput)) &&
            cleaned !== bulkSeedInput
        ) {
            setBulkSeedInput(cleaned);
        }
    }, [bulkSeedInput, isBulkMode]);

    const prepareConfirmationStep = () => {
        const shuffled = [...generatedSeed].map((word, index) => ({ word, index }));
        shuffled.sort(() => 0.5 - Math.random());
        const selected = shuffled.slice(0, 3).map((item) => ({ index: item.index, value: '' }));
        selected.sort((a, b) => a.index - b.index);
        setConfirmationWords(selected);
        setUserConfirmationInput({});
        setBulkSeedInput('');
        setIsBulkMode(false);
        setFeedbackMsg('');
        setWizardStep('confirm_seed');
    };

    const handleConfirmSeedSubmit = (e: FormEvent) => {
        e.preventDefault();

        if (passphrase !== confirmPassphrase) {
            setFeedbackMsg(t('profile.passphraseMismatch'));
            return;
        }

        if (isBulkMode) {
            if (cleanSeedText(bulkSeedInput) !== generatedSeed.join(' ')) {
                setFeedbackMsg(t('profile.seedMismatch'));
                return;
            }
        } else {
            for (const word of confirmationWords) {
                if (userConfirmationInput[word.index] !== generatedSeed[word.index]) {
                    setFeedbackMsg(t('profile.seedWordsIncorrect'));
                    return;
                }
            }
        }
        setFeedbackMsg(t('profile.seedConfirmed'));
        setTimeout(() => {
            setFeedbackMsg('');
            setWizardStep('set_password');
        }, 1000);
    };

    const handleFinalizeProfile = async (e?: FormEvent) => {
        e?.preventDefault();
        const trimmedName = profileName.trim();
        if (!trimmedName) {
            setFeedbackMsg(t('profile.profileNameRequired'));
            return;
        }
        if (password !== confirmPassword) {
            setFeedbackMsg(t('profile.passwordMismatch'));
            return;
        }
        if (password.length < 8) {
            setFeedbackMsg(t('profile.passwordMinLength'));
            return;
        }
        if (isExpertMode && passphrase !== confirmPassphrase) {
            setFeedbackMsg(t('profile.passphraseMismatch'));
            return;
        }

        setIsLoading(true);
        setFeedbackMsg(t('profile.creatingProgress'));
        setTimeout(async () => {
            try {
                let mnemonic = generatedSeed.join(' ');
                if (!isExpertMode && (!mnemonic.trim() || generatedSeed.length < 12)) {
                    mnemonic = await profileService.generateMnemonic(12, selectedLanguage);
                }
                const localInstanceId = await authService.getLocalInstanceId();
                await profileService.createProfile({
                    profileName: trimmedName,
                    mnemonic,
                    passphrase: isExpertMode ? passphrase || undefined : undefined,
                    userPrefix:
                        isExpertMode && isSubAccount ? userPrefix || undefined : undefined,
                    password,
                    localInstanceId,
                    language: selectedLanguage,
                    retainMnemonic: isExpertMode ? retainMnemonic : true,
                    initialReachPreset: 'medium',
                });
                const listed = await authService.listProfiles().catch(() => []);
                const created = Array.isArray(listed)
                    ? listed.find((p) => p.profileName === trimmedName)
                    : undefined;
                onProfileCreated(trimmedName, created?.folderName);
            } catch (err) {
                setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(err, t)}`);
            } finally {
                setIsLoading(false);
            }
        }, 150);
    };

    const renderContent = () => {
        if (!isExpertMode) {
            return (
                <StandardQuickCreateStep
                    profilesCount={profilesCount}
                    profileName={profileName}
                    password={password}
                    confirmPassword={confirmPassword}
                    isLoading={isLoading}
                    onProfileNameChange={setProfileName}
                    onPasswordChange={setPassword}
                    onConfirmPasswordChange={setConfirmPassword}
                    onSubmit={handleFinalizeProfile}
                    onEnterExpertMode={() => setIsExpertMode(true)}
                    onSwitchToLogin={onSwitchToLogin}
                    onSwitchToRestore={onSwitchToRestore}
                />
            );
        }

        switch (wizardStep) {
            case 'display_seed':
                return (
                    <DisplaySeedStep
                        profilesCount={profilesCount}
                        generatedSeed={generatedSeed}
                        selectedLanguage={selectedLanguage}
                        wordCount={wordCount}
                        passphrase={passphrase}
                        passphraseOrdinal={passphraseOrdinal}
                        showPassphraseAccordion={showPassphraseAccordion}
                        isLoading={isLoading}
                        onSwitchToRestore={onSwitchToRestore}
                        onHideExpertMode={() => setIsExpertMode(false)}
                        onLanguageChange={setSelectedLanguage}
                        onWordCountChange={setWordCount}
                        onTogglePassphraseAccordion={() =>
                            setShowPassphraseAccordion((prev) => !prev)
                        }
                        onPassphraseChange={setPassphrase}
                        onProceed={prepareConfirmationStep}
                        onSwitchToLogin={onSwitchToLogin}
                    />
                );

            case 'confirm_seed':
                return (
                    <ConfirmSeedStep
                        isBulkMode={isBulkMode}
                        bulkSeedInput={bulkSeedInput}
                        confirmationWords={confirmationWords}
                        userConfirmationInput={userConfirmationInput}
                        passphrase={passphrase}
                        confirmPassphrase={confirmPassphrase}
                        onToggleBulkMode={() => {
                            setIsBulkMode((prev) => !prev);
                            setFeedbackMsg('');
                        }}
                        onBulkSeedChange={setBulkSeedInput}
                        onWordInputChange={(index, value) =>
                            setUserConfirmationInput((prev) => ({
                                ...prev,
                                [index]: value.trim().toLowerCase(),
                            }))
                        }
                        onConfirmPassphraseChange={setConfirmPassphrase}
                        onBack={() => setWizardStep('display_seed')}
                        onSubmit={handleConfirmSeedSubmit}
                    />
                );

            case 'set_password':
                return (
                    <form
                        onSubmit={handleFinalizeProfile}
                        className="space-y-6 sm:space-y-8 animate-in slide-in-from-right-8 duration-500 pb-10"
                    >
                        <div className="text-center space-y-2">
                            <h2 className="text-2xl font-black text-theme-primary tracking-tight">
                                {t('profile.step3Title')}
                            </h2>
                            <p className="text-sm font-medium text-theme-light">
                                {t('profile.step3Subtitle')}
                            </p>
                        </div>

                        <div className="space-y-6">
                            <ProfileCredentialsCard
                                profileName={profileName}
                                password={password}
                                confirmPassword={confirmPassword}
                                onProfileNameChange={setProfileName}
                                onPasswordChange={setPassword}
                                onConfirmPasswordChange={setConfirmPassword}
                                passwordRef={passwordInputRef}
                            />

                            <ExpertDeviceSettingsCard
                                isSubAccount={isSubAccount}
                                userPrefix={userPrefix}
                                retainMnemonic={retainMnemonic}
                                showPrefixInfo={showPrefixInfo}
                                onSubAccountChange={(checked) => {
                                    setIsSubAccount(checked);
                                    if (!checked) setUserPrefix('');
                                }}
                                onUserPrefixChange={setUserPrefix}
                                onRetainMnemonicChange={setRetainMnemonic}
                                onOpenPrefixInfo={() => setShowPrefixInfo(true)}
                                onClosePrefixInfo={() => setShowPrefixInfo(false)}
                            />
                        </div>

                        <div className="flex flex-col items-center gap-4 pt-4">
                            <div className="flex gap-4 w-full">
                                <Button
                                    type="button"
                                    variant="secondary"
                                    onClick={() => setWizardStep('confirm_seed')}
                                    className="flex-1 py-4 rounded-2xl gap-2"
                                >
                                    <ArrowLeft size={18} /> {t('profile.back')}
                                </Button>
                                <Button
                                    type="submit"
                                    disabled={isLoading}
                                    data-testid="onboarding-finish-button"
                                    className="flex-[2] py-5 rounded-3xl shadow-premium-lg text-lg gap-3"
                                >
                                    {isLoading ? (
                                        <RefreshCw className="animate-spin" size={24} />
                                    ) : (
                                        <Fingerprint size={24} />
                                    )}
                                    {isLoading ? t('profile.creating') : t('profile.create')}
                                </Button>
                            </div>
                            <p className="text-[10px] font-bold text-theme-light flex items-center gap-2">
                                <Lock size={12} /> {t('profile.dataEncrypted')}
                            </p>
                        </div>
                    </form>
                );
        }
    };

    return (
        <AuthLayout maxWidth="max-w-2xl">
            <div className="flex items-center justify-center gap-3 sm:gap-6">
                <img
                    src={logo}
                    alt="Human Money Logo"
                    className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm"
                />
                <div className="text-left space-y-0 sm:space-y-0.5">
                    <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">
                        HUMAN MONEY
                    </h1>
                    <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">
                        {t('profile.createNewProfile')}
                    </p>
                </div>
            </div>

            {activeSocialSession &&
                activeSocialSession.sessionMode !== 'reset_local_password' &&
                onSwitchToRestore && (
                    <ResumeSocialRecoveryBanner
                        session={activeSocialSession}
                        bannerTestId="create-profile-resume-social-banner"
                        buttonTestId="create-profile-resume-social-btn"
                        onResume={() => onSwitchToRestore('social')}
                    />
                )}

            {renderContent()}

            {feedbackMsg && !isLoading && (
                <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in shake duration-500">
                    <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                    <p className="text-sm font-bold text-rose-800 leading-tight">{feedbackMsg}</p>
                </div>
            )}
        </AuthLayout>
    );
}
