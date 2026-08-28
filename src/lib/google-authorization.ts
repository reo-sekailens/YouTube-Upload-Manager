/** Opens a native-issued Google consent URL and copies it for the operator. */
export function validateGoogleAuthorizationUrl(authorizationUrl: string): string {
  const url = new URL(authorizationUrl);
  if (url.protocol !== "https:" || url.hostname !== "accounts.google.com")
    throw new Error("The Google authorization request must use HTTPS on accounts.google.com.");
  return url.toString();
}

export async function openAndCopyGoogleAuthorization(authorizationUrl: string): Promise<boolean> {
  const url = validateGoogleAuthorizationUrl(authorizationUrl);

  let copied = false;
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(url);
      copied = true;
    } else {
      const textarea = document.createElement("textarea");
      textarea.value = url;
      textarea.setAttribute("readonly", "");
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      copied = document.execCommand("copy");
      textarea.remove();
    }
  } catch {
    // Opening consent remains available if this platform denies clipboard access.
  }

  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
  return copied;
}
