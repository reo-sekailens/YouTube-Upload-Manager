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

/** A newly imported local video whose title matches an uploaded channel video. */
export type UploadTitleDuplicate = {
  itemId: string;
  title: string;
  /** Matching titles from the owner-authorized, freshly synchronized YouTube inventory. */
  matchedTitles: string[];
};

export type UploadTitleDuplicateDecision = "ignore" | "skip";

/** A file checked locally before it is imported into the upload workspace. */
export type PreIngestDuplicateFile = {
  /** Opaque, short-lived native handle for deleting only this verified desktop source file. */
  localDeleteToken?: string;
  fileName: string;
  sizeBytes: number;
  localMatches: Array<{ title: string; fileName: string; status: string }>;
  droppedDuplicateFileNames: string[];
  uploadedTitleMatches: string[];
  error?: string;
};

/** Persisted, device-local pre-ingest work. Source paths never enter the webview. */
export type PreIngestDuplicateScan = {
  id: string;
  mode: "light" | "deep" | string;
  status: "queued" | "running" | "syncing" | "complete" | string;
  totalFiles: number;
  completedFiles: number;
  files: PreIngestDuplicateFile[];
  youtubeTitleChecked: boolean;
  youtubeCheckDetail?: string;
};

export type DashboardSnapshot = {
  activeChannel?: string;
  items: UploadItem[];
  duplicates: DuplicateCandidate[];
  /** Title matches held for an explicit operator decision before they can upload. */
  pendingTitleDuplicates: UploadTitleDuplicate[];
};

/** Safe, device-local status for the operator-approved watched folder. */
export type FolderMonitorSettings = {
  enabled: boolean;
  folderPath?: string;
  channelName?: string;
  visibility: FolderMonitorVisibility;
  madeForKids: boolean;
  playlistId?: string;
  playlistTitle?: string;
  status: string;
  detail: string;
  lastScanAt?: string;
  lastFileName?: string;
};

/** Safe-to-render metadata. OAuth tokens never enter the webview. */
export type ConnectionSettings = {
  /** The native layer has an operator-imported Desktop OAuth client; its ID is not exposed to the webview. */
  oauthConfigured?: boolean;
  activeChannel?: string;
  connected: boolean;
  detail?: string;
  secureStoreAvailable?: boolean;
  /** True after native OAuth has granted the YouTube deletion scope to this device. */
  deletionAuthorized?: boolean;
  /** A temporary, local deletion session is active; every video still requires confirmation. */
  deletionSudoActive?: boolean;
  deletionSudoExpiresAt?: string;
};

export type YouTubeConnectionStart = {
  authorizationUrl: string;
};

/** Safe receipt for a compact cross-device metadata archive. */
export type PortableArchiveReceipt = {
  uploadCount: number;
  remoteVideoCount: number;
  bytes: number;
  detail: string;
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
