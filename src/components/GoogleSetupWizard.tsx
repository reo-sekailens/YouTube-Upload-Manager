import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { googleSetupSteps, setupStepProgress } from "../lib/google-setup";
import { importDesktopOAuthClient, isTauri, openGoogleSetupBrowser } from "../lib/local";
import type { ConnectionSettings } from "../lib/types";

type GoogleSetupWizardProps = {
  onConfigured: (settings: ConnectionSettings) => void;
  onDismiss: () => void;
};

export function GoogleSetupWizard({ onConfigured, onDismiss }: GoogleSetupWizardProps) {
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const current = googleSetupSteps[step];
  const importing = step === googleSetupSteps.length - 1;

  const openSetupPage = async (destination: "account" | "cloud") => {
    setBusy(true);
    setError("");
    try {
      await openGoogleSetupBrowser(destination);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Google setup could not be opened.");
    } finally {
      setBusy(false);
    }
  };

  const importClient = async () => {
    if (!isTauri) return;
    setError("");
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Google Desktop OAuth JSON", extensions: ["json"] }],
    });
    if (typeof selected !== "string") return;
    setBusy(true);
    try {
      onConfigured(await importDesktopOAuthClient(selected));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "The Desktop OAuth JSON could not be imported.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-describedby="google-setup-detail" aria-labelledby="google-setup-heading" aria-modal="true" className="google-setup" role="dialog">
      <p className="eyebrow">FIRST-OPEN SETUP</p>
      <div className="google-setup__heading">
        <div>
          <h2 id="google-setup-heading">Set up your own Google project</h2>
          <p id="google-setup-detail">This guide opens Google in a separate app window for you to complete each account action. Nothing is created automatically.</p>
        </div>
        <span>{setupStepProgress(step)} of {googleSetupSteps.length}</span>
      </div>
      <ol aria-label="Google setup steps" className="google-setup__steps">
        {googleSetupSteps.map((guide, index) => <li className={index === step ? "is-current" : index < step ? "is-complete" : ""} key={guide.title}><span>{index + 1}</span>{guide.title}</li>)}
      </ol>
      <section className="google-setup__content" aria-live="polite">
        <p className="eyebrow">STEP {setupStepProgress(step)}</p>
        <h3>{current.title}</h3>
        <p>{current.detail}</p>
        {step === 0 && <button disabled={!isTauri || busy} onClick={() => void openSetupPage("account")} type="button">Open Google account</button>}
        {step > 0 && step < 5 && <button disabled={!isTauri || busy} onClick={() => void openSetupPage("cloud")} type="button">Open Google Cloud Console</button>}
        {step === 3 && <p className="google-setup__scope-note">Add `youtube.upload` and `youtube.readonly` under Data Access. `youtube.force-ssl` is requested later only if you choose to use video deletion.</p>}
        {importing && <button disabled={!isTauri || busy} onClick={() => void importClient()} type="button">{busy ? "Importing…" : "Choose Desktop OAuth JSON"}</button>}
        {!isTauri && <p className="google-setup__preview-note">Open the signed desktop app to launch Google setup and import the JSON.</p>}
        {error && <p className="google-setup__error" role="alert">{error}</p>}
      </section>
      <footer className="google-setup__actions">
        <button className="secondary-button" disabled={step === 0 || busy} onClick={() => setStep((currentStep) => currentStep - 1)} type="button">Back</button>
        {!importing && <button disabled={busy} onClick={() => setStep((currentStep) => currentStep + 1)} type="button">Continue</button>}
        <button className="text-button" disabled={busy} onClick={onDismiss} type="button">Continue later</button>
      </footer>
    </section>
  );
}
