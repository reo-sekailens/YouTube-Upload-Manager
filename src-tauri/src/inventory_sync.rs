//! Bounded inventory traversal. Recent reads only add/update evidence; they never
//! establish that an older video was removed from the channel.
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const PAGE_SIZE: usize = 50;
const MAX_PAGES: usize = 10_000;
const MAX_VIDEOS: usize = 100_000;
const MAX_VIDEO_BYTES: usize = 65_536;
const MAX_SNAPSHOT_BYTES: usize = 64 * 1_024 * 1_024;
const VIDEO_FIELDS: &str = "items(id,snippet(channelId,title),contentDetails/duration,status(privacyStatus,uploadStatus),processingDetails/processingStatus)";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshMode {
    Full,
    Recent,
}

#[derive(Debug)]
pub struct Snapshot {
    pub uploads_playlist: String,
    pub channel_name: String,
    pub videos: Vec<Value>,
    pub complete: bool,
}

fn text<'a>(value: &'a Value, path: &str, max: usize) -> Result<&'a str, String> {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "YouTube returned incomplete or invalid inventory metadata.".into())
}

fn identity(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("YouTube returned an invalid inventory identity.".into());
    }
    Ok(value)
}

fn items(value: &Value) -> Result<&Vec<Value>, String> {
    value
        .get("items")
        .and_then(Value::as_array)
        .filter(|items| items.len() <= PAGE_SIZE)
        .ok_or_else(|| "YouTube returned an invalid inventory page.".into())
}

fn next_page(value: &Value) -> Result<Option<String>, String> {
    match value.get("nextPageToken") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(token))
            if !token.is_empty() && token.len() <= 4096 && !token.chars().any(char::is_control) =>
        {
            Ok(Some(token.clone()))
        }
        _ => Err("YouTube returned an invalid inventory page token.".into()),
    }
}

/// `cached_identity` must originate from a previous authenticated read for
/// `expected_channel`. The caller retains credential/channel fencing for the
/// entire traversal. Full mode always rechecks `channels.mine`; Recent may
/// reuse that scoped identity and stops after the first known-upload page.
pub fn read(
    expected_channel: &str,
    cached_identity: Option<(&str, &str)>,
    known_titles: &HashMap<String, String>,
    mode: RefreshMode,
    mut fetch: impl FnMut(&str, &[(&str, &str)]) -> Result<Value, String>,
) -> Result<Snapshot, String> {
    identity(expected_channel)?;
    let (uploads_playlist, channel_name) = match (mode, cached_identity) {
        (RefreshMode::Recent, Some((playlist, name))) => {
            identity(playlist)?;
            if name.trim().is_empty() || name.len() > 1024 || name.chars().any(char::is_control) {
                return Err("The cached YouTube channel identity is invalid.".into());
            }
            (playlist.to_owned(), name.to_owned())
        }
        _ => {
            let response = fetch(
                "channels",
                &[
                    ("part", "contentDetails,snippet"),
                    ("mine", "true"),
                    (
                        "fields",
                        "items(id,snippet/title,contentDetails/relatedPlaylists/uploads)",
                    ),
                ],
            )?;
            let channels = items(&response)?;
            if channels.len() != 1 {
                return Err("YouTube did not return one selected channel identity.".into());
            }
            let channel = &channels[0];
            if identity(text(channel, "/id", 256)?)? != expected_channel {
                return Err("The protected YouTube credential no longer matches the selected channel. Reconnect that account before refreshing its library.".into());
            }
            (
                identity(text(
                    channel,
                    "/contentDetails/relatedPlaylists/uploads",
                    256,
                )?)?
                .to_owned(),
                text(channel, "/snippet/title", 1024)?.to_owned(),
            )
        }
    };
    let mut videos = Vec::new();
    let mut snapshot_bytes = 0_usize;
    let mut seen_ids = HashSet::<String>::new();
    let mut seen_tokens = HashSet::<String>::new();
    let mut page_token: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut query = vec![
            ("part", "contentDetails,snippet"),
            ("playlistId", uploads_playlist.as_str()),
            ("maxResults", "50"),
            (
                "fields",
                "nextPageToken,items(contentDetails/videoId,snippet(title,channelId,videoOwnerChannelId))",
            ),
        ];
        if let Some(token) = page_token.as_deref() {
            query.push(("pageToken", token));
        }
        let playlist = fetch("playlistItems", &query)?;
        let page = items(&playlist)?;
        let next = next_page(&playlist)?;
        // Check the token even when Recent would stop on this page. A malformed
        // provider response must not produce apparently valid sync evidence.
        if let Some(token) = next.as_ref() {
            if !seen_tokens.insert(token.clone()) {
                return Err(
                    "YouTube repeated an inventory page token; the previous library was retained."
                        .into(),
                );
            }
            if page.is_empty() {
                return Err("YouTube returned an empty inventory page with more pages.".into());
            }
        }
        let mut requested = Vec::<String>::new();
        let mut found_known = false;
        for item in page {
            let id = identity(text(item, "/contentDetails/videoId", 256)?)?;
            let title = text(item, "/snippet/title", 1024)?;
            for path in ["/snippet/channelId", "/snippet/videoOwnerChannelId"] {
                if item.pointer(path).is_some()
                    && identity(text(item, path, 256)?)? != expected_channel
                {
                    return Err("YouTube returned a video belonging to another channel; the previous library was retained.".into());
                }
            }
            if !seen_ids.insert(id.to_owned()) {
                return Err("YouTube repeated a video in its upload inventory; the previous library was retained.".into());
            }
            if seen_ids.len() > MAX_VIDEOS {
                return Err("YouTube inventory exceeded the video limit; the previous library was retained.".into());
            }
            let known_title = known_titles.get(id);
            found_known |= known_title.is_some();
            if mode == RefreshMode::Full || known_title.is_none_or(|known| known != title) {
                requested.push(id.to_owned());
            }
        }
        if !requested.is_empty() {
            let joined = requested.join(",");
            let response = fetch(
                "videos",
                &[
                    ("part", "snippet,contentDetails,status,processingDetails"),
                    ("id", joined.as_str()),
                    ("maxResults", "50"),
                    ("fields", VIDEO_FIELDS),
                ],
            )?;
            let mut returned = HashSet::new();
            for video in items(&response)? {
                let id = identity(text(video, "/id", 256)?)?;
                if !requested.iter().any(|requested_id| requested_id == id) || !returned.insert(id)
                {
                    return Err("YouTube returned an unexpected inventory video; the previous library was retained.".into());
                }
                if identity(text(video, "/snippet/channelId", 256)?)? != expected_channel {
                    return Err("YouTube returned a video belonging to another channel; the previous library was retained.".into());
                }
                text(video, "/snippet/title", 1024)?;
                for (path, max) in [
                    ("/contentDetails/duration", 128),
                    ("/status/privacyStatus", 32),
                    ("/status/uploadStatus", 32),
                    ("/processingDetails/processingStatus", 32),
                ] {
                    if video.pointer(path).is_some() {
                        text(video, path, max)?;
                    }
                }
                let bytes = serde_json::to_vec(video)
                    .map_err(|_| "YouTube returned invalid inventory video metadata.".to_string())?
                    .len();
                snapshot_bytes = snapshot_bytes.saturating_add(bytes);
                if bytes > MAX_VIDEO_BYTES || snapshot_bytes > MAX_SNAPSHOT_BYTES {
                    return Err("YouTube inventory metadata exceeded the local safety limit; the previous library was retained.".into());
                }
                videos.push(video.clone());
            }
        }
        if (mode == RefreshMode::Recent && found_known) || next.is_none() {
            return Ok(Snapshot {
                uploads_playlist,
                channel_name,
                videos,
                complete: mode == RefreshMode::Full,
            });
        }
        page_token = next;
    }
    Err("YouTube inventory exceeded the page limit; the previous library was retained.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn channel() -> Value {
        json!({"items":[{"id":"channel-a","snippet":{"title":"Channel A"},"contentDetails":{"relatedPlaylists":{"uploads":"uploads-a"}}}]})
    }

    fn playlist_item(id: &str, title: &str) -> Value {
        json!({"contentDetails":{"videoId":id},"snippet":{"title":title,"channelId":"channel-a","videoOwnerChannelId":"channel-a"}})
    }

    fn video(id: &str, title: &str) -> Value {
        json!({"id":id,"snippet":{"channelId":"channel-a","title":title},"contentDetails":{"duration":"PT5M"},"status":{"privacyStatus":"private","uploadStatus":"processed"},"processingDetails":{"processingStatus":"succeeded"}})
    }

    fn query<'a>(query: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
        query
            .iter()
            .find_map(|(name, value)| (*name == key).then_some(*value))
    }

    #[test]
    fn full_5000_uses_201_queries_and_refreshes_identity() {
        let mut calls = 0;
        let snapshot = read(
            "channel-a",
            Some(("wrong-cached-playlist", "Stale channel name")),
            &HashMap::new(),
            RefreshMode::Full,
            |endpoint, params| {
                calls += 1;
                assert!(query(params, "fields").is_some());
                match endpoint {
                    "channels" => Ok(channel()),
                    "playlistItems" => {
                        assert_eq!(query(params, "playlistId"), Some("uploads-a"));
                        let page = query(params, "pageToken").unwrap_or("0").parse::<usize>().unwrap();
                        let mut response = json!({"items":(page*50..page*50+50).map(|index| playlist_item(&format!("video-{index}"), &format!("Video {index}"))).collect::<Vec<_>>()});
                        if page < 99 {
                            response["nextPageToken"] = json!((page + 1).to_string());
                        }
                        Ok(response)
                    }
                    "videos" => Ok(json!({"items":query(params,"id").unwrap().split(',').map(|id|video(id,&format!("Video {}",id.strip_prefix("video-").unwrap()))).collect::<Vec<_>>()})),
                    _ => panic!("Unexpected endpoint"),
                }
            },
        ).unwrap();
        assert_eq!(calls, 201);
        assert_eq!(snapshot.videos.len(), 5000);
        assert_eq!(snapshot.uploads_playlist, "uploads-a");
        assert_eq!(snapshot.channel_name, "Channel A");
        assert!(snapshot.complete);
    }

    #[test]
    fn unchanged_recent_uses_one_query_and_cannot_claim_deletions() {
        let known = HashMap::from([
            ("video-a".into(), "Known".into()),
            ("old-video".into(), "Older".into()),
        ]);
        let mut calls = 0;
        let snapshot = read(
            "channel-a",
            Some(("uploads-a", "Channel A")),
            &known,
            RefreshMode::Recent,
            |endpoint, _| {
                calls += 1;
                assert_eq!(endpoint, "playlistItems");
                Ok(json!({"items":[playlist_item("video-a","Known")],"nextPageToken":"older-page"}))
            },
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert!(snapshot.videos.is_empty());
        assert!(!snapshot.complete);
        assert!(known.contains_key("old-video"));
    }

    #[test]
    fn recent_fetches_only_new_and_changed_ids_on_boundary_page() {
        let known = HashMap::from([
            ("video-a".into(), "Known".into()),
            ("video-b".into(), "Old title".into()),
        ]);
        let mut calls = 0;
        let snapshot = read("channel-a",Some(("uploads-a","Channel A")),&known,RefreshMode::Recent,|endpoint,params| {
            calls += 1;
            match endpoint {
                "playlistItems" => Ok(json!({"items":[playlist_item("new-video","New"),playlist_item("video-a","Known"),playlist_item("video-b","Changed")],"nextPageToken":"older-page"})),
                "videos" => {
                    assert_eq!(query(params,"id"),Some("new-video,video-b"));
                    Ok(json!({"items":[video("new-video","New"),video("video-b","Changed")]}))
                },
                _ => panic!("Recent cached read must not query channels"),
            }
        }).unwrap();
        assert_eq!(calls, 2);
        assert_eq!(snapshot.videos.len(), 2);
        assert!(!snapshot.complete);
    }

    #[test]
    fn recent_without_cache_verifies_identity_and_pages_until_boundary() {
        let known = HashMap::from([("video-a".into(), "Known".into())]);
        let mut endpoints = Vec::new();
        let snapshot = read("channel-a",None,&known,RefreshMode::Recent,|endpoint,params| {
            endpoints.push(endpoint.to_owned());
            match endpoint {
                "channels" => Ok(channel()),
                "playlistItems" if query(params,"pageToken").is_none() => Ok(json!({"items":[playlist_item("new-video","New")],"nextPageToken":"page-two"})),
                "playlistItems" => Ok(json!({"items":[playlist_item("video-a","Known")],"nextPageToken":"older"})),
                "videos" => Ok(json!({"items":[video("new-video","New")]})),
                _ => panic!("Unexpected endpoint"),
            }
        }).unwrap();
        assert_eq!(
            endpoints,
            ["channels", "playlistItems", "videos", "playlistItems"]
        );
        assert_eq!(snapshot.videos.len(), 1);
        assert!(!snapshot.complete);
    }

    #[test]
    fn foreign_credential_playlist_owner_or_video_cannot_enter_snapshot() {
        for failure in [
            "credential",
            "playlist-owner",
            "video-owner",
            "unrequested-video",
        ] {
            let result = read(
                "channel-a",
                None,
                &HashMap::new(),
                RefreshMode::Full,
                |endpoint, _| match endpoint {
                    "channels" => {
                        let mut value = channel();
                        if failure == "credential" {
                            value["items"][0]["id"] = json!("channel-b");
                        }
                        Ok(value)
                    }
                    "playlistItems" => {
                        let mut value = playlist_item("video-a", "A");
                        if failure == "playlist-owner" {
                            value["snippet"]["videoOwnerChannelId"] = json!("channel-b");
                        }
                        Ok(json!({"items":[value]}))
                    }
                    "videos" => {
                        let mut value = video("video-a", "A");
                        if failure == "video-owner" {
                            value["snippet"]["channelId"] = json!("channel-b");
                        }
                        if failure == "unrequested-video" {
                            value["id"] = json!("video-b");
                        }
                        Ok(json!({"items":[value]}))
                    }
                    _ => panic!("Unexpected endpoint"),
                },
            );
            assert!(result.is_err(), "{failure} must fail atomically");
        }
    }

    #[test]
    fn malformed_pages_and_video_metadata_fail_without_a_snapshot() {
        for malformed in [
            json!({}),
            json!({"items":null}),
            json!({"items":[{}]}),
            json!({"items":[],"nextPageToken":"next"}),
            json!({"items":[],"nextPageToken":10}),
            json!({"items":[playlist_item("video-a","A")],"nextPageToken":"\n"}),
        ] {
            assert!(read(
                "channel-a",
                Some(("uploads-a", "Channel A")),
                &HashMap::new(),
                RefreshMode::Recent,
                |_, _| Ok(malformed.clone())
            )
            .is_err());
        }
        for malformed in [
            json!({"id":"video-a","snippet":{"title":"A"}}),
            video("video-b", "B"),
            json!({"id":"video-a","snippet":{"channelId":"channel-a","title":"A"},"contentDetails":{"duration":2}}),
            json!({"id":"video-a","snippet":{"channelId":"channel-a","title":"A"},"extra":"x".repeat(MAX_VIDEO_BYTES)}),
        ] {
            assert!(read(
                "channel-a",
                Some(("uploads-a", "Channel A")),
                &HashMap::new(),
                RefreshMode::Recent,
                |endpoint, _| Ok(if endpoint == "playlistItems" {
                    json!({"items":[playlist_item("video-a","A")]})
                } else {
                    json!({"items":[malformed.clone()]})
                })
            )
            .is_err());
        }
    }

    #[test]
    fn repeated_pages_ids_and_failed_later_requests_fail_atomically() {
        for failure in ["token-cycle", "repeated-id", "network"] {
            let mut pages = 0;
            let result = read(
                "channel-a",
                Some(("uploads-a", "Channel A")),
                &HashMap::new(),
                RefreshMode::Recent,
                |endpoint, _| {
                    if endpoint == "videos" {
                        return Ok(json!({"items":[]}));
                    }
                    pages += 1;
                    if failure == "network" && pages == 2 {
                        return Err("Synthetic provider unavailable".into());
                    }
                    let id = if failure == "repeated-id" {
                        "same-id".to_string()
                    } else {
                        format!("video-{pages}")
                    };
                    Ok(json!({"items":[playlist_item(&id,"A")],"nextPageToken":"page-two"}))
                },
            );
            assert!(result.is_err(), "{failure}");
            assert_eq!(pages, 2);
        }
    }

    #[test]
    fn empty_recent_remains_merge_only_and_full_allows_unavailable_details() {
        let recent = read(
            "channel-a",
            Some(("uploads-a", "Channel A")),
            &HashMap::new(),
            RefreshMode::Recent,
            |_, _| Ok(json!({"items":[]})),
        )
        .unwrap();
        assert!(!recent.complete);
        let full = read(
            "channel-a",
            None,
            &HashMap::new(),
            RefreshMode::Full,
            |endpoint, _| {
                Ok(match endpoint {
                    "channels" => channel(),
                    "playlistItems" => json!({"items":[playlist_item("video-a","A")]}),
                    _ => json!({"items":[]}),
                })
            },
        )
        .unwrap();
        assert!(full.complete);
        assert!(full.videos.is_empty());
    }
}
