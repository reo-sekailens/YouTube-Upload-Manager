import { invoke, isTauri } from "@tauri-apps/api/core";

export type PerceptualSource = {
  itemId: string; title: string; fileName: string;
  state: "pending" | "analyzed" | "retained" | "unavailable" | "failed";
  detail: string; durationSeconds?: number | null; sampleCount: number; informativeSamples: number;
};
export type PerceptualSample = { position: number; leftTimeSeconds: number; rightTimeSeconds: number; distance: number; informative: boolean };
export type PerceptualCandidate = {
  candidateId: string; leftItemId: string; rightItemId: string; leftTitle: string; rightTitle: string;
  leftDurationSeconds: number; rightDurationSeconds: number; durationDifferencePercent: number;
  alignedSamples: number; totalSamples: number; medianDistance: number; maxDistance: number;
  evidence: "possible" | "limited"; warnings: string[]; samples: PerceptualSample[];
  sourceEvidenceCurrent: boolean; retainedEvidenceAsOf?: string | null;
};
export type PerceptualReview = {
  reviewId: string; channelId: string; state: "running" | "complete" | "cancelled" | "interrupted" | "failed";
  algorithmVersion: string; decoderAvailable: boolean; processed: number; total: number;
  sources: PerceptualSource[]; candidates: PerceptualCandidate[]; warnings: string[];
  startedAt: string; completedAt?: string | null;
};

function desktop() { if (!isTauri()) throw new Error("Local footage analysis is available in the desktop app."); }
export function startPerceptualReview(expectedChannelId: string, itemIds: string[], approved: boolean): Promise<PerceptualReview> {
  desktop(); return invoke("start_perceptual_review", { expectedChannelId, itemIds, approved });
}
export function loadPerceptualReview(expectedChannelId: string, reviewId: string): Promise<PerceptualReview> {
  desktop(); return invoke("load_perceptual_review", { expectedChannelId, reviewId });
}
export function cancelPerceptualReview(expectedChannelId: string, reviewId: string): Promise<PerceptualReview> {
  desktop(); return invoke("cancel_perceptual_review", { expectedChannelId, reviewId });
}
