import { useEffect, useState } from "react";
import {
  createYouTubePlaylist,
  listYouTubePlaylists,
  loadManualUploadDefaults,
} from "../lib/local";
import type {
  ManualUploadSettings,
  UploadVisibility,
  YouTubePlaylist,
} from "../lib/types";

type UploadIntakeReviewProps = {
  paths: string[];
  onCancel: () => void;
  onConfirm: (settings: ManualUploadSettings) => void;
};

export function UploadIntakeReview({
  paths,
  onCancel,
  onConfirm,
}: UploadIntakeReviewProps) {
  const [madeForKids, setMadeForKids] = useState<boolean>();
  const [visibility, setVisibility] = useState<UploadVisibility>("private");
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistId, setPlaylistId] = useState("");
  const [newPlaylistTitle, setNewPlaylistTitle] = useState("");
  const [creatingPlaylist, setCreatingPlaylist] = useState(false);
  const [deleteSourceAfterUpload, setDeleteSourceAfterUpload] = useState(false);
  const [playlistError, setPlaylistError] = useState("");

  useEffect(() => {
    let active = true;
    void listYouTubePlaylists()
      .then((items) => {
        if (active) setPlaylists(items);
      })
      .catch(() => {
        if (active)
          setPlaylistError(
            "Playlists could not be loaded. You can continue without one.",
          );
      });
    return () => {
      active = false;
    };
  }, []);
  useEffect(() => {
    void loadManualUploadDefaults().then((defaults) =>
      setMadeForKids(defaults.madeForKids),
    );
  }, []);

  const selectedPlaylist = playlists.find(
    (playlist) => playlist.id === playlistId,
  );
  const createPlaylist = async () => {
    const title = newPlaylistTitle.trim();
    if (!title) {
      setPlaylistError("Enter a playlist name first.");
      return;
    }
    setCreatingPlaylist(true);
    setPlaylistError("");
    try {
      const playlist = await createYouTubePlaylist(title);
      setPlaylists((current) =>
        [...current, playlist].sort((left, right) => left.title.localeCompare(right.title)),
      );
      setPlaylistId(playlist.id);
      setNewPlaylistTitle("");
    } catch (error) {
      setPlaylistError(
        error instanceof Error ? error.message : "The playlist could not be created.",
      );
    } finally {
      setCreatingPlaylist(false);
    }
  };
  return (
    <section
      className="fixed top-1/2 left-1/2 z-10 grid w-[calc(100%-2rem)] max-w-[34rem] -translate-x-1/2 -translate-y-1/2 gap-3.5 rounded-xl border border-[#bfcfe7] bg-white p-5 shadow-[0_18px_42px_rgba(27,50,83,0.18)]"
      aria-labelledby="intake-review-heading"
      role="dialog"
      aria-modal="true"
    >
      <p className="mb-0 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">REQUIRED BEFORE IMPORT</p>
      <h2 className="-mt-1.5 mb-0 text-[1.15rem] text-[#1e2d48]" id="intake-review-heading">
        Review {paths.length} video{paths.length === 1 ? "" : "s"}
      </h2>
      <p className="-mt-1 mb-0 text-[0.78rem] leading-snug text-[#617086]">
        These choices apply to this drop before the original files are
        referenced by this device-local queue.
      </p>
      <fieldset className="grid gap-2 rounded-lg border border-[#dce3ed] p-3">
        <legend className="px-1 text-[0.78rem] font-bold text-[#344a67]">Made for Kids</legend>
        <label className="flex items-center gap-2 text-[0.78rem] text-[#4e5f78]">
          <input className="accent-[#2463df]"
            checked={madeForKids === true}
            name="made-for-kids"
            onChange={() => setMadeForKids(true)}
            type="radio"
          />{" "}
          Yes, made for kids
        </label>
        <label className="flex items-center gap-2 text-[0.78rem] text-[#4e5f78]">
          <input className="accent-[#2463df]"
            checked={madeForKids === false}
            name="made-for-kids"
            onChange={() => setMadeForKids(false)}
            type="radio"
          />{" "}
          No, not made for kids
        </label>
      </fieldset>
      <label className="grid gap-1 text-[0.78rem] font-bold text-[#344a67]">
        Visibility
        <select className="rounded-md border border-[#ccd6e4] bg-white px-2.5 py-2 text-[#2d3f5d] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
          onChange={(event) =>
            setVisibility(event.target.value as UploadVisibility)
          }
          value={visibility}
        >
          <option value="private">Private</option>
          <option value="unlisted">Unlisted</option>
          <option value="public">Public</option>
        </select>
      </label>
      <div className="grid gap-1 rounded-lg border border-[#dce3ed] bg-[#f7f9fc] p-3" aria-label="Create a new playlist">
        <label className="grid gap-1 text-[0.78rem] font-bold text-[#344a67]" htmlFor="intake-new-playlist">
          Create a private playlist
        </label>
        <div className="flex gap-2 max-sm:flex-col">
          <input className="min-w-0 flex-1 rounded-md border border-[#ccd6e4] bg-white px-2.5 py-2 text-[#2d3f5d] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
            disabled={creatingPlaylist}
            id="intake-new-playlist"
            maxLength={150}
            onChange={(event) => setNewPlaylistTitle(event.target.value)}
            placeholder="Playlist name"
            value={newPlaylistTitle}
          />
          <button
            className="shrink-0 rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50 max-sm:w-full"
            disabled={creatingPlaylist || newPlaylistTitle.trim().length === 0}
            onClick={() => void createPlaylist()}
            type="button"
          >
            {creatingPlaylist ? "Creating…" : "Create playlist"}
          </button>
        </div>
      </div>
      <label className="grid gap-1 text-[0.78rem] font-bold text-[#344a67]">
        Add to playlist
        <select className="rounded-md border border-[#ccd6e4] bg-white px-2.5 py-2 text-[#2d3f5d] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
          onChange={(event) => setPlaylistId(event.target.value)}
          value={playlistId}
        >
          <option value="">No playlist</option>
          {playlists.map((playlist) => (
            <option key={playlist.id} value={playlist.id}>
              {playlist.title}
            </option>
          ))}
        </select>
      </label>
      <label className="flex items-start gap-1.5 text-[0.78rem] leading-snug font-semibold text-[#344a67]">
        <input className="mt-0.5 shrink-0 accent-[#2463df]"
          checked={deleteSourceAfterUpload}
          onChange={(event) => setDeleteSourceAfterUpload(event.target.checked)}
          type="checkbox"
        />{" "}
        Delete each original source file only after YouTube confirms its upload.
        No app-managed copy is created.
      </label>
      {playlistError && <p className="m-0 text-[0.72rem] text-[#a4413b]">{playlistError}</p>}
      <footer className="flex flex-wrap justify-end gap-2">
        <button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] hover:bg-[#f0f3f7]" onClick={onCancel} type="button">
          Cancel drop
        </button>
        <button
          className="rounded-md border border-[#2463df] bg-[#2463df] px-3 py-2 text-[0.79rem] font-[680] text-white hover:border-[#1b54c6] hover:bg-[#1b54c6] disabled:cursor-not-allowed disabled:opacity-50"
          disabled={madeForKids === undefined}
          onClick={() =>
            onConfirm({
              madeForKids: madeForKids!,
              visibility,
              playlistId: selectedPlaylist?.id,
              playlistTitle: selectedPlaylist?.title,
              deleteSourceAfterUpload,
            })
          }
          type="button"
        >
          Import reviewed videos
        </button>
      </footer>
    </section>
  );
}
