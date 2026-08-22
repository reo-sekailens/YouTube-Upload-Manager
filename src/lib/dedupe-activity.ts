export type DedupeActivityState = "running" | "success" | "error";
export type DedupeProgressPhase = "idle" | "syncing" | "rebuilding" | "complete" | "error";

export type DedupeActivityEntry = {
  id: number;
  state: DedupeActivityState;
  message: string;
};

export const maxDedupeActivityEntries = 8;
export const dedupeProgressStepCount = 3;

export function dedupeProgressStep(phase: DedupeProgressPhase): number {
  switch (phase) {
    case "syncing": return 1;
    case "rebuilding": return 2;
    case "complete": return dedupeProgressStepCount;
    case "idle":
    case "error": return 0;
  }
}

export function dedupeProgressLabel(phase: DedupeProgressPhase): string {
  switch (phase) {
    case "syncing": return "Step 1 of 3: synchronizing the uploaded-video inventory from YouTube.";
    case "rebuilding": return "Step 2 of 3: rebuilding title candidates locally.";
    case "complete": return "Step 3 of 3: duplicate review is ready.";
    case "error": return "Dedupe stopped before all steps completed.";
    case "idle": return "Dedupe has not started.";
  }
}

export function recordDedupeActivity(
  entries: DedupeActivityEntry[],
  entry: DedupeActivityEntry,
): DedupeActivityEntry[] {
  return [...entries, entry].slice(-maxDedupeActivityEntries);
}
