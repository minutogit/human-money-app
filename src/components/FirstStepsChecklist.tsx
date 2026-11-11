// src/components/FirstStepsChecklist.tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { 
    CheckCircle2, 
    Circle, 
    X, 
    HardDrive, 
    UserPlus, 
    Ticket, 
    Cloud,
    ArrowRight
} from "lucide-react";
import { Card } from "./ui/Card";
import { useNavigation } from "../context/NavigationContext";

export interface FirstStepsChecklistProps {
    hasBackup: boolean;
    hasContact: boolean;
    hasVoucher: boolean;
    hasSync: boolean;
    userId?: string;
    onDismiss?: () => void;
}

export const FirstStepsChecklist: React.FC<FirstStepsChecklistProps> = ({
    hasBackup,
    hasContact,
    hasVoucher,
    hasSync,
    userId,
    onDismiss,
}) => {
    const { t } = useTranslation();
    const { navigate } = useNavigation();

    const storageKey = userId ? `humoco_first_steps_dismissed_${userId}` : "humoco_first_steps_dismissed";

    const [isDismissed, setIsDismissed] = useState<boolean>(() => {
        try {
            return (
                localStorage.getItem(storageKey) === "true" ||
                localStorage.getItem("humoco_first_steps_dismissed") === "true"
            );
        } catch {
            return false;
        }
    });

    const allCompleted = hasBackup && hasContact && hasVoucher && hasSync;

    const handleDismiss = () => {
        setIsDismissed(true);
        try {
            localStorage.setItem(storageKey, "true");
            localStorage.setItem("humoco_first_steps_dismissed", "true");
        } catch {
            // ignore best-effort storage
        }
        onDismiss?.();
    };

    if (isDismissed || allCompleted) {
        return null;
    }

    const steps = [
        {
            id: "backup",
            title: t("dashboard.stepBackupTitle", "Backup einrichten"),
            desc: t("dashboard.stepBackupDesc", "Schütze dein Profil vor Datenverlust"),
            done: hasBackup,
            icon: HardDrive,
            onClick: () => navigate({ view: "settings", initialSection: "backup" }),
        },
        {
            id: "contact",
            title: t("dashboard.stepContactTitle", "Ersten Kontakt hinzufügen"),
            desc: t("dashboard.stepContactDesc", "Verbinde dich mit Menschen, denen du vertraust"),
            done: hasContact,
            icon: UserPlus,
            onClick: () => navigate({ view: "address_book" }),
        },
        {
            id: "voucher",
            title: t("dashboard.stepVoucherTitle", "Ersten Gutschein erstellen oder empfangen"),
            desc: t("dashboard.stepVoucherDesc", "Erstelle dein erstes Versprechen oder empfange eines"),
            done: hasVoucher,
            icon: Ticket,
            onClick: () => navigate({ view: "create_voucher" }),
        },
        {
            id: "sync",
            title: t("dashboard.stepSyncTitle", "Sync-Dienst verbinden (optional)"),
            desc: t("dashboard.stepSyncDesc", "Gleiche Daten automatisch mit Kontakten ab"),
            done: hasSync,
            icon: Cloud,
            onClick: () => navigate({ view: "settings", initialSection: "sync" }),
        },
    ];

    const completedCount = steps.filter((s) => s.done).length;

    return (
        <div data-testid="first-steps-checklist">
            <Card
                variant="glass"
                className="p-5 border border-theme-subtle/60 bg-gradient-to-br from-white/90 to-theme-subtle/10 dark:from-gray-900/90 dark:to-gray-800/40 shadow-sm"
            >
            <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/40 mb-3">
                <div className="flex items-center gap-2">
                    <h3 className="text-sm font-bold text-theme-primary">
                        {t("dashboard.firstStepsTitle", "Erste Schritte")}
                    </h3>
                    <span className="text-[11px] font-semibold text-theme-light bg-theme-subtle/30 px-2 py-0.5 rounded-full">
                        {t("dashboard.stepsProgress", "{{completed}} von {{total}} erledigt", {
                            completed: completedCount,
                            total: steps.length,
                        })}
                    </span>
                </div>
                <button
                    type="button"
                    onClick={handleDismiss}
                    aria-label={t("dashboard.dismissChecklist", "Checkliste ausblenden")}
                    title={t("dashboard.dismissChecklist", "Checkliste ausblenden")}
                    className="p-1 rounded-lg text-theme-light hover:text-theme-secondary hover:bg-bg-app transition-colors cursor-pointer"
                    data-testid="dismiss-checklist-button"
                >
                    <X size={16} />
                </button>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                {steps.map((step) => {
                    const Icon = step.icon;
                    return (
                        <div
                            key={step.id}
                            onClick={step.onClick}
                            role="button"
                            tabIndex={0}
                            onKeyDown={(e) => {
                                if (e.key === "Enter" || e.key === " ") {
                                    e.preventDefault();
                                    step.onClick();
                                }
                            }}
                            data-testid={`checklist-step-${step.id}`}
                            className={`p-3 rounded-xl border transition-all duration-150 flex items-center justify-between gap-3 text-left cursor-pointer group ${
                                step.done
                                    ? "bg-emerald-50/50 dark:bg-emerald-950/20 border-emerald-200/60 dark:border-emerald-900/40 opacity-75"
                                    : "bg-white/80 dark:bg-gray-800/70 border-theme-subtle/60 hover:border-theme-primary hover:shadow-xs"
                            }`}
                        >
                            <div className="flex items-center gap-3 min-w-0">
                                <div className="shrink-0">
                                    {step.done ? (
                                        <CheckCircle2 size={18} className="text-emerald-500" />
                                    ) : (
                                        <Circle size={18} className="text-theme-placeholder group-hover:text-theme-primary transition-colors" />
                                    )}
                                </div>
                                <div className="min-w-0">
                                    <div className="flex items-center gap-1.5">
                                        <Icon size={14} className="text-theme-light shrink-0" />
                                        <p
                                            className={`text-xs font-bold truncate ${
                                                step.done
                                                    ? "text-emerald-800 dark:text-emerald-200"
                                                    : "text-theme-primary group-hover:text-theme-primary"
                                            }`}
                                        >
                                            {step.title}
                                        </p>
                                    </div>
                                    <p className="text-[11px] text-theme-light truncate">
                                        {step.desc}
                                    </p>
                                </div>
                            </div>
                            <div className="shrink-0 text-theme-placeholder group-hover:text-theme-primary transition-colors">
                                <ArrowRight size={14} />
                            </div>
                        </div>
                    );
                })}
            </div>
        </Card>
        </div>
    );
};

export default FirstStepsChecklist;
