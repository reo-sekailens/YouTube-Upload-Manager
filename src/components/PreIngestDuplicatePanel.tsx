import { useState } from "react";
import type { PreIngestDuplicateScan } from "../lib/types";

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

export function PreIngestDuplicatePanel({
  busy,
  fileCount,
  dropActive,
  scan,
  onCancel,
  onChoose,
  onPrepareLocalDuplicateDelete,
  onDeleteLocalDuplicate,
}: PreIngestDuplicatePanelProps) {
  const [deleteTarget, setDeleteTarget] = useState<LocalDeleteTarget>();
  const [selectedOrdinals, setSelectedOrdinals] = useState<Set<number>>(
    new Set(),
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
          stage: `Preparing “${file.fileName}”…`,
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
      className="panel pre-ingest-duplicate"
    >
      <header className="section-heading pre-ingest-duplicate__heading">
        <div>
          <p className="eyebrow">BEFORE INGEST</p>
          <h2 id="pre-ingest-duplicate-heading">Check files for duplicates</h2>
          <p className="section-copy">
            Fast match is the default: it compares filenames without reading
            media. Choose deep SHA-256 matching only when you need exact file
            evidence. Both accept INSV, LRV, and other file types without
            ingesting or uploading them.
          </p>
        </div>
        <div className="pre-ingest-duplicate__actions">
          <button
            className="secondary-action"
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
            className="secondary-action"
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
              className="text-button"
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
        className={`pre-ingest-duplicate__dropzone${dropActive ? " pre-ingest-duplicate__dropzone--active" : ""}`}
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
      {scan?.youtubeCheckDetail && (
        <p className="pre-ingest-duplicate__notice" role="status">
          {scan.youtubeCheckDetail}
        </p>
      )}
      {scan && scan.files.length > 0 && (
        <div
          className="pre-ingest-duplicate__results"
          role="list"
          aria-label="Pre-ingest duplicate results"
        >
          {eligibleOrdinals.length > 0 && (
            <div className="pre-ingest-duplicate__bulk-toolbar">
              <label>
                <input
                  checked={allEligibleSelected}
                  disabled={busy || bulkDeleting}
                  onChange={toggleSelectAll}
                  type="checkbox"
                />{" "}
                Select all matched local files
              </label>
              <span>{selectedFiles.length} selected</span>
              <button
                className="danger-button"
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
          {scan.files.map((file) => (
            <article
              className="pre-ingest-duplicate__result"
              key={file.ordinal}
              role="listitem"
            >
              <header>
                {isLocalDeleteEligible(file) && (
                  <label
                    className="pre-ingest-duplicate__select"
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
                  <span>{file.sizeBytes.toLocaleString()} bytes</span>
                </div>
                <b
                  className={
                    file.error
                      ? "is-error"
                      : verdict(file) === "No match found"
                        ? "is-clear"
                        : "is-match"
                  }
                >
                  {verdict(file)}
                </b>
              </header>
              {file.error && <p>{file.error}</p>}
              {file.localMatches.length > 0 && (
                <div
                  className="pre-ingest-duplicate__comparison"
                  aria-label={`Local duplicate comparison for ${file.fileName}`}
                >
                  <section>
                    <span>Selected source</span>
                    <strong>{file.fileName}</strong>
                    <p>
                      {scan.mode === "deep"
                        ? "Exact SHA-256 evidence"
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
                </div>
              )}
              {file.droppedDuplicateFileNames.length > 0 && (
                <p>
                  Matches another dropped file:{" "}
                  {file.droppedDuplicateFileNames.join(", ")}.
                </p>
              )}
              {file.uploadedTitleMatches.length > 0 && (
                <div
                  className="pre-ingest-duplicate__comparison pre-ingest-duplicate__comparison--remote"
                  aria-label={`Remote title comparison for ${file.fileName}`}
                >
                  <section>
                    <span>Desktop source</span>
                    <strong>{file.fileName}</strong>
                    <p>Local file awaiting ingest</p>
                  </section>
                  <section>
                    <span>YouTube library title</span>
                    {file.uploadedTitleMatches.map((title) => (
                      <div key={title}>
                        <strong>{title}</strong>
                        <p>Title evidence only — not an exact file hash</p>
                      </div>
                    ))}
                  </section>
                </div>
              )}
              {isLocalDeleteEligible(file) && (
                <button
                  className="danger-button pre-ingest-duplicate__delete"
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
                <p className="pre-ingest-duplicate__delete-error" role="alert">
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
      )}
      {deleteTarget && (
        <section
          className="pre-ingest-duplicate__delete-confirmation"
          aria-labelledby="local-duplicate-delete-heading"
        >
          <p className="eyebrow">PERMANENT, IRREVERSIBLE ACTION</p>
          <h3 id="local-duplicate-delete-heading">
            Delete “{deleteTarget.file.fileName}” from this device
          </h3>
          <p>
            This deletes only the dropped desktop source file. The matching
            managed upload copy and any YouTube video stay unchanged.
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
            <p className="pre-ingest-duplicate__delete-error" role="alert">
              {deleteError}
            </p>
          )}
          <div>
            <button
              className="secondary-action"
              disabled={busy || bulkDeleting}
              onClick={clearSingleDelete}
              type="button"
            >
              Keep file
            </button>
            <button
              className="danger-button"
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
          className="pre-ingest-duplicate__delete-confirmation"
          aria-labelledby="bulk-local-delete-heading"
        >
          <p className="eyebrow">BULK PERMANENT DELETION</p>
          <h3 id="bulk-local-delete-heading">
            Delete {selectedFiles.length} selected local file
            {selectedFiles.length === 1 ? "" : "s"}
          </h3>
          <p>
            Each file is revalidated against the persisted duplicate review,
            hashed before token creation, then hashed again immediately before
            removal. Type <code>{bulkPhrase}</code> to confirm this batch.
          </p>
          <ul className="pre-ingest-duplicate__bulk-selection">
            {selectedFiles.map(({ file, ordinal }) => (
              <li key={ordinal}>{file.fileName}</li>
            ))}
          </ul>
          {bulkProgress.total > 0 && (
            <p className="pre-ingest-duplicate__bulk-progress" role="status">
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
            <p className="pre-ingest-duplicate__delete-error" role="alert">
              {bulkError}
            </p>
          )}
          <div>
            <button
              className="secondary-action"
              disabled={busy || bulkDeleting}
              onClick={closeBulkDelete}
              type="button"
            >
              Keep selected files
            </button>
            <button
              className="danger-button"
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
