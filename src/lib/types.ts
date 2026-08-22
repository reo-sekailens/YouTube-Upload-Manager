export type UploadStatus =
  | "draft"
  | "queued"
  | "importing"
  | "dispatching"
  | "uploading"
  | "needs_reconciliation"
  | "uploaded"
  | "failed"
  | "cancelled";

/** YouTube publishing visibility selected by the operator for one manual upload. */
export type UploadVisibility = "private" | "unlisted" | "public";
export type FolderMonitorVisibility = "private" | "unlisted";

/** Required operator declaration collected before manual intake. */
export type ManualUploadSettings = {
  madeForKids: boolean;
  visibility: UploadVisibility;
  playlistId?: string;
  playlistTitle?: string;
};

/** Device-wide default used to prefill, never hide, the manual intake declaration. */
export type ManualUploadDefaults = {
  madeForKids: boolean;
};

/** Safe playlist metadata retrieved from the owner-authorized channel. */
export type YouTubePlaylist = {
  id: string;
  title: string;
};

export type UploadItem = {
  id: string;
  title: string;
  fileName: string;
  sizeBytes: number;
  digest?: string;
  status: UploadStatus;
  confirmedBytes: number;
  totalBytes: number;
  /** Explicit operator choice for this item. Manual uploads start private. */
  visibility: UploadVisibility;
  madeForKids: boolean;
  playlistId?: string;
  playlistTitle?: string;
  /** When this native upload attempt began; reset for each new attempt. */
  uploadStartedAt?: string;
  /** Native, acknowledged transfer rate. Absent until enough upload progress is measured. */
  transferBytesPerSecond?: number;
  videoId?: string;
  detail?: string;
  updatedAt: string;
};

export type DuplicateCandidate = {
  id: string;
  confidence: "exact_local" | "strong_remote" | "metadata";
  leftTitle: string;
  rightTitle: string;
  leftVideoId?: string;
  rightVideoId?: string;
  evidence: string;
  decision?: "keep" | "dismiss" | "delete_requested";
};

export type DashboardSnapshot = {
  activeChannel?: string;
  items: UploadItem[];
  duplicates: DuplicateCandidate[];
};

/** Safe, device-local status for the operator-approved watched folder. */
export type FolderMonitorSettings = {
  enabled: boolean;
  folderPath?: string;
  channelName?: string;
  visibility: FolderMonitorVisibility;
  status: string;
  detail: string;
  lastScanAt?: string;
  lastFileName?: string;
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
