// src/components/ReceiveView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { VoucherStandardInfo, ReceiveSuccessPayload, VoucherDetails } from '../types';
import { Button } from './ui/Button';
import { useSession } from '../context/SessionContext';
import { ConfirmationModal } from './ui/ConfirmationModal'; // <--- NEU

interface ReceiveViewProps {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload) => void;
}

export function ReceiveView({ onBack, onReceiveSuccess }: ReceiveViewProps) {
    const { protectAction } = useSession();
    const [bundlePath, setBundlePath] = useState<string | null>(null);
    const [bundleName, setBundleName] = useState<string | null>(null);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [showConfirm, setShowConfirm] = useState(false);
    const [isProcessing, setIsProcessing] = useState(false);
    const [fileType, setFileType] = useState<'transfer' | 'humocoreq' | 'humocosig' | null>(null);
    const [bundlePassword, setBundlePassword] = useState("");

    useEffect(() => {
        async function fetchStandards() {
            try {
                logger.info("ReceiveView opened, fetching standards.");
                const standards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setVoucherStandards(standards);
            } catch (e) {
                const msg = `Failed to fetch voucher standards: ${e}`;
                logger.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            }
        }
        fetchStandards();
    }, []);

    const handleFileSelect = async () => {
        setFeedbackMsg('');
        try {
            const selectedPath = await open({
                multiple: false,
                filters: [
                    { name: 'Transfer Bundle', extensions: ['transfer'] },
                    { name: 'Signature Request', extensions: ['humocoreq'] },
                    { name: 'Signature Response', extensions: ['humocosig'] }
                ]
            });

            if (typeof selectedPath === 'string') {
                logger.info(`File selected via dialog: ${selectedPath}`);
                setBundlePath(selectedPath);
                const fileName = selectedPath.split(/[/\\]/).pop() || '';
                setBundleName(fileName);
                
                if (fileName.endsWith('.transfer')) {
                    setFileType('transfer');
                } else if (fileName.endsWith('.humocoreq')) {
                    setFileType('humocoreq');
                } else if (fileName.endsWith('.humocosig')) {
                    setFileType('humocosig');
                }
            }
        } catch (e) {
            const msg = `Error selecting file: ${e}`;
            logger.error(msg);
            setFeedbackMsg(msg);
        }
    };



    const clearSelection = () => {
        setBundlePath(null);
        setBundleName(null);
        setFileType(null);
        setDroppedFileContent(null);
    }

    // Drag and drop functionality
    const [isDragOver, setIsDragOver] = useState(false);
    const [droppedFileContent, setDroppedFileContent] = useState<number[] | null>(null);

    // Prevent default browser behavior for drag events to enable drop functionality
    useEffect(() => {
        // Add a more comprehensive drag event handler
        const preventGlobalDefaults = (e: Event) => {
            e.preventDefault();
            // Don't stop propagation so events can reach our component
        };

        const handleGlobalDrop = (e: Event) => {
            // Still need to prevent default to prevent browser default behavior
            e.preventDefault();
        };

        // Add capture-phase listeners to catch events early
        document.addEventListener("dragenter", preventGlobalDefaults, true);
        document.addEventListener("dragover", preventGlobalDefaults, true);
        document.addEventListener("dragleave", preventGlobalDefaults, true);
        document.addEventListener("drop", handleGlobalDrop, true);

        // Also add bubble-phase listeners
        document.addEventListener("dragenter", preventGlobalDefaults, false);
        document.addEventListener("dragover", preventGlobalDefaults, false);
        document.addEventListener("dragleave", preventGlobalDefaults, false);
        document.addEventListener("drop", handleGlobalDrop, false);

        // Cleanup
        return () => {
            document.removeEventListener("dragenter", preventGlobalDefaults, true);
            document.removeEventListener("dragover", preventGlobalDefaults, true);
            document.removeEventListener("dragleave", preventGlobalDefaults, true);
            document.removeEventListener("drop", handleGlobalDrop, true);

            document.removeEventListener("dragenter", preventGlobalDefaults, false);
            document.removeEventListener("dragover", preventGlobalDefaults, false);
            document.removeEventListener("dragleave", preventGlobalDefaults, false);
            document.removeEventListener("drop", handleGlobalDrop, false);
        };
    }, []);

    const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        if (!isDragOver) {
            setIsDragOver(true);
        }
    };

    const handleDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        // Only reset if we're actually leaving the element, not just moving between child elements
        setTimeout(() => {
            if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                setIsDragOver(false);
            }
        }, 0);
    };

    const handleDrop = async (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragOver(false);

        // Log data transfer info

        const files = e.dataTransfer.files;
        
        if (files.length > 0) {
            const file = files[0];
            logger.info(`Processing dropped file: ${file.name} (Size: ${file.size})`);
            
            if (file.name.endsWith('.transfer') || file.name.endsWith('.humocoreq') || file.name.endsWith('.humocosig')) {
                try {
                    const fileBuffer = await file.arrayBuffer();
                    const uint8Array = new Uint8Array(fileBuffer);
                    const bundleData = Array.from(uint8Array);
                    logger.info(`File data length: ${bundleData.length}`);
                    
                    setBundlePath(file.name);
                    setBundleName(file.name);
                    setDroppedFileContent(bundleData);
                    
                    if (file.name.endsWith('.transfer')) {
                        setFileType('transfer');
                    } else if (file.name.endsWith('.humocoreq')) {
                        setFileType('humocoreq');
                    } else if (file.name.endsWith('.humocosig')) {
                        setFileType('humocosig');
                    }
                    
                    logger.info(`Successfully set dropped file: ${file.name}`);
                } catch (error) {
                    const msg = `Error processing dropped file: ${error}`;
                    logger.error(msg);
                    setFeedbackMsg(msg);
                }
            } else {
                const msg = `Invalid file type. Please drop a '.transfer', '.humocoreq', or '.humocosig' file. Got: ${file.name}`;
                logger.warn(msg);
                setFeedbackMsg(msg);
            }
        } else {
            const msg = "No files found in drop event. This might be due to security restrictions or an unsupported file type.";
            logger.info(msg);
            setFeedbackMsg(msg);
        }
    };

    const handleProcessClick = (event: FormEvent) => {
        event.preventDefault();
        
        if ((!bundlePath) && !droppedFileContent) {
            setFeedbackMsg("Please select a file.");
            return;
        }

        if (!fileType) {
            setFeedbackMsg("Unable to determine file type.");
            return;
        }

        setFeedbackMsg('');
        setShowConfirm(true);
    };

    async function executeReceive() {
        setIsProcessing(true);
        logger.info(`Attempting to process file of type: ${fileType}`);
        try {
            let fileData: number[];
            if (droppedFileContent) {
                fileData = droppedFileContent;
            } else if (bundlePath) {
                const fileUint8Array = await readFile(bundlePath);
                fileData = Array.from(fileUint8Array);
            } else {
                throw new Error("No file provided");
            }

            if (fileType === 'transfer') {
                const standardDefinitionsToml: Record<string, string> = {};
                voucherStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        standardDefinitionsToml[uuid] = standard.content;
                    }
                });

                logger.info(`Calling receive_bundle with ${fileData.length} bytes`);

                const payload = await protectAction(async (password) => {
                    return await invoke<ReceiveSuccessPayload>("receive_bundle", {
                        bundleData: fileData,
                        standardDefinitionsToml,
                        password
                    });
                });

                if (!payload) return;
                logger.info("Bundle received and processed successfully.");
                onReceiveSuccess(payload);
            } else if (fileType === 'humocoreq') {
                const voucher = await invoke<VoucherDetails>("open_voucher_signing_request", {
                    containerBytes: fileData,
                    password: bundlePassword || null
                });
                logger.info("Signature request opened successfully.");
                (onReceiveSuccess as any)({
                    senderId: voucher.creator.id,
                    transferSummary: { summableAmounts: {}, countableItems: {} },
                    involvedVouchers: [],
                    voucherData: voucher
                });
            } else if (fileType === 'humocosig') {
                const standardDefinitionsToml: Record<string, string> = {};
                voucherStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        standardDefinitionsToml[uuid] = standard.content;
                    }
                });
                
                const standardTomlContent = Object.values(standardDefinitionsToml)[0];
                if (!standardTomlContent) {
                    throw new Error("No voucher standards available");
                }

                await protectAction(async (password) => {
                    await invoke("process_and_attach_signature", {
                        containerBytes: fileData,
                        standardTomlContent,
                        containerPassword: bundlePassword || null,
                        walletPassword: password
                    });
                });
                
                logger.info("Signature processed and attached successfully.");
                setFeedbackMsg("Signature successfully attached to voucher!");
                setTimeout(() => onBack(), 2000);
            }

        } catch (e) {
            const msg = `Failed during file processing. Error: ${e instanceof Error ? e.message : String(e)}`;
            logger.error(msg);
            if (e instanceof Error && e.stack) logger.error(e.stack);
            setFeedbackMsg(`Error: ${msg}`);
        } finally {
            setIsProcessing(false);
            setShowConfirm(false);
        }
    }

    return (
        <div className="flex flex-col h-full max-w-2xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center justify-between">
                    <h1 className="text-2xl font-bold text-theme-primary">Receive & Process</h1>
                    <Button variant="secondary" onClick={onBack}>Cancel</Button>
                </div>
                <p className="text-theme-light mt-1">Process transfers, signature requests, or responses you have received.</p>
            </header>

            <div className="flex-grow">
                <form onSubmit={handleProcessClick} className="space-y-6">
                    {feedbackMsg && <p className="text-center text-red-500">{feedbackMsg}</p>}

                    <div
                        id="bundle-drop-zone"
                        className={`border-2 border-dashed rounded-lg p-8 text-center cursor-pointer ${
                            isDragOver ? 'border-theme-primary bg-bg-input-focus' : 'border-theme-subtle'
                        }`}
                        onDragOver={handleDragOver}
                        onDragLeave={handleDragLeave}
                        onDrop={handleDrop}
                        style={{ minHeight: '200px', width: '100%' }}
                    >
                        {bundleName ? (
                            <div>
                                <p className="font-semibold text-theme-primary">{bundleName}</p>
                                <p className="text-sm text-theme-light mt-2">File is ready to be processed.</p>
                                <Button type="button" variant="outline" onClick={clearSelection}>Choose a different file</Button>
                            </div>
                        ) : (
                            <div>
                                <p className="text-theme-light mb-4">Drag & Drop your '.transfer', '.humocoreq', or '.humocosig' file here</p>
                                <p className="text-theme-light text-sm mb-4">or</p>
                                <Button type="button" onClick={handleFileSelect}>Select Bundle File</Button>
                            </div>
                        )}
                     </div>

                     {(bundlePath || droppedFileContent) && (
                         <Button size="lg" type="submit" className="w-full">
                             Process File
                         </Button>
                     )}

                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title={`Process ${fileType === 'transfer' ? 'Transfer Bundle' : fileType === 'humocoreq' ? 'Signature Request' : 'Signature Response'}`}
                description={
                    <p>
                        Do you want to process the file <strong>{bundleName}</strong>?<br/>
                        {fileType === 'transfer' && 'This will check for funds and add them to your wallet.'}
                        {fileType === 'humocoreq' && 'This will open a signature request for you to review and sign.'}
                        {fileType === 'humocosig' && 'This will attach the signature to the corresponding voucher in your wallet.'}
                        
                        {(fileType === 'humocoreq' || fileType === 'humocosig') && (
                            <div className="mt-4">
                                <label className="block text-sm font-medium text-theme-primary mb-1">
                                    Bundle Password (optional)
                                </label>
                                <input
                                    type="password"
                                    value={bundlePassword}
                                    onChange={(e) => setBundlePassword(e.target.value)}
                                    placeholder="Only if encrypted with password"
                                    className="w-full px-3 py-2 border border-theme-subtle rounded-md bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary text-sm"
                                />
                                <p className="text-[10px] text-theme-light mt-1">
                                    If the sender protected the bundle with a password, enter it here.
                                </p>
                            </div>
                        )}
                    </p>
                }
                confirmText="Yes, Process"
                onConfirm={executeReceive}
                onCancel={() => {
                    setShowConfirm(false);
                    setBundlePassword("");
                }}
                isProcessing={isProcessing}
            />
        </div>
    );
}