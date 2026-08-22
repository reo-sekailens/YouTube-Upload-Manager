import { Component, type ErrorInfo, type ReactNode, useCallback, useState } from "react";
import {
  acknowledgeCrashRecovery,
  isTauri,
  loadDiagnosticReport,
  recordWebviewError,
} from "../lib/local";

const githubNewIssueUrl =
  "https://github.com/reo-sekailens/YouTube-Upload-Manager/issues/new";

async function copyText(text: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  textarea.style.cssText = "left:-9999px;position:fixed;top:0";
  document.body.append(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) throw new Error("Your browser did not allow copying the report.");
}

type CrashRecoveryScreenProps = {
  detectedAt?: string;
  failureKind?: string;
  liveFailure?: boolean;
  onContinue: () => void;
};

export function CrashRecoveryScreen({
  detectedAt,
  failureKind,
  liveFailure = false,
  onContinue,
}: CrashRecoveryScreenProps) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("Nothing is sent automatically.");

  const finishRecovery = useCallback(async () => {
    setBusy(true);
    try {
      await acknowledgeCrashRecovery();
      onContinue();
    } catch (error) {
      setStatus(
        error instanceof Error
          ? error.message
          : "The crash marker could not be cleared. You can still copy the report.",
      );
    } finally {
      setBusy(false);
    }
  }, [onContinue]);

  const copyIssueReport = useCallback(async () => {
    setBusy(true);
    try {
      await copyText(await loadDiagnosticReport());
      setStatus("Copied. Paste it into a new GitHub issue, then add what you were doing.");
    } catch (error) {
      setStatus(
        error instanceof Error
          ? error.message
          : "The diagnostic report could not be copied.",
      );
    } finally {
      setBusy(false);
    }
  }, []);

  const openPrefilledIssue = useCallback(async () => {
    setBusy(true);
    try {
      const report = await loadDiagnosticReport();
      await copyText(report);
      const issueUrl = new URL(githubNewIssueUrl);
      issueUrl.searchParams.set("title", "Crash recovery diagnostics report");
      issueUrl.searchParams.set("body", report);
      if (isTauri) {
        const { openUrl } = await import("@tauri-apps/plugin-opener");
        await openUrl(issueUrl.toString());
      } else {
        window.open(issueUrl.toString(), "_blank", "noopener,noreferrer");
      }
      setStatus("Opened a pre-filled GitHub issue. Review it, then submit it yourself.");
    } catch (error) {
      setStatus(
        error instanceof Error
          ? error.message
          : "The pre-filled GitHub issue could not be opened.",
      );
    } finally {
      setBusy(false);
    }
  }, []);

  return (
    <main className="crash-recovery" role="alert">
      <section className="crash-recovery__card" aria-labelledby="crash-heading">
        <div aria-hidden="true" className="crash-recovery__code">:(</div>
        <p className="eyebrow">RECOVERY MODE</p>
        <h1 id="crash-heading">
          {liveFailure ? "The workspace hit an unexpected error." : "A previous app crash was detected."}
        </h1>
        <p>
          Your saved upload queue and local recovery records were left intact.
          No upload is retried from this screen.
        </p>
        {detectedAt && (
          <p className="crash-recovery__timestamp">
            Detected: {new Date(detectedAt).toLocaleString()}
          </p>
        )}
        {failureKind && (
          <p className="crash-recovery__failure" role="status">
            <strong>Detected error:</strong> {failureKind}
          </p>
        )}
        <div className="crash-recovery__steps">
          <span>1</span>
          <p>Open a safe, GitHub-ready report with the diagnostics already filled in.</p>
          <span>2</span>
          <p>Continue only when you are ready to return to the workspace.</p>
        </div>
        <div className="crash-recovery__actions">
          <button disabled={busy} onClick={() => void copyIssueReport()} type="button">
            {busy ? "Preparing report…" : "Copy GitHub issue report"}
          </button>
          <button
            className="secondary-action"
            disabled={busy}
            onClick={() => void openPrefilledIssue()}
            type="button"
          >
            Report to GitHub
          </button>
          <button
            className="secondary-action"
            disabled={busy}
            onClick={() => void finishRecovery()}
            type="button"
          >
            {liveFailure ? "Restart workspace" : "Continue to workspace"}
          </button>
        </div>
        <p className="crash-recovery__status" role="status">{status}</p>
      </section>
    </main>
  );
}

type CrashBoundaryProps = { children: ReactNode };
type CrashBoundaryState = { failed: boolean };

export class CrashBoundary extends Component<CrashBoundaryProps, CrashBoundaryState> {
  state: CrashBoundaryState = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    void recordWebviewError();
  }

  render() {
    if (this.state.failed) {
      return (
        <CrashRecoveryScreen
          failureKind="React render error"
          liveFailure
          onContinue={() => window.location.reload()}
        />
      );
    }
    return this.props.children;
  }
}
