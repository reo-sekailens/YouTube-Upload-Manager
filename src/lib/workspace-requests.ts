import { useEffect, useRef } from "react";
import type { ConnectionSettings } from "./types";

/** A late read or completion must never update a different workspace scope. */
export function createWorkspaceRequestFence(initialScope?: string) {
  let scope = initialScope;
  let generation = 0;
  return {
    setScope(nextScope?: string) {
      if (scope !== nextScope) {
        scope = nextScope;
        generation += 1;
      }
    },
    invalidate() { generation += 1; },
    begin() {
      const requestScope = scope;
      const requestGeneration = ++generation;
      return () => Boolean(requestScope) && scope === requestScope && generation === requestGeneration;
    },
  };
}

export function useWorkspaceRequestFence(scope?: string) {
  const fence = useRef<ReturnType<typeof createWorkspaceRequestFence> | null>(null);
  if (!fence.current) fence.current = createWorkspaceRequestFence(scope);
  fence.current.setScope(scope);
  const current = fence.current;
  useEffect(() => () => current.invalidate(), [current]);
  return current;
}

/** Native command failures may be strings rather than Error instances. */
export function workspaceErrorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (typeof error === "object" && error && "message" in error && typeof error.message === "string" && error.message.trim()) return error.message;
  return fallback;
}

export function assertWorkspaceChannel(settings: ConnectionSettings, channelId?: string) {
  if (!channelId || !settings.connected || settings.activeChannelId !== channelId)
    throw new Error("The connected channel changed. Review this channel's library before continuing.");
}
