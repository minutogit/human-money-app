// src/utils/appNotices.ts
import { TFunction } from "i18next";
import { AppState, FrontendAppNotice } from "../types";

export interface DescribedAppNotice {
    title: string;
    subtitle: string;
    target: AppState;
    category: "payment" | "inquiry" | "market" | "trust";
}

export function describeAppNotice(notice: FrontendAppNotice, t: TFunction): DescribedAppNotice {
    switch (notice.kind) {
        case "payment":
            return {
                category: "payment",
                title: t("common.noticePaymentTitle", "Zahlung über Sync empfangen"),
                subtitle: t("common.noticePaymentDesc", "{{count}} neue Zahlung(en) empfangen", {
                    count: notice.importedPayments,
                }),
                target: { view: "wallet" },
            };

        case "inquiry": {
            const total = Math.max(1, notice.newInquiries + notice.newInquiryMessages);
            const title =
                notice.newInquiries > 0 && notice.newInquiryMessages === 0
                    ? t("common.noticeNewInquiryTitle", "Neue Marktplatz-Anfrage")
                    : t("common.noticeInquiryMessageTitle", "Neue Nachricht zur Anfrage");
            const subtitle = notice.counterpartyName
                ? t("common.noticeInquiryWithPerson", "Von {{name}}", {
                      name: notice.counterpartyName,
                  })
                : t("common.noticeInquiryDesc", "{{count}} neue Anfrage-Aktivität(en)", {
                      count: total,
                  });
            return {
                category: "inquiry",
                title,
                subtitle,
                target: { view: "marketplace", initialTab: "mine" },
            };
        }

        case "market": {
            const parts: string[] = [];
            if (notice.newAds > 0) {
                parts.push(
                    t("common.noticeNewAdsPart", "{{count}} neue Inserate", {
                        count: notice.newAds,
                    })
                );
            }
            if (notice.importedCards > 0) {
                parts.push(
                    t("common.noticeImportedCardsPart", "{{count}} Visitenkarte(n)", {
                        count: notice.importedCards,
                    })
                );
            }
            return {
                category: "market",
                title: t("common.noticeMarketTitle", "Marktplatz aktualisiert"),
                subtitle: parts.join(" · ") || t("common.noticeMarketFallback", "Neue Einträge verfügbar"),
                target: { view: "marketplace", initialTab: notice.newAds > 0 ? "board" : "people" },
            };
        }

        default: {
            const parts: string[] = [];
            if (notice.newWotHandshakes > 0) {
                parts.push(
                    t("common.noticeWotPart", "{{count}} Vertrauensanfrage(n)", {
                        count: notice.newWotHandshakes,
                    })
                );
            }
            if (notice.newSignRequests > 0) {
                parts.push(
                    t("common.noticeSignReqPart", "{{count}} Bürgschaftsanfrage(n)", {
                        count: notice.newSignRequests,
                    })
                );
            }
            if (notice.importedSignatures > 0) {
                parts.push(
                    t("common.noticeSignaturesPart", "{{count}} Unterschrift(en)", {
                        count: notice.importedSignatures,
                    })
                );
            }
            return {
                category: "trust",
                title: t("common.noticeTrustTitle", "Netzwerk & Unterschriften"),
                subtitle: parts.join(" · ") || t("common.noticeTrustFallback", "Neue Netzwerk-Aktivität"),
                target: notice.newWotHandshakes > 0 ? { view: "address_book" } : { view: "logged_in" },
            };
        }
    }
}

export function formatLastSyncLabel(lastSyncAtSecs: number | null | undefined, t: TFunction): string {
    if (!lastSyncAtSecs) {
        return t("common.lastUpdatedNever", "Noch nie aktualisiert");
    }
    const nowSecs = Math.floor(Date.now() / 1000);
    const diffSecs = Math.max(0, nowSecs - lastSyncAtSecs);
    if (diffSecs < 60) {
        return t("common.lastUpdatedJustNow", "Gerade eben");
    }
    if (diffSecs < 3600) {
        const mins = Math.floor(diffSecs / 60);
        return t("common.lastUpdatedMinutes", "vor {{count}} Min.", { count: mins });
    }
    const d = new Date(lastSyncAtSecs * 1000);
    if (diffSecs < 86_400) {
        return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }
    return d.toLocaleDateString([], { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
}
