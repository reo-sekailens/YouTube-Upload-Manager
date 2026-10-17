/** Keep background invalidations bounded without discarding an in-flight result. */
export function createFolderMonitorRefreshQueue(
  refresh: () => Promise<void>,
  onFailure: (failure: unknown) => void,
  delayMs = 250,
) {
  let active = true;
  let running = false;
  let pending = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const schedule = () => {
    if (!active || running || timer !== undefined) return;
    // This window starts with the first event. More events cannot postpone it.
    timer = setTimeout(() => { timer = undefined; void flush(); }, delayMs);
  };
  const flush = async () => {
    if (!active || running) return;
    pending = false;
    running = true;
    try { await refresh(); }
    catch (failure) { if (active) onFailure(failure); }
    finally {
      running = false;
      if (pending) schedule();
    }
  };

  return {
    start() { if (active && !running && timer === undefined) void flush(); },
    request() { if (active) { pending = true; schedule(); } },
    dispose() {
      active = false;
      pending = false;
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
    },
  };
}
