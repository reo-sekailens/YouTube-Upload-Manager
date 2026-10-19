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

/** Explicit native-validated metadata captured with one reviewed upload. */
export type UploadMetadata = {
  title?: string;
  description?: string;
  tags?: string[];
  categoryId?: string;
  language?: string;
  defaultAudioLanguage?: string;
  license?: "youtube" | "creativeCommon";
  notifySubscribers?: boolean;
  containsSyntheticMedia?: boolean;
  publishAt?: string;
};
export type UploadMetadataPreset = { id: string; channelId: string; name: string; metadata: UploadMetadata; updatedAt?: string };
export type UploadMetadataMapping = {
  matches: Array<{ index: number; fileName: string; metadata: UploadMetadata; templateMetadata?: UploadMetadata }>;
  issues: string[];
};

/** Required operator declaration collected before manual intake. */
export type ManualUploadSettings = {
  madeForKids: boolean;
  visibility: UploadVisibility;
  playlistId?: string;
  playlistTitle?: string;
  deleteSourceAfterUpload: boolean;
  metadata?: UploadMetadata;
  /** Complete per-item overrides, keyed by the exact reviewed source selection. */
  itemMetadata?: Record<string, UploadMetadata>;
  expectedChannelId?: string;
  publicationContexts?: Record<string, { timeZone: string; tzdbVersion: string }>;
};

/** Device-wide default used to prefill, never hide, the manual intake declaration. */
export type ManualUploadDefaults = {
  madeForKids: boolean;
  visibility: UploadVisibility;
  /** The saved playlist belongs only to this connected channel on this device. */
  playlistId?: string;
  playlistTitle?: string;
  playlistChannelId?: string;
};

export type ManualUploadDefaultsPatch = {
  madeForKids?: boolean;
  visibility?: UploadVisibility;
  /** Empty clears the connected channel's default; omission preserves it. */
  playlistId?: string;
  expectedChannelId?: string;
};

/** Safe playlist metadata retrieved from the owner-authorized channel. */
export type YouTubePlaylist = {
  id: string;
  title: string;
};

export type PlaylistPrivacy = "private" | "unlisted" | "public";

/** Confirmed bulk membership outcomes; unresolved IDs remain selected for review. */
export type PlaylistAddReceipt = {
  channelId: string;
  playlistId: string;
  requestedVideoIds: string[];
  addedVideoIds: string[];
  skippedVideoIds: string[];
  remainingVideoIds: string[];
  error?: string;
  uncertain?: boolean;
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
  deleteSourceAfterUpload?: boolean;
  sourceDeleteStatus?:
    | "pending"
    | "waiting_for_youtube_processing"
    | "processing_verified"
    | "deleted"
    | "retained"
    | "retained_youtube_processing_failed"
    | "retained_youtube_upload_failed"
    | "retained_youtube_upload_rejected";
  videoId?: string;
  detail?: string;
  /** Native sanitized setup failure identity; no provider payload or secrets. */
  retryFailureKey?: string;
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

/** A local upload held for review after native filename-light-dedupe finds a match. */
export type UploadTitleDuplicate = {
  itemId: string;
  title: string;
  /** Matching titles from the synchronized YouTube library and/or local queue. */
  matchedTitles: string[];
  matchScope: "youtube" | "local_queue" | "youtube_and_local";
};

export type UploadTitleDuplicateDecision = "ignore" | "skip";

/** Native-owned duplicate evidence; no filesystem paths are accepted by the player. */
export type DuplicateMediaPreviewTarget =
  | { kind: "upload"; itemId: string }
  | { kind: "preflight"; jobId: string; ordinal: number }
  | { kind: "candidate"; candidateId: string }
  | { kind: "perceptual"; reviewId: string; candidateId: string };

export type DuplicateMediaPreviewEvidence = {
  unavailableLocalSources?: number;
  localSources: Array<{ token: string; url: string; title: string; fileName: string }>;
  youtubeVideos: Array<{ videoId: string; title: string }>;
};

/** A file checked locally before it is imported into the upload workspace. */
export type PreIngestDuplicateFile = {
  /** Opaque, short-lived native handle for deleting only this verified desktop source file. */
  localDeleteToken?: string;
  /** A persisted local match has an eligible desktop source; native code still revalidates it before deletion. */
  canDeleteLocalDuplicate: boolean;
  /** Stable persisted ordinal used by native delete preparation and UI selection. */
  ordinal: number;
  fileName: string;
  sizeBytes: number;
  /** Locally derived source-file facts; source paths remain native-only. */
  localMetadata: {
    fileType?: string;
    modifiedAt?: string;
    durationSeconds?: number;
    sizeBytes?: number;
    containerFormat?: string;
    bitRate?: string;
    streams: Array<{ kind: string; label: string; fields: Array<{ label: string; value: string }> }>;
    metadataFields: Array<{ label: string; value: string }>;
  };
  localMatches: Array<{ title: string; fileName: string; status: string }>;
  droppedDuplicateFileNames: string[];
  uploadedTitleMatches: Array<{
    title: string;
    duration?: string;
    privacyStatus?: string;
    /** Timestamp when this inventory record was last synchronized locally. */
    updatedAt: string;
  }>;
  /** Total evidence counts can exceed the bounded preview arrays above. */
  localMatchCount?: number;
  droppedDuplicateCount?: number;
  uploadedTitleMatchCount?: number;
  error?: string;
};

/** Persisted, device-local pre-ingest work. Source paths never enter the webview. */
export type PreIngestDuplicateScan = {
  id: string;
  mode: "light" | "deep" | string;
  status: "queued" | "running" | "syncing" | "complete" | string;
  totalFiles: number;
  completedFiles: number;
  /** The persisted file currently being checked, if native work is active. */
  currentFileName?: string;
  /** FFprobe metadata still being collected independently of duplicate matching. */
  pendingMetadataFiles: number;
  matchedFiles?: number;
  /** Native paging cursor; files and activity are capped to one page. */
  fileOffset?: number;
  fileLimit?: number;
  activityOffset?: number;
  activityLimit?: number;
  activityTotal?: number;
  files: PreIngestDuplicateFile[];
  /** Safe, persisted operation events. They contain filenames but never source paths. */
  activityLog: Array<{ fileName?: string; message: string; createdAt: string }>;
  youtubeTitleChecked: boolean;
  youtubeCheckDetail?: string;
};

export type PreIngestDuplicateScanStatus = Omit<
  PreIngestDuplicateScan,
  "files" | "activityLog" | "fileOffset" | "fileLimit" | "activityOffset" | "activityLimit"
>;

export type BatchImportItemReceipt = {
  ordinal: number;
  fileName: string;
  status: "imported" | "queued" | "duplicate_review" | "failed" | string;
  item?: UploadItem;
  detail?: string;
};

/** One native request owns a complete intake wave and returns safe per-file receipts. */
export type BatchImportReceipt = {
  requestedCount: number;
  importedCount: number;
  queuedCount: number;
  duplicateCount: number;
  failedCount: number;
  items: BatchImportItemReceipt[];
  detail?: string;
};

export type DashboardSnapshot = {
  activeChannel?: string;
  /** Immutable provider channel ID used for isolation; never use the display name as a key. */
  activeChannelId?: string;
  /** Durable channel-scoped state cursor represented by this snapshot. */
  revision: number;
  items: UploadItem[];
  duplicates: DuplicateCandidate[];
  /** Title matches held for an explicit operator decision before they can upload. */
  pendingTitleDuplicates: UploadTitleDuplicate[];
};

/** Safe crash-marker metadata included in the single native startup envelope. */
export type CrashRecoveryStatus = {
  crashDetected: boolean;
  detectedAt?: string;
  failureKind?: string;
};

/** Native startup fence state. Queue actions remain unavailable until explicitly enabled. */
export type StartupReadiness = {
  classificationComplete: boolean;
  safeShellRendered: boolean;
  deferredRecoveryState: "pending" | "running" | "complete" | "failed";
  queueActionsEnabled: boolean;
  detail: string;
};

/** Safe, device-local status for the operator-approved watched folder. */
export type FolderMonitorSettings = {
  /** Stable native identity; each configured folder has one independent record. */
  id?: string;
  enabled: boolean;
  folderPath?: string;
  channelName?: string;
  channelId?: string;
  visibility: FolderMonitorVisibility;
  madeForKids: boolean;
  deleteSourceAfterUpload?: boolean;
  playlistId?: string;
  playlistTitle?: string;
  status: string;
  detail: string;
  lastScanAt?: string;
  lastFileName?: string;
};

/** Safe, folder-scoped activity projection. Source paths never enter the webview. */
export type FolderMonitorFileActivity = {
  monitorId?: string;
  /** Opaque local identifier used only to requeue this already-authorized job. */
  itemId?: string;
  /** YouTube ID is present only for a watched item that completed an upload. */
  videoId?: string;
  /** The active channel's authenticated YouTube inventory still contains this ID. */
  liveConfirmed: boolean;
  /** The watched source is still present locally when this overview was loaded. */
  localSourceAvailable: boolean;
  /** Local cleanup receipt; deleted sources are never offered again. */
  sourceDeleteStatus?: string;
  fileName: string;
  observationState: string;
  sizeBytes: number;
  updatedAt: string;
  uploadTitle?: string;
  uploadStatus?: string;
  confirmedBytes?: number;
  totalBytes?: number;
  detail?: string;
};

/** Outcome for one operator-confirmed local watched-file deletion. */
export type FolderMonitorLocalDeleteResult = {
  itemId: string;
  fileName: string;
  status: "deleted" | "retained";
  detail: string;
};

/** Safe, bounded audit history for the currently monitored folder/channel. */
export type FolderMonitorLogEntry = {
  kind: string;
  detail?: string;
  createdAt: string;
};

export type FolderMonitorOverview = {
  settings: FolderMonitorSettings;
  files: FolderMonitorFileActivity[];
  logs: FolderMonitorLogEntry[];
};

export type FolderMonitorCollection = {
  /** Only the connected channel's configured folders, including paused folders. */
  monitors: FolderMonitorSettings[];
};

export type FolderMonitorLocalDeleteProgress = {
  monitorId: string;
  channelId: string;
  completed: number;
  total: number;
  result: FolderMonitorLocalDeleteResult;
};

/** Safe-to-render metadata. OAuth tokens never enter the webview. */
export type ConnectionSettings = {
  /** Opaque native credential-scope identity; changes on channel/client/project/bundle replacement. */
  connectionRevision?: string;
  /** The native layer has an operator-imported Desktop OAuth client; its ID is not exposed to the webview. */
  oauthConfigured?: boolean;
  activeChannel?: string;
  /** Immutable provider channel ID used for event and cache isolation. */
  activeChannelId?: string;
  connected: boolean;
  detail?: string;
  secureStoreAvailable?: boolean;
  /** True after native OAuth has granted the YouTube deletion scope to this device. */
  deletionAuthorized?: boolean;
  /** A temporary, local deletion session is active; every video still requires confirmation. */
  deletionSudoActive?: boolean;
  deletionSudoExpiresAt?: string;
};

/** One durable, channel-scoped state mutation emitted by the native layer. */
export type StateChange = {
  revision: number;
  channelId: string;
  surface: string;
  entityId: string;
  eventKind: string;
  payload?: unknown;
};

/** Recoverable event/catch-up envelope. `fromRevision` is the cursor before `changes`. */
export type StateChangeBatch = {
  fromRevision: number;
  toRevision: number;
  resetRequired: boolean;
  changes: StateChange[];
};

/** One coherent, safe-to-render view of native startup state. */
export type StartupBootstrap = {
  crashRecovery: CrashRecoveryStatus;
  connection: ConnectionSettings;
  snapshot: DashboardSnapshot;
  readiness: StartupReadiness;
};

export type YouTubeConnectionStart = {
  authorizationUrl: string;
  attemptId: string;
};

/** Provider limits contain no OAuth client identifiers or credential material. */
export type UploadLimitStatus = {
  kind: "none" | "api_quota" | "channel_upload";
  resumesAt?: string;
  channelResumesAt?: string;
  detail?: string;
};

/** An opaque native review of a replacement Desktop OAuth JSON file. */
export type OAuthClientReplacementReview = {
  reviewId: string;
  channelId: string;
  channelName: string;
  cloudQuotaPaused: boolean;
  channelUploadPaused: boolean;
};

export type OAuthClientReplacementStatus = {
  state: "ready" | "authorizing" | "completed" | "failed" | "cancelled";
  detail?: string;
  settings?: ConnectionSettings;
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
  /** YouTube processing state; only `processed` videos are dedupe evidence. */
  uploadStatus?: string;
  updatedAt: string;
};

/** A review-bound title change. The native layer rechecks both channel ownership and the previous title before updating YouTube. */
export type VideoTitleRename = {
  videoId: string;
  previousTitle: string;
  title: string;
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
