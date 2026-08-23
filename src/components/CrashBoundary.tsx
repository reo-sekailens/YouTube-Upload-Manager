import { Component, lazy, Suspense } from "react";
import type { ErrorInfo, ReactNode } from "react";
import { recordWebviewError } from "../lib/local";

const CrashRecoveryScreen = lazy(() =>
  import("./CrashRecoveryScreen").then((module) => ({
    default: module.CrashRecoveryScreen,
  })),
);

type CrashBoundaryProps = { children: ReactNode };
type CrashBoundaryState = { failed: boolean };

export class CrashBoundary extends Component<
  CrashBoundaryProps,
  CrashBoundaryState
> {
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
        <Suspense fallback={<main className="crash-recovery" role="status" />}>
          <CrashRecoveryScreen
            failureKind="React render error"
            liveFailure
            onContinue={() => window.location.reload()}
          />
        </Suspense>
      );
    }
    return this.props.children;
  }
}
