import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ProviderConnectionContext, providerConnectionScope, useProviderConnectionScope } from "./provider-connection";
import type { ConnectionSettings } from "./types";

const connected: ConnectionSettings = { connected: true, activeChannelId: "channel-a", connectionRevision: "binding-one" };

describe("provider connection identity", () => {
  it("changes for a same-channel credential replacement", () => {
    expect(providerConnectionScope(connected)).not.toBe(providerConnectionScope({ ...connected, connectionRevision: "binding-two" }));
  });

  it("retains identity through routine settings polling and permission refreshes", () => {
    expect(providerConnectionScope(connected)).toBe(providerConnectionScope({ ...connected, activeChannel: "Renamed channel", detail: "Connected", deletionAuthorized: true, deletionSudoActive: true }));
  });

  it("distinguishes channel switches and disconnects without requiring a revision", () => {
    const legacy = { ...connected, connectionRevision: undefined };
    expect(providerConnectionScope(legacy)).not.toBe(providerConnectionScope({ ...legacy, activeChannelId: "channel-b" }));
    expect(providerConnectionScope(legacy)).not.toBe(providerConnectionScope({ ...legacy, connected: false }));
    expect(providerConnectionScope()).toBe(providerConnectionScope({ connected: false }));
  });

  it("shares the current connection scope with nested provider workspaces", () => {
    function Workspace() { return <output>{useProviderConnectionScope()}</output>; }
    const markup = renderToStaticMarkup(<ProviderConnectionContext.Provider value="current-binding"><Workspace /></ProviderConnectionContext.Provider>);
    expect(markup).toContain("current-binding");
  });
});
