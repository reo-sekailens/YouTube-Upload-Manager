import { open } from "@tauri-apps/plugin-dialog";

import { selectedFilePaths } from "./file-picker";

const supportedVideoExtensions = [
  "3g2",
  "3gp",
  "avi",
  "flv",
  "m4v",
  "mkv",
  "mov",
  "mp4",
  "mpeg",
  "mpg",
  "webm",
  "wmv",
];

export async function selectVideoFiles(): Promise<string[]> {
  const selected = await open({
    multiple: true,
    directory: false,
    filters: [{ name: "Video", extensions: supportedVideoExtensions }],
  });
  return selectedFilePaths(selected);
}

export async function selectPreflightFiles(): Promise<string[]> {
  return selectedFilePaths(
    await open({
      multiple: true,
      directory: false,
      pickerMode: "document",
      fileAccessMode: "scoped",
    }),
  );
}
