// src/components/transfer/sendTransferData.ts
import { voucherService } from '../../services/voucherService';
import { transferService } from '../../services/transferService';
import { settingsService } from '../../services/settingsService';
import { profileService } from '../../services/profileService';
import { standardsService } from '../../services/standardsService';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { parseStandardInfo } from '../../hooks/useParsedStandards';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';
import { formatProfileDisplayName } from './sendTransferHelpers';
import type {
    AppSettings,
    AssetClassSummary,
    Contact,
    VoucherStandardDefinition,
    VoucherStandardInfo,
    VoucherSummary,
} from '../../types';

export interface LoadedSendData {
    hasPublishedCard: boolean;
    defaultSenderName: string;
    standardsDict: Record<string, VoucherStandardDefinition>;
    standardIdToUuidMap: Map<string, string>;
    uuidToPrecisionMap: Map<string, number>;
    availableVouchers: VoucherSummary[];
    voucherStandards: VoucherStandardInfo[];
    ownUserId: string;
    appSettings: AppSettings;
    contacts: Contact[];
    activeAssetClasses: AssetClassSummary[];
}

export async function loadSendInitialData(): Promise<LoadedSendData> {
    const [
        summaries,
        userId,
        profile,
        standards,
        settings,
        fetchedContacts,
        activeClasses,
        marketProfile,
    ] = await Promise.all([
        voucherService.getSummaries(),
        profileService.getUserId(),
        profileService.getProfile().catch(() => null),
        standardsService.getStandards(),
        settingsService.getSettings(),
        contactService.getContacts(),
        transferService.getActiveAssetClasses(),
        marketService.getMyMarketProfile().catch(() => null),
    ]);

    const activeVouchers = summaries.filter((v: VoucherSummary) => {
        const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
        const amt = parseFloat(String(v.currentAmount ?? '0').replace(',', '.'));
        return statusName.toLowerCase() === 'active' && !Number.isNaN(amt) && amt > 0;
    });

    const newIdMap = new Map<string, string>();
    const newPrecisionMap = new Map<string, number>();
    const parsedDict: Record<string, VoucherStandardDefinition> = {};

    for (const s of standards) {
        const parsed = parseStandardInfo(s);
        if (parsed.uuid) {
            newIdMap.set(s.id, parsed.uuid);
            newPrecisionMap.set(parsed.uuid, parsed.decimalPlaces);
        }
    }

    await Promise.all(
        standards.map(async (s) => {
            try {
                const def = await standardsService.parseStandard(s.content);
                if (def) {
                    const parsed = parseStandardInfo(s);
                    parsedDict[def.immutable?.identity?.uuid || parsed.uuid || s.id] = def;
                }
            } catch (err) {
                logger.warn(
                    `Failed to parse standard definition ${s.id}: ${stringifyError(err)}`,
                );
            }
        }),
    );

    const availableVouchers = activeVouchers.map((v: VoucherSummary) => {
        const allowPartial =
            parsedDict[v.voucherStandardUuid]?.immutable?.features?.allowPartialTransfers;
        return typeof allowPartial === 'boolean'
            ? { ...v, allowPartialTransfers: allowPartial }
            : v;
    });

    return {
        hasPublishedCard: Boolean(marketProfile?.published),
        defaultSenderName:
            formatProfileDisplayName(marketProfile?.profile) || formatProfileDisplayName(profile),
        standardsDict: parsedDict,
        standardIdToUuidMap: newIdMap,
        uuidToPrecisionMap: newPrecisionMap,
        availableVouchers,
        voucherStandards: standards,
        ownUserId: userId,
        appSettings: settings,
        contacts: fetchedContacts,
        activeAssetClasses: activeClasses || [],
    };
}
