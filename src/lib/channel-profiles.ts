import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ConnectionSettings, YouTubeConnectionStart } from "./types";

/** Safe profile projections never contain tokens or credential-store references. */
export type ChannelProfile = {
  profileId: string;
  channelId: string;
  channelName: string;
  projectId?: string | null;
  selected: boolean;
  managementAuthorized: boolean;
  removed: false;
};
export type ChannelProfileWorkspace = {
  activeChannelId?: string | null;
  profiles: ChannelProfile[];
  canSwitch: boolean;
  holdReason?: string | null;
};
export type ChannelProfileReview = {
  reviewId: string;
  state: "ready" | "authorizing" | "verified" | "completed" | "failed" | "cancelled";
  channelId?: string | null;
  channelName?: string | null;
  projectId?: string | null;
  detail?: string;
};

function nativeOnly() {
  if (!isTauri()) throw new Error("Saved channel profiles are available in the desktop app.");
}
function scope(expectedChannelId?: string) { return expectedChannelId ?? null; }

export function loadChannelProfiles(expectedChannelId?: string) {
  nativeOnly();
  return invoke<ChannelProfileWorkspace>("load_channel_profiles", { expectedChannelId: scope(expectedChannelId) });
}
export function prepareChannelProfile(path: string, expectedChannelId?: string) {
  nativeOnly();
  return invoke<ChannelProfileReview>("prepare_channel_profile", { path, expectedChannelId: scope(expectedChannelId) });
}
export function beginChannelProfileAuthorization(reviewId: string, expectedChannelId?: string) {
  nativeOnly();
  return invoke<YouTubeConnectionStart>("begin_channel_profile_authorization", { reviewId, expectedChannelId: scope(expectedChannelId) });
}
export function loadChannelProfileReview(reviewId: string) {
  nativeOnly();
  return invoke<ChannelProfileReview>("load_channel_profile_review", { reviewId });
}
export function commitChannelProfile(reviewId: string, expectedChannelId: string | undefined, approval: boolean) {
  nativeOnly();
  return invoke<ChannelProfileWorkspace>("commit_channel_profile", { reviewId, expectedChannelId: scope(expectedChannelId), approval });
}
export function cancelChannelProfileReview(reviewId: string) {
  nativeOnly();
  return invoke<ConnectionSettings>("cancel_channel_profile_review", { reviewId });
}
export function switchChannelProfile(profileId: string, expectedChannelId: string | undefined, approval: boolean) {
  nativeOnly();
  return invoke<ConnectionSettings>("switch_channel_profile", { profileId, expectedChannelId: scope(expectedChannelId), approval });
}
export function removeChannelProfile(profileId: string, expectedChannelId: string | undefined, approval: boolean) {
  nativeOnly();
  return invoke<ConnectionSettings>("remove_channel_profile", { profileId, expectedChannelId: scope(expectedChannelId), approval });
}

export function profileWorkspaceMatches(workspace: ChannelProfileWorkspace, expectedChannelId?: string) {
  return (workspace.activeChannelId ?? undefined) === expectedChannelId
    && new Set(workspace.profiles.map((profile) => profile.profileId)).size === workspace.profiles.length
    && new Set(workspace.profiles.map((profile) => profile.channelId)).size === workspace.profiles.length
    && workspace.profiles.every((profile) => Boolean(profile.profileId && profile.channelId) && profile.removed === false && profile.selected === (profile.channelId === expectedChannelId));
}

export function verifiedProfileReview(review: ChannelProfileReview) {
  return review.state === "verified" && Boolean(review.reviewId && review.channelId && review.channelName);
}
