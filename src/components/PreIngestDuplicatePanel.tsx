import { useEffect, useMemo, useRef, useState } from "react";
import { windowItems } from "../lib/list-windowing";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import type { PreIngestDuplicateScan } from "../lib/types";
import { PaginationControls } from "./PaginationControls";

type PreIngestDuplicatePanelProps = {
  busy: boolean;
  fileCount: number;
  dropActive: boolean;
  scan?: PreIngestDuplicateScan;
  onCancel: () => void;
  onChoose: (mode: "light" | "deep") => void;
  onPrepareLocalDuplicateDelete: (
    jobId: string,
    ordinal: number,
  ) => Promise<string>;
  onDeleteLocalDuplicate: (
    token: string,
    confirmation: string,
    ordinal: number,
  ) => Promise<void>;
  onLoadPage?: (kind: "files" | "activity", page: number) => Promise<void>;
  onLoadMetadata?: (
    jobId: string,
    ordinal: number,
  ) => Promise<PreIngestDuplicateScan["files"][number]["localMetadata"]>;
};

type LocalDeleteTarget = {
  file: PreIngestDuplicateScan["files"][number];
  ordinal: number;
};

function verdict(file: PreIngestDuplicateScan["files"][number]) {
  if (file.error) return "Could not check";
  if (file.localMatches.length > 0 || file.droppedDuplicateFileNames.length > 0)
    return "Local match";
  if (file.uploadedTitleMatches.length > 0) return "Uploaded title match";
  return "No match found";
}

function isLocalDeleteEligible(file: PreIngestDuplicateScan["files"][number]) {
  return Boolean(file.localDeleteToken || file.canDeleteLocalDuplicate);
}

function formatYoutubeDuration(value?: string) {
  if (!value) return "Unavailable";
  const match = value.match(/^PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?$/);
  if (!match) return value;
  const [, hours = "0", minutes = "0", seconds = "0"] = match;
  return (
    [hours, minutes, seconds]
      .map((part, index) =>
        index === 0 ? String(Number(part)) : part.padStart(2, "0"),
      )
      .filter((part, index) => index > 0 || part !== "0")
      .join(":") || "0:00"
  );
}

function formatLocalDuration(seconds?: number) {
  if (seconds === undefined || !Number.isFinite(seconds)) return "Not available";
  const total = Math.max(0, Math.floor(seconds));
  return [Math.floor(total / 3600), Math.floor((total % 3600) / 60), total % 60]
    .map((part, index) => index === 0 ? String(part) : String(part).padStart(2, "0"))
    .filter((part, index) => index > 0 || part !== "0")
    .join(":") || "0:00";
}

function formatBytes(bytes?: number) {
  if (bytes === undefined) return "Not available";
  if (bytes < 1024) return `${bytes.toLocaleString()} bytes`;
  const units = ["KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length);
  return `${(bytes / 1024 ** index).toFixed(index === 1 ? 0 : 1)} ${units[index - 1]}`;
}

function FullMetadataDetails({
  metadata,
  onLoad,
}: {
  metadata: PreIngestDuplicateScan["files"][number]["localMetadata"];
  onLoad: () => Promise<
    PreIngestDuplicateScan["files"][number]["localMetadata"]
  >;
}) {
  const [expanded, setExpanded] = useState(false);
  const [loadedMetadata, setLoadedMetadata] = useState(metadata);
  const [loading, setLoading] = useState(false);
  const [requested, setRequested] = useState(false);
  const visibleMetadata = loadedMetadata;
  return (
    <details
      className="mt-1.5 border-t border-line pt-2 text-[0.69rem] text-[#38516f] [&_summary]:cursor-pointer [&_summary]:font-bold [&_dl]:mt-2 [&_dl]:grid [&_dl]:gap-1.5 [&_dl>div]:grid [&_dl>div]:gap-0.5 [&_dl>div]:rounded-md [&_dl>div]:border [&_dl>div]:border-line [&_dl>div]:bg-white [&_dl>div]:px-1.5 [&_dl>div]:py-1 [&_dt]:text-[0.6rem] [&_dt]:font-bold [&_dt]:uppercase [&_dt]:text-[#718095] [&_dd]:m-0 [&_dd]:overflow-wrap-anywhere [&_dd]:text-[0.68rem] [&_dd]:text-[#3c4e68] [&_section]:border-0 [&_section]:bg-transparent [&_section]:pt-0.5 [&_section>strong]:text-[0.71rem] [&_section>strong]:text-[#304968]"
      onToggle={(event) => {
        const open = event.currentTarget.open;
        setExpanded(open);
        if (open && !loading && !requested) {
          setRequested(true);
          setLoading(true);
          void onLoad()
            .then(setLoadedMetadata)
            .finally(() => setLoading(false));
        }
      }}
    >
      <summary>Full container metadata</summary>
      {expanded && (
        <>
          {loading && <p role="status">Loading retained metadata…</p>}
          <dl>
            {visibleMetadata.metadataFields.map((field) => (
              <div key={`container-${field.label}`}>
                <dt>{field.label}</dt>
                <dd>{field.value}</dd>
              </div>
            ))}
          </dl>
          {visibleMetadata.streams.map((stream) => (
            <section key={stream.label}>
              <strong>{stream.label}</strong>
              <dl>
                {stream.fields.map((field) => (
                  <div key={`${stream.label}-${field.label}`}>
                    <dt>{field.label}</dt>
                    <dd>{field.value}</dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </>
      )}
    </details>
  );
}

export function PreIngestDuplicatePanel({
  busy,
  fileCount,
  dropActive,
  scan,
  onCancel,
  onChoose,
  onPrepareLocalDuplicateDelete,
  onDeleteLocalDuplicate,
  onLoadPage,
  onLoadMetadata,
}: PreIngestDuplicatePanelProps) {
  const [deleteTarget, setDeleteTarget] = useState<LocalDeleteTarget>();
  const [selectedOrdinals, setSelectedOrdinals] =
    useRetainedWorkspaceState<Set<number>>(
      "dedupe.preflight-selected-ordinals",
      () => new Set(),
    );
  const [resultsPage, setResultsPage] = useRetainedWorkspaceState(
    "dedupe.preflight-results-page",
    1,
  );
  const [activityPage, setActivityPage] = useRetainedWorkspaceState(
    "dedupe.preflight-activity-page",
    1,
  );
  const [bulkOpen, setBulkOpen] = useState(false);
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkConfirmation, setBulkConfirmation] = useState("");
  const [bulkProgress, setBulkProgress] = useState({
    completed: 0,
    total: 0,
    stage: "",
  });
  const [deleteError, setDeleteError] = useState("");
  const [bulkError, setBulkError] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [copyStatus, setCopyStatus] = useState("");
  const activeScanId = useRef(scan?.id);
  const eligibleOrdinals =
    scan?.files.flatMap((file) =>
      isLocalDeleteEligible(file) ? [file.ordinal] : [],
    ) ?? [];
  const selectedFiles =
    scan?.files.flatMap((file) =>
      selectedOrdinals.has(file.ordinal) && isLocalDeleteEligible(file)
        ? [{ file, ordinal: file.ordinal }]
        : [],
    ) ?? [];
  const allEligibleSelected =
    eligibleOrdinals.length > 0 &&
    eligibleOrdinals.every((ordinal) => selectedOrdinals.has(ordinal));
  const bulkPhrase = `DELETE ${selectedFiles.length} LOCAL FILE${selectedFiles.length === 1 ? "" : "S"}`;
  const progressPercent = scan && scan.totalFiles > 0
    ? Math.min(100, Math.round((scan.completedFiles / scan.totalFiles) * 100))
    : 0;
  const progressLog = scan?.activityLog
    .map((entry) => `${new Date(entry.createdAt).toLocaleTimeString()}${entry.fileName ? ` — ${entry.fileName}` : ""} — ${entry.message}`)
    .join("\n") ?? "";
  const visibleFiles = useMemo(() => {
    if (scan?.fileOffset === undefined) return windowItems(scan?.files ?? [], resultsPage);
    const limit = scan.fileLimit ?? 48;
    return {
      items: scan.files,
      page: Math.floor(scan.fileOffset / limit) + 1,
      pageCount: Math.max(1, Math.ceil(scan.totalFiles / limit)),
      start: scan.files.length === 0 ? 0 : scan.fileOffset + 1,
      end: scan.fileOffset + scan.files.length,
      total: scan.totalFiles,
    };
  }, [resultsPage, scan]);
  const visibleActivity = useMemo(() => {
    if (scan?.activityOffset === undefined)
      return windowItems(scan?.activityLog ?? [], activityPage);
    const limit = scan.activityLimit ?? 48;
    const total = scan.activityTotal ?? scan.activityLog.length;
    return {
      items: scan.activityLog,
      page: Math.floor(scan.activityOffset / limit) + 1,
      pageCount: Math.max(1, Math.ceil(total / limit)),
      start: scan.activityLog.length === 0 ? 0 : scan.activityOffset + 1,
      end: scan.activityOffset + scan.activityLog.length,
      total,
    };
  }, [activityPage, scan]);

  useEffect(() => {
    if (activeScanId.current === scan?.id) return;
    activeScanId.current = scan?.id;
    setSelectedOrdinals(new Set());
    setResultsPage(1);
    setActivityPage(1);
  }, [scan?.id, setActivityPage, setResultsPage, setSelectedOrdinals]);

  const copyProgressLog = async () => {
    if (!progressLog) return;
    try {
      await navigator.clipboard.writeText(progressLog);
      setCopyStatus("Copied");
    } catch {
      setCopyStatus("Copy unavailable");
    }
  };

  const clearSingleDelete = () => {
    setDeleteTarget(undefined);
    setConfirmation("");
    setDeleteError("");
  };
  const toggleSelection = (ordinal: number) =>
    setSelectedOrdinals((current) => {
      const next = new Set(current);
      if (next.has(ordinal)) next.delete(ordinal);
      else next.add(ordinal);
      return next;
    });
  const toggleSelectAll = () =>
    setSelectedOrdinals(
      allEligibleSelected ? new Set() : new Set(eligibleOrdinals),
    );
  const removeDeletedOrdinal = (ordinal: number) =>
    setSelectedOrdinals((current) => {
      const next = new Set(current);
      next.delete(ordinal);
      return next;
    });
  const prepareToken = async (
    file: PreIngestDuplicateScan["files"][number],
    ordinal: number,
  ) =>
    file.localDeleteToken ?? onPrepareLocalDuplicateDelete(scan!.id, ordinal);
  const startLocalDuplicateDelete = async (
    file: PreIngestDuplicateScan["files"][number],
    ordinal: number,
  ) => {
    setDeleteError("");
    try {
      const localDeleteToken = await prepareToken(file, ordinal);
      setDeleteTarget({ file: { ...file, localDeleteToken }, ordinal });
      setConfirmation("");
    } catch (error) {
      setDeleteError(
        error instanceof Error
          ? error.message
          : "The local file could not be prepared for deletion.",
      );
    }
  };
  const deleteSingleFile = async () => {
    if (
      !deleteTarget?.file.localDeleteToken ||
      confirmation !== deleteTarget.file.fileName
    )
      return;
    setDeleteError("");
    try {
      await onDeleteLocalDuplicate(
        deleteTarget.file.localDeleteToken,
        confirmation,
        deleteTarget.ordinal,
      );
      removeDeletedOrdinal(deleteTarget.ordinal);
      clearSingleDelete();
    } catch (error) {
      setDeleteError(
        error instanceof Error
          ? error.message
          : "The local file was not deleted.",
      );
    }
  };
  const closeBulkDelete = () => {
    setBulkOpen(false);
    setBulkConfirmation("");
    setBulkError("");
    setBulkProgress({ completed: 0, total: 0, stage: "" });
  };
  const deleteSelectedFiles = async () => {
    if (bulkConfirmation !== bulkPhrase || selectedFiles.length === 0) return;
    setBulkError("");
    setBulkDeleting(true);
    let deleted = 0;
    try {
      for (const { file, ordinal } of selectedFiles) {
        setBulkProgress({
          completed: deleted,
          total: selectedFiles.length,
          stage: `Using the accepted duplicate review for “${file.fileName}”…`,
        });
        const token = await prepareToken(file, ordinal);
        setBulkProgress({
          completed: deleted,
          total: selectedFiles.length,
          stage: `Deleting “${file.fileName}”…`,
        });
        await onDeleteLocalDuplicate(token, file.fileName, ordinal);
        deleted += 1;
        removeDeletedOrdinal(ordinal);
        setBulkProgress({
          completed: deleted,
          total: selectedFiles.length,
          stage: `Deleted ${deleted} of ${selectedFiles.length} local file${selectedFiles.length === 1 ? "" : "s"}.`,
        });
      }
      closeBulkDelete();
    } catch (error) {
      setBulkError(
        `${deleted} local file${deleted === 1 ? " was" : "s were"} deleted. ${error instanceof Error ? error.message : "The remaining files were not deleted."}`,
      );
    } finally {
      setBulkDeleting(false);
    }
  };

  return (
    <section
      aria-labelledby="pre-ingest-duplicate-heading"
      className="mb-4 rounded-xl border border-line bg-surface p-5"
    >
      <header className="mb-3.5 flex items-start justify-between gap-4 max-compact:flex-col max-compact:items-stretch">
        <div>
          <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">BEFORE INGEST</p>
          <h2 id="pre-ingest-duplicate-heading" className="m-0 tracking-[-0.035em] text-ink">Check files for duplicates</h2>
          <p className="mt-2 text-[0.78rem] leading-[1.45] text-muted">
            Fast match returns filename results immediately, then reads optional
            media metadata in the background. Choose deep BLAKE3 matching only
            when you need exact file evidence. Both accept INSV, LRV, and other
            file types without ingesting or uploading them.
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 max-compact:items-stretch max-compact:[&_button]:w-full">
          <button
            className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50"
            disabled={
              busy ||
              (scan !== undefined &&
                scan.status !== "complete" &&
                scan.status !== "cancelled")
            }
            onClick={() => onChoose("light")}
            type="button"
          >
            Light match files
          </button>
          <button
            className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50"
            disabled={
              busy ||
              (scan !== undefined &&
                scan.status !== "complete" &&
                scan.status !== "cancelled")
            }
            onClick={() => onChoose("deep")}
            type="button"
          >
            Deep hash files
          </button>
          {scan && !["complete", "cancelled"].includes(scan.status) && (
            <button
              className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onClick={onCancel}
              type="button"
            >
              Cancel check
            </button>
          )}
        </div>
      </header>
      <div
        className={dropActive
          ? "grid gap-1 rounded-[0.62rem] border-[1.5px] border-dashed border-brand bg-[#f1f6ff] p-3.5 text-[0.73rem] leading-[1.45] text-[#59708f] transition-[background,border-color] duration-150 [&_strong]:text-[0.82rem] [&_strong]:text-[#324966]"
          : "grid gap-1 rounded-[0.62rem] border-[1.5px] border-dashed border-[#b8c7dd] bg-[#fbfcfe] p-3.5 text-[0.73rem] leading-[1.45] text-[#59708f] transition-[background,border-color] duration-150 [&_strong]:text-[0.82rem] [&_strong]:text-[#324966]"}
      >
        <strong>
          {scan && scan.status !== "complete"
            ? `${scan.mode === "deep" ? "Deep hashing" : "Matching filenames"}: ${scan.completedFiles} of ${scan.totalFiles} checkpointed`
            : busy
              ? `Preparing ${fileCount} file${fileCount === 1 ? "" : "s"}…`
              : "Drop files anywhere in this Duplicate review tab on desktop"}
        </strong>
        <span>
          Desktop drops start fast filename matching. Use either picker button
          on Android or iOS; deep jobs keep completed file checkpoints if the
          app closes.
        </span>
      </div>
      {scan && scan.totalFiles > 0 && (
        <section className="mt-3 grid gap-2.5 rounded-[0.62rem] border border-[#d8e5f8] bg-[#f6f9ff] px-3.5 py-3 text-[0.72rem] text-[#60728a] [&>div]:flex [&>div]:flex-wrap [&>div]:items-center [&>div]:justify-between [&>div]:gap-2 [&>p]:m-0 [&>progress]:h-2.5 [&>progress]:w-full [&>progress]:accent-brand" aria-live="polite">
          <div>
            <strong className="text-[0.79rem] text-[#304c73]">{scan.mode === "deep" ? "Deep duplicate check" : "Light duplicate check"}</strong>
            <span>{scan.completedFiles} of {scan.totalFiles} files checked · {progressPercent}%</span>
          </div>
          <progress max={scan.totalFiles} value={scan.completedFiles} aria-label={`${scan.completedFiles} of ${scan.totalFiles} files checked`} />
          <p>{scan.currentFileName ? `Checking ${scan.currentFileName}` : scan.status === "complete" && scan.pendingMetadataFiles > 0 ? `Duplicate check complete. Reading media metadata for ${scan.pendingMetadataFiles} file${scan.pendingMetadataFiles === 1 ? "" : "s"} in the background.` : scan.status === "complete" ? "Duplicate check complete." : "Preparing the next selected file…"}</p>
          <details className="mt-0.5 border-t border-[#dce6f3] pt-2.5 [&_summary]:cursor-pointer [&_summary]:text-[0.74rem] [&_summary]:font-bold [&_summary]:text-[#405778] [&>div]:mt-2.5 [&>div]:flex [&>div]:flex-wrap [&>div]:items-center [&>div]:justify-start [&>div]:gap-2 [&_ol]:mt-2.5 [&_ol]:grid [&_ol]:max-h-52 [&_ol]:gap-1.5 [&_ol]:overflow-auto [&_ol]:pl-5 [&_li]:grid [&_li]:gap-0.5 [&_li]:text-[0.72rem] [&_time]:text-[0.66rem] [&_time]:text-[#7a899c] [&_li_strong]:text-[0.72rem] [&_li_strong]:text-[#38516f]">
            <summary>Activity log ({scan.activityTotal ?? scan.activityLog.length})</summary>
            <div>
              <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50" disabled={!progressLog} onClick={() => void copyProgressLog()} type="button">Copy log</button>
              {copyStatus && <span role="status">{copyStatus}</span>}
            </div>
            <ol>
              {visibleActivity.items.map((entry, index) => (
                <li
                  data-preflight-activity-record
                  key={`${entry.createdAt}-${entry.fileName ?? "job"}-${index}`}
                >
                  <time dateTime={entry.createdAt}>{new Date(entry.createdAt).toLocaleTimeString()}</time>
                  {entry.fileName && <strong>{entry.fileName}</strong>}
                  <span>{entry.message}</span>
                </li>
              ))}
            </ol>
            <PaginationControls
              end={visibleActivity.end}
              label="Pre-ingest activity"
              onPageChange={(page) => {
                setActivityPage(page);
                if (onLoadPage) void onLoadPage("activity", page);
              }}
              page={visibleActivity.page}
              pageCount={visibleActivity.pageCount}
              start={visibleActivity.start}
              total={visibleActivity.total}
            />
          </details>
        </section>
      )}
      {scan?.youtubeCheckDetail && (
        <p className="mt-3 rounded-md border border-[#f1dc9b] bg-[#fff9eb] px-2.5 py-2 text-[0.73rem] leading-[1.45] text-[#6c5a27]" role="status">
          {scan.youtubeCheckDetail}
        </p>
      )}
      {scan && scan.files.length > 0 && (
        <div
          className="mt-3 grid gap-2"
        >
          {eligibleOrdinals.length > 0 && (
            <div className="mb-1 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[#cbdaf4] bg-[#f3f7ff] px-3.5 py-3 max-compact:items-stretch max-compact:[&_button]:w-full">
              <label className="inline-flex items-center gap-2 text-sm text-[#34405a]">
                <input
                  checked={allEligibleSelected}
                  disabled={busy || bulkDeleting}
                  onChange={toggleSelectAll}
                  type="checkbox"
                />{" "}
                Select all matched local files (all pages)
              </label>
              <span>{selectedFiles.length} selected</span>
              <button
                className="cursor-pointer rounded-md border border-danger bg-danger px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-[#85342f] hover:bg-[#85342f] disabled:cursor-not-allowed disabled:opacity-50"
                disabled={busy || bulkDeleting || selectedFiles.length === 0}
                onClick={() => {
                  setBulkOpen(true);
                  setBulkConfirmation("");
                  setBulkError("");
                  setBulkProgress({
                    completed: 0,
                    total: selectedFiles.length,
                    stage: "Ready to delete",
                  });
                }}
                type="button"
              >
                Delete selected ({selectedFiles.length})
              </button>
            </div>
          )}
          <div aria-label="Pre-ingest duplicate results" role="list">
          {visibleFiles.items.map((file) => (
            <article
              className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] px-3 py-3 [&>header]:flex [&>header]:items-start [&>header]:justify-between [&>header]:gap-3 [&>header>div]:grid [&>header>div]:min-w-0 [&>header>div]:gap-0.5 [&_strong]:overflow-wrap-anywhere [&_strong]:text-[0.8rem] [&_strong]:text-[#2d3f5d] [&>header_span]:text-[0.72rem] [&_p]:mt-1.5 [&_p]:text-[0.72rem] [&_p]:leading-[1.45] [&_p]:text-[#68748a] max-compact:[&>header]:flex-col max-compact:[&>header]:items-stretch"
              data-preflight-record
              key={file.ordinal}
              role="listitem"
            >
              <header>
                {isLocalDeleteEligible(file) && (
                  <label
                    className="mr-[-0.1rem] inline-flex flex-none items-center gap-2"
                    aria-label={`Select ${file.fileName} for local deletion`}
                  >
                    <input
                      checked={selectedOrdinals.has(file.ordinal)}
                      disabled={busy || bulkDeleting}
                      onChange={() => toggleSelection(file.ordinal)}
                      type="checkbox"
                    />
                  </label>
                )}
                <div>
                  <strong>{file.fileName}</strong>
                  <span>{formatBytes(file.localMetadata.sizeBytes ?? file.sizeBytes)}</span>
                </div>
                <b
                  className={
                    file.error
                      ? "self-start rounded-full bg-[#fff0ef] px-2 py-1 text-[0.67rem] text-danger"
                      : verdict(file) === "No match found"
                        ? "self-start rounded-full bg-[#eaf7ef] px-2 py-1 text-[0.67rem] text-success"
                        : "self-start rounded-full bg-[#fff2dd] px-2 py-1 text-[0.67rem] text-[#885a14]"
                  }
                >
                  {verdict(file)}
                </b>
              </header>
              {file.error && <p>{file.error}</p>}
              {file.localMatches.length > 0 && (
                <div
                  className="mt-2.5 grid grid-cols-2 gap-2 max-compact:grid-cols-1 [&>section]:grid [&>section]:min-w-0 [&>section]:gap-0.5 [&>section]:rounded-md [&>section]:border [&>section]:border-[#dce5f1] [&>section]:bg-white [&>section]:px-2.5 [&>section]:py-2 [&>section>span]:text-[0.64rem] [&>section>span]:font-bold [&>section>span]:uppercase [&>section>span]:tracking-[0.05em] [&>section>span]:text-[#66758b] [&>section>div+div]:mt-0.5 [&>section>div+div]:border-t [&>section>div+div]:border-[#e7edf5] [&>section>div+div]:pt-1"
                  aria-label={`Local duplicate comparison for ${file.fileName}`}
                >
                  <section>
                    <span>Selected source</span>
                    <strong>{file.fileName}</strong>
                    <p>
                      {scan.mode === "deep"
                        ? "Exact BLAKE3 evidence"
                        : "Matching filename evidence"}
                    </p>
                  </section>
                  <section>
                    <span>Saved local copy</span>
                    {file.localMatches.map((match) => (
                      <div key={`${match.fileName}-${match.status}`}>
                        <strong>{match.fileName}</strong>
                        <p>
                          {match.status} · {match.title}
                        </p>
                      </div>
                    ))}
                  </section>
                  {(file.localMatchCount ?? file.localMatches.length) >
                    file.localMatches.length && (
                    <p>
                      Showing {file.localMatches.length} of {file.localMatchCount} saved local matches.
                    </p>
                  )}
                </div>
              )}
              {file.droppedDuplicateFileNames.length > 0 && (
                <p>
                  Matches another dropped file:{" "}
                  {file.droppedDuplicateFileNames.join(", ")}.
                  {(file.droppedDuplicateCount ?? file.droppedDuplicateFileNames.length) >
                    file.droppedDuplicateFileNames.length &&
                    ` Showing ${file.droppedDuplicateFileNames.length} of ${file.droppedDuplicateCount}.`}
                </p>
              )}
              {file.uploadedTitleMatches.length > 0 && (
                <div
                  className="mt-2.5 grid grid-cols-2 gap-3 max-compact:grid-cols-1 [&>section]:grid [&>section]:min-w-0 [&>section]:gap-1.5 [&>section]:rounded-md [&>section]:border [&>section]:border-[#dce5f1] [&>section]:bg-white [&>section]:px-3 [&>section]:py-3 [&>section:last-child]:border-[#f0dda8] [&>section:last-child]:bg-[#fffaf0] [&_details]:mt-0.5 [&_details]:border-t [&_details]:border-[#f0dda8] [&_details]:pt-2 [&_details_summary]:cursor-pointer [&_details_summary]:text-[0.69rem] [&_details_summary]:font-bold [&_details_summary]:text-[#76581e]"
                  aria-label={`Remote title comparison for ${file.fileName}`}
                >
                  <section className="border-[#d5e2f5]! bg-[#f8fbff]!">
                    <span>Local file</span>
                    <strong>{file.fileName}</strong>
                    <p className="m-0 text-[0.7rem]! text-[#607089]!">Selected desktop source</p>
                    <dl className="mt-0.5 grid grid-cols-2 gap-1.5 max-compact:grid-cols-1 [&>div]:grid [&>div]:min-w-0 [&>div]:gap-0.5 [&>div]:rounded-md [&>div]:border [&>div]:border-[#cfdbea]/80 [&>div]:bg-white/70 [&>div]:px-2 [&>div]:py-1.5 [&_dt]:text-[0.6rem] [&_dt]:font-bold [&_dt]:uppercase [&_dt]:tracking-[0.055em] [&_dt]:text-[#718095] [&_dd]:m-0 [&_dd]:overflow-wrap-anywhere [&_dd]:text-[0.72rem] [&_dd]:font-semibold [&_dd]:text-[#2d3f5d]">
                      <div><dt>Type</dt><dd>{file.localMetadata.fileType ?? "Unknown"}</dd></div>
                      <div><dt>Size</dt><dd>{formatBytes(file.localMetadata.sizeBytes ?? file.sizeBytes)}</dd></div>
                      <div><dt>Duration</dt><dd>{formatLocalDuration(file.localMetadata.durationSeconds)}</dd></div>
                      <div><dt>Modified</dt><dd>{file.localMetadata.modifiedAt ? new Date(file.localMetadata.modifiedAt).toLocaleString() : "Not available"}</dd></div>
                      {file.localMetadata.containerFormat && <div><dt>Container</dt><dd>{file.localMetadata.containerFormat}</dd></div>}
                      {file.localMetadata.bitRate && <div><dt>Bit rate</dt><dd>{Number(file.localMetadata.bitRate).toLocaleString()} b/s</dd></div>}
                    </dl>
                    {!file.error && (
                      <FullMetadataDetails
                        metadata={file.localMetadata}
                        onLoad={() =>
                          onLoadMetadata
                            ? onLoadMetadata(scan.id, file.ordinal)
                            : Promise.resolve(file.localMetadata)
                        }
                      />
                    )}
                  </section>
                  <section>
                    <span>YouTube video</span>
                    {file.uploadedTitleMatches.map((match) => (
                      <div className="[&>p]:m-0 [&>p]:text-[0.7rem]! [&>p]:text-[#607089]! [&_dl]:mt-0.5 [&_dl]:grid [&_dl]:grid-cols-2 [&_dl]:gap-1.5 max-compact:[&_dl]:grid-cols-1 [&_dl>div]:grid [&_dl>div]:min-w-0 [&_dl>div]:gap-0.5 [&_dl>div]:rounded-md [&_dl>div]:border [&_dl>div]:border-[#edd391]/70 [&_dl>div]:bg-white/70 [&_dl>div]:px-2 [&_dl>div]:py-1.5 [&_dt]:text-[0.6rem] [&_dt]:font-bold [&_dt]:uppercase [&_dt]:text-[#7e6a43] [&_dd]:m-0 [&_dd]:overflow-wrap-anywhere [&_dd]:text-[0.7rem] [&_dd]:text-[#5d5139]" key={`${match.title}-${match.updatedAt}`}>
                        <strong>{match.title}</strong>
                        <p>Possible title match — not an exact file hash</p>
                        <dl>
                          <div><dt>Duration</dt><dd>{formatYoutubeDuration(match.duration)}</dd></div>
                          <div><dt>Visibility</dt><dd>{match.privacyStatus ?? "Unavailable"}</dd></div>
                        </dl>
                        <details>
                          <summary>More YouTube details</summary>
                          <dl>
                            <div>
                              <dt>Inventory synced</dt>
                              <dd>
                                {new Date(match.updatedAt).toLocaleString()}
                              </dd>
                            </div>
                          </dl>
                        </details>
                      </div>
                    ))}
                    {(file.uploadedTitleMatchCount ?? file.uploadedTitleMatches.length) >
                      file.uploadedTitleMatches.length && (
                      <p>
                        Showing {file.uploadedTitleMatches.length} of{" "}
                        {file.uploadedTitleMatchCount} uploaded-title matches.
                      </p>
                    )}
                  </section>
                </div>
              )}
              {isLocalDeleteEligible(file) && (
                <button
                  className="mt-2.5 cursor-pointer rounded-md border border-danger bg-danger px-2 py-1.5 text-[0.72rem] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-[#85342f] hover:bg-[#85342f] disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={busy || bulkDeleting}
                  onClick={() =>
                    void startLocalDuplicateDelete(file, file.ordinal)
                  }
                  type="button"
                >
                  Delete local file
                </button>
              )}
              {deleteError && !deleteTarget && (
                <p className="font-semibold text-danger!" role="alert">
                  {deleteError}
                </p>
              )}
              {!file.error &&
                file.localMatches.length === 0 &&
                file.droppedDuplicateFileNames.length === 0 &&
                file.uploadedTitleMatches.length === 0 && (
                  <p>
                    {scan.mode === "deep"
                      ? "No saved-file hash or uploaded-title match."
                      : "No matching filename or uploaded-title match. Run deep hash matching for exact file evidence."}
                  </p>
                )}
            </article>
          ))}
          </div>
          <PaginationControls
            end={visibleFiles.end}
            label="Pre-ingest duplicate results"
            onPageChange={(page) => {
              setResultsPage(page);
              if (onLoadPage) void onLoadPage("files", page);
            }}
            page={visibleFiles.page}
            pageCount={visibleFiles.pageCount}
            start={visibleFiles.start}
            total={visibleFiles.total}
          />
        </div>
      )}
      {deleteTarget && (
        <section
          className="mt-3.5 grid gap-2.5 rounded-lg border border-[#edcbc8] bg-[#fff8f7] p-3.5 [&_.delete-eyebrow]:m-0 [&_.delete-eyebrow]:text-[0.67rem] [&_.delete-eyebrow]:font-bold [&_.delete-eyebrow]:uppercase [&_.delete-eyebrow]:tracking-[0.1em] [&_.delete-eyebrow]:text-danger [&_h3]:m-0 [&_h3]:text-[0.93rem] [&_h3]:text-[#3e2a2a] [&>p]:m-0 [&>p]:text-[0.75rem] [&>p]:leading-[1.45] [&>p]:text-[#6e5554] [&_label]:grid [&_label]:gap-1 [&_label]:text-[0.74rem] [&_label]:font-bold [&_label]:text-[#5e4848] [&_input]:rounded-md [&_input]:border [&_input]:border-[#d8bebb] [&_input]:bg-white [&_input]:px-2 [&_input]:py-2 [&_input]:text-[#303040] [&_input]:focus:border-[#b85048] [&_input]:focus:outline-3 [&_input]:focus:outline-[#c44f46]/16 [&>div]:flex [&>div]:flex-wrap [&>div]:justify-end [&>div]:gap-2 max-compact:[&>div]:flex-col max-compact:[&>div]:items-stretch max-compact:[&>div_button]:w-full"
          aria-labelledby="local-duplicate-delete-heading"
        >
          <p className="m-0 text-[0.67rem] font-bold tracking-[0.1em] text-danger uppercase">PERMANENT, IRREVERSIBLE ACTION</p>
          <h3 id="local-duplicate-delete-heading">
            Delete “{deleteTarget.file.fileName}” from this device
          </h3>
          <p>
            This deletes only the dropped desktop source file. The matching
            managed upload copy and any YouTube video stay unchanged. It reuses
            the accepted duplicate review and does not hash this file again.
          </p>
          <label htmlFor="local-duplicate-delete-confirmation">
            Type the exact file name to confirm
          </label>
          <input
            autoComplete="off"
            autoFocus
            id="local-duplicate-delete-confirmation"
            onChange={(event) => setConfirmation(event.target.value)}
            placeholder={deleteTarget.file.fileName}
            spellCheck={false}
            value={confirmation}
          />
          {deleteError && (
            <p className="font-semibold text-danger!" role="alert">
              {deleteError}
            </p>
          )}
          <div>
            <button
              className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy || bulkDeleting}
              onClick={clearSingleDelete}
              type="button"
            >
              Cancel deletion
            </button>
            <button
              className="cursor-pointer rounded-md border border-danger bg-danger px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-[#85342f] hover:bg-[#85342f] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={
                busy ||
                bulkDeleting ||
                confirmation !== deleteTarget.file.fileName
              }
              onClick={() => void deleteSingleFile()}
              type="button"
            >
              Delete permanently
            </button>
          </div>
        </section>
      )}
      {bulkOpen && (
        <section
          className="mt-3.5 grid gap-2.5 rounded-lg border border-[#edcbc8] bg-[#fff8f7] p-3.5 [&_.delete-eyebrow]:m-0 [&_.delete-eyebrow]:text-[0.67rem] [&_.delete-eyebrow]:font-bold [&_.delete-eyebrow]:uppercase [&_.delete-eyebrow]:tracking-[0.1em] [&_.delete-eyebrow]:text-danger [&_h3]:m-0 [&_h3]:text-[0.93rem] [&_h3]:text-[#3e2a2a] [&>p]:m-0 [&>p]:text-[0.75rem] [&>p]:leading-[1.45] [&>p]:text-[#6e5554] [&_label]:grid [&_label]:gap-1 [&_label]:text-[0.74rem] [&_label]:font-bold [&_label]:text-[#5e4848] [&_input]:rounded-md [&_input]:border [&_input]:border-[#d8bebb] [&_input]:bg-white [&_input]:px-2 [&_input]:py-2 [&_input]:text-[#303040] [&_input]:focus:border-[#b85048] [&_input]:focus:outline-3 [&_input]:focus:outline-[#c44f46]/16 [&>div]:flex [&>div]:flex-wrap [&>div]:justify-end [&>div]:gap-2 max-compact:[&>div]:flex-col max-compact:[&>div]:items-stretch max-compact:[&>div_button]:w-full"
          aria-labelledby="bulk-local-delete-heading"
        >
          <p className="m-0 text-[0.67rem] font-bold tracking-[0.1em] text-danger uppercase">BULK PERMANENT DELETION</p>
          <h3 id="bulk-local-delete-heading">
            Delete {selectedFiles.length} selected local file
            {selectedFiles.length === 1 ? "" : "s"}
          </h3>
          <p>
            The accepted opt-in duplicate review is reused for each selected
            file. Deletion does not hash the files again; hashing happens only
            when you run duplicate review. Type <code>{bulkPhrase}</code> to
            confirm this batch.
          </p>
          <ul className="my-2.5 max-h-40 overflow-auto pl-5 [&_li+li]:mt-1">
            {selectedFiles.map(({ file, ordinal }) => (
              <li key={ordinal}>{file.fileName}</li>
            ))}
          </ul>
          {bulkProgress.total > 0 && (
            <p className="font-semibold text-muted!" role="status">
              {bulkProgress.stage}
            </p>
          )}
          <label htmlFor="bulk-local-delete-confirmation">
            Type {bulkPhrase} to permanently delete the selected local files
            <input
              autoComplete="off"
              autoFocus
              id="bulk-local-delete-confirmation"
              onChange={(event) => setBulkConfirmation(event.target.value)}
              placeholder={bulkPhrase}
              spellCheck={false}
              value={bulkConfirmation}
            />
          </label>
          {bulkError && (
            <p className="font-semibold text-danger!" role="alert">
              {bulkError}
            </p>
          )}
          <div>
            <button
              className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy || bulkDeleting}
              onClick={closeBulkDelete}
              type="button"
            >
              Cancel deletion
            </button>
            <button
              className="cursor-pointer rounded-md border border-danger bg-danger px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-[#85342f] hover:bg-[#85342f] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy || bulkDeleting || bulkConfirmation !== bulkPhrase}
              onClick={() => void deleteSelectedFiles()}
              type="button"
            >
              {bulkDeleting
                ? "Deleting selected local files…"
                : `Delete ${selectedFiles.length} local file`}
              {bulkDeleting
                ? ""
                : `${selectedFiles.length === 1 ? "" : "s"} permanently`}
            </button>
          </div>
        </section>
      )}
    </section>
  );
}
