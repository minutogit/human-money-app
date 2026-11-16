export interface RecoveryQuestionOption {
    id: number;
    key: string;
}

export const RECOVERY_QUESTION_CATALOG: RecoveryQuestionOption[] = [
    { id: 1, key: 'backup.recoveryQuestion1' },
    { id: 2, key: 'backup.recoveryQuestion2' },
    { id: 3, key: 'backup.recoveryQuestion3' },
    { id: 4, key: 'backup.recoveryQuestion4' },
    { id: 5, key: 'backup.recoveryQuestion5' },
    { id: 6, key: 'backup.recoveryQuestion6' },
    { id: 7, key: 'backup.recoveryQuestion7' },
    { id: 8, key: 'backup.recoveryQuestion8' },
    { id: 9, key: 'backup.recoveryQuestion9' },
    { id: 10, key: 'backup.recoveryQuestion10' },
    { id: 11, key: 'backup.recoveryQuestion11' },
    { id: 12, key: 'backup.recoveryQuestion12' },
];

export const RECOVERY_QUESTION_MAP: Record<number, string> = Object.fromEntries(
    RECOVERY_QUESTION_CATALOG.map((q) => [q.id, q.key])
);
