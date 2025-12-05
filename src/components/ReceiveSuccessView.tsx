// src/components/ReceiveSuccessView.tsx
import { useEffect } from "react";
import { ReceiveSuccessPayload } from "../types"; // <--- NEU
import { Button } from "./ui/Button";
import { logger } from '../utils/log';

interface ReceiveSuccessViewProps {
    payload: ReceiveSuccessPayload;
    onDone: () => void;
}

function formatAmount(amountStr: string): string {
    const num = parseFloat(amountStr);
    if (isNaN(num)) return amountStr;
    return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function ReceiveSuccessView({ payload, onDone }: ReceiveSuccessViewProps) {

    // Erzeuge einen detaillierten String aus der TransferSummary
    const summable = Object.entries(payload.transferSummary.summableAmounts)
        .map(([unit, total]) => `${formatAmount(total)} ${unit}`);

    const countable = Object.entries(payload.transferSummary.countableItems)
        // Einfaches Plural-Handling für die Anzeige
        .map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);

    const summaryString = [...summable, ...countable].join(', ') || 'No items received';


    useEffect(() => {
        logger.info(`Receive success screen shown. Received: ${summaryString} from ${payload.senderId} (${payload.senderProfileName ?? 'N/A'})`);
    }, [payload, summaryString]);

    return (
        <div className="flex flex-col h-full items-center justify-center text-center max-w-lg mx-auto">
            <div className="p-8 bg-bg-card-alternate rounded-lg border border-theme-subtle shadow-lg">
                <div className="mb-4">
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-16 w-16 text-theme-success mx-auto" viewBox="0 0 20 20" fill="currentColor">
                        <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                    </svg>
                </div>
                <h1 className="text-2xl font-bold text-theme-primary mb-2">Successfully Received!</h1>
                <p className="text-theme-light mb-6">The funds have been added to your wallet.</p>

                <div className="text-left bg-input-readonly rounded-lg p-4 space-y-3 border border-theme-subtle mb-8">
                    <div>
                        <p className="text-sm text-theme-light">Total Amount Received</p>
                        <p className="text-2xl font-bold text-theme-primary">{summaryString}</p>
                    </div>
                    <div>
                        <p className="text-sm text-theme-light">From</p>
                        {payload.senderProfileName ? (
                            <p className="text-lg font-semibold text-theme-primary">{payload.senderProfileName}</p>
                        ) : (
                            <p className="text-base font-mono text-theme-secondary break-all">{payload.senderId}</p>
                        )}
                        {payload.senderProfileName && (
                            <p className="text-xs font-mono text-theme-light break-all" title={payload.senderId}>({payload.senderId})</p>
                        )}
                    </div>
                    {payload.notes && (
                        <div>
                            <p className="text-sm text-theme-light">Notes / Verwendungszweck</p>
                            <p className="text-base text-theme-secondary whitespace-pre-wrap">{payload.notes}</p>
                        </div>
                    )}

                    {/* --- NEU: ANZEIGE DER DETAILS --- */}
                    {payload.involvedVouchersDetails && payload.involvedVouchersDetails.length > 0 && (
                         <div>
                            <p className="text-sm text-theme-light mt-3">Received into Vouchers</p>
                            <ul className="list-none space-y-1 mt-1 font-mono text-xs">
                                {payload.involvedVouchersDetails.map((detail, index) => (
                                    <li key={index} className="p-1.5 bg-black/10 rounded">
                                        <span className="font-semibold text-theme-primary">{detail.amount} {detail.unit}</span>
                                        <span className="text-theme-light"> ({detail.standard_name})</span>
                                    </li>
                                ))}
                            </ul>
                        </div>
                    )}
                    {/* --- ENDE --- */}
                </div>

                <Button size="lg" onClick={onDone} className="w-full">Back to Dashboard</Button>
            </div>
        </div>
    );
}