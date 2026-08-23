import { useState } from "react";
import { ArrowLeft, ArrowRight, Check, Clock3, ExternalLink } from "lucide-react";
import { googleSetupSteps, setupStepProgress } from "../lib/google-setup";
import { isTauri, openGoogleSetupBrowser } from "../lib/local";

type GoogleSetupWizardProps = {
  onOpenConnectedAccount: () => void;
  onDismiss: () => void;
};

export function GoogleSetupWizard({ onOpenConnectedAccount, onDismiss }: GoogleSetupWizardProps) {
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

  return (
    <section aria-describedby="google-setup-detail" aria-labelledby="google-setup-heading" aria-modal="true" className="fixed top-1/2 left-1/2 z-20 grid w-[calc(100%-2rem)] max-w-[48rem] -translate-x-1/2 -translate-y-1/2 gap-3.5 rounded-xl border border-[#bfcfe7] bg-white p-5 shadow-[0_20px_52px_rgba(27,50,83,0.22)] max-compact:max-h-[calc(100vh-1.25rem)] max-compact:overflow-y-auto max-compact:p-4" role="dialog">
      <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">FIRST-OPEN SETUP</p>
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 id="google-setup-heading" className="m-0 text-[1.22rem] tracking-[-0.025em] text-[#1e2d48]">Set up your own Google project</h2>
          <p id="google-setup-detail" className="mt-1.5 text-[0.79rem] leading-[1.5] text-[#617086]">This guide opens Google in a separate app window for you to complete each account action. Nothing is created automatically.</p>
        </div>
        <span className="shrink-0 whitespace-nowrap rounded-full bg-[#edf3ff] px-2.5 py-1.5 text-[0.71rem] font-bold text-[#2b63bc]">{setupStepProgress(step)} of {googleSetupSteps.length}</span>
      </div>
      <ol aria-label="Google setup steps" className="grid grid-cols-3 gap-x-3 gap-y-1.5 p-0 max-compact:grid-cols-2">
        {googleSetupSteps.map((guide, index) => <li className={index === step ? "flex min-w-0 items-center gap-1.5 text-[0.68rem] font-semibold text-[#2859b1]" : index < step ? "flex min-w-0 items-center gap-1.5 text-[0.68rem] font-semibold text-[#37755a]" : "flex min-w-0 items-center gap-1.5 text-[0.68rem] font-semibold text-[#7a8799]"} key={guide.title}><span className={index === step ? "inline-flex size-[1.15rem] shrink-0 items-center justify-center rounded-full bg-brand text-[0.62rem] text-white" : index < step ? "inline-flex size-[1.15rem] shrink-0 items-center justify-center rounded-full bg-[#dff1e7] text-[0.62rem] text-success" : "inline-flex size-[1.15rem] shrink-0 items-center justify-center rounded-full bg-[#e8ecf1] text-[0.62rem] text-[#68748a]"}>{index < step ? <Check aria-hidden="true" className="size-3" strokeWidth={3} /> : index + 1}</span>{guide.title}</li>)}
      </ol>
      <section className="grid gap-2.5 rounded-lg border border-[#dce5f1] bg-[#f7f9fd] px-4 py-3.5 [&>h3]:m-0 [&>h3]:text-[1rem] [&>h3]:tracking-[-0.025em] [&>h3]:text-[#1e2d48] [&>p]:mt-0 [&>p]:text-[0.79rem] [&>p]:leading-[1.5] [&>p]:text-[#617086]" aria-live="polite">
        <p className="-mb-0.5 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">STEP {setupStepProgress(step)}</p>
        <h3>{current.title}</h3>
        <p>{current.detail}</p>
        {step === 0 && <button className="inline-flex cursor-pointer items-center gap-1.5 justify-self-start rounded-md border border-brand bg-brand px-3 py-2 text-[0.77rem] font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50 max-compact:w-full max-compact:justify-center" disabled={!isTauri || busy} onClick={() => void openSetupPage("account")} type="button"><ExternalLink aria-hidden="true" className="size-3.5" />Open Google account</button>}
        {step > 0 && step < 5 && <button className="inline-flex cursor-pointer items-center gap-1.5 justify-self-start rounded-md border border-brand bg-brand px-3 py-2 text-[0.77rem] font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50 max-compact:w-full max-compact:justify-center" disabled={!isTauri || busy} onClick={() => void openSetupPage("cloud")} type="button"><ExternalLink aria-hidden="true" className="size-3.5" />Open Google Cloud Console</button>}
        {step === 3 && <p className="rounded-md border border-[#d4e3fa] bg-[#eef5ff] px-2.5 py-2 text-[0.72rem]! text-[#42618a]!">Add `youtube.upload`, `youtube.readonly`, and `youtube.force-ssl` under Data Access. The management scope is used for operator-created private playlists and explicit video deletion.</p>}
        {importing && <button className="inline-flex cursor-pointer items-center gap-1.5 justify-self-start rounded-md border border-brand bg-brand px-3 py-2 text-[0.77rem] font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50 max-compact:w-full max-compact:justify-center" disabled={busy} onClick={onOpenConnectedAccount} type="button"><ArrowRight aria-hidden="true" className="size-3.5" />Open Connected account</button>}
        {!isTauri && <p className="text-[0.72rem]! text-[#85651c]!">Open Connected account in the signed desktop app to import the JSON.</p>}
        {error && <p className="font-semibold text-danger!" role="alert">{error}</p>}
      </section>
      <footer className="flex flex-wrap items-center justify-end gap-2 max-compact:flex-col-reverse max-compact:items-stretch max-compact:[&_button]:w-full">
        <button className="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-[#cdd4df] bg-white px-3 py-2 text-[0.77rem] font-semibold text-[#34405a] disabled:cursor-not-allowed disabled:opacity-50" disabled={step === 0 || busy} onClick={() => setStep((currentStep) => currentStep - 1)} type="button"><ArrowLeft aria-hidden="true" className="size-3.5" />Back</button>
        {!importing && <button className="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-brand bg-brand px-3 py-2 text-[0.77rem] font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50" disabled={busy} onClick={() => setStep((currentStep) => currentStep + 1)} type="button">Continue<ArrowRight aria-hidden="true" className="size-3.5" /></button>}
        <button className="mr-auto inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-[#cdd4df] bg-white px-3 py-2 text-[0.77rem] font-semibold text-[#34405a] disabled:cursor-not-allowed disabled:opacity-50 max-compact:mr-0" disabled={busy} onClick={onDismiss} type="button"><Clock3 aria-hidden="true" className="size-3.5" />Continue later</button>
      </footer>
    </section>
  );
}
