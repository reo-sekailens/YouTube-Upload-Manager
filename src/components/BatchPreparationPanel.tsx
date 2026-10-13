import { ManualUploadDefaultsPanel } from "./ManualUploadDefaultsPanel";
import { BatchWorkflowTools } from "./BatchWorkflowTools";
import type { ConnectionSettings, UploadItem } from "../lib/types";

export default function BatchPreparationPanel({ items, channelId, channelName, busy, onRefresh, onNotice, onConnectionChanged }: {
  items: UploadItem[]; channelId?: string; channelName?: string; busy: boolean;
  onRefresh: () => Promise<void>; onNotice: (message: string) => void;
  onConnectionChanged?: (settings?: ConnectionSettings) => void | Promise<void>;
}) {
  return <>
    <ManualUploadDefaultsPanel activeChannelId={channelId} activeChannel={channelName} />
    <BatchWorkflowTools items={items} channelId={channelId} channelName={channelName} busy={busy} onRefresh={onRefresh} onNotice={onNotice} onConnectionChanged={onConnectionChanged} />
  </>;
}
