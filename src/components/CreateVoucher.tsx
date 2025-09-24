// src/components/CreateVoucher.tsx
import { useState, useEffect, FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { error, info } from "@tauri-apps/plugin-log";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { NewVoucherData, VoucherStandardInfo } from "../types";

interface CreateVoucherProps {
    onVoucherCreated: () => void;
    onCancel: () => void;
}

export function CreateVoucher({ onVoucherCreated, onCancel }: CreateVoucherProps) {
    const [standards, setStandards] = useState<VoucherStandardInfo[]>([]);
    const [selectedStandardId, setSelectedStandardId] = useState<string>("");
    const [amount, setAmount] = useState("");
    const [firstName, setFirstName] = useState("");
    const [lastName, setLastName] = useState("");
    const [validityValue, setValidityValue] = useState<number>(3);
    const [validityUnit, setValidityUnit] = useState<"Y" | "M" | "D">("Y");
    const [password, setPassword] = useState("");

    const [isLoading, setIsLoading] = useState(true);
    const [feedback, setFeedback] = useState<{ type: 'error' | 'success', msg: string } | null>(null);

    useEffect(() => {
        // Log when component is displayed
        logger.info("CreateVoucher component displayed");
        
        async function fetchStandards() {
            try {
                info("CreateVoucher: Fetching voucher standards.");
                const fetchedStandards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setStandards(fetchedStandards);
                if (fetchedStandards.length > 0) {
                    setSelectedStandardId(fetchedStandards[0].id);
                }
                info(`CreateVoucher: Loaded ${fetchedStandards.length} standards.`);
            } catch (e) {
                const msg = `Failed to fetch voucher standards: ${e}`;
                error(msg);
                setFeedback({ type: 'error', msg });
            } finally {
                setIsLoading(false);
            }
        }
        fetchStandards();
    }, []);

    async function handleSubmit(event: FormEvent) {
        event.preventDefault();
        setFeedback(null);
        setIsLoading(true);

        const selectedStandard = standards.find(s => s.id === selectedStandardId);
        if (!selectedStandard) {
            setFeedback({ type: 'error', msg: "Selected standard not found." });
            setIsLoading(false);
            return;
        }

        // Construct the ISO 8601 duration string from the user-friendly inputs
        const validityDuration = validityValue > 0 ? `P${validityValue}${validityUnit}` : null;

        // For the prototype (minuto_v1), the unit is hardcoded.
        const voucherData: NewVoucherData = {
            nominal_value: {
                amount,
                unit: "Minuto",
            },
            creator: {
                first_name: firstName,
                last_name: lastName,
            },
            // Send null if the field is empty, so the library can use its default.
            validity_duration: validityDuration,
        };

        try {
            info("CreateVoucher: Invoking create_new_voucher command.");
            await invoke("create_new_voucher", {
                standardTomlContent: selectedStandard.content,
                data: voucherData,
                password,
            });
            setFeedback({ type: 'success', msg: "Voucher created successfully! Redirecting..." });
            setTimeout(onVoucherCreated, 2000);
        } catch (e) {
            const msg = `Failed to create voucher: ${e}`;
            error(msg);
            setFeedback({ type: 'error', msg });
            setIsLoading(false);
        }
    }

    return (
        <div className="mx-auto max-w-lg">
            <h1 className="text-3xl font-bold text-center mb-6 text-theme-primary">Create New Voucher</h1>
            <div className="rounded-lg border border-theme-subtle bg-card p-6 shadow-lg">
                <form onSubmit={handleSubmit} className="space-y-4">
                    <div>
                        <label htmlFor="standard" className="block text-sm font-medium text-theme-secondary mb-1">Voucher Type</label>
                        <select
                            id="standard"
                            value={selectedStandardId}
                            onChange={(e) => setSelectedStandardId(e.target.value)}
                            disabled={isLoading || standards.length === 0}
                            className="block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                        >
                            {standards.map(s => <option key={s.id} value={s.id}>{s.id}</option>)}
                        </select>
                        {standards.length === 0 && !isLoading && <p className="text-sm text-theme-light mt-1">No voucher standards found.</p>}
                    </div>

                    <div>
                        <label htmlFor="amount" className="block text-sm font-medium text-theme-secondary mb-1">Amount (e.g., 60)</label>
                        <input
                            id="amount"
                            type="number"
                            value={amount}
                            onChange={(e) => setAmount(e.target.value)}
                            required
                            disabled={isLoading}
                            className="block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                        />
                    </div>

                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <div>
                            <label htmlFor="firstName" className="block text-sm font-medium text-theme-secondary mb-1">Creator First Name</label>
                            <input
                                id="firstName"
                                type="text"
                                value={firstName}
                                onChange={(e) => setFirstName(e.target.value)}
                                required
                                disabled={isLoading}
                                className="block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                            />
                        </div>
                        <div>
                            <label htmlFor="lastName" className="block text-sm font-medium text-theme-secondary mb-1">Creator Last Name</label>
                            <input
                                id="lastName"
                                type="text"
                                value={lastName}
                                onChange={(e) => setLastName(e.target.value)}
                                required
                                disabled={isLoading}
                                className="block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                            />
                        </div>
                    </div>

                    <div>
                        <label htmlFor="validity" className="block text-sm font-medium text-theme-secondary mb-1">Validity
                            (e.g., P3Y for 3 years)</label>
                        <div>
                            <label htmlFor="validityValue"
                                   className="block text-sm font-medium text-theme-secondary mb-1">Validity</label>
                            <div className="flex items-center gap-2">
                                <input
                                    id="validityValue"
                                    type="number"
                                    value={validityValue}
                                    onChange={(e) => setValidityValue(parseInt(e.target.value, 10) || 0)}
                                    disabled={isLoading}
                                    className="block w-2/3 rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                                />
                                <select
                                    id="validityUnit"
                                    value={validityUnit}
                                    onChange={(e) => setValidityUnit(e.target.value as "Y" | "M" | "D")}
                                    disabled={isLoading}
                                    className="block w-1/3 rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                                >
                                    <option value="Y">Years</option>
                                    <option value="M">Months</option>
                                    <option value="D">Days</option>
                                </select>
                            </div>
                        </div>
                    </div>

                    <div>
                        <label htmlFor="password" className="block text-sm font-medium text-theme-secondary mb-1">Your
                            Wallet Password</label>
                        <input
                            id="password"
                            type="password"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            required
                            disabled={isLoading}
                            autoComplete="current-password"
                            className="block w-full rounded-md border-theme-subtle bg-bg-app px-3 py-2 text-theme-secondary shadow-sm focus:border-theme-accent focus:ring focus:ring-theme-accent focus:ring-opacity-50"
                        />
                    </div>

                    {feedback && (
                        <p className={`text-center text-sm font-semibold ${feedback.type === 'error' ? 'text-theme-error' : 'text-theme-success'}`}>
                            {feedback.msg}
                        </p>
                    )}

                    <div className="flex justify-end gap-4 pt-2">
                        <Button type="button" onClick={onCancel} variant="secondary" disabled={isLoading}>
                            Cancel
                        </Button>
                        <Button type="submit" disabled={isLoading}>
                            {isLoading ? "Creating..." : "Create Voucher"}
                        </Button>
                    </div>
                </form>
            </div>
        </div>
    );
}