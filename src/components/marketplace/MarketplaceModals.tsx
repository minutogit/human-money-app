import { useTranslation } from 'react-i18next';
import type {
    Ad,
    Contact,
    FrontendAdInput,
    FrontendMyAd,
    FrontendMyMarketProfile,
    FrontendSaveMyMarketProfileRequest,
} from '../../types';
import ContactDialog from '../contacts/ContactDialog';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { AdEditorModal } from './AdEditorModal';
import { ImportCardModal } from './ImportCardModal';
import { ProfileCardEditor } from './ProfileCardEditor';
import { RenewAdModal } from './RenewAdModal';
import { ShareCardModal } from './ShareCardModal';

interface MarketplaceModalsProps {
    myProfile: FrontendMyMarketProfile | null;
    currentProfile: FrontendMyMarketProfile;
    activePublishedMyAdsCount: number;
    isCreateAdOpen: boolean;
    editingAd: Ad | FrontendMyAd | null;
    renewingAd: FrontendMyAd | null;
    isProfileEditorOpen: boolean;
    isShareModalOpen: boolean;
    isImportModalOpen: boolean;
    isPublishPromptOpen: boolean;
    addContactTarget: { did: string; suggestedName?: string | null } | null;
    onCloseAdEditor: () => void;
    onSaveAd: (input: FrontendAdInput) => Promise<void>;
    onCloseRenewModal: () => void;
    onRenewAd: (id: string, durationDays: number) => Promise<void>;
    onCloseProfileEditor: () => void;
    onSaveProfile: (req: FrontendSaveMyMarketProfileRequest) => Promise<void>;
    onCloseShareModal: () => void;
    onCloseImportModal: () => void;
    onImportSuccess: (did?: string | null) => void;
    onCancelPublishPrompt: () => void;
    onConfirmPublishAndAd: () => Promise<void>;
    onCloseContactDialog: () => void;
    onSaveContactFromInquiry: (contact: Contact) => Promise<void>;
}

export function MarketplaceModals({
    myProfile,
    currentProfile,
    activePublishedMyAdsCount,
    isCreateAdOpen,
    editingAd,
    renewingAd,
    isProfileEditorOpen,
    isShareModalOpen,
    isImportModalOpen,
    isPublishPromptOpen,
    addContactTarget,
    onCloseAdEditor,
    onSaveAd,
    onCloseRenewModal,
    onRenewAd,
    onCloseProfileEditor,
    onSaveProfile,
    onCloseShareModal,
    onCloseImportModal,
    onImportSuccess,
    onCancelPublishPrompt,
    onConfirmPublishAndAd,
    onCloseContactDialog,
    onSaveContactFromInquiry,
}: MarketplaceModalsProps) {
    const { t } = useTranslation();

    return (
        <>
            <AdEditorModal
                isOpen={isCreateAdOpen || !!editingAd}
                onClose={onCloseAdEditor}
                onSave={onSaveAd}
                editingAd={editingAd}
                activeAdsCount={activePublishedMyAdsCount}
                maxAds={currentProfile?.mediaMaxCount ?? 10}
                cardPublished={currentProfile.published}
                allowInquiries={currentProfile.allowInquiries}
                ownerDid={currentProfile?.profile?.id}
                mediaMaxBytes={currentProfile?.mediaMaxBytes}
                mediaMaxCount={currentProfile?.mediaMaxCount}
                mediaUsedCount={currentProfile?.mediaUsedCount}
            />

            <RenewAdModal
                isOpen={!!renewingAd}
                onClose={onCloseRenewModal}
                ad={renewingAd}
                onRenew={onRenewAd}
            />

            {myProfile && (
                <ProfileCardEditor
                    isOpen={isProfileEditorOpen}
                    onClose={onCloseProfileEditor}
                    myProfile={myProfile}
                    onSave={onSaveProfile}
                />
            )}

            <ShareCardModal
                isOpen={isShareModalOpen}
                onClose={onCloseShareModal}
                title={t('marketplace.shareOwnCardTitle', 'Meine Visitenkarte teilen')}
                subtitle={t(
                    'marketplace.shareOwnCardSubtitle',
                    'Sichere Datei oder QR-Code zum Austausch',
                )}
                fileWire={currentProfile.fileWire}
                shareUri={currentProfile.shareUri}
                qrSvg={currentProfile.qrSvg}
                did={currentProfile.profile.id}
                qrIsPartial={currentProfile.qrIsPartial}
            />

            <ImportCardModal
                isOpen={isImportModalOpen}
                onClose={onCloseImportModal}
                onImportSuccess={onImportSuccess}
            />

            <ConfirmationModal
                isOpen={isPublishPromptOpen}
                onCancel={onCancelPublishPrompt}
                onConfirm={onConfirmPublishAndAd}
                title={t('marketplace.publishCardBeforeAdTitle', 'Visitenkarte veröffentlichen?')}
                description={t(
                    'marketplace.publishCardAndAdDesc',
                    'Um dieses Inserat für andere sichtbar zu machen, muss auch deine Visitenkarte veröffentlicht sein. Möchtest du Visitenkarte und Inserat jetzt veröffentlichen?',
                )}
                confirmText={t('marketplace.publishCardAndAdBtn', 'Beides veröffentlichen')}
                cancelText={t('common.cancel', 'Abbrechen')}
            />

            {addContactTarget && (
                <ContactDialog
                    isOpen={!!addContactTarget}
                    onClose={onCloseContactDialog}
                    onSave={onSaveContactFromInquiry}
                    initialDid={addContactTarget.did}
                    initialProfile={
                        addContactTarget.suggestedName
                            ? { firstName: addContactTarget.suggestedName }
                            : undefined
                    }
                />
            )}
        </>
    );
}
