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
      className="intake-review"
      aria-labelledby="intake-review-heading"
      role="dialog"
      aria-modal="true"
    >
      <p className="eyebrow">REQUIRED BEFORE IMPORT</p>
      <h2 id="intake-review-heading">
        Review {paths.length} video{paths.length === 1 ? "" : "s"}
      </h2>
      <p>
        These choices apply to this drop before the files are copied into this
        device’s managed workspace.
      </p>
      <fieldset>
        <legend>Made for Kids</legend>
        <label>
          <input
            checked={madeForKids === true}
            name="made-for-kids"
            onChange={() => setMadeForKids(true)}
            type="radio"
          />{" "}
          Yes, made for kids
        </label>
        <label>
          <input
            checked={madeForKids === false}
            name="made-for-kids"
            onChange={() => setMadeForKids(false)}
            type="radio"
          />{" "}
          No, not made for kids
        </label>
      </fieldset>
      <label className="intake-review__field">
        Visibility
        <select
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
      <div className="playlist-create" aria-label="Create a new playlist">
        <label className="intake-review__field" htmlFor="intake-new-playlist">
          Create a private playlist
        </label>
        <div>
          <input
            disabled={creatingPlaylist}
            id="intake-new-playlist"
            maxLength={150}
            onChange={(event) => setNewPlaylistTitle(event.target.value)}
            placeholder="Playlist name"
            value={newPlaylistTitle}
          />
          <button
            className="secondary-action"
            disabled={creatingPlaylist || newPlaylistTitle.trim().length === 0}
            onClick={() => void createPlaylist()}
            type="button"
          >
            {creatingPlaylist ? "Creating…" : "Create playlist"}
          </button>
        </div>
      </div>
      <label className="intake-review__field">
        Add to playlist
        <select
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
      <label className="intake-review__field intake-review__source-cleanup">
        <input
          checked={deleteSourceAfterUpload}
          onChange={(event) => setDeleteSourceAfterUpload(event.target.checked)}
          type="checkbox"
        />{" "}
        Delete each original source file only after YouTube confirms its upload.
        The managed app copy is retained.
      </label>
      {playlistError && <p className="intake-review__error">{playlistError}</p>}
      <footer>
        <button className="secondary-action" onClick={onCancel} type="button">
          Cancel drop
        </button>
        <button
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
