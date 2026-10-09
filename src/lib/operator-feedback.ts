const fallbackMessages = {
  preflight: "The selected files could not be checked for duplicates.",
  cancelPreflight: "The duplicate check could not be cancelled.",
  clearQueue: "The queue could not be cleared. Your saved work remains available.",
  cancelUpload: "This upload could not be removed from the queue.",
  import: "The import could not finish. Any saved intake state remains local. Recovery requires the original files to remain available and unchanged.",
  videoPicker: "The video picker could not be opened. Your saved queue is unchanged.",
  queue: "This upload could not be queued. Its saved queue record and recovery evidence were retained. Keep the original file available and unchanged before retrying.",
  preflightPicker: "The file picker could not be opened.",
  duplicateDecision: "The duplicate decision could not be saved.",
  visibility: "The upload visibility could not be changed.",
  cleanupChoice: "The source cleanup choice could not be saved.",
  cleanupOriginal: "The original source could not be deleted.",
  library: "The YouTube library could not be refreshed. Your last complete local library was kept.",
  dedupe: "The YouTube library could not be synchronized.",
  ignoreDuplicate: "The duplicate review decision could not be saved.",
  reAudit: "Ignored duplicate matches could not be restored.",
} as const;

export type OperatorAction = keyof typeof fallbackMessages;

/** Failure-only feedback stays outside the safe-shell startup payload. */
export function operatorFailureMessage(error: unknown, action: OperatorAction): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallbackMessages[action];
}
