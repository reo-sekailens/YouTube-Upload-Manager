import { createContext, useContext } from "react";
import type { ConnectionSettings } from "./types";

/** Native opaque binding identity; it contains no client, project or token material. */
export const ProviderConnectionContext = createContext("");

export function providerConnectionScope(settings?: ConnectionSettings): string {
  return settings?.connected ? settings.connectionRevision ?? settings.activeChannelId ?? "" : "";
}

export function useProviderConnectionScope(): string {
  return useContext(ProviderConnectionContext);
}
