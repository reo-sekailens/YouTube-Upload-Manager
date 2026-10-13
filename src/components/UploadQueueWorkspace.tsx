import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { QueueTable } from "./QueueTable";
import type { ComponentProps } from "react";

const UploadRetryDialog = lazy(() => import("./UploadRetryDialog").then((module) => ({ default: module.UploadRetryDialog })));

type Props = ComponentProps<typeof QueueTable> & {
  channelId?: string;
  channelName?: string;
  onRefresh: () => Promise<void>;
  onNotice: (message: string) => void;
};

/** Keep retry authority in a lazy, channel-scoped queue surface. */
export default function UploadQueueWorkspace({ channelId, channelName, onRefresh, onNotice, ...props }: Props) {
  const [review, setReview] = useState<{ channelId: string; itemId: string; allMatching: boolean }>();
  const current = useRef(channelId); current.current = channelId;
  useEffect(() => setReview(undefined), [channelId]);
  const open = (itemId: string, allMatching: boolean) => {
    if (channelId) setReview({ channelId, itemId, allMatching });
    else onNotice("Connect this upload’s reviewed channel before retrying.");
  };
  return <>
    <QueueTable {...props} onQueue={(item) => item.status === "draft" ? props.onQueue(item) : open(item.id, false)} onRetryMatching={(item) => open(item.id, true)} />
    <Suspense fallback={<p role="status" className="text-sm text-muted">Opening retry review…</p>}>
      {review && review.channelId === channelId ? <UploadRetryDialog expectedChannelId={review.channelId} anchorItemId={review.itemId} allMatching={review.allMatching} channelName={channelName} onClose={() => setReview(undefined)} onRetried={(receipt) => {
        if (receipt.channelId !== current.current) return;
        void onRefresh().then(() => {
          if (receipt.channelId === current.current) onNotice(`${receipt.queuedItemIds.length} saved upload${receipt.queuedItemIds.length === 1 ? "" : "s"} queued for retry${receipt.blockedItems.length ? `; ${receipt.blockedItems.length} need attention` : ""}.`);
        }).catch(() => { if (receipt.channelId === current.current) onNotice("Retry results are saved. Refresh the queue to see their current progress."); });
      }} /> : null}
    </Suspense>
  </>;
}
