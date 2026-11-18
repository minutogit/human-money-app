// src/components/auth/Login.tsx
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { BookOpen } from 'lucide-react';
import logo from '../../assets/logo.png';
import { useNavigation } from '../../context/NavigationContext';
import { stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { AuthLayout } from './AuthLayout';
import { DeviceLockoutModal, PostHandoverSuccessModal } from './DeviceLockoutModals';
import { LoginCredentialPanel, LoginOnboardingCards } from './LoginCredentialPanel';
import { LoginDeleteModals } from './ProfileDeleteModals';
import { ResumeSocialRecoveryBanner } from './ResumeSocialRecoveryBanner';
import { useLoginFlow } from './useLoginFlow';

interface LoginProps {
    onLoginSuccess: (profileName: string, folderName?: string) => void;
    onSwitchToCreate: () => void;
    onSwitchToRestoreBackup: () => void;
    onSwitchToReset: () => void;
}

export function Login({
    onLoginSuccess,
    onSwitchToCreate,
    onSwitchToRestoreBackup,
    onSwitchToReset,
}: LoginProps) {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
    const flow = useLoginFlow({ onLoginSuccess, onSwitchToCreate });

    return (
        <AuthLayout maxWidth="max-w-2xl">
            <DeviceLockoutModal
                kind="handover"
                isOpen={flow.showSupersededUI}
                profileName={flow.activeProfileName}
                targetEpoch={flow.supersededTargetEpoch}
                reclaimPassword={flow.password}
                onReclaimPasswordChange={flow.setPassword}
                isReclaiming={flow.isLoggingIn}
                onReclaim={async () => {
                    flow.setShowSupersededUI(false);
                    await flow.handleHandover();
                }}
                onSwitchOrLogout={() => {
                    flow.setShowSupersededUI(false);
                    flow.setFeedback('', null);
                }}
                onRequestDeleteLocal={() => {
                    flow.setShowSupersededUI(false);
                    flow.setShowDeletePasswordPrompt(true);
                    flow.setDeletePassword('');
                    flow.setDeleteError('');
                }}
                zIndexClass="z-50"
            />

            <DeviceLockoutModal
                kind="clone"
                isOpen={flow.showCloneUI}
                profileName={flow.activeProfileName}
                onStartRecovery={() => {
                    flow.setShowCloneUI(false);
                    onSwitchToRestoreBackup();
                }}
                onSwitchOrLogout={() => {
                    flow.setShowCloneUI(false);
                    flow.setFeedback('', null);
                }}
                secondaryCancelLabel={t('common.cancel', 'Cancel')}
                onSecondaryCancel={() => {
                    flow.setShowCloneUI(false);
                    flow.setFeedback('', null);
                }}
                zIndexClass="z-50"
            />

            <LoginDeleteModals
                activeProfileName={flow.activeProfileName}
                showPasswordPrompt={flow.showDeletePasswordPrompt}
                showConfirmModal={flow.showDeleteConfirm}
                deletePassword={flow.deletePassword}
                deleteUserId={flow.deleteUserId}
                deleteError={flow.deleteError}
                isVerifying={flow.isVerifyingDelete}
                isDeleting={flow.isDeleting}
                onPasswordChange={flow.setDeletePassword}
                onClearError={() => flow.setDeleteError('')}
                onVerifyPassword={flow.handleVerifyDeletePassword}
                onConfirmDelete={flow.handleDeleteProfile}
                onCancelPrompt={() => {
                    flow.setShowDeletePasswordPrompt(false);
                    flow.setDeletePassword('');
                    flow.setDeleteError('');
                }}
                onCancelConfirm={() => {
                    flow.setShowDeleteConfirm(false);
                    flow.setDeleteError('');
                    flow.setDeletePassword('');
                }}
            />

            <PostHandoverSuccessModal
                isOpen={flow.showPostHandoverWarning}
                userId={flow.handoverUserId}
                onAcknowledge={() => {
                    const loggedInProfile = flow.profiles.find(
                        (p) => p.folderName === flow.selectedProfile
                    );
                    if (loggedInProfile) onLoginSuccess(loggedInProfile.profileName);
                }}
            />

            {!flow.showPostHandoverWarning && (
                <>
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
                                {flow.profiles.length > 0
                                    ? t('auth.loginToYourWallet')
                                    : t('dashboard.welcomeTitle')}
                            </p>
                        </div>
                    </div>

                    <ResumeSocialRecoveryBanner
                        session={flow.activeSocialSession}
                        bannerTestId="login-resume-social-banner"
                        buttonTestId="login-resume-social-btn"
                        onResume={() => {
                            if (flow.activeSocialSession?.sessionMode === 'reset_local_password') {
                                onSwitchToReset();
                            } else {
                                navigate({ view: 'restore_backup', initialMethod: 'social' });
                            }
                        }}
                    />

                    {flow.profiles.length > 0 ? (
                        <LoginCredentialPanel
                            profiles={flow.profiles}
                            selectedProfile={flow.selectedProfile}
                            onSelectProfile={(folder) => {
                                flow.setSelectedProfile(folder);
                                flow.setPassword('');
                                flow.setPin('');
                                flow.setShowHandoverUI(false);
                                flow.setShowSupersededUI(false);
                                flow.setShowCloneUI(false);
                                flow.setFeedback('', null);
                            }}
                            onRequestDelete={() => {
                                flow.setShowDeletePasswordPrompt(true);
                                flow.setDeletePassword('');
                                flow.setDeleteError('');
                            }}
                            authMethod={flow.authMethod}
                            onSwitchAuthMethod={(nextMethod) => {
                                flow.setAuthMethod(nextMethod);
                                if (nextMethod === 'password') flow.setPin('');
                                else flow.setPassword('');
                                flow.setFeedback('', null);
                            }}
                            pinStatus={flow.pinStatus}
                            pin={flow.pin}
                            onPinChange={(nextPin) => {
                                flow.setPin(nextPin);
                                if (flow.feedbackType === 'error') flow.setFeedback('', null);
                            }}
                            password={flow.password}
                            onPasswordChange={(nextPassword) => {
                                flow.setPassword(nextPassword);
                                if (flow.feedbackType === 'error') flow.setFeedback('', null);
                            }}
                            pinInputRef={flow.pinInputRef}
                            passwordInputRef={flow.passwordInputRef}
                            showHandoverUI={flow.showHandoverUI}
                            onCancelHandover={() => {
                                flow.setShowHandoverUI(false);
                                flow.setFeedback('', null);
                            }}
                            onHandover={() => void flow.handleHandover()}
                            isLoading={flow.isLoading}
                            isLoggingIn={flow.isLoggingIn}
                            onSubmit={() => {
                                if (
                                    flow.authMethod === 'pin' &&
                                    flow.pinStatus?.enabled &&
                                    !flow.showHandoverUI
                                ) {
                                    void flow.handlePinLogin();
                                } else {
                                    void flow.handleLogin();
                                }
                            }}
                            onSwitchToCreate={onSwitchToCreate}
                            onSwitchToRestoreBackup={onSwitchToRestoreBackup}
                            onSwitchToReset={onSwitchToReset}
                            feedbackMsg={flow.feedbackMsg}
                            feedbackType={flow.feedbackType}
                        />
                    ) : (
                        <div className="space-y-6 pt-6">
                            <LoginOnboardingCards
                                onSwitchToCreate={onSwitchToCreate}
                                onSwitchToRestoreBackup={onSwitchToRestoreBackup}
                            />
                        </div>
                    )}

                    <div className="pt-4 border-t border-theme-subtle/40 flex flex-col items-center gap-1.5 text-center">
                        <button
                            type="button"
                            onClick={() => navigate({ view: 'help_center' })}
                            className="text-xs text-theme-primary hover:text-theme-primary/85 font-bold hover:underline focus:outline-none flex items-center gap-1.5 mt-4 cursor-pointer transition-colors"
                        >
                            <BookOpen size={14} /> {t('helpCenter.readConceptLink')}
                        </button>
                        <button
                            type="button"
                            onClick={async (e) => {
                                e.preventDefault();
                                try {
                                    await openUrl('https://menschlich-miteinander.org');
                                } catch (err) {
                                    logger.error(`Failed to open URL: ${stringifyError(err)}`);
                                }
                            }}
                            className="text-xs text-theme-light hover:text-theme-secondary hover:underline focus:outline-none mt-2 font-medium cursor-pointer"
                        >
                            {t('auth.supportTeaserLink')}
                        </button>
                    </div>
                </>
            )}
        </AuthLayout>
    );
}
