import { useState } from "react";
import type { PreIngestDuplicateScan } from "../lib/types";

type PreIngestDuplicatePanelProps = {
  busy: boolean;
  fileCount: number;
  dropActive: boolean;
  scan?: PreIngestDuplicateScan;
  onCancel: () => void;
  onChoose: (mode: "light" | "deep") => void;
  onDeleteLocalDuplicate: (token: string, confirmation: string) => Promise<void>;
};

function verdict(file: PreIngestDuplicateScan["files"][number]) {
  if (file.error) return "Could not check";
  if (file.localMatches.length > 0 || file.droppedDuplicateFileNames.length > 0) return "Local match";
  if (file.uploadedTitleMatches.length > 0) return "Uploaded title match";
  return "No match found";
}

export function PreIngestDuplicatePanel({ busy, fileCount, dropActive, scan, onCancel, onChoose, onDeleteLocalDuplicate }: PreIngestDuplicatePanelProps) {
  const [deleteFile, setDeleteFile] = useState<PreIngestDuplicateScan["files"][number]>();
  const [confirmation, setConfirmation] = useState("");
  const [deleteError, setDeleteError] = useState("");
  const deleteFileLocally = async () => {
    if (!deleteFile?.localDeleteToken || confirmation !== deleteFile.fileName) return;
    setDeleteError("");
    try {
      await onDeleteLocalDuplicate(deleteFile.localDeleteToken, confirmation);
      setDeleteFile(undefined);
      setConfirmation("");
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : "The local file was not deleted.");
    }
  };
  return (
    <section aria-labelledby="pre-ingest-duplicate-heading" className="panel pre-ingest-duplicate">
      <header className="section-heading pre-ingest-duplicate__heading">
        <div>
          <p className="eyebrow">BEFORE INGEST</p>
          <h2 id="pre-ingest-duplicate-heading">Check files for duplicates</h2>
          <p className="section-copy">Fast match is the default: it compares filenames without reading media. Choose deep SHA-256 matching only when you need exact file evidence. Both accept INSV, LRV, and other file types without ingesting or uploading them.</p>
        </div>
        <div className="pre-ingest-duplicate__actions"><button className="secondary-action" disabled={busy || (scan !== undefined && scan.status !== "complete" && scan.status !== "cancelled")} onClick={() => onChoose("light")} type="button">Light match files</button><button className="secondary-action" disabled={busy || (scan !== undefined && scan.status !== "complete" && scan.status !== "cancelled")} onClick={() => onChoose("deep")} type="button">Deep hash files</button>{scan && !["complete", "cancelled"].includes(scan.status) && <button className="text-button" disabled={busy} onClick={onCancel} type="button">Cancel check</button>}</div>
      </header>
      <div className={`pre-ingest-duplicate__dropzone${dropActive ? " pre-ingest-duplicate__dropzone--active" : ""}`}>
        <strong>{scan && scan.status !== "complete" ? `${scan.mode === "deep" ? "Deep hashing" : "Matching filenames"}: ${scan.completedFiles} of ${scan.totalFiles} checkpointed` : busy ? `Preparing ${fileCount} file${fileCount === 1 ? "" : "s"}…` : "Drop files anywhere in this Duplicate review tab on desktop"}</strong>
        <span>Desktop drops start fast filename matching. Use either picker button on Android or iOS; deep jobs keep completed file checkpoints if the app closes.</span>
      </div>
      {scan?.youtubeCheckDetail && <p className="pre-ingest-duplicate__notice" role="status">{scan.youtubeCheckDetail}</p>}
      {scan && scan.files.length > 0 && <div className="pre-ingest-duplicate__results" role="list" aria-label="Pre-ingest duplicate results">
        {scan.files.map((file, index) => <article className="pre-ingest-duplicate__result" key={`${file.fileName}-${index}`} role="listitem">
          <header><div><strong>{file.fileName}</strong><span>{file.sizeBytes.toLocaleString()} bytes</span></div><b className={file.error ? "is-error" : verdict(file) === "No match found" ? "is-clear" : "is-match"}>{verdict(file)}</b></header>
          {file.error && <p>{file.error}</p>}
          {file.localMatches.length > 0 && <div className="pre-ingest-duplicate__comparison" aria-label={`Local duplicate comparison for ${file.fileName}`}>
            <section><span>Selected source</span><strong>{file.fileName}</strong><p>{scan.mode === "deep" ? "Exact SHA-256 evidence" : "Matching filename evidence"}</p></section>
            <section><span>Saved local copy</span>{file.localMatches.map((match) => <div key={`${match.fileName}-${match.status}`}><strong>{match.fileName}</strong><p>{match.status} · {match.title}</p></div>)}</section>
          </div>}
          {file.droppedDuplicateFileNames.length > 0 && <p>Matches another dropped file: {file.droppedDuplicateFileNames.join(", ")}.</p>}
          {file.uploadedTitleMatches.length > 0 && <div className="pre-ingest-duplicate__comparison pre-ingest-duplicate__comparison--remote" aria-label={`Remote title comparison for ${file.fileName}`}><section><span>Desktop source</span><strong>{file.fileName}</strong><p>Local file awaiting ingest</p></section><section><span>YouTube library title</span>{file.uploadedTitleMatches.map((title) => <div key={title}><strong>{title}</strong><p>Title evidence only — not an exact file hash</p></div>)}</section></div>}
          {file.localDeleteToken && <button className="danger-button pre-ingest-duplicate__delete" disabled={busy} onClick={() => { setDeleteFile(file); setConfirmation(""); setDeleteError(""); }} type="button">Delete this local duplicate</button>}
          {!file.error && file.localMatches.length === 0 && file.droppedDuplicateFileNames.length === 0 && file.uploadedTitleMatches.length === 0 && <p>{scan.mode === "deep" ? "No saved-file hash or uploaded-title match." : "No matching filename or uploaded-title match. Run deep hash matching for exact file evidence."}</p>}
        </article>)}
      </div>}
      {deleteFile && <section className="pre-ingest-duplicate__delete-confirmation" aria-labelledby="local-duplicate-delete-heading">
        <p className="eyebrow">PERMANENT, IRREVERSIBLE ACTION</p>
        <h3 id="local-duplicate-delete-heading">Delete “{deleteFile.fileName}” from this device</h3>
        <p>This deletes only the dropped desktop source file. The matching managed upload copy and any YouTube video stay unchanged.</p>
        <label htmlFor="local-duplicate-delete-confirmation">Type the exact file name to confirm</label>
        <input autoComplete="off" autoFocus id="local-duplicate-delete-confirmation" onChange={(event) => setConfirmation(event.target.value)} placeholder={deleteFile.fileName} spellCheck={false} value={confirmation} />
        {deleteError && <p className="pre-ingest-duplicate__delete-error" role="alert">{deleteError}</p>}
        <div><button className="secondary-action" disabled={busy} onClick={() => { setDeleteFile(undefined); setConfirmation(""); }} type="button">Keep file</button><button className="danger-button" disabled={busy || confirmation !== deleteFile.fileName} onClick={() => void deleteFileLocally()} type="button">Delete permanently</button></div>
      </section>}
    </section>
  );
}
