export type UploadStatus =
  | "draft"
  | "queued"
  | "importing"
  | "uploading"
  | "needs_reconciliation"
  | "uploaded"
  | "failed"
  | "cancelled";

export type UploadItem = {
  id: string;
  title: string;
  fileName: string;
  sizeBytes: number;
  digest?: string;
  status: UploadStatus;
  confirmedBytes: number;
  totalBytes: number;
  videoId?: string;
  detail?: string;
  updatedAt: string;
};

export type DuplicateCandidate = {
  id: string;
  confidence: "exact_local" | "strong_remote" | "metadata";
  leftTitle: string;
  rightTitle: string;
  evidence: string;
  decision?: "keep" | "dismiss" | "delete_requested";
};

export type DashboardSnapshot = {
  activeChannel?: string;
  items: UploadItem[];
  duplicates: DuplicateCandidate[];
};

/** Safe-to-render metadata. OAuth tokens never enter the webview. */
export type ConnectionSettings = {
  clientId?: string;
  activeChannel?: string;
  connected: boolean;
  detail?: string;
  secureStoreAvailable?: boolean;
  /** True only after a fresh native OAuth re-authorization included YouTube deletion scope. */
  deletionAuthorized?: boolean;
};

export type YouTubeConnectionStart = {
  authorizationUrl: string;
};

/** Owner-authorized YouTube inventory metadata, kept local to this device. */
export type RemoteVideo = {
  videoId: string;
  title: string;
  duration?: string;
  privacyStatus?: string;
  updatedAt: string;
};

/** A local, auditable deletion workflow record. A request is not a deletion result. */
export type DeletionRequest = {
  id: string;
  videoId: string;
  title: string;
  status: "pending" | "cancelled" | "deleted" | "failed" | string;
  detail: string;
  updatedAt: string;
};
