import { useCallback, useEffect, useState } from "react";
import {
  isTauri,
  loadDiagnosticReport,
  loadReleaseIdentity,
  type ReleaseIdentity,
} from "../lib/local";

const supportLinks = {
  wiki: "https://github.com/reo-sekailens/YouTube-Upload-Manager/wiki",
  pages: "https://reo-sekailens.github.io/YouTube-Upload-Manager/",
} as const;
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

export function DiagnosticsPanel() {
  const [status, setStatus] = useState("Nothing is sent automatically.");
  const [busy, setBusy] = useState(false);
  const [release, setRelease] = useState<ReleaseIdentity | null>(null);

  useEffect(() => {
    void loadReleaseIdentity().then(setRelease).catch(() => undefined);
  }, []);

  const nightlySuffix = release?.channel.match(/(?:^nightly-|\-nightly\.)([a-z]+|\d+)$/)?.[1];
  const nightlyLabel = nightlySuffix
    ? /^\d+$/.test(nightlySuffix)
      ? `Nightly #${nightlySuffix}`
      : `Nightly build ${nightlySuffix}`
    : release?.channel === "nightly"
      ? "Nightly build"
      : "Regular release";
  const releaseLabel = release
    ? `${release.version === "browser preview" ? "Browser preview" : `v${release.version}`} · ${nightlyLabel}`
    : "Release information loading…";

  const copyIssueReport = useCallback(async () => {
    setBusy(true);
    try {
      const report = await loadDiagnosticReport();
      await copyText(report);
      setStatus("Copied. Paste it into a new GitHub issue.");
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
      issueUrl.searchParams.set("title", "Diagnostics report");
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

  const openSupportLink = useCallback(
    async (destination: keyof typeof supportLinks) => {
      const url = supportLinks[destination];
      if (isTauri) {
        const { openUrl } = await import("@tauri-apps/plugin-opener");
        await openUrl(url);
        return;
      }
      window.open(url, "_blank", "noopener,noreferrer");
    },
    [],
  );

  return (
    <section className="grid gap-4 rounded-xl border border-line bg-surface p-5" aria-labelledby="about-heading">
      <div className="flex items-start justify-between gap-4 max-compact:flex-col max-compact:items-stretch">
        <div>
          <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">ABOUT AND SUPPORT</p>
          <h2 id="about-heading" className="m-0 text-[1.2rem] font-bold tracking-[-0.035em] text-ink">YouTube Upload Manager</h2>
          <p className="mt-1.5 text-[0.78rem] font-bold text-[#315389]" aria-live="polite">
            {releaseLabel}
          </p>
          <p className="mt-2 max-w-[40rem] text-[0.82rem] leading-[1.5] text-muted">
            A local-first desktop app for reviewed YouTube uploads, duplicate
            review, and channel-library maintenance.
          </p>
        </div>
        <span className="self-start whitespace-nowrap rounded-full bg-[#edf7f0] px-2.5 py-1.5 text-[0.71rem] font-bold text-success">Device-local</span>
      </div>

      <div className="grid grid-cols-2 gap-3 max-compact:grid-cols-1">
        <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5">
          <h3 className="m-0 text-[0.86rem] text-[#2d3f5d]">Privacy by design</h3>
          <p className="mt-1.5 text-[0.76rem] leading-[1.5] text-[#65758b]">
            Upload files, queue state, and diagnostic history remain on this
            device. Nothing is sent when you create a report.
          </p>
        </section>
        <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5">
          <h3 className="m-0 text-[0.86rem] text-[#2d3f5d]">What the report includes</h3>
          <p className="mt-1.5 text-[0.76rem] leading-[1.5] text-[#65758b]">
            App and system details, saved operational warnings and errors, and
            any safely captured app-crash marker. Secrets, access tokens,
            account identifiers, file paths, and provider payloads are omitted.
          </p>
        </section>
      </div>

      <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5" aria-labelledby="support-actions-heading">
        <div>
          <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">SUPPORT ACTIONS</p>
          <h3 id="support-actions-heading" className="m-0 text-[0.86rem] text-[#2d3f5d]">Learn, troubleshoot, and report</h3>
          <p className="mt-1.5 text-[0.76rem] leading-[1.5] text-[#65758b]">Documentation opens in your default browser. Diagnostics stay on this device until you copy them.</p>
        </div>
        <div className="mt-3.5 grid grid-cols-4 gap-2.5 max-compact:grid-cols-1">
          <button className="grid min-h-20 cursor-pointer items-start gap-1 rounded-md border border-[#cdd4df] bg-white p-3 text-left transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50 [&>strong]:text-[0.78rem] [&>strong]:text-[#2d3f5d] [&>span]:text-[0.7rem] [&>span]:leading-[1.38] [&>span]:text-[#65758b]" onClick={() => void openSupportLink("wiki")} type="button">
            <strong>GitHub Wiki</strong>
            <span>Setup, recovery, and security guides</span>
          </button>
          <button className="grid min-h-20 cursor-pointer items-start gap-1 rounded-md border border-[#cdd4df] bg-white p-3 text-left transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50 [&>strong]:text-[0.78rem] [&>strong]:text-[#2d3f5d] [&>span]:text-[0.7rem] [&>span]:leading-[1.38] [&>span]:text-[#65758b]" onClick={() => void openSupportLink("pages")} type="button">
            <strong>Project website</strong>
            <span>Overview and release information</span>
          </button>
          <button
            aria-busy={busy}
            className="grid min-h-20 cursor-pointer items-start gap-1 rounded-md border border-[#cdd4df] bg-white p-3 text-left transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50 [&>strong]:text-[0.78rem] [&>strong]:text-[#2d3f5d] [&>span]:text-[0.7rem] [&>span]:leading-[1.38] [&>span]:text-[#65758b]"
            disabled={busy}
            onClick={() => void copyIssueReport()}
            type="button"
          >
            <strong>{busy ? "Preparing report…" : "Copy issue report"}</strong>
            <span>GitHub-ready Markdown with safe diagnostics</span>
          </button>
          <button
            aria-busy={busy}
            className="grid min-h-20 cursor-pointer items-start gap-1 rounded-md border border-[#cdd4df] bg-white p-3 text-left transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50 [&>strong]:text-[0.78rem] [&>strong]:text-[#2d3f5d] [&>span]:text-[0.7rem] [&>span]:leading-[1.38] [&>span]:text-[#65758b]"
            disabled={busy}
            onClick={() => void openPrefilledIssue()}
            type="button"
          >
            <strong>{busy ? "Preparing report…" : "Report to GitHub"}</strong>
            <span>Opens a new issue with the full report already filled in</span>
          </button>
        </div>
      </section>
      <p className="m-0 rounded-md border border-[#d7e4ff] bg-[#f0f5ff] px-2.5 py-2 text-[0.76rem] leading-[1.5] text-[#315389]" role="status">
        {status}
      </p>
    </section>
  );
}
