import { useCallback, useState } from "react";
import {
  acknowledgeCrashRecovery,
  isTauri,
  loadDiagnosticReport,
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
    <main className="grid min-h-screen items-center bg-[radial-gradient(circle_at_12%_10%,rgba(96,165,250,0.2),transparent_32rem),linear-gradient(135deg,#06244c,#0a4a86_52%,#0b6aab)] p-8 text-[#f8fbff] max-compact:items-start max-compact:px-5 max-compact:py-9" role="alert">
      <section className="mx-auto w-full max-w-[45rem]" aria-labelledby="crash-heading">
        <div aria-hidden="true" className="mb-7 ml-[-0.18rem] text-[clamp(4rem,12vw,7.5rem)] leading-[0.8] font-light tracking-[-0.12em] text-[#b9e4ff]">:(</div>
        <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-[#b9e4ff]">RECOVERY MODE</p>
        <h1 id="crash-heading" className="m-0 text-[clamp(1.75rem,5vw,2.75rem)] leading-[1.08] tracking-[-0.045em]">
          {liveFailure ? "The workspace hit an unexpected error." : "A previous app crash was detected."}
        </h1>
        <p className="mt-4 max-w-[40rem] text-base leading-[1.6] text-[#dceeff]">
          Your saved upload queue and local recovery records were left intact.
          No upload is retried from this screen.
        </p>
        {detectedAt && (
          <p className="mt-4 text-[0.8rem] text-[#b9e4ff]">
            Detected: {new Date(detectedAt).toLocaleString()}
          </p>
        )}
        {failureKind && (
          <p className="mt-4 border-l-[0.2rem] border-[#8fd3ff] bg-[#dff3ff]/12 px-3 py-2 text-[0.84rem] text-[#e7f5ff] [&_strong]:text-[#b9e4ff]" role="status">
            <strong>Detected error:</strong> {failureKind}
          </p>
        )}
        <div className="my-6 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2.5 [&_span]:inline-flex [&_span]:size-6 [&_span]:items-center [&_span]:justify-center [&_span]:rounded-full [&_span]:border [&_span]:border-[#dff3ff]/35 [&_span]:bg-[#dff3ff]/16 [&_span]:text-[0.72rem] [&_span]:font-bold [&_p]:m-0 [&_p]:text-[0.86rem] [&_p]:leading-[1.45] [&_p]:text-[#e3f3ff]">
          <span>1</span>
          <p>Open a safe, GitHub-ready report with the diagnostics already filled in.</p>
          <span>2</span>
          <p>Continue only when you are ready to return to the workspace.</p>
        </div>
        <div className="flex flex-wrap gap-2.5 max-compact:flex-col max-compact:[&_button]:w-full">
          <button className="cursor-pointer rounded-md border border-white bg-white px-3.5 py-3 text-[0.84rem] font-semibold text-[#083d72] disabled:cursor-not-allowed disabled:opacity-60" disabled={busy} onClick={() => void copyIssueReport()} type="button">
            {busy ? "Preparing report…" : "Copy GitHub issue report"}
          </button>
          <button
            className="cursor-pointer rounded-md border border-white/55 bg-transparent px-3.5 py-3 text-[0.84rem] font-semibold text-white disabled:cursor-not-allowed disabled:opacity-60"
            disabled={busy}
            onClick={() => void openPrefilledIssue()}
            type="button"
          >
            Report to GitHub
          </button>
          <button
            className="cursor-pointer rounded-md border border-white/55 bg-transparent px-3.5 py-3 text-[0.84rem] font-semibold text-white disabled:cursor-not-allowed disabled:opacity-60"
            disabled={busy}
            onClick={() => void finishRecovery()}
            type="button"
          >
            {liveFailure ? "Restart workspace" : "Continue to workspace"}
          </button>
        </div>
        <p className="mt-4 text-[0.8rem] text-[#b9e4ff]" role="status">{status}</p>
      </section>
    </main>
  );
}
