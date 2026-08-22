import { useEffect, useState } from "react";
import { listYouTubePlaylists, loadManualUploadDefaults } from "../lib/local";
import type { ManualUploadSettings, UploadVisibility, YouTubePlaylist } from "../lib/types";

type UploadIntakeReviewProps = {
  paths: string[];
  onCancel: () => void;
  onConfirm: (settings: ManualUploadSettings) => void;
};

export function UploadIntakeReview({ paths, onCancel, onConfirm }: UploadIntakeReviewProps) {
  const [madeForKids, setMadeForKids] = useState<boolean>();
  const [visibility, setVisibility] = useState<UploadVisibility>("private");
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistId, setPlaylistId] = useState("");
  const [playlistError, setPlaylistError] = useState("");

  useEffect(() => {
    let active = true;
    void listYouTubePlaylists().then((items) => { if (active) setPlaylists(items); }).catch(() => { if (active) setPlaylistError("Playlists could not be loaded. You can continue without one."); });
    return () => { active = false; };
  }, []);
  useEffect(() => { void loadManualUploadDefaults().then((defaults) => setMadeForKids(defaults.madeForKids)); }, []);

  const selectedPlaylist = playlists.find((playlist) => playlist.id === playlistId);
  return <section className="intake-review" aria-labelledby="intake-review-heading" role="dialog" aria-modal="true">
    <p className="eyebrow">REQUIRED BEFORE IMPORT</p>
    <h2 id="intake-review-heading">Review {paths.length} video{paths.length === 1 ? "" : "s"}</h2>
    <p>These choices apply to this drop before the files are copied into this device’s managed workspace.</p>
    <fieldset><legend>Made for Kids</legend><label><input checked={madeForKids === true} name="made-for-kids" onChange={() => setMadeForKids(true)} type="radio" /> Yes, made for kids</label><label><input checked={madeForKids === false} name="made-for-kids" onChange={() => setMadeForKids(false)} type="radio" /> No, not made for kids</label></fieldset>
    <label className="intake-review__field">Visibility<select onChange={(event) => setVisibility(event.target.value as UploadVisibility)} value={visibility}><option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option></select></label>
    <label className="intake-review__field">Add to playlist<select onChange={(event) => setPlaylistId(event.target.value)} value={playlistId}><option value="">No playlist</option>{playlists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label>
    {playlistError && <p className="intake-review__error">{playlistError}</p>}
    <footer><button className="secondary-action" onClick={onCancel} type="button">Cancel drop</button><button disabled={madeForKids === undefined} onClick={() => onConfirm({ madeForKids: madeForKids!, visibility, playlistId: selectedPlaylist?.id, playlistTitle: selectedPlaylist?.title })} type="button">Import reviewed videos</button></footer>
  </section>;
}
