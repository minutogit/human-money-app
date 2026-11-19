// src/help/helpTopics.ts
export type HelpGroup =
    | 'firstSteps'
    | 'dailyVouchers'
    | 'contactsTrust'
    | 'marketplace'
    | 'securityBackup'
    | 'faq'
    | 'expert';

export type HelpTopicId =
    // Erste Schritte / Getting Started
    | 'conceptParadigm'
    | 'voucherBasics'
    | 'profile'
    | 'benutzerId'
    | 'uiModes'
    | 'conceptPrivacy'
    | 'conceptFuture'
    // Gutscheine im Alltag / Everyday Vouchers
    | 'sendReceive'
    | 'voucherMode'
    | 'voucherStandards'
    | 'voucherExpiry'
    | 'voucherCreator'
    | 'senderName'
    | 'privacy'
    | 'settingsPrivacy'
    // Kontakte & Vertrauen / Contacts & Trust
    | 'conceptCommunity'
    | 'trustLevels'
    | 'guarantorVsRecommendation'
    | 'wotNetwork'
    | 'visibilityReach'
    | 'voucherSignatures'
    | 'signatureRequest'
    | 'voucherTrust'
    // Marktplatz & Angebote / Marketplace & Offers
    | 'marketplace'
    | 'marketCard'
    | 'cardVisibility'
    | 'ads'
    // Sicherheit & Sicherung / Security & Backup
    | 'whichBackup'
    | 'deviceLoss'
    | 'backup'
    | 'mnemonic'
    | 'socialRecovery'
    | 'emergencySheet'
    | 'offlineProtection'
    | 'offlineRisk'
    | 'conflicts'
    | 'multiDevice'
    | 'connection'
    // Häufige Fragen / FAQ
    | 'faqIsItCrypto'
    | 'faqWhatDoesItCost'
    | 'faqCanIBeCheated'
    | 'faqSyncDown'
    // Für Fortgeschrittene / For Advanced Users
    | 'profileSeal'
    | 'passphrase'
    | 'subaccount'
    | 'handover'
    | 'deviceBinding'
    | 'l2Protection';

export type HelpAudience = 'everyone' | 'expert';

export interface HelpTopic {
    id: HelpTopicId;
    group: HelpGroup;
    audience: HelpAudience;
    titleKey: string; // help.<id>.title
    shortKey: string; // help.<id>.short - max. 1 sentence, ~120 chars
    longKey: string; // help.<id>.long - Markdown-light: paragraphs (\n\n), "• " bullet lists, **bold**
    deepDiveKey?: string; // help.<id>.deepDive - Collapsible technical background section
    related?: HelpTopicId[];
}

export interface HelpGroupDefinition {
    id: HelpGroup;
    audience: HelpAudience;
    labelKey: string;
}

export const HELP_GROUPS: HelpGroupDefinition[] = [
    { id: 'firstSteps', audience: 'everyone', labelKey: 'helpCenter.groupFirstSteps' },
    { id: 'dailyVouchers', audience: 'everyone', labelKey: 'helpCenter.groupDailyVouchers' },
    { id: 'contactsTrust', audience: 'everyone', labelKey: 'helpCenter.groupContactsTrust' },
    { id: 'marketplace', audience: 'everyone', labelKey: 'helpCenter.groupMarketplace' },
    { id: 'securityBackup', audience: 'everyone', labelKey: 'helpCenter.groupSecurityBackup' },
    { id: 'faq', audience: 'everyone', labelKey: 'helpCenter.groupFaq' },
    { id: 'expert', audience: 'expert', labelKey: 'helpCenter.groupExpert' },
];

export const HELP_TOPICS: Record<HelpTopicId, HelpTopic> = {
    // --- Erste Schritte / Getting Started ---
    conceptParadigm: {
        id: 'conceptParadigm',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.conceptParadigm.title',
        shortKey: 'help.conceptParadigm.short',
        longKey: 'help.conceptParadigm.long',
        related: ['voucherBasics', 'benutzerId', 'conceptCommunity'],
    },
    voucherBasics: {
        id: 'voucherBasics',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.voucherBasics.title',
        shortKey: 'help.voucherBasics.short',
        longKey: 'help.voucherBasics.long',
        related: ['conceptParadigm', 'sendReceive', 'voucherMode', 'voucherTrust'],
    },
    profile: {
        id: 'profile',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.profile.title',
        shortKey: 'help.profile.short',
        longKey: 'help.profile.long',
        related: ['benutzerId', 'whichBackup', 'deviceLoss', 'multiDevice'],
    },
    benutzerId: {
        id: 'benutzerId',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.benutzerId.title',
        shortKey: 'help.benutzerId.short',
        longKey: 'help.benutzerId.long',
        deepDiveKey: 'help.benutzerId.deepDive',
        related: ['profile', 'subaccount', 'mnemonic'],
    },
    uiModes: {
        id: 'uiModes',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.uiModes.title',
        shortKey: 'help.uiModes.short',
        longKey: 'help.uiModes.long',
        related: ['profile', 'multiDevice'],
    },
    conceptPrivacy: {
        id: 'conceptPrivacy',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.conceptPrivacy.title',
        shortKey: 'help.conceptPrivacy.short',
        longKey: 'help.conceptPrivacy.long',
        related: ['privacy', 'offlineProtection', 'connection'],
    },
    conceptFuture: {
        id: 'conceptFuture',
        group: 'firstSteps',
        audience: 'everyone',
        titleKey: 'help.conceptFuture.title',
        shortKey: 'help.conceptFuture.short',
        longKey: 'help.conceptFuture.long',
        related: ['conceptParadigm', 'marketplace'],
    },

    // --- Gutscheine im Alltag / Everyday Vouchers ---
    sendReceive: {
        id: 'sendReceive',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.sendReceive.title',
        shortKey: 'help.sendReceive.short',
        longKey: 'help.sendReceive.long',
        related: ['voucherBasics', 'offlineRisk', 'connection', 'senderName'],
    },
    voucherMode: {
        id: 'voucherMode',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.voucherMode.title',
        shortKey: 'help.voucherMode.short',
        longKey: 'help.voucherMode.long',
        related: ['voucherBasics', 'voucherCreator'],
    },
    voucherStandards: {
        id: 'voucherStandards',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.voucherStandards.title',
        shortKey: 'help.voucherStandards.short',
        longKey: 'help.voucherStandards.long',
        related: ['voucherBasics', 'signatureRequest', 'voucherSignatures'],
    },
    voucherExpiry: {
        id: 'voucherExpiry',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.voucherExpiry.title',
        shortKey: 'help.voucherExpiry.short',
        longKey: 'help.voucherExpiry.long',
        related: ['voucherBasics', 'voucherStandards'],
    },
    voucherCreator: {
        id: 'voucherCreator',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.voucherCreator.title',
        shortKey: 'help.voucherCreator.short',
        longKey: 'help.voucherCreator.long',
        deepDiveKey: 'help.voucherCreator.deepDive',
        related: ['voucherBasics', 'voucherSignatures', 'voucherTrust'],
    },
    senderName: {
        id: 'senderName',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.senderName.title',
        shortKey: 'help.senderName.short',
        longKey: 'help.senderName.long',
        related: ['privacy', 'benutzerId'],
    },
    privacy: {
        id: 'privacy',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.privacy.title',
        shortKey: 'help.privacy.short',
        longKey: 'help.privacy.long',
        deepDiveKey: 'help.privacy.deepDive',
        related: ['settingsPrivacy', 'conceptPrivacy', 'senderName'],
    },
    settingsPrivacy: {
        id: 'settingsPrivacy',
        group: 'dailyVouchers',
        audience: 'everyone',
        titleKey: 'help.settingsPrivacy.title',
        shortKey: 'help.settingsPrivacy.short',
        longKey: 'help.settingsPrivacy.long',
        related: ['privacy', 'conceptPrivacy'],
    },

    // --- Kontakte & Vertrauen / Contacts & Trust ---
    conceptCommunity: {
        id: 'conceptCommunity',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.conceptCommunity.title',
        shortKey: 'help.conceptCommunity.short',
        longKey: 'help.conceptCommunity.long',
        related: ['voucherSignatures', 'signatureRequest', 'wotNetwork'],
    },
    trustLevels: {
        id: 'trustLevels',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.trustLevels.title',
        shortKey: 'help.trustLevels.short',
        longKey: 'help.trustLevels.long',
        related: ['wotNetwork', 'guarantorVsRecommendation', 'visibilityReach'],
    },
    guarantorVsRecommendation: {
        id: 'guarantorVsRecommendation',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.guarantorVsRecommendation.title',
        shortKey: 'help.guarantorVsRecommendation.short',
        longKey: 'help.guarantorVsRecommendation.long',
        related: ['trustLevels', 'voucherSignatures', 'wotNetwork'],
    },
    wotNetwork: {
        id: 'wotNetwork',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.wotNetwork.title',
        shortKey: 'help.wotNetwork.short',
        longKey: 'help.wotNetwork.long',
        related: ['trustLevels', 'guarantorVsRecommendation', 'visibilityReach', 'voucherTrust'],
    },
    visibilityReach: {
        id: 'visibilityReach',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.visibilityReach.title',
        shortKey: 'help.visibilityReach.short',
        longKey: 'help.visibilityReach.long',
        deepDiveKey: 'help.visibilityReach.deepDive',
        related: ['wotNetwork', 'trustLevels', 'cardVisibility'],
    },
    voucherSignatures: {
        id: 'voucherSignatures',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.voucherSignatures.title',
        shortKey: 'help.voucherSignatures.short',
        longKey: 'help.voucherSignatures.long',
        related: ['signatureRequest', 'guarantorVsRecommendation', 'voucherCreator'],
    },
    signatureRequest: {
        id: 'signatureRequest',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.signatureRequest.title',
        shortKey: 'help.signatureRequest.short',
        longKey: 'help.signatureRequest.long',
        related: ['voucherSignatures', 'voucherStandards'],
    },
    voucherTrust: {
        id: 'voucherTrust',
        group: 'contactsTrust',
        audience: 'everyone',
        titleKey: 'help.voucherTrust.title',
        shortKey: 'help.voucherTrust.short',
        longKey: 'help.voucherTrust.long',
        deepDiveKey: 'help.voucherTrust.deepDive',
        related: ['wotNetwork', 'trustLevels', 'voucherCreator'],
    },

    // --- Marktplatz & Angebote / Marketplace & Offers ---
    marketplace: {
        id: 'marketplace',
        group: 'marketplace',
        audience: 'everyone',
        titleKey: 'help.marketplace.title',
        shortKey: 'help.marketplace.short',
        longKey: 'help.marketplace.long',
        related: ['marketCard', 'ads', 'cardVisibility'],
    },
    marketCard: {
        id: 'marketCard',
        group: 'marketplace',
        audience: 'everyone',
        titleKey: 'help.marketCard.title',
        shortKey: 'help.marketCard.short',
        longKey: 'help.marketCard.long',
        related: ['marketplace', 'cardVisibility', 'ads'],
    },
    cardVisibility: {
        id: 'cardVisibility',
        group: 'marketplace',
        audience: 'everyone',
        titleKey: 'help.cardVisibility.title',
        shortKey: 'help.cardVisibility.short',
        longKey: 'help.cardVisibility.long',
        related: ['marketCard', 'visibilityReach', 'marketplace'],
    },
    ads: {
        id: 'ads',
        group: 'marketplace',
        audience: 'everyone',
        titleKey: 'help.ads.title',
        shortKey: 'help.ads.short',
        longKey: 'help.ads.long',
        related: ['marketplace', 'marketCard'],
    },

    // --- Sicherheit & Sicherung / Security & Backup ---
    whichBackup: {
        id: 'whichBackup',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.whichBackup.title',
        shortKey: 'help.whichBackup.short',
        longKey: 'help.whichBackup.long',
        related: ['backup', 'emergencySheet', 'socialRecovery', 'deviceLoss'],
    },
    deviceLoss: {
        id: 'deviceLoss',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.deviceLoss.title',
        shortKey: 'help.deviceLoss.short',
        longKey: 'help.deviceLoss.long',
        related: ['whichBackup', 'socialRecovery', 'emergencySheet', 'multiDevice'],
    },
    backup: {
        id: 'backup',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.backup.title',
        shortKey: 'help.backup.short',
        longKey: 'help.backup.long',
        related: ['whichBackup', 'profile', 'mnemonic', 'deviceLoss'],
    },
    mnemonic: {
        id: 'mnemonic',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.mnemonic.title',
        shortKey: 'help.mnemonic.short',
        longKey: 'help.mnemonic.long',
        deepDiveKey: 'help.mnemonic.deepDive',
        related: ['emergencySheet', 'passphrase', 'backup'],
    },
    socialRecovery: {
        id: 'socialRecovery',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.socialRecovery.title',
        shortKey: 'help.socialRecovery.short',
        longKey: 'help.socialRecovery.long',
        deepDiveKey: 'help.socialRecovery.deepDive',
        related: ['whichBackup', 'deviceLoss', 'emergencySheet'],
    },
    emergencySheet: {
        id: 'emergencySheet',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.emergencySheet.title',
        shortKey: 'help.emergencySheet.short',
        longKey: 'help.emergencySheet.long',
        related: ['whichBackup', 'mnemonic', 'deviceLoss'],
    },
    offlineProtection: {
        id: 'offlineProtection',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.offlineProtection.title',
        shortKey: 'help.offlineProtection.short',
        longKey: 'help.offlineProtection.long',
        deepDiveKey: 'help.offlineProtection.deepDive',
        related: ['conflicts', 'offlineRisk', 'conceptPrivacy'],
    },
    offlineRisk: {
        id: 'offlineRisk',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.offlineRisk.title',
        shortKey: 'help.offlineRisk.short',
        longKey: 'help.offlineRisk.long',
        related: ['offlineProtection', 'sendReceive', 'l2Protection'],
    },
    conflicts: {
        id: 'conflicts',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.conflicts.title',
        shortKey: 'help.conflicts.short',
        longKey: 'help.conflicts.long',
        deepDiveKey: 'help.conflicts.deepDive',
        related: ['offlineProtection', 'multiDevice'],
    },
    multiDevice: {
        id: 'multiDevice',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.multiDevice.title',
        shortKey: 'help.multiDevice.short',
        longKey: 'help.multiDevice.long',
        related: ['handover', 'subaccount', 'profileSeal', 'conflicts'],
    },
    connection: {
        id: 'connection',
        group: 'securityBackup',
        audience: 'everyone',
        titleKey: 'help.connection.title',
        shortKey: 'help.connection.short',
        longKey: 'help.connection.long',
        deepDiveKey: 'help.connection.deepDive',
        related: ['conceptPrivacy', 'faqSyncDown', 'visibilityReach'],
    },

    // --- Häufige Fragen / FAQ ---
    faqIsItCrypto: {
        id: 'faqIsItCrypto',
        group: 'faq',
        audience: 'everyone',
        titleKey: 'help.faqIsItCrypto.title',
        shortKey: 'help.faqIsItCrypto.short',
        longKey: 'help.faqIsItCrypto.long',
        related: ['conceptParadigm', 'voucherBasics', 'conceptPrivacy'],
    },
    faqWhatDoesItCost: {
        id: 'faqWhatDoesItCost',
        group: 'faq',
        audience: 'everyone',
        titleKey: 'help.faqWhatDoesItCost.title',
        shortKey: 'help.faqWhatDoesItCost.short',
        longKey: 'help.faqWhatDoesItCost.long',
        related: ['conceptParadigm', 'connection'],
    },
    faqCanIBeCheated: {
        id: 'faqCanIBeCheated',
        group: 'faq',
        audience: 'everyone',
        titleKey: 'help.faqCanIBeCheated.title',
        shortKey: 'help.faqCanIBeCheated.short',
        longKey: 'help.faqCanIBeCheated.long',
        related: ['offlineProtection', 'offlineRisk', 'conflicts', 'voucherTrust'],
    },
    faqSyncDown: {
        id: 'faqSyncDown',
        group: 'faq',
        audience: 'everyone',
        titleKey: 'help.faqSyncDown.title',
        shortKey: 'help.faqSyncDown.short',
        longKey: 'help.faqSyncDown.long',
        related: ['connection', 'sendReceive', 'whichBackup'],
    },

    // --- Für Fortgeschrittene / For Advanced Users ---
    profileSeal: {
        id: 'profileSeal',
        group: 'expert',
        audience: 'expert',
        titleKey: 'help.profileSeal.title',
        shortKey: 'help.profileSeal.short',
        longKey: 'help.profileSeal.long',
        deepDiveKey: 'help.profileSeal.deepDive',
        related: ['multiDevice', 'whichBackup', 'connection'],
    },
    passphrase: {
        id: 'passphrase',
        group: 'expert',
        audience: 'expert',
        titleKey: 'help.passphrase.title',
        shortKey: 'help.passphrase.short',
        longKey: 'help.passphrase.long',
        related: ['mnemonic', 'backup'],
    },
    subaccount: {
        id: 'subaccount',
        group: 'expert',
        audience: 'expert',
        titleKey: 'help.subaccount.title',
        shortKey: 'help.subaccount.short',
        longKey: 'help.subaccount.long',
        deepDiveKey: 'help.subaccount.deepDive',
        related: ['multiDevice', 'handover', 'deviceBinding', 'benutzerId'],
    },
    handover: {
        id: 'handover',
        group: 'expert',
        audience: 'expert',
        titleKey: 'help.handover.title',
        shortKey: 'help.handover.short',
        longKey: 'help.handover.long',
        deepDiveKey: 'help.handover.deepDive',
        related: ['multiDevice', 'subaccount', 'deviceBinding', 'offlineProtection'],
    },
    deviceBinding: {
        id: 'deviceBinding',
        group: 'expert',
        audience: 'expert',
        titleKey: 'help.deviceBinding.title',
        shortKey: 'help.deviceBinding.short',
        longKey: 'help.deviceBinding.long',
        deepDiveKey: 'help.deviceBinding.deepDive',
        related: ['profile', 'handover', 'multiDevice'],
    },
    l2Protection: {
        id: 'l2Protection',
        group: 'expert',
        audience: 'expert',
        titleKey: 'help.l2Protection.title',
        shortKey: 'help.l2Protection.short',
        longKey: 'help.l2Protection.long',
        deepDiveKey: 'help.l2Protection.deepDive',
        related: ['connection', 'offlineProtection', 'offlineRisk'],
    },
};
