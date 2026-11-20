// scripts/tour-catalog.mjs
// Canonical tour steps and Tauri mock initialization script for HuMoCo UI Tour & Audit.

import fs from 'node:fs';

const pkg = JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
export const APP_VERSION = pkg.version;

export const MOCK_USER_DID = 'did:key:z6MkuSLmPtN3yDiCuPebk3Y7Y4YtK8m5eALICE';
export const MOCK_RECIPIENT_DID = 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doKBOB';
export const MOCK_CAROL_DID = 'did:key:z6MktfW8w21oXgD9z5mNuZ2gR5c7bW5vE9k1mP4jL8yQCAROL';
export const MOCK_OFFENDER_DID = 'did:key:z6MkeEvilSpenderDeterministic1234567890OFFENDER';

export const MOCK_VOUCHER_ID = 'v_sample_minuto_01';
export const MOCK_PROOF_ID = 'proof_sample_sstv4_01';

export const TOUR_STEPS = [
    // --- Chapter 1: Onboarding & Authentifizierung ---
    {
        id: '1.1',
        chapter: 1,
        title: 'Login & Profilauswahl',
        file: 'src/components/auth/Login.tsx',
        description: 'Auswahl eines lokalen Profils, Passworteingabe und Schnellzugriff auf Wiederherstellung.',
        auth: false,
        appState: { view: 'needs_login' }
    },
    {
        id: '1.2',
        chapter: 1,
        title: 'Neues Profil erstellen',
        file: 'src/components/auth/CreateNewProfile.tsx',
        description: 'Erstellung einer neuen Identität mit Mnemonic BIP-39 Seed-Phrase, Passwort und Profildaten.',
        auth: false,
        appState: { view: 'needs_profile' }
    },
    {
        id: '1.3',
        chapter: 1,
        title: 'Profil öffnen oder wiederherstellen (Wörter / Notfall-Blatt)',
        file: 'src/components/auth/RestoreBackupView.tsx',
        description: 'Öffnen eines bestehenden Profils oder Unterkontos über 12/24 Wiederherstellungswörter, Notfall-Blatt oder Sync-Dienst.',
        auth: false,
        appState: { view: 'restore_backup', initialMethod: 'words_or_sheet' }
    },
    {
        id: '1.4',
        chapter: 1,
        title: 'Bestehendes Profil öffnen / wiederherstellen (Übersicht)',
        file: 'src/components/auth/RestoreBackupView.tsx',
        description: '3 gleichberechtigte Wege: Vertrauenskontakte, Wiederherstellungswörter/Notfall-Blatt oder Sicherungsdatei/-ordner.',
        auth: false,
        appState: { view: 'restore_backup' }
    },
    {
        id: '1.5',
        chapter: 1,
        title: 'Passwort vergessen / Wallet-Recovery',
        file: 'src/components/auth/WalletRecovery.tsx',
        description: 'Notfall-Rettung eines Profils mit Mnemonic bei vergessenem Passwort.',
        auth: false,
        appState: { view: 'needs_recovery' }
    },

    // --- Chapter 2: Kernfunktionen & Gutschein-Transaktionen ---
    {
        id: '2.1',
        chapter: 2,
        title: 'Dashboard & Startübersicht',
        file: 'src/components/wallet/Dashboard.tsx',
        description: 'Aggregierte Guthaben nach Standards, Schnellaktionen und Status-Badges.',
        auth: true,
        appState: { view: 'logged_in' }
    },
    {
        id: '2.2',
        chapter: 2,
        title: 'Wallet & Gutscheinliste',
        file: 'src/components/wallet/WalletView.tsx',
        description: 'Vollständige Liste aller Gutscheine mit Filterung, Status und Standards.',
        auth: true,
        appState: { view: 'wallet' }
    },
    {
        id: '2.3',
        chapter: 2,
        title: 'Gutschein-Details & Timeline',
        file: 'src/components/wallet/VoucherDetailsView.tsx',
        description: 'Audit-Trail, kryptographische ProofChain, Bürgenliste und L2 Quorum Status.',
        auth: true,
        appState: { view: 'voucher_details', voucherId: MOCK_VOUCHER_ID }
    },
    {
        id: '2.4',
        chapter: 2,
        title: 'Neuen Gutschein ausstellen',
        file: 'src/components/wallet/CreateVoucher.tsx',
        description: 'Erstellung eines werthaltigen L1 Gutscheins basierend auf Standard, Wert und Frist.',
        auth: true,
        appState: { view: 'create_voucher' }
    },
    {
        id: '2.5',
        chapter: 2,
        title: 'Gutschein senden',
        file: 'src/components/transfer/SendView.tsx',
        description: 'Multi-Transfer, Empfängerauswahl aus Adressbuch und SST V4 Trap-Vorbereitung.',
        auth: true,
        appState: { view: 'send_vouchers' }
    },
    {
        id: '2.6',
        chapter: 2,
        title: 'Gutschein-Versand erfolgreich',
        file: 'src/components/transfer/TransferSuccessView.tsx',
        description: 'Erfolgsmeldung nach Transfer mit L2 Shard-Quorum Bestätigung und Export-Option.',
        auth: true,
        appState: {
            view: 'transfer_success',
            bundleData: [104, 117, 109, 111, 99, 111],
            recipientId: MOCK_RECIPIENT_DID,
            summary: '60 Minuto an Bob übertragen',
            bundleId: 'bundle_tour_tx_001',
            depositedTo: 1
        }
    },
    {
        id: '2.7',
        chapter: 2,
        title: 'Gutschein / Bundle empfangen',
        file: 'src/components/transfer/ReceiveView.tsx',
        description: 'Import von Gutschein-Dateien, QR-Codes oder BLE/NFC Payloads.',
        auth: true,
        appState: { view: 'receive_bundle' }
    },
    {
        id: '2.8',
        chapter: 2,
        title: 'Empfang erfolgreich',
        file: 'src/components/transfer/ReceiveSuccessView.tsx',
        description: 'Bestätigung des importierten Gutscheins und Verbuchung im L1 Speicher.',
        auth: true,
        appState: {
            view: 'receive_success',
            payload: {
                senderId: MOCK_CAROL_DID,
                senderProfileName: 'Carol',
                notes: '1 Stunde Gartenarbeit Minuto',
                transferSummary: {
                    summableAmounts: { 'Minuto': '60.00' },
                    countableItems: {}
                },
                involvedVouchers: ['inst_sample_01'],
                involvedVouchersDetails: [
                    {
                        localInstanceId: 'inst_sample_01',
                        voucherId: MOCK_VOUCHER_ID,
                        standardName: 'Minuto',
                        amountTransferred: '60.00'
                    }
                ],
                l2Quorum: {
                    status: 'quorum_reached',
                    confirmedNodes: 16,
                    totalNodes: 20
                }
            }
        }
    },
    {
        id: '2.9',
        chapter: 2,
        title: 'Bürgschafts- / Signaturanfrage',
        file: 'src/components/wallet/SignRequestView.tsx',
        description: 'Prüfung und Unterzeichnung einer Bürgschaftsanfrage für einen Mitmenschen.',
        auth: true,
        appState: {
            view: 'sign_request',
            voucherData: {
                localInstanceId: 'inst_sign_req_01',
                status: 'incomplete',
                displayCurrency: 'Minuto',
                displayStandardName: 'Minuto Standard',
                isTestVoucher: false,
                voucher: {
                    voucherStandard: {
                        name: 'Minuto',
                        uuid: 'standard_minuto_de',
                        standardDefinitionHash: 'hash_minuto_std_def',
                        template: {
                            description: '1 Stunde Lebenszeit für Arbeit oder Hilfe',
                            allowPartialTransfers: true
                        }
                    },
                    voucherId: 'v_minuto_sign_pending_01',
                    voucherNonce: 'nonce_req_123',
                    creationDate: '2026-11-10T08:00:00Z',
                    validUntil: '2027-11-10T08:00:00Z',
                    nonRedeemableTestVoucher: false,
                    nominalValue: { amount: '60', unit: 'Minuto' },
                    creator: {
                        id: MOCK_RECIPIENT_DID,
                        firstName: 'Bob',
                        lastName: 'Baumeister',
                        city: 'Freiburg'
                    },
                    signatures: [],
                    transactions: []
                }
            },
            senderDid: MOCK_RECIPIENT_DID
        }
    },
    {
        id: '2.11',
        chapter: 2,
        title: 'Transaktionshistorie',
        file: 'src/components/transfer/TransactionHistoryView.tsx',
        description: 'Detaillierter Zahlungsverlauf mit Filter, CSV-Export und integriertem Ereignis-Protokoll im Expertenmodus.',
        auth: true,
        appState: { view: 'transaction_history' }
    },

    // --- Chapter 3: Vertrauensnetzwerk & Marktplatz ---
    {
        id: '3.1',
        chapter: 3,
        title: 'Kontakte & Vertrauen (Übersicht)',
        file: 'src/components/contacts/AddressBook.tsx',
        description: 'Subjektives Vertrauensnetzwerk mit 4 Stufen (Als Mensch bestätigt, Bekannt, Freund, Enger Vertrauter), Reichweite und Visitenkarten-Teilen.',
        auth: true,
        appState: { view: 'address_book' }
    },
    {
        id: '3.2',
        chapter: 3,
        title: 'Kontakt hinzufügen (Einstieg)',
        file: 'src/components/contacts/AddContactModal.tsx',
        description: 'Zentrales Einstiegsmodal mit 4 Wegen: QR scannen, Zwischenablage, Datei (.hmcard) und Benutzer-ID.',
        auth: true,
        appState: { view: 'address_book', initialOpenAddModal: true }
    },
    {
        id: '3.3',
        chapter: 3,
        title: 'Marktplatz: Pinnwand',
        file: 'src/components/marketplace/BoardTab.tsx',
        description: 'Aktuelle Inserate (Angebote und Gesuche) aus dem persönlichen Vertrauensumfeld mit Restlaufzeit und Filter.',
        auth: true,
        appState: { view: 'marketplace', initialTab: 'board' }
    },
    {
        id: '3.4',
        chapter: 3,
        title: 'Marktplatz: Menschen',
        file: 'src/components/marketplace/PeopleTab.tsx',
        description: 'Verzeichnis der Menschen im Umfeld mit Fähigkeiten, Vertrauens-Badge und Ortsangaben.',
        auth: true,
        appState: { view: 'marketplace', initialTab: 'people' }
    },
    {
        id: '3.5',
        chapter: 3,
        title: 'Marktplatz: Meins & Inserate',
        file: 'src/components/marketplace/MineTab.tsx',
        description: 'Eigene Visitenkarte, aktive und abgelaufene Inserate, Statusanzeige und eingegangene Anfragen.',
        auth: true,
        appState: { view: 'marketplace', initialTab: 'mine' }
    },
    {
        id: '3.6',
        chapter: 3,
        title: 'Visitenkarten- & Profil-Editor',
        file: 'src/components/marketplace/ProfileCardEditor.tsx',
        description: 'Einheitlicher Editor für Profildaten, Fähigkeiten, selektive Freigabe pro Feld (Alle, Kontakte, Niemand) und Reichweite.',
        auth: true,
        appState: { view: 'marketplace', initialTab: 'mine', initialOpenEditor: true }
    },

    // --- Chapter 4: Einstellungen & Systemkonfiguration ---
    {
        id: '4.1',
        chapter: 4,
        title: 'Einstellungen: Profil & Benutzer-ID',
        file: 'src/components/settings/ProfileTab.tsx',
        description: 'Eigene Benutzer-ID (kopierbar), Sprachauswahl und Verweis-Karte zum Bearbeiten der Visitenkarte im Marktplatz.',
        auth: true,
        appState: { view: 'settings', initialSection: 'profile' },
        settingsTab: 'profile'
    },
    {
        id: '4.2',
        chapter: 4,
        title: 'Einstellungen: Sicherheit & Backup',
        file: 'src/components/settings/SecurityBackupTab.tsx',
        description: 'Vollverschlüsselte automatische Datensicherung mit Statusbanner, Primäraktion zur Sofortsicherung und Privatsphäre.',
        auth: true,
        appState: { view: 'settings', initialSection: 'security' },
        settingsTab: 'security'
    },
    {
        id: '4.3',
        chapter: 4,
        title: 'Einstellungen: Sync-Dienst',
        file: 'src/components/settings/ConnectionTab.tsx',
        description: 'Verbundene Sync-Dienste, Einladungslinks und Kanal-Schalter für den verschlüsselten Datenabgleich.',
        auth: true,
        appState: { view: 'settings', initialSection: 'connection' },
        settingsTab: 'connection'
    },
    {
        id: '4.4',
        chapter: 4,
        title: 'Einstellungen: Erweitert',
        file: 'src/components/settings/AdvancedTab.tsx',
        description: 'L2 Collision Lock Shards, CEL-Gutschein-Standards und Entwickleroptionen.',
        auth: true,
        appState: { view: 'settings', initialSection: 'advanced' },
        settingsTab: 'advanced'
    },

    // --- Chapter 5: Sicherheit, Konflikte & Hilfe ---
    {
        id: '5.1',
        chapter: 5,
        title: 'Konflikt-Übersicht',
        file: 'src/components/conflicts/ConflictListView.tsx',
        description: 'Erkannte Double-Spend Versuche und Shared-Signature Trap (SST V4) Meldungen.',
        auth: true,
        appState: { view: 'conflict_list' }
    },
    {
        id: '5.2',
        chapter: 5,
        title: 'Konflikt-Details',
        file: 'src/components/conflicts/ConflictDetailsView.tsx',
        description: 'Kryptographischer Beweis des Double-Spend Betrugs mit Offender-Enttarnung.',
        auth: true,
        appState: { view: 'conflict_details', proofId: MOCK_PROOF_ID }
    },
    {
        id: '5.3',
        chapter: 5,
        title: 'Fork-Lock Sicherheits-Sperre',
        file: 'src/components/auth/ForkLockOverlay.tsx',
        description: 'Sofortige Profil-Sperre bei gleichzeitiger Fork-/Klon-Nutzung auf mehreren Geräten.',
        auth: true,
        appState: { view: 'logged_in' },
        sessionOverrides: { isForkLocked: true }
    },
    {
        id: '5.4',
        chapter: 5,
        title: 'Hilfe-Center',
        file: 'src/components/support/HelpCenterView.tsx',
        description: 'Zentrales Hilfe-Center mit thematischen Erklärungen, Suchfunktion, Kategorien und Verwandten Themen.',
        auth: false,
        appState: { view: 'help_center' }
    },
    {
        id: '5.5',
        chapter: 5,
        title: 'Support & Spenden',
        file: 'src/components/support/SupportView.tsx',
        description: 'Hilfskanäle, Projektbeitrag, Feedback und Spendenmöglichkeiten für die Entwickler.',
        auth: true,
        appState: { view: 'support' }
    },
    {
        id: '5.6',
        chapter: 5,
        title: 'Bug-Report & Diagnose',
        file: 'src/components/support/BugReportView.tsx',
        description: 'Systemdiagnose, anonymisierte Fehler-Logs und GitHub-Issue Vorbereitung.',
        auth: true,
        appState: { view: 'bug_report' }
    }
];

export function getTauriMockInitScript(initialScenario = 'normal') {
    return `
(function() {
    if (window.__TAURI_MOCK_INITIALIZED__) return;
    window.__TAURI_MOCK_INITIALIZED__ = true;
    window.__HUMOCO_SCENARIO__ = "${initialScenario}"; // 'normal' | 'empty' | 'error'

    console.info('[HuMoCo Tour] Tauri IPC Mock initialized (scenario: ' + window.__HUMOCO_SCENARIO__ + ')');

    const MOCK_USER_DID = "${MOCK_USER_DID}";
    const MOCK_RECIPIENT_DID = "${MOCK_RECIPIENT_DID}";
    const MOCK_CAROL_DID = "${MOCK_CAROL_DID}";
    const MOCK_OFFENDER_DID = "${MOCK_OFFENDER_DID}";
    const MOCK_VOUCHER_ID = "${MOCK_VOUCHER_ID}";
    const MOCK_PROOF_ID = "${MOCK_PROOF_ID}";

    const MOCK_PROFILE = {
        id: MOCK_USER_DID,
        firstName: "Alice",
        lastName: "Musterfrau",
        organization: "HuMoCo Community Freiburg",
        community: "Dreisamtal",
        email: "alice@humoco.example.org",
        phone: "+49 761 123456",
        coordinates: "47.9959,7.8522",
        serviceOffer: "Gartenarbeit, Permakultur-Beratung, Holzhandwerk",
        needs: "Fahrradreparatur, Bio-Äpfel",
        address: {
            street: "Musterstraße",
            houseNumber: "42",
            zipCode: "79098",
            city: "Freiburg",
            country: "Deutschland",
            fullAddress: "Musterstraße 42, 79098 Freiburg, Deutschland"
        }
    };

    const MOCK_SETTINGS = {
        autoLockMinutes: 15,
        sessionTimeoutSeconds: 300,
        devModeUnlocked: true,
        privacyDefault: "ask",
        preferredLanguage: "de",
        shareMinLevel: 1,
        mediaAutoLoadMaxHops: 2,
        marketPollWifiOnly: false,
        marketSyncEnabled: true,
        wotShareViaSync: true,
        postAcceptFromContacts: true,
        syncBackupEnabled: true,
        autoBackupEnabled: true,
        uiMode: "simple",
        customServers: [
            { url: "https://l2-node1.humoco.org", name: "Frankfurt Shard #1", isActive: true },
            { url: "https://l2-node2.humoco.org", name: "Zürich Shard #2", isActive: true },
            { url: "https://l2-node3.humoco.org", name: "Wien Shard #3", isActive: true }
        ],
        regionalMeshSources: [],
        marketSelectedSyncs: [
            {
                writeUrl: "https://sync-freiburg.humoco.org/write",
                readUrl: "https://sync-freiburg.humoco.org/read",
                dropPub: "drop_pub_freiburg_01"
            }
        ],
        publicSyncs: []
    };

    const MOCK_WOT_SUMMARY = {
        ratedContactsCount: 2,
        verifiedHumansCount: 0,
        closeConfidantsCount: 1,
        importedPeerListsCount: 2,
        ownListVersion: 1,
        ownListMaxHops: 2,
        providerFiltersCount: 0
    };

    function detectMockReachPreset() {
        const wot = MOCK_WOT_SUMMARY.ownListMaxHops;
        const share = MOCK_SETTINGS.shareMinLevel;
        const card = MOCK_MY_MARKET_PROFILE.maxHops;
        const media = MOCK_SETTINGS.mediaAutoLoadMaxHops;
        let preset = 'custom';
        if (wot === 1 && share === 2 && card === 1 && media === 1) preset = 'near';
        else if (wot === 2 && share === 1 && card === 2 && media === 2) preset = 'medium';
        else if (wot === 3 && share === 1 && card === 3 && media === 2) preset = 'far';
        return {
            preset: preset,
            ownListMaxHops: wot,
            cardMaxHops: card,
            shareMinLevel: share,
            mediaAutoLoadMaxHops: media
        };
    }

    const MOCK_STANDARDS = [
        {
            id: "minuto_de",
            displayName: "Minuto (1 Std. Lebenszeit)",
            content: "name = \\"Minuto\\"\\nunit = \\"Minuto\\"\\nabbreviation = \\"MIN\\"\\nnominal_minutes = 60"
        },
        {
            id: "freetaler_v1",
            displayName: "FreeTaler (Gemeinwohl)",
            content: "name = \\"FreeTaler\\"\\nunit = \\"FreeTaler\\"\\nabbreviation = \\"FT\\""
        }
    ];

    const MOCK_VOUCHERS = [
        {
            localInstanceId: "inst_sample_01",
            status: "active",
            validUntil: "2027-12-31T23:59:59Z",
            creatorId: MOCK_USER_DID,
            description: "1 Stunde Gartenarbeit oder Handwerkshilfe",
            currentAmount: "60",
            unit: "Minuto",
            rawStandardName: "Minuto",
            voucherStandardUuid: "std_minuto_de",
            transactionCount: 2,
            signaturesCount: 2,
            hasCollateral: true,
            creatorFirstName: "Alice",
            creatorLastName: "Musterfrau",
            creatorCoordinates: "47.9959,7.8522",
            isTestVoucher: false,
            displayCurrency: "Minuto",
            displayStandardName: "Minuto Standard",
            allowPartialTransfers: true,
            trust: {
                score: 0.85,
                trustPercent: 85,
                isOwnVoucher: true,
                blockedByOffender: false,
                shortestHops: 0,
                additionalPathsCount: 0,
                viaGuarantor: false,
                bestPathNames: ["Alice Musterfrau"],
            }
        },
        {
            localInstanceId: "inst_sample_02",
            status: "active",
            validUntil: "2027-06-30T18:00:00Z",
            creatorId: MOCK_RECIPIENT_DID,
            description: "Bio-Gemüsekiste & Honig aus eigener Imkerei",
            currentAmount: "120",
            unit: "FreeTaler",
            rawStandardName: "FreeTaler",
            voucherStandardUuid: "std_freetaler_v1",
            transactionCount: 1,
            signaturesCount: 3,
            hasCollateral: false,
            creatorFirstName: "Bob",
            creatorLastName: "Baumeister",
            isTestVoucher: false,
            displayCurrency: "FreeTaler",
            displayStandardName: "FreeTaler Standard",
            allowPartialTransfers: true,
            trust: {
                score: 0.60,
                trustPercent: 60,
                isOwnVoucher: false,
                blockedByOffender: false,
                shortestHops: 1,
                primaryBridgeName: "Bob Baumeister",
                additionalPathsCount: 1,
                viaGuarantor: false,
                bestPathNames: ["Bob Baumeister"],
            }
        }
    ];

    const MOCK_VOUCHER_DETAILS = {
        localInstanceId: "inst_sample_01",
        status: "active",
        displayCurrency: "Minuto",
        displayStandardName: "Minuto Standard",
        isTestVoucher: false,
        trust: {
            score: 0.85,
            trustPercent: 85,
            isOwnVoucher: false,
            blockedByOffender: false,
            shortestHops: 1,
            primaryBridgeName: "Bob Baumeister",
            additionalPathsCount: 1,
            viaGuarantor: false,
            bestPathNames: ["Bob Baumeister"],
        },
        voucher: {
            voucherStandard: {
                name: "Minuto",
                uuid: "std_minuto_de",
                standardDefinitionHash: "b3_minuto_hash_standard_verification_ok",
                template: {
                    description: "1 Stunde Lebenszeit für Arbeit oder Hilfe",
                    allowPartialTransfers: true
                }
            },
            voucherId: MOCK_VOUCHER_ID,
            voucherNonce: "nonce_sec_89374928174",
            creationDate: "2026-01-15T10:00:00Z",
            validUntil: "2027-12-31T23:59:59Z",
            nonRedeemableTestVoucher: false,
            nominalValue: { amount: "60", unit: "Minuto" },
            collateral: {
                amount: "60",
                unit: "Minuto",
                type: "personalGuarantee",
                redeemCondition: "Vor-Ort Einlösung im Raum Freiburg"
            },
            creator: MOCK_PROFILE,
            signatures: [
                {
                    voucherId: MOCK_VOUCHER_ID,
                    signatureId: "sig_buerge_carol_01",
                    signerId: MOCK_CAROL_DID,
                    signature: "ed25519_sig_carol_trust_valid_sample",
                    signatureTime: "2026-01-15T12:00:00Z",
                    role: "guarantor",
                    details: {
                        id: MOCK_CAROL_DID,
                        firstName: "Carol",
                        lastName: "Kranz",
                        city: "Freiburg"
                    }
                }
            ],
            transactions: [
                {
                    tId: "tx_01_init",
                    tType: "creation",
                    tTime: "2026-01-15T10:00:00Z",
                    senderId: MOCK_USER_DID,
                    recipientId: MOCK_USER_DID,
                    amount: "60"
                }
            ]
        }
    };

    const MOCK_CONTACTS = [
        {
            did: MOCK_RECIPIENT_DID,
            profile: {
                id: MOCK_RECIPIENT_DID,
                firstName: "Bob",
                lastName: "Baumeister",
                organization: "Bau & Handwerk",
                city: "Freiburg",
                serviceOffer: "Holzarbeiten, Dämmung, Reparaturen",
                needs: "Minuto-Gutscheine, Buchhaltung"
            },
            tags: ["Handwerk", "Freiburg", "Freund"],
            addedAt: "2026-02-01T14:00:00Z",
            trustLevel: 2,
            desiredHops: 2,
            hasPeerList: true
        },
        {
            did: MOCK_CAROL_DID,
            profile: {
                id: MOCK_CAROL_DID,
                firstName: "Carol",
                lastName: "Kranz",
                organization: "Bio-Imkerei",
                city: "Waldkirch",
                serviceOffer: "Regionaler Bio-Honig, Wachskerzen",
                needs: "Gartenhilfe"
            },
            tags: ["Bio", "Imkerei", "Bürgin"],
            addedAt: "2026-03-10T09:30:00Z",
            trustLevel: 3,
            desiredHops: 3,
            hasPeerList: true
        }
    ];

    const MOCK_MY_MARKET_PROFILE = {
        profile: MOCK_PROFILE,
        visibility: {
            name: "everyone",
            organization: "everyone",
            community: "everyone",
            city: "everyone",
            url: "everyone",
            picture: "everyone",
            street: "contacts",
            email: "contacts",
            phone: "contacts",
            coordinates: "nobody",
            gender: "nobody"
        },
        skills: [
            {
                title: "Gartenarbeit & Permakultur",
                text: "Beratung und Hilfe bei Naturgärten und Obstbaumschnitt im Dreisamtal.",
                pricing: "60 Minuto / Std."
            },
            {
                title: "Holzhandwerk",
                text: "Kleine Reparaturen von Holzmöbeln und Schnitzereien.",
                pricing: "Auf Anfrage"
            }
        ],
        ads: [
            {
                id: "ad_tour_sample_01",
                kind: "offer",
                title: "Bio-Saatgut & Kompost abzugeben",
                text: "Alte Tomatensorten und frischer Schwarzerde-Kompost aus dem eigenen Garten.",
                pricing: "10 Minuto",
                publishedAt: Math.floor(Date.now() / 1000) - 86400 * 10,
                expiresAt: Math.floor(Date.now() / 1000) + 86400 * 80,
                allowInquiries: true,
                status: "active",
                daysLeft: 80
            }
        ],
        maxHops: 2,
        allowInquiries: true,
        published: true,
        shareUri: "humoco://card/tour_mock_payload",
        fileWire: "mock_card_wire_data",
        qrSvg: "<svg viewBox='0 0 100 100'><rect width='100' height='100' fill='#0284c7'/></svg>",
        qrIsPartial: false
    };

    const MOCK_MARKET_PERSONS = [
        {
            did: MOCK_RECIPIENT_DID,
            profile: MOCK_CONTACTS[0].profile,
            skills: [
                {
                    title: "Holzreparaturen & Möbelbau",
                    text: "Ich helfe gerne bei Reparaturen von Holzmöbeln oder beim Terrassenbau.",
                    pricing: "60 Minuto / Std.",
                    origin: "card"
                }
            ],
            activeAds: [
                {
                    id: "ad_bob_01",
                    kind: "offer",
                    title: "Holzreparaturen & Möbelbau",
                    text: "Ich helfe gerne bei Reparaturen von Holzmöbeln oder beim Terrassenbau.",
                    pricing: "60 Minuto / Std.",
                    publishedAt: Math.floor(Date.now() / 1000) - 86400 * 5,
                    expiresAt: Math.floor(Date.now() / 1000) + 86400 * 85,
                    allowInquiries: true
                }
            ],
            adsPending: 0,
            allowInquiries: true,
            sources: [{ kind: "card", hops: 1 }],
            asOf: Math.floor(Date.now() / 1000) - 3600,
            standards: [{ name: "Minuto", uuid: "std_minuto_de" }],
            trust: "trusted",
            canForward: true,
            cardSyncs: []
        },
        {
            did: MOCK_CAROL_DID,
            profile: MOCK_CONTACTS[1].profile,
            skills: [
                {
                    title: "Regionaler Bio-Honig",
                    text: "Kalt geschleuderter Frühlingsblütenhonig aus dem Schwarzwald.",
                    pricing: "10 FreeTaler / Glas",
                    origin: "card"
                }
            ],
            activeAds: [
                {
                    id: "ad_carol_01",
                    kind: "offer",
                    title: "Regionaler Bio-Honig (500g Glas)",
                    text: "Kalt geschleuderter Frühlingsblütenhonig aus dem Schwarzwald.",
                    pricing: "10 FreeTaler / Glas",
                    publishedAt: Math.floor(Date.now() / 1000) - 86400 * 2,
                    expiresAt: Math.floor(Date.now() / 1000) + 86400 * 88,
                    allowInquiries: true
                }
            ],
            adsPending: 0,
            allowInquiries: true,
            sources: [{ kind: "card", hops: 1 }],
            asOf: Math.floor(Date.now() / 1000) - 7200,
            standards: [{ name: "FreeTaler", uuid: "std_freetaler_v1" }],
            trust: "trusted",
            canForward: true,
            cardSyncs: []
        }
    ];

    const MOCK_BOARD_ITEMS = [
        {
            ownerDid: MOCK_RECIPIENT_DID,
            ownerName: "Bob Baumeister",
            trust: "trusted",
            ad: {
                id: "ad_bob_01",
                kind: "offer",
                title: "Holzreparaturen & Möbelbau",
                text: "Ich helfe gerne bei Reparaturen von Holzmöbeln oder beim Terrassenbau.",
                pricing: "60 Minuto / Std.",
                publishedAt: Math.floor(Date.now() / 1000) - 86400 * 5,
                expiresAt: Math.floor(Date.now() / 1000) + 86400 * 85,
                allowInquiries: true
            },
            daysLeft: 85
        },
        {
            ownerDid: MOCK_CAROL_DID,
            ownerName: "Carol Kranz",
            trust: "trusted",
            ad: {
                id: "ad_carol_01",
                kind: "offer",
                title: "Regionaler Bio-Honig (500g Glas)",
                text: "Kalt geschleuderter Frühlingsblütenhonig aus dem Schwarzwald.",
                pricing: "10 FreeTaler / Glas",
                publishedAt: Math.floor(Date.now() / 1000) - 86400 * 2,
                expiresAt: Math.floor(Date.now() / 1000) + 86400 * 88,
                allowInquiries: true
            },
            daysLeft: 88
        }
    ];

    const MOCK_MARKET_SNAPSHOT = {
        board: MOCK_BOARD_ITEMS,
        people: MOCK_MARKET_PERSONS,
        now: Math.floor(Date.now() / 1000)
    };

    const MOCK_CONFLICT_PROOF = {
        proofId: MOCK_PROOF_ID,
        offenderId: MOCK_OFFENDER_DID,
        forkPointPrevHash: "blake3_prev_tx_hash_098234710928347",
        reportTimestamp: "2026-11-09T16:20:00Z",
        isResolved: false,
        hasL2Verdict: true,
        localOverride: false,
        conflictRole: "victim",
        affectedVoucherName: "Minuto Gutschein #892",
        voucherStandardUuid: "std_minuto_de",
        isTestVoucher: false,
        isCryptographicallyAttributed: true,
        conflictingTransactions: [
            {
                tId: "tx_double_spend_a",
                tType: "transfer",
                tTime: "2026-11-09T16:15:00Z",
                senderId: MOCK_OFFENDER_DID,
                recipientId: MOCK_USER_DID,
                amount: "60"
            },
            {
                tId: "tx_double_spend_b",
                tType: "transfer",
                tTime: "2026-11-09T16:15:10Z",
                senderId: MOCK_OFFENDER_DID,
                recipientId: MOCK_RECIPIENT_DID,
                amount: "60"
            }
        ],
        deletableAt: "2027-12-31T23:59:59Z",
        reporterId: MOCK_USER_DID,
        reporterSignature: "ed25519_reporter_signature_valid"
    };

    const handlers = {
        // --- Auth & Profiles ---
        "list_profiles": () => {
            if (window.__HUMOCO_SCENARIO__ === 'empty') return [];
            return [
                {
                    folderName: "alice_wallet",
                    profileName: "Alice Musterfrau",
                    userId: MOCK_USER_DID,
                    createdAt: "2026-01-15T10:00:00Z",
                    isPasswordProtected: true
                }
            ];
        },
        "get_user_profile": () => {
            if (window.__HUMOCO_SCENARIO__ === 'empty') return null;
            if (window.__HUMOCO_TOUR__) {
                const s = window.__HUMOCO_TOUR__.getState();
                if (s.isAuthenticated === false) {
                    throw new Error("No active profile session");
                }
            }
            return MOCK_PROFILE;
        },
        "get_user_id": () => MOCK_USER_DID,
        "get_local_instance_id": () => "inst_alice_local_dev_node",
        "get_session_status": () => {
            let isUnlocked = true;
            let hasCachedSession = true;
            if (window.__HUMOCO_SCENARIO__ === 'empty') {
                isUnlocked = false;
                hasCachedSession = false;
            } else if (window.__HUMOCO_TOUR__) {
                const s = window.__HUMOCO_TOUR__.getState();
                isUnlocked = s.isAuthenticated ?? true;
                hasCachedSession = s.hasCachedSession ?? isUnlocked;
            }
            return {
                isUnlocked,
                hasCachedSession,
                accessMode: isUnlocked ? 'full' : 'locked',
                baseAccessMode: isUnlocked ? 'full' : 'locked',
            };
        },
        "login": () => null,
        "logout": () => null,
        "unlock_session": () => ({
            isUnlocked: true,
            hasCachedSession: true,
            accessMode: 'full',
            baseAccessMode: 'full',
        }),
        "unlock_session_with_pin": () => ({
            isUnlocked: true,
            hasCachedSession: true,
            accessMode: 'dailyPay',
            baseAccessMode: 'dailyPay',
        }),
        "lock_session": () => null,
        "refresh_session_activity": () => null,
        "update_user_profile": () => null,
        "create_profile": () => null,
        "delete_profile": () => null,
        "generate_mnemonic": () => "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        "get_bip39_wordlist": () => ["abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract"],
        "validate_mnemonic": () => null,
        "recover_wallet_and_set_new_password": () => null,
        "check_reputation": () => "clean",
        "handover_to_this_device": () => null,

        // --- Settings ---
        "get_settings": () => MOCK_SETTINGS,
        "get_app_settings": () => MOCK_SETTINGS,
        "save_settings": (args) => {
            if (args && args.settings) Object.assign(MOCK_SETTINGS, args.settings);
            return null;
        },
        "save_app_settings": (args) => {
            if (args && args.settings) Object.assign(MOCK_SETTINGS, args.settings);
            return null;
        },
        "get_reach_preset": () => detectMockReachPreset(),
        "apply_reach_preset": (args) => {
            const p = args && args.preset;
            if (p === 'near') {
                MOCK_WOT_SUMMARY.ownListMaxHops = 1;
                MOCK_SETTINGS.shareMinLevel = 2;
                MOCK_MY_MARKET_PROFILE.maxHops = 1;
                MOCK_SETTINGS.mediaAutoLoadMaxHops = 1;
            } else if (p === 'medium') {
                MOCK_WOT_SUMMARY.ownListMaxHops = 2;
                MOCK_SETTINGS.shareMinLevel = 1;
                MOCK_MY_MARKET_PROFILE.maxHops = 2;
                MOCK_SETTINGS.mediaAutoLoadMaxHops = 2;
            } else if (p === 'far') {
                MOCK_WOT_SUMMARY.ownListMaxHops = 3;
                MOCK_SETTINGS.shareMinLevel = 1;
                MOCK_MY_MARKET_PROFILE.maxHops = 3;
                MOCK_SETTINGS.mediaAutoLoadMaxHops = 2;
            } else {
                throw new Error('Invalid reach preset: ' + p);
            }
            return detectMockReachPreset();
        },
        "get_wot_summary": () => MOCK_WOT_SUMMARY,
        "set_wot_max_hops": (args) => {
            if (args && typeof args.maxHops === 'number') {
                MOCK_WOT_SUMMARY.ownListMaxHops = Math.min(3, Math.max(1, args.maxHops));
            }
            return MOCK_WOT_SUMMARY;
        },
        "benchmark_l2_servers": () => [
            { url: "https://l2-node1.humoco.org", latencyMs: 14, reachable: true },
            { url: "https://l2-node2.humoco.org", latencyMs: 22, reachable: true },
            { url: "https://l2-node3.humoco.org", latencyMs: 28, reachable: true }
        ],
        "refresh_regional_mesh_views": () => true,
        "check_voucher_l2_status": () => ({ status: "unspent", archived: false }),

        // --- Vouchers & Standards ---
        "get_vouchers": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : MOCK_VOUCHERS,
        "get_voucher_list": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : MOCK_VOUCHERS,
        "get_voucher_details": () => MOCK_VOUCHER_DETAILS,
        "get_voucher_standards": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : MOCK_STANDARDS,
        "parse_standard_toml": () => ({
            immutable: {
                identity: { uuid: "std_minuto_de", name: "Minuto", abbreviation: "MIN" },
                blueprint: { unit: "Minuto", primaryRedemptionType: "time", collateralType: "personalGuarantee" },
                features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 0, privacyMode: "flexible", allowedTTypes: ["creation", "transfer"] },
                issuance: { validityDurationRange: ["1y", "3y"], issuanceMinimumValidityDuration: "1m", additionalSignaturesRange: [0, 5], allowedSignatureRoles: ["guarantor"] },
                customRules: {}
            },
            mutable: {
                metadata: { issuerName: "Minuto Deutschland e.V.", keywords: ["zeit", "hilfe"] },
                appConfig: { defaultValidityDuration: "1y" },
                i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} }
            }
        }),
        "evaluate_signature_suitability": () => ({
            isAllowedRole: true,
            fatalConflicts: [],
            resolvedRules: ["Standard rule verified"],
            gentleHints: ["Empfohlene Bürgenanzahl erreicht"]
        }),
        "export_voucher_standard": () => [1, 2, 3, 4],
        "inspect_voucher_standard_file": () => ({}),
        "install_voucher_standard": () => null,

        // --- Transactions & Activities ---
        "get_transactions": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : [
            {
                tId: "tx_transfer_001",
                tType: "transferSent",
                tTime: "2026-11-08T14:30:00Z",
                senderId: MOCK_USER_DID,
                recipientId: MOCK_RECIPIENT_DID,
                amount: "60",
                displayCurrency: "Minuto"
            },
            {
                tId: "tx_transfer_002",
                tType: "transferReceived",
                tTime: "2026-11-07T11:15:00Z",
                senderId: MOCK_CAROL_DID,
                recipientId: MOCK_USER_DID,
                amount: "10",
                displayCurrency: "FreeTaler"
            }
        ],
        "get_transaction_history": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : [
            {
                id: "tx_transfer_001",
                bundleId: "bundle_tour_001",
                direction: "sent",
                senderId: MOCK_USER_DID,
                recipientId: MOCK_RECIPIENT_DID,
                timestamp: "2026-11-08T14:30:00Z",
                summableAmounts: { "Minuto": "60.00" },
                countableItems: {},
                involvedVouchers: ["inst_sample_01"],
                notes: "Gartenhilfe im November"
            },
            {
                id: "tx_transfer_002",
                bundleId: "bundle_tour_002",
                direction: "received",
                senderId: MOCK_CAROL_DID,
                recipientId: MOCK_USER_DID,
                timestamp: "2026-11-07T11:15:00Z",
                summableAmounts: { "FreeTaler": "10.00" },
                countableItems: {},
                involvedVouchers: ["inst_sample_02"],
                notes: "Bio-Äpfel Lieferung"
            }
        ],
        "get_events": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : [
            {
                eventId: "ev_01",
                localInstanceId: "inst_sample_01",
                voucherId: MOCK_VOUCHER_ID,
                timestamp: "2026-11-08T14:30:00Z",
                eventType: "transferSent",
                bffData: {
                    displayCurrency: "Minuto",
                    amount: "60",
                    isTestVoucher: false,
                    counterpartyName: "Bob Baumeister"
                }
            }
        ],

        // --- Contacts & Marketplace ---
        "get_contacts": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : MOCK_CONTACTS,
        "save_contact": () => null,
        "delete_contact": () => null,
        "get_my_quick_contact": () => ({
            did: MOCK_USER_DID,
            name: "Alice Musterfrau",
            syncUrls: ["https://sync.humoco.org"],
            uri: 'humoco://contact?did=' + encodeURIComponent(MOCK_USER_DID) + '&name=Alice%20Musterfrau',
            qrSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#ffffff"/><rect x="12" y="12" width="24" height="24" fill="#0f172a"/></svg>'
        }),
        "encode_text_qr_svg": () => '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#ffffff"/><rect x="12" y="12" width="24" height="24" fill="#0f172a"/></svg>',
        "get_market_view": () => window.__HUMOCO_SCENARIO__ === 'empty' ? { board: [], people: [], now: Math.floor(Date.now() / 1000) } : MOCK_MARKET_SNAPSHOT,
        "get_my_market_profile": () => window.__HUMOCO_SCENARIO__ === 'empty' ? null : MOCK_MY_MARKET_PROFILE,
        "save_my_market_profile": (args) => {
            if (args?.request) {
                Object.assign(MOCK_MY_MARKET_PROFILE, args.request);
            }
            return MOCK_MY_MARKET_PROFILE;
        },
        "set_card_published": (args) => {
            MOCK_MY_MARKET_PROFILE.published = args?.published ?? true;
            return MOCK_MY_MARKET_PROFILE;
        },
        "upsert_ad": (args) => {
            if (args?.ad) {
                const existingIdx = MOCK_MY_MARKET_PROFILE.ads.findIndex(a => a.id === args.ad.id);
                const newAd = {
                    id: args.ad.id || ("ad_mock_" + Date.now()),
                    kind: args.ad.kind,
                    title: args.ad.title,
                    text: args.ad.text,
                    pricing: args.ad.pricing,
                    publishedAt: Math.floor(Date.now() / 1000),
                    expiresAt: Math.floor(Date.now() / 1000) + (args.ad.durationDays || 90) * 86400,
                    allowInquiries: args.ad.allowInquiries,
                    status: 'active',
                    daysLeft: args.ad.durationDays || 90
                };
                if (existingIdx >= 0) {
                    MOCK_MY_MARKET_PROFILE.ads[existingIdx] = newAd;
                } else {
                    MOCK_MY_MARKET_PROFILE.ads.push(newAd);
                }
            }
            return MOCK_MY_MARKET_PROFILE;
        },
        "renew_ad": (args) => {
            const ad = MOCK_MY_MARKET_PROFILE.ads.find(a => a.id === args?.id);
            if (ad) {
                ad.expiresAt = Math.floor(Date.now() / 1000) + (args.durationDays || 90) * 86400;
                ad.daysLeft = args.durationDays || 90;
                ad.status = 'active';
            }
            return MOCK_MY_MARKET_PROFILE;
        },
        "delete_ad": (args) => {
            MOCK_MY_MARKET_PROFILE.ads = MOCK_MY_MARKET_PROFILE.ads.filter(a => a.id !== args?.id);
            return MOCK_MY_MARKET_PROFILE;
        },
        "export_forward_card": () => ({ exportPayload: "humoco://card/alice_payload", targetDid: MOCK_USER_DID }),
        "import_contact_card": () => ({ did: MOCK_CAROL_DID, importedCount: 1 }),
        "preview_contact_trust_change": () => ({ scoreDelta: 0.25, newLevel: 3, bottlenecks: [] }),

        // --- Sync Services & Relays ---
        "get_sync_overview_status": () => ({
            configuredCount: 2,
            activeCount: 2,
            pendingItemsCount: 0,
            lastSyncTimestamp: "2026-11-10T08:15:00Z"
        }),
        "get_sync_admin_status": () => ({
            descriptor: {
                writeUrl: "https://sync-admin.humoco.org/write",
                readUrl: "https://sync-admin.humoco.org/read",
                dropPub: "drop_pub_admin_01",
                slotId: "slot_admin_01"
            },
            adminPubkey: "pubkey_admin_01",
            generation: 1,
            dataDir: "/home/user/.local/share/humoco/sync",
            maxMembers: 10,
            maxTotalStorageMb: 250,
            usedBytes: 131072,
            maxBytes: 262144000,
            softWarning: false,
            cardPolicy: {
                kind: "friendsAndContacts"
            },
            allow: [],
            totalMemberCount: 3
        }),
        "prepare_sync_setup": () => ({ token: "setup_token_demo" }),
        "save_sync_setup_zip": () => null,
        "finalize_sync_setup": () => handlers["get_sync_admin_status"](),
        "test_sync_connection": () => ({ success: true, latencyMs: 18, message: "Sync Relay OK" }),
        "update_sync_admin_config": () => handlers["get_sync_admin_status"](),
        "probe_public_sync": () => ({ name: "Public HuMoCo Relay Frankfurt", url: "https://sync.humoco.org", latencyMs: 18 }),
        "save_public_sync": () => ({ name: "Public HuMoCo Relay Frankfurt", url: "https://sync.humoco.org" }),
        "remove_public_sync": () => null,
        "delete_sync_admin": () => null,
        "discover_available_syncs": () => [
            {
                descriptor: {
                    writeUrl: "https://sync-freiburg.humoco.org/write",
                    readUrl: "https://sync-freiburg.humoco.org/read",
                    dropPub: "drop_pub_freiburg_01",
                    slotId: "slot_01"
                },
                isAuthorized: true,
                isSelected: true,
                sourceLabel: "Freiburg Mesh Relay"
            }
        ],
        "set_selected_upload_syncs": () => [],
        "sync_market_channels": () => ({ syncedProfilesCount: 2, newOffersCount: 1 }),

        // --- Backup & Integrity ---
        "get_backup_status": () => ({
            lastBackupTimestamp: "2026-11-10T07:00:00Z",
            autoBackupEnabled: true,
            storagePath: "/home/user/.local/share/humoco/backups",
            totalBlobs: 42,
            backupSizeKb: 256
        }),
        "probe_wallet_backup": () => ({
            sourceKind: "file",
            protectionMode: "password",
            supportsPasswordRestore: true,
            supportsMnemonicRestore: true,
            blobCount: 42,
            customStandardCount: 2
        }),
        "inspect_wallet_backup": () => ({
            profileName: "Alice Musterfrau",
            userId: MOCK_USER_DID,
            epoch: 4,
            txNonce: 18,
            sealTimestamp: "2026-11-10T07:00:00Z",
            blobCount: 42,
            customStandardCount: 2,
            protectionMode: "password",
            localCollision: null
        }),
        "restore_wallet_backup": () => ({
            folderName: "alice_wallet",
            profileName: "Alice Musterfrau",
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            restoredStandardsCount: 2,
            l2ReconciliationStatus: "reconciled"
        }),
        "reset_local_password_via_backup": () => null,
        "preview_mnemonic_identity": () => ({
            userId: MOCK_USER_DID,
            localCollision: null
        }),
        "stage_dropped_backup_bytes": () => "/tmp/staged_dropped.hmcbackup",
        "sync_backup": () => ({ uploadedTo: 1, authorizedTargets: 1, paddedBytes: 4096, errors: [] }),
        "probe_sync_backup": () => ({
            status: "found",
            stagedPath: "/tmp/staged_backup",
            matchedKeyKind: "stealth",
            sourceUrl: "https://sync.humoco.org",
            inspection: handlers["inspect_wallet_backup"](),
            userId: MOCK_USER_DID,
            localCollision: null,
            resolvedSyncTargets: [{
                writeUrl: "https://sync.humoco.org/sync.php",
                readUrl: "https://sync.humoco.org/data",
                dropPub: "drop_pub_01",
                registerUrl: null
            }],
            error: null
        }),
        "reconcile_restored_wallet_l2": () => ({
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            l2ReconciliationStatus: "reconciled",
            pendingRestoreL2Reconciliation: false
        }),
        "check_wallet_integrity": () => ({ isHealthy: true, issues: [] }),
        "get_double_spend_conflicts": () => window.__HUMOCO_SCENARIO__ === 'empty' ? [] : [
            {
                proofId: MOCK_PROOF_ID,
                offenderId: MOCK_OFFENDER_DID,
                forkPointPrevHash: "blake3_prev_tx_hash_098234710928347",
                reportTimestamp: "2026-11-09T16:20:00Z",
                isResolved: false,
                hasL2Verdict: true,
                localOverride: false,
                conflictRole: "victim",
                affectedVoucherName: "Minuto Gutschein #892",
                voucherStandardUuid: "std_minuto_de",
                isTestVoucher: false,
                isCryptographicallyAttributed: true
            }
        ],
        "get_proof_of_double_spend": () => ({
            proof: MOCK_CONFLICT_PROOF,
            localOverride: false,
            conflictRole: "victim"
        }),
        "set_conflict_local_override": () => null,
        "repair_wallet_integrity": () => null,
        "export_double_spend_proof": () => [1, 2, 3, 4],
        "import_double_spend_proof": () => MOCK_PROOF_ID,

        // --- Postbox & Messaging ---
        "get_post_inbox_summary": () => ({
            totalPending: 1,
            signingRequests: 1,
            signaturesToAttach: 0,
            wotHandshakes: 0,
            marketInquiries: 0,
            pendingSignRequests: [],
            pendingWotHandshakes: [],
            inquiries: [],
            unacknowledgedPayments: []
        }),
        "sync_post_channels": () => ({ fetchedItems: 1, processedItems: 1, errors: [] }),
        "send_signing_request_via_post": () => ({ success: true }),
        "reply_detached_signature_via_post": () => ({ success: true }),
        "accept_wot_handshake": () => ({ success: true }),
        "send_market_inquiry_via_post": () => ({ inquiryId: "inq_01", status: "open" }),
        "send_market_inquiry_message_via_post": () => ({ inquiryId: "inq_01", status: "confirmed" }),
        "mark_inquiry_read": () => null,
        "dismiss_pending_post_item": () => null,
        "get_app_notices": () => ({ lastSyncAt: null, notices: [] }),
        "mark_app_notices_read": () => ({ lastSyncAt: null, notices: [] }),
        "clear_app_notices": () => ({ lastSyncAt: null, notices: [] }),

        // --- Social & Emergency Recovery ---
        "export_profile_mnemonic": () => ({
            phrase: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            languageCode: 0,
            hasBip39Passphrase: false,
            kitUri: "humoco://kit?v=1&ent=00000000000000000000000000000000&lang=0",
            qrSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#eee"/></svg>',
            syncUrls: ["https://sync.humoco.org"],
            guardianNames: ["Bob", "Carol", "David"]
        }),
        "confirm_emergency_sheet_saved": () => ({
            confirmedAt: Math.floor(Date.now() / 1000),
            backupSyncResult: { uploadedTo: 1, authorizedTargets: 1, paddedBytes: 65536, errors: [] }
        }),
        "get_recovery_protection_status": () => ({
            hasVoucher: true,
            confirmedGuardiansCount: 2,
            requiredThreshold: 2,
            guardians: [
                {
                    userId: MOCK_RECIPIENT_DID,
                    displayName: "Bob Baumeister",
                    shareIndex: 1,
                    status: { status: "confirmed", ackedAt: Math.floor(Date.now() / 1000) - 3600 },
                    assignedAt: Math.floor(Date.now() / 1000) - 7200,
                    lastContactedAt: Math.floor(Date.now() / 1000) - 3600,
                    isStale90d: false,
                    updatedAt: Math.floor(Date.now() / 1000) - 3600
                },
                {
                    userId: MOCK_CAROL_DID,
                    displayName: "Carol Designer",
                    shareIndex: 2,
                    status: { status: "confirmed", ackedAt: Math.floor(Date.now() / 1000) - 3600 },
                    assignedAt: Math.floor(Date.now() / 1000) - 7200,
                    lastContactedAt: Math.floor(Date.now() / 1000) - 3600,
                    isStale90d: false,
                    updatedAt: Math.floor(Date.now() / 1000) - 3600
                },
                {
                    userId: "did:key:z6MkdDavidHelper789",
                    displayName: "David Helfer",
                    shareIndex: 3,
                    status: { status: "pending" },
                    assignedAt: Math.floor(Date.now() / 1000) - 7200,
                    lastContactedAt: null,
                    isStale90d: false,
                    updatedAt: Math.floor(Date.now() / 1000) - 7200
                }
            ],
            emergencySheetConfirmedAt: Math.floor(Date.now() / 1000) - 86400,
            hasIdentityRecovery: true,
            syncBackupCount: 2,
            hasSyncBackup: true,
            hasLocalBackup: true,
            hasVoucherBackup: true,
            guardianWards: []
        }),
        "setup_social_recovery_guardians": () => ({
            rotationEpoch: 1,
            thresholdM: 2,
            guardiansCount: 3,
            postSyncResult: { fetchedItems: 0, processedItems: 0, errors: [] },
            backupSyncResult: { uploadedTo: 1, authorizedTargets: 1, paddedBytes: 65536, errors: [] }
        }),
        "revoke_social_recovery": () => ({
            rotationEpoch: 2,
            revokedGuardiansCount: 3,
            postSyncResult: { fetchedItems: 0, processedItems: 0, errors: [] },
            backupSyncResult: null
        }),
        "inspect_emergency_kit_uri": () => ({
            phrase: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            wordCount: 12,
            language: "english",
            hasBip39Passphrase: false,
            userPrefix: null,
            syncUrls: ["https://sync.humoco.org"],
            syncDescriptorsJson: []
        }),
        "start_social_recovery_session": () => ({
            sasCode: "482-910",
            createdAt: Math.floor(Date.now() / 1000),
            nameHint: "Alice",
            requestUri: "humoco://recovery-req?v=1&pk=abcdef&ts=" + Math.floor(Date.now() / 1000) + "&name=Alice",
            sessionMode: "restore_new_device",
            targetFolderName: null,
            ownerDid: null,
            ownerDisplayName: null,
            rotationEpoch: null,
            thresholdM: null,
            collectedCount: 0,
            questionIds: null,
            hasEnoughShares: false,
            discoveredSyncUrls: []
        }),
        "get_active_recovery_session": () => null,
        "cancel_social_recovery_session": () => null,
        "import_guardian_recovery_reply": () => ({
            sasCode: "482-910",
            createdAt: Math.floor(Date.now() / 1000),
            nameHint: "Alice",
            requestUri: "humoco://recovery-req?v=1&pk=abcdef&ts=" + Math.floor(Date.now() / 1000) + "&name=Alice",
            sessionMode: "restore_new_device",
            targetFolderName: null,
            ownerDid: MOCK_USER_DID,
            ownerDisplayName: "Alice",
            rotationEpoch: 1,
            thresholdM: 2,
            collectedCount: 2,
            questionIds: [1, 2, 3],
            hasEnoughShares: true,
            discoveredSyncUrls: ["https://sync.humoco.org"]
        }),
        "inspect_guardian_recovery_request": () => ({
            sasCode: "482-910",
            requestedAt: Math.floor(Date.now() / 1000) - 60,
            isStale48h: false,
            nameHint: "Alice",
            wards: [
                {
                    ownerDid: MOCK_USER_DID,
                    ownerDisplayName: "Alice",
                    rotationEpoch: 1,
                    thresholdM: 2,
                    totalN: 3,
                    updatedAt: Math.floor(Date.now() / 1000) - 3600
                }
            ]
        }),
        "approve_guardian_recovery_request": () => ({
            replyUri: "humoco://recovery-reply?v=1&data=mockdata",
            replyQrSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#eee"/></svg>',
            sasCode: "482-910",
            ownerDisplayName: "Alice"
        }),
        "complete_social_recovery": () => ({
            folderName: "alice_wallet",
            profileName: "Alice",
            mode: "restore_new_device",
            outcome: "restored_from_sync",
            backupEpoch: 1,
            backupTxNonce: 2,
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            restoredStandardsCount: 1,
            l2ReconciliationStatus: "reconciled",
            pendingRestoreL2Reconciliation: false,
            localCollision: null,
            resolvedSyncTargets: []
        }),

        // --- Logs & Diagnostics ---
        "get_latest_logs": () => [
            "[INFO] HuMoCo Core v${APP_VERSION} initialized with strict L1 autonomy",
            "[INFO] SST V4 Trap verification: deterministic double-spend protection armed",
            "[INFO] Connected to 3/3 configured L2 Collision Lock nodes (14/20 mesh quorum ready)",
            "[DEBUG] Background PostBox poll: 1 pending item retrieved (latency 22ms)",
            "[DEBUG] Web of Trust SD-JWT sync completed: 2 contacts verified (depth=2)"
        ].join("\\n"),
        "log_to_backend": () => null,

        // --- Tauri Plugins ---
        "plugin:app|version": () => "${APP_VERSION}",
        "plugin:app|name": () => "HuMoCo Wallet",
        "plugin:app|tauri_version": () => "2.11.0",
        "plugin:log|log": () => null,
        "plugin:dialog|open": () => null,
        "plugin:dialog|save": () => null,
        "plugin:dialog|message": () => null,
        "plugin:clipboard-manager|write_text": () => null,
        "plugin:clipboard-manager|read_text": () => "",
        "plugin:opener|open_url": () => null,
        "plugin:opener|open_path": () => null
    };

    window.__TAURI_INTERNALS__ = {
        invoke: async function(cmd, args, options) {
            if (window.__HUMOCO_SCENARIO__ === 'error') {
                const errorCommands = ['get_vouchers', 'get_voucher_details', 'get_contacts', 'get_market_view', 'get_my_market_profile', 'save_settings'];
                if (errorCommands.includes(cmd)) {
                    const err = new Error("Simulated Backend Error in Scenario 'error'");
                    err.code = "error.mock.simulated";
                    err.details = { command: cmd };
                    throw err;
                }
            }

            if (Object.prototype.hasOwnProperty.call(handlers, cmd)) {
                try {
                    return await handlers[cmd](args, options);
                } catch (err) {
                    console.error('[Tauri Mock] Error executing ' + cmd + ':', err);
                    throw err;
                }
            }

            console.warn('[UNMOCKED IPC] ' + cmd, args);
            if (cmd.startsWith('get_') || cmd.startsWith('list_')) {
                return [];
            }
            return null;
        }
    };

    window.__HUMOCO_SET_SCENARIO__ = function(sc) {
        window.__HUMOCO_SCENARIO__ = sc;
        console.info('[HuMoCo Tour] Scenario switched to: ' + sc);
        if (window.__HUMOCO_TOUR__) {
            const cur = window.__HUMOCO_TOUR__.getState();
            window.__HUMOCO_TOUR__.setState({ ...cur });
        }
    };
})();
`;
}
