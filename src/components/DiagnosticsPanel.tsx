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

  const nightlySuffix = release?.channel.match(/(?:^nightly-|\-nightly\.)([a-z]+)$/)?.[1];
  const releaseLabel = release
    ? `${release.version === "browser preview" ? "Browser preview" : `v${release.version}`} · ${nightlySuffix ? `Nightly build ${nightlySuffix}` : release.channel === "nightly" ? "Nightly build" : "Regular release"}`
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
    <section className="panel diagnostics-panel" aria-labelledby="about-heading">
      <div className="section-heading diagnostics-panel__heading">
        <div>
          <p className="eyebrow">ABOUT AND SUPPORT</p>
          <h2 id="about-heading">YouTube Upload Manager</h2>
          <p className="diagnostics-panel__release" aria-live="polite">
            {releaseLabel}
          </p>
          <p className="section-copy">
            A local-first desktop app for reviewed YouTube uploads, duplicate
            review, and channel-library maintenance.
          </p>
        </div>
        <span className="diagnostics-panel__local">Device-local</span>
      </div>

      <div className="diagnostics-panel__grid">
        <section>
          <h3>Privacy by design</h3>
          <p>
            Upload files, queue state, and diagnostic history remain on this
            device. Nothing is sent when you create a report.
          </p>
        </section>
        <section>
          <h3>What the report includes</h3>
          <p>
            App and system details, saved operational warnings and errors, and
            any safely captured app-crash marker. Secrets, access tokens,
            account identifiers, file paths, and provider payloads are omitted.
          </p>
        </section>
      </div>

      <section className="diagnostics-panel__support" aria-labelledby="support-actions-heading">
        <div>
          <p className="eyebrow">SUPPORT ACTIONS</p>
          <h3 id="support-actions-heading">Learn, troubleshoot, and report</h3>
          <p>Documentation opens in your default browser. Diagnostics stay on this device until you copy them.</p>
        </div>
        <div className="diagnostics-panel__support-actions">
          <button className="secondary-action" onClick={() => void openSupportLink("wiki")} type="button">
            <strong>GitHub Wiki</strong>
            <span>Setup, recovery, and security guides</span>
          </button>
          <button className="secondary-action" onClick={() => void openSupportLink("pages")} type="button">
            <strong>Project website</strong>
            <span>Overview and release information</span>
          </button>
          <button
            aria-busy={busy}
            className="secondary-action"
            disabled={busy}
            onClick={() => void copyIssueReport()}
            type="button"
          >
            <strong>{busy ? "Preparing report…" : "Copy issue report"}</strong>
            <span>GitHub-ready Markdown with safe diagnostics</span>
          </button>
          <button
            aria-busy={busy}
            className="secondary-action"
            disabled={busy}
            onClick={() => void openPrefilledIssue()}
            type="button"
          >
            <strong>{busy ? "Preparing report…" : "Report to GitHub"}</strong>
            <span>Opens a new issue with the full report already filled in</span>
          </button>
        </div>
      </section>
      <p className="diagnostics-panel__status" role="status">
        {status}
      </p>
    </section>
  );
}
